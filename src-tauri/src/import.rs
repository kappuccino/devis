//! Import du classeur « LPN finale.xlsx » dans SQLite.
//! Remplace produits, clients et listes de prix ; les devis ne sont jamais touchés.

use calamine::{open_workbook, Data, Range, Reader, Xlsx};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

const SHEET_PRODUCTS: &str = "DATA";
const SHEET_CLIENTS: &str = "gestion tarifs";
const SHEET_PRICES: &str = "fichier injection tarif";

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub products: usize,
    pub clients: usize,
    pub price_lists: usize,
    pub price_list_items: usize,
    pub client_links: usize,
    pub warnings: Vec<String>,
}

pub fn import_lpn(conn: &mut Connection, path: &Path) -> Result<ImportReport, String> {
    let mut wb: Xlsx<_> = open_workbook(path).map_err(|e| format!("Ouverture du fichier : {e}"))?;
    let products = sheet(&mut wb, SHEET_PRODUCTS)?;
    let clients = sheet(&mut wb, SHEET_CLIENTS)?;
    let prices = sheet(&mut wb, SHEET_PRICES)?;

    import_ranges(conn, &products, &clients, &prices).map_err(|e| e.to_string())
}

/// Remplace produits, clients et listes de prix, en une transaction.
fn import_ranges(
    conn: &mut Connection,
    products: &Range<Data>,
    clients: &Range<Data>,
    prices: &Range<Data>,
) -> rusqlite::Result<ImportReport> {
    let mut report = ImportReport::default();
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM client_price_lists;
         DELETE FROM price_list_items;
         DELETE FROM price_lists;
         DELETE FROM clients;
         DELETE FROM products;",
    )?;

    import_products(&tx, products, &mut report)?;
    import_price_lists(&tx, prices, &mut report)?;
    import_clients(&tx, clients, &mut report)?;

    tx.commit()?;
    Ok(report)
}

/// Trouve une feuille par nom en ignorant les espaces et la casse
/// (« fichier injection tarif » a un espace final dans le classeur).
fn sheet(wb: &mut Xlsx<std::io::BufReader<std::fs::File>>, wanted: &str) -> Result<Range<Data>, String> {
    let name = wb
        .sheet_names()
        .into_iter()
        .find(|n| n.trim().eq_ignore_ascii_case(wanted))
        .ok_or_else(|| format!("Feuille « {wanted} » absente du fichier"))?;
    let range = wb.worksheet_range(&name).map_err(|e| format!("Lecture de « {name} » : {e}"))?;
    // Les index de colonnes ci-dessous supposent que l'en-tête commence en A1.
    if range.start() != Some((0, 0)) {
        return Err(format!("La feuille « {name} » doit commencer en A1"));
    }
    Ok(range)
}

/// Itère sur les lignes de données non vides (après l'en-tête).
fn data_rows(range: &Range<Data>) -> impl Iterator<Item = &[Data]> {
    range
        .rows()
        .skip(1)
        .filter(|row| row.iter().any(|c| !matches!(c, Data::Empty)))
}

fn cell(row: &[Data], col: usize) -> &Data {
    row.get(col).unwrap_or(&Data::Empty)
}

fn text(row: &[Data], col: usize) -> Option<String> {
    let s = match cell(row, col) {
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 => format!("{f:.0}"),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        _ => return None,
    };
    if s.is_empty() || s.starts_with('#') {
        None
    } else {
        Some(s)
    }
}

fn number(row: &[Data], col: usize) -> Option<f64> {
    match cell(row, col) {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

fn import_products(tx: &Connection, range: &Range<Data>, report: &mut ImportReport) -> rusqlite::Result<()> {
    // A réf | B code ENEDIS | C désignation | D prix tarif | E éco-taxe | F code éco-taxe | G famille | L prix seuil
    let mut stmt = tx.prepare(
        "INSERT OR IGNORE INTO products
         (ref, enedis_code, designation, public_price, eco_tax, eco_tax_code, family, threshold_price)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut duplicates = Vec::new();
    for row in data_rows(range) {
        let Some(r) = text(row, 0) else { continue };
        let family = text(row, 6).map(|f| f.to_uppercase()).filter(|f| f == "CFA" || f == "CFO");
        let inserted = stmt.execute(params![
            r,
            text(row, 1).filter(|c| c != "-"),
            text(row, 2).unwrap_or_default(),
            number(row, 3).unwrap_or(0.0),
            number(row, 4),
            text(row, 5),
            family,
            number(row, 11),
        ])?;
        if inserted == 0 {
            duplicates.push(r);
        } else {
            report.products += 1;
        }
    }
    if !duplicates.is_empty() {
        report.warnings.push(format!(
            "{} référence(s) produit en double (première conservée) : {}",
            duplicates.len(),
            duplicates.join(", ")
        ));
    }
    Ok(())
}

fn import_price_lists(tx: &Connection, range: &Range<Data>, report: &mut ImportReport) -> rusqlite::Result<()> {
    // B client | D code analyse (= code liste) | E réf | J description | O prix | Q désignation
    let mut list_stmt = tx.prepare("INSERT OR IGNORE INTO price_lists (code, label) VALUES (?1, ?2)")?;
    let mut item_stmt = tx.prepare(
        "INSERT INTO price_list_items (price_list_code, product_ref, designation, price, label)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut skipped = 0;
    for row in data_rows(range) {
        let (Some(code), Some(r)) = (text(row, 3), text(row, 4)) else {
            skipped += 1;
            continue;
        };
        let Some(price) = number(row, 14).filter(|p| *p > 0.0) else {
            skipped += 1;
            continue;
        };
        let code = code.to_uppercase();
        report.price_lists += list_stmt.execute(params![code, text(row, 1)])?;
        item_stmt.execute(params![code, r, text(row, 16), price, text(row, 9)])?;
        report.price_list_items += 1;
    }
    if skipped > 0 {
        report.warnings.push(format!(
            "{skipped} ligne(s) de prix ignorée(s) (code liste, référence ou prix manquant ou nul)"
        ));
    }
    Ok(())
}

fn import_clients(tx: &Connection, range: &Range<Data>, report: &mut ImportReport) -> rusqlite::Result<()> {
    // A raison sociale | B code | D groupe | E sous-groupe | F commercial
    // I, K, L, M listes de prix | P remise CFA | R remise CFO | S mails | U franco | V SIREN
    const LIST_COLS: [usize; 4] = [8, 10, 11, 12];
    let known_lists: HashSet<String> = tx
        .prepare("SELECT code FROM price_lists")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    let mut client_stmt = tx.prepare(
        "INSERT OR IGNORE INTO clients
         (code, name, group_name, subgroup, sales_rep, discount_cfa, discount_cfo, email, franco, siren)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut link_stmt =
        tx.prepare("INSERT OR IGNORE INTO client_price_lists (client_code, price_list_code) VALUES (?1, ?2)")?;

    let mut duplicates = Vec::new();
    let mut missing_discount = 0;
    let mut unknown_lists = HashSet::new();
    for row in data_rows(range) {
        let Some(code) = text(row, 1).map(|c| c.to_uppercase()) else { continue };
        if code.chars().all(|c| c == '0') {
            continue;
        }
        let (cfa, cfo) = (number(row, 15), number(row, 17));
        if cfa.is_none() || cfo.is_none() {
            missing_discount += 1;
        }
        let inserted = client_stmt.execute(params![
            code,
            text(row, 0).unwrap_or_default(),
            text(row, 3),
            text(row, 4),
            text(row, 5),
            cfa.unwrap_or(0.0),
            cfo.unwrap_or(0.0),
            text(row, 18),
            text(row, 20),
            text(row, 21),
        ])?;
        if inserted == 0 {
            duplicates.push(code);
            continue;
        }
        report.clients += 1;
        for col in LIST_COLS {
            let Some(list) = text(row, col).map(|l| l.to_uppercase()) else { continue };
            if known_lists.contains(&list) {
                report.client_links += link_stmt.execute(params![code, list])?;
            } else if list.starts_with('T') {
                unknown_lists.insert(list);
            }
        }
    }
    if !duplicates.is_empty() {
        report.warnings.push(format!(
            "{} code(s) client en double (première ligne conservée) : {}",
            duplicates.len(),
            duplicates.join(", ")
        ));
    }
    if missing_discount > 0 {
        report
            .warnings
            .push(format!("{missing_discount} client(s) sans remise CFA ou CFO (0 % appliqué)"));
    }
    if !unknown_lists.is_empty() {
        report.warnings.push(format!(
            "{} liste(s) rattachée(s) à des clients mais sans aucune ligne de prix",
            unknown_lists.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `LPN_XLSX=chemin/vers/LPN.xlsx cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn import_real_file() {
        let path = std::env::var("LPN_XLSX").expect("LPN_XLSX non défini");
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let report = import_lpn(&mut conn, Path::new(&path)).unwrap();
        println!("{report:#?}");
        assert!(report.products > 0 && report.clients > 0 && report.price_list_items > 0);
        // Un prix de liste pris dans le fichier lui-même doit se retrouver tel quel.
        let (list, product, price): (String, String, f64) = conn
            .query_row(
                "SELECT price_list_code, product_ref, MIN(price) FROM price_list_items
                 GROUP BY price_list_code, product_ref LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let ctx = crate::pricing::PricingContext { price_lists: vec![list], ..Default::default() };
        let p = crate::pricing::resolve_price(&conn, &ctx, &product).unwrap();
        assert_eq!(p.unit_price, crate::pricing::round2(price));
    }
}
