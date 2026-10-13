//! Import du classeur « LPN finale.xlsx » dans SQLite.
//! Remplace produits, clients et listes de prix ; les devis ne sont jamais touchés.
//! Les clients viennent de « gestion tarifs », complétés par « CLIENTS IC » (code et nom seulement),
//! puis par un client par liste de prix rattachée à personne (code de la liste sans son « T ») :
//! la liste est rattachée à ce client, existant ou créé.

use calamine::{open_workbook, Data, Range, Reader, Xlsx};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

const SHEET_PRODUCTS: &str = "DATA";
const SHEET_CLIENTS: &str = "gestion tarifs";
const SHEET_PRICES: &str = "fichier injection tarif";
const SHEET_CLIENTS_IC: &str = "CLIENTS IC";

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub products: usize,
    pub clients: usize,
    /// Dont clients venus seulement de « CLIENTS IC » (ni remise ni liste de prix).
    pub clients_ic: usize,
    /// Dont clients créés pour une liste de prix rattachée à aucun client.
    pub clients_from_lists: usize,
    /// Listes de prix sans client rattachées au client existant de même code (sans le « T »).
    pub lists_linked_to_existing: usize,
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
    let mut warnings = Vec::new();
    let clients_ic = match sheet(&mut wb, SHEET_CLIENTS_IC) {
        Ok(range) => Some(range),
        Err(e) => {
            warnings.push(e);
            None
        }
    };

    let mut report =
        import_ranges(conn, &products, &clients, &prices, clients_ic.as_ref()).map_err(|e| e.to_string())?;
    report.warnings.extend(warnings);
    Ok(report)
}

/// Remplace produits, clients et listes de prix, en une transaction.
fn import_ranges(
    conn: &mut Connection,
    products: &Range<Data>,
    clients: &Range<Data>,
    prices: &Range<Data>,
    clients_ic: Option<&Range<Data>>,
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
    if let Some(range) = clients_ic {
        import_clients_ic(&tx, range, &mut report)?;
    }
    clients_for_orphan_lists(&tx, &mut report)?;

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
        // Code en double : la première ligne est conservée, mais reçoit aussi les listes de celle-ci.
        if inserted == 0 {
            duplicates.push(code.clone());
        } else {
            report.clients += 1;
        }
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
            "{} code(s) client en double (première ligne conservée, listes de prix des doublons rattachées) : {}",
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

/// Ajoute les clients de « CLIENTS IC » absents de « gestion tarifs » (prioritaire) :
/// code et raison sociale seulement, sans remise ni liste de prix.
fn import_clients_ic(tx: &Connection, range: &Range<Data>, report: &mut ImportReport) -> rusqlite::Result<()> {
    // A code | B raison sociale
    let mut stmt = tx.prepare("INSERT OR IGNORE INTO clients (code, name) VALUES (?1, ?2)")?;
    for row in data_rows(range) {
        let Some(code) = text(row, 0).map(|c| c.to_uppercase()) else { continue };
        if code.chars().all(|c| c == '0') {
            continue;
        }
        let inserted = stmt.execute(params![code, text(row, 1).unwrap_or_default()])?;
        report.clients += inserted;
        report.clients_ic += inserted;
    }
    Ok(())
}

/// Rattache chaque liste de prix sans client au client dont le code est celui de la liste sans son
/// « T » initial (T205000 → 205000), en le créant au besoin (nom = intitulé de la liste).
fn clients_for_orphan_lists(tx: &Connection, report: &mut ImportReport) -> rusqlite::Result<()> {
    let orphans: Vec<(String, Option<String>)> = tx
        .prepare(
            "SELECT code, label FROM price_lists
             WHERE code NOT IN (SELECT price_list_code FROM client_price_lists)
             ORDER BY code",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let mut client_stmt = tx.prepare("INSERT OR IGNORE INTO clients (code, name) VALUES (?1, ?2)")?;
    let mut link_stmt = tx.prepare("INSERT INTO client_price_lists (client_code, price_list_code) VALUES (?1, ?2)")?;
    for (list, label) in orphans {
        let code = list.strip_prefix('T').unwrap_or(&list).to_string();
        let name = label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| list.clone());
        if client_stmt.execute(params![code, name])? == 1 {
            report.clients += 1;
            report.clients_from_lists += 1;
        } else {
            report.lists_linked_to_existing += 1;
        }
        link_stmt.execute(params![code, list])?;
        report.client_links += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(rows: &[&[&str]]) -> Range<Data> {
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        let mut range = Range::new((0, 0), (rows.len() as u32 - 1, width as u32 - 1));
        for (i, row) in rows.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                if !v.is_empty() {
                    range.set_value((i as u32, j as u32), Data::String(v.to_string()));
                }
            }
        }
        range
    }

    #[test]
    fn clients_ic_completes_gestion_tarifs() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let header: &[&str] = &["en-tête"];
        let mut tarifs_row = vec![""; 18];
        tarifs_row[0] = "CLIENT TARIF";
        tarifs_row[1] = "001200";
        tarifs_row[8] = "T999000";
        tarifs_row[15] = "57,5";
        tarifs_row[17] = "60";
        let mut duplicate_row = vec![""; 13];
        (duplicate_row[0], duplicate_row[1], duplicate_row[12]) = ("DOUBLON", "001200", "T205000");
        let tarifs = range(&[header, &tarifs_row, &duplicate_row]);
        let ic = range(&[
            header,
            &["001200", "NOM IC IGNORÉ"],
            &["13770k", "CEF 13 AUBAGNE"],
            &["000000", "ZÉRO"],
            &["", "SANS CODE"],
        ]);
        // B intitulé | D code liste | E réf | O prix
        let price = |label: &'static str, list: &'static str| -> Vec<&'static str> {
            let mut row = vec![""; 15];
            (row[1], row[3], row[4], row[14]) = (label, list, "REF", "1");
            row
        };
        let prices = range(&[
            header,
            &price("LISTE RATTACHÉE", "T999000"),
            &price("COTRELEC SERTELEC", "T205000"),
            &price("CLIENT EXISTANT", "T13770K"),
            &price("NOUVEAU CLIENT", "T300000"),
        ]);
        let empty = range(&[header]);
        let report = import_ranges(&mut conn, &empty, &tarifs, &prices, Some(&ic)).unwrap();
        assert_eq!((report.clients, report.clients_ic, report.clients_from_lists), (3, 1, 1));
        assert_eq!(report.lists_linked_to_existing, 1);

        let rows: Vec<(String, String, f64)> = conn
            .prepare("SELECT code, name, discount_cfa FROM clients ORDER BY code")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("001200".into(), "CLIENT TARIF".into(), 57.5),
                ("13770K".into(), "CEF 13 AUBAGNE".into(), 0.0),
                ("300000".into(), "NOUVEAU CLIENT".into(), 0.0),
            ]
        );
        let links: Vec<(String, String)> = conn
            .prepare("SELECT client_code, price_list_code FROM client_price_lists ORDER BY client_code")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            links,
            vec![
                ("001200".into(), "T205000".into()),
                ("001200".into(), "T999000".into()),
                ("13770K".into(), "T13770K".into()),
                ("300000".into(), "T300000".into()),
            ]
        );
    }

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
