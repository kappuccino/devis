use crate::import::{self, ImportReport};
use crate::pricing::{self, discounted_total, line_total, round2, PricingContext, ResolvedPrice};

/// Lignes de frais (toujours en bas du devis) : montant HT dans `unit_price`, jamais remisé.
fn is_fee(kind: &str) -> bool {
    kind == "shipping" || kind == "billing"
}

/// Totaux d'un devis.
/// - total HT : produits (hors options) + frais de port et de facturation ;
/// - remise globale : sur les produits seulement ;
/// - options : à part, hors total.
struct Totals {
    total_ht: f64,
    total_net: f64,
    options: f64,
}

fn quote_totals(lines: &[QuoteLine], discount_pct: f64) -> Totals {
    let (mut products, mut fees, mut options) = (0.0, 0.0, 0.0);
    for l in lines {
        if l.kind == "item" {
            let amount = line_total(l.quantity, l.unit_price, l.discount);
            if l.is_option {
                options += amount;
            } else {
                products += amount;
            }
        } else if is_fee(&l.kind) {
            fees += round2(l.unit_price);
        }
    }
    let products = round2(products);
    let discount = round2(products - discounted_total(products, discount_pct));
    let total_ht = round2(products + fees);
    Totals { total_ht, total_net: round2(total_ht - discount), options: round2(options) }
}
use crate::AppState;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::State;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn query_all<T>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    map: impl FnMut(&Row) -> rusqlite::Result<T>,
) -> CmdResult<Vec<T>> {
    let mut stmt = conn.prepare(sql).map_err(err)?;
    let rows = stmt.query_map(params, map).map_err(err)?;
    rows.collect::<Result<_, _>>().map_err(err)
}

// ---------- Produits ----------

#[derive(Serialize)]
pub struct Product {
    pub r#ref: String,
    pub enedis_code: Option<String>,
    pub designation: String,
    pub public_price: f64,
    pub eco_tax: Option<f64>,
    pub eco_tax_code: Option<String>,
    pub family: Option<String>,
    pub threshold_price: Option<f64>,
}

#[tauri::command]
pub fn list_products(state: State<AppState>) -> CmdResult<Vec<Product>> {
    let conn = state.conn();
    query_all(
        &conn,
        "SELECT ref, enedis_code, designation, public_price, eco_tax, eco_tax_code, family, threshold_price
         FROM products ORDER BY ref",
        [],
        |r| {
            Ok(Product {
                r#ref: r.get(0)?,
                enedis_code: r.get(1)?,
                designation: r.get(2)?,
                public_price: r.get(3)?,
                eco_tax: r.get(4)?,
                eco_tax_code: r.get(5)?,
                family: r.get(6)?,
                threshold_price: r.get(7)?,
            })
        },
    )
}

#[derive(Serialize)]
pub struct ProductHit {
    pub r#ref: String,
    pub designation: String,
    pub enedis_code: Option<String>,
}

/// Recherche pour l'autocomplétion des lignes de devis : produits du catalogue
/// et références présentes uniquement dans les listes de prix du client.
#[tauri::command]
pub fn search_products(state: State<AppState>, query: String, price_lists: Vec<String>) -> CmdResult<Vec<ProductHit>> {
    let conn = state.conn();
    let like = format!("%{}%", query.trim());
    query_all(
        &conn,
        "SELECT ref, designation, enedis_code FROM (
             SELECT ref, designation, enedis_code FROM products
             WHERE ref LIKE ?1 OR designation LIKE ?1
                OR replace(enedis_code, '.', '') LIKE replace(?1, '.', '')
             UNION
             SELECT pli.product_ref, MAX(COALESCE(pli.designation, '')), NULL
             FROM price_list_items pli
             WHERE pli.price_list_code IN (SELECT value FROM json_each(?2))
               AND (pli.product_ref LIKE ?1 OR pli.designation LIKE ?1)
               AND pli.product_ref NOT IN (SELECT ref FROM products)
             GROUP BY pli.product_ref
         ) ORDER BY ref LIMIT 50",
        params![like, serde_json::to_string(&price_lists).map_err(err)?],
        |r| Ok(ProductHit { r#ref: r.get(0)?, designation: r.get(1)?, enedis_code: r.get(2)? }),
    )
}

// ---------- Clients ----------

#[derive(Serialize, Deserialize)]
pub struct Client {
    pub code: String,
    pub name: String,
    pub group_name: Option<String>,
    pub subgroup: Option<String>,
    pub sales_rep: Option<String>,
    pub discount_cfa: f64,
    pub discount_cfo: f64,
    pub email: Option<String>,
    pub franco: Option<String>,
    pub siren: Option<String>,
    pub price_lists: Vec<String>,
}

const CLIENT_SELECT: &str =
    "SELECT c.code, c.name, c.group_name, c.subgroup, c.sales_rep, c.discount_cfa, c.discount_cfo,
            c.email, c.franco, c.siren,
            (SELECT group_concat(price_list_code, ',') FROM client_price_lists WHERE client_code = c.code)
     FROM clients c";

fn map_client(r: &Row) -> rusqlite::Result<Client> {
    let lists: Option<String> = r.get(10)?;
    Ok(Client {
        code: r.get(0)?,
        name: r.get(1)?,
        group_name: r.get(2)?,
        subgroup: r.get(3)?,
        sales_rep: r.get(4)?,
        discount_cfa: r.get(5)?,
        discount_cfo: r.get(6)?,
        email: r.get(7)?,
        franco: r.get(8)?,
        siren: r.get(9)?,
        price_lists: lists
            .map(|l| l.split(',').map(str::to_string).collect())
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub fn list_clients(state: State<AppState>) -> CmdResult<Vec<Client>> {
    let conn = state.conn();
    query_all(&conn, &format!("{CLIENT_SELECT} ORDER BY c.name"), [], map_client)
}

#[tauri::command]
pub fn get_client(state: State<AppState>, code: String) -> CmdResult<Option<Client>> {
    let conn = state.conn();
    conn.query_row(&format!("{CLIENT_SELECT} WHERE c.code = ?1"), [code], map_client)
        .optional()
        .map_err(err)
}

/// Modifie les remises et les listes de prix rattachées d'un client.
#[tauri::command]
pub fn update_client(
    state: State<AppState>,
    code: String,
    discount_cfa: f64,
    discount_cfo: f64,
    price_lists: Vec<String>,
) -> CmdResult<()> {
    let mut conn = state.conn();
    let tx = conn.transaction().map_err(err)?;
    tx.execute(
        "UPDATE clients SET discount_cfa = ?2, discount_cfo = ?3 WHERE code = ?1",
        params![code, discount_cfa, discount_cfo],
    )
    .map_err(err)?;
    tx.execute("DELETE FROM client_price_lists WHERE client_code = ?1", [&code])
        .map_err(err)?;
    for list in price_lists {
        tx.execute(
            "INSERT OR IGNORE INTO client_price_lists (client_code, price_list_code) VALUES (?1, ?2)",
            params![code, list],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

/// Rattache (`attached`) ou détache un client d'une liste de prix.
#[tauri::command]
pub fn set_client_price_list(
    state: State<AppState>,
    client_code: String,
    price_list_code: String,
    attached: bool,
) -> CmdResult<()> {
    link_client_price_list(&state.conn(), &client_code, &price_list_code, attached)
}

fn link_client_price_list(conn: &Connection, client: &str, list: &str, attached: bool) -> CmdResult<()> {
    let sql = if attached {
        "INSERT OR IGNORE INTO client_price_lists (client_code, price_list_code) VALUES (?1, ?2)"
    } else {
        "DELETE FROM client_price_lists WHERE client_code = ?1 AND price_list_code = ?2"
    };
    conn.execute(sql, params![client, list]).map(|_| ()).map_err(err)
}

// ---------- Listes de prix ----------

#[derive(Serialize)]
pub struct PriceList {
    pub code: String,
    pub label: Option<String>,
    pub item_count: i64,
    pub client_count: i64,
}

#[tauri::command]
pub fn list_price_lists(state: State<AppState>) -> CmdResult<Vec<PriceList>> {
    let conn = state.conn();
    query_all(
        &conn,
        "SELECT pl.code, pl.label,
                (SELECT COUNT(*) FROM price_list_items WHERE price_list_code = pl.code),
                (SELECT COUNT(*) FROM client_price_lists WHERE price_list_code = pl.code)
         FROM price_lists pl ORDER BY pl.code",
        [],
        |r| Ok(PriceList { code: r.get(0)?, label: r.get(1)?, item_count: r.get(2)?, client_count: r.get(3)? }),
    )
}

#[derive(Serialize)]
pub struct PriceListItem {
    pub product_ref: String,
    pub designation: Option<String>,
    pub price: f64,
    pub label: Option<String>,
    pub public_price: Option<f64>,
    pub family: Option<String>,
}

#[tauri::command]
pub fn get_price_list_items(state: State<AppState>, code: String) -> CmdResult<Vec<PriceListItem>> {
    let conn = state.conn();
    query_all(
        &conn,
        "SELECT pli.product_ref, COALESCE(p.designation, pli.designation), pli.price, pli.label,
                p.public_price, p.family
         FROM price_list_items pli LEFT JOIN products p ON p.ref = pli.product_ref
         WHERE pli.price_list_code = ?1
         ORDER BY pli.product_ref, pli.price",
        [code],
        |r| {
            Ok(PriceListItem {
                product_ref: r.get(0)?,
                designation: r.get(1)?,
                price: r.get(2)?,
                label: r.get(3)?,
                public_price: r.get(4)?,
                family: r.get(5)?,
            })
        },
    )
}

// ---------- Statistiques ----------

/// Statistiques des devis d'une année (`"2026"`), ou de toute la base si `year` est absent.
#[tauri::command]
pub fn quote_stats(state: State<AppState>, year: Option<String>) -> CmdResult<crate::stats::Stats> {
    crate::stats::quote_stats(&state.conn(), year.as_deref())
}

// ---------- Sauvegarde ----------

#[derive(Serialize)]
pub struct BackupInfo {
    /// Dossier utilisé, et dossier par défaut (à côté de la base).
    dir: String,
    default_dir: String,
    files: Vec<crate::backup::BackupFile>,
}

#[tauri::command]
pub fn backup_info(state: State<AppState>) -> CmdResult<BackupInfo> {
    let conn = state.conn();
    let dir = crate::backup::dir(&conn, &state.db_path)?;
    Ok(BackupInfo {
        files: crate::backup::list(&dir)?,
        dir: dir.to_string_lossy().into_owned(),
        default_dir: crate::backup::default_dir(&state.db_path).to_string_lossy().into_owned(),
    })
}

/// Sauvegarde maintenant ; renvoie le chemin de la copie.
#[tauri::command]
pub fn backup_now(state: State<AppState>) -> CmdResult<String> {
    let conn = state.conn();
    crate::backup::backup_now(&conn, &state.db_path).map(|p| p.to_string_lossy().into_owned())
}

/// Restaure une copie ; renvoie le chemin de la copie de sécurité de la base remplacée.
#[tauri::command]
pub fn restore_backup(state: State<AppState>, path: String) -> CmdResult<String> {
    let mut conn = state.conn();
    crate::backup::restore(&mut conn, &state.db_path, std::path::Path::new(&path))
        .map(|p| p.to_string_lossy().into_owned())
}

/// Contact imprimé dans le haut de l'export Excel (saisi au moment de l'export).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportContact {
    pub last_name: String,
    pub first_name: String,
    pub email: String,
}

/// Exporte une liste de prix d'un client vers Excel (`path`), à partir du gabarit des réglages.
#[tauri::command]
pub fn export_price_list(
    state: State<AppState>,
    path: String,
    client_code: String,
    price_list_code: String,
    contact: ExportContact,
) -> CmdResult<()> {
    let conn = state.conn();
    let template = setting(&conn, "price_list_template")?
        .filter(|p| !p.trim().is_empty())
        .ok_or("Choisissez d'abord le gabarit Excel dans Réglages → Config PDF → Export Excel.")?;
    let template = std::fs::read(&template).map_err(|e| format!("Gabarit « {template} » : {e}"))?;
    let client_name: String = conn
        .query_row("SELECT name FROM clients WHERE code = ?1", [&client_code], |r| r.get(0))
        .optional()
        .map_err(err)?
        .ok_or_else(|| format!("Client {client_code} introuvable"))?;
    let rows = price_list_export_rows(&conn, &price_list_code)?;
    let header = crate::xlsx_export::Header {
        client_name,
        client_code,
        email: contact.email,
        last_name: contact.last_name,
        first_name: contact.first_name,
    };
    let bytes = crate::xlsx_export::fill_template(&template, &header, &rows)?;
    std::fs::write(&path, bytes).map_err(|e| format!("Enregistrement de « {path} » : {e}"))
}

/// Lignes de l'export : une par référence (prix le plus bas si la liste en a plusieurs).
fn price_list_export_rows(conn: &Connection, code: &str) -> CmdResult<Vec<crate::xlsx_export::Row>> {
    query_all(
        conn,
        "SELECT pli.product_ref, COALESCE(p.designation, MAX(pli.designation), ''), COALESCE(p.enedis_code, ''),
                MIN(pli.price)
         FROM price_list_items pli LEFT JOIN products p ON p.ref = pli.product_ref
         WHERE pli.price_list_code = ?1
         GROUP BY pli.product_ref
         ORDER BY pli.product_ref",
        [code],
        |r| {
            Ok(crate::xlsx_export::Row {
                product_ref: r.get(0)?,
                designation: r.get(1)?,
                enedis_code: r.get(2)?,
                price: r.get(3)?,
            })
        },
    )
}

#[derive(Serialize)]
pub struct ClientRef {
    pub code: String,
    pub name: String,
}

#[tauri::command]
pub fn get_price_list_clients(state: State<AppState>, code: String) -> CmdResult<Vec<ClientRef>> {
    let conn = state.conn();
    query_all(
        &conn,
        "SELECT c.code, c.name FROM clients c
         JOIN client_price_lists cpl ON cpl.client_code = c.code
         WHERE cpl.price_list_code = ?1 ORDER BY c.name",
        [code],
        |r| Ok(ClientRef { code: r.get(0)?, name: r.get(1)? }),
    )
}

// ---------- Prix ----------

#[tauri::command]
pub fn resolve_price(state: State<AppState>, pricing: PricingContext, product_ref: String) -> CmdResult<ResolvedPrice> {
    pricing::resolve_price(&state.conn(), &pricing, &product_ref)
}

// ---------- Devis ----------

#[derive(Serialize)]
pub struct QuoteSummary {
    pub id: i64,
    pub number: String,
    /// None : client ponctuel.
    pub client_code: Option<String>,
    pub client_name: String,
    pub date: String,
    pub total_ht: f64,
    /// Total après la remise globale.
    pub total_net: f64,
    pub line_count: i64,
}

#[tauri::command]
pub fn list_quotes(state: State<AppState>) -> CmdResult<Vec<QuoteSummary>> {
    let conn = state.conn();
    query_all(
        &conn,
        "SELECT q.id, q.number, q.client_code, q.client_name, q.date, q.total_ht, q.total_net,
                (SELECT COUNT(*) FROM quote_lines WHERE quote_id = q.id AND kind = 'item')
         FROM quotes q ORDER BY q.date DESC, q.id DESC",
        [],
        |r| {
            Ok(QuoteSummary {
                id: r.get(0)?,
                number: r.get(1)?,
                client_code: code_or_none(r.get(2)?),
                client_name: r.get(3)?,
                date: r.get(4)?,
                total_ht: r.get(5)?,
                total_net: r.get(6)?,
                line_count: r.get(7)?,
            })
        },
    )
}

fn item_kind() -> String {
    "item".to_string()
}

/// Ligne de devis : un article (`item`), une ligne de texte (`text`, dans `designation`)
/// ou un sous-total (`subtotal`) des articles depuis le sous-total précédent.
#[derive(Serialize, Deserialize, Clone)]
pub struct QuoteLine {
    #[serde(default = "item_kind")]
    pub kind: String,
    pub product_ref: String,
    /// Code ENEDIS, copié du catalogue à la saisie de la ligne.
    #[serde(default)]
    pub enedis_code: Option<String>,
    pub designation: String,
    pub quantity: f64,
    pub unit_price: f64,
    /// Remise supplémentaire de la ligne, en %.
    #[serde(default)]
    pub discount: f64,
    /// Ligne en option : comptée dans « Total options », pas dans le total HT ni les sous-totaux.
    #[serde(default)]
    pub is_option: bool,
    /// Affichage seulement (jamais sur le PDF) : prix public remisé et prix LPN au moment du devis.
    #[serde(default)]
    pub discounted_price: Option<f64>,
    #[serde(default)]
    pub lpn_price: Option<f64>,
    #[serde(default)]
    pub lpn_list: Option<String>,
    pub price_source: Option<String>,
    #[serde(default)]
    pub public_price: Option<f64>,
    /// Affiché dans l'appli, jamais sur le PDF.
    #[serde(default)]
    pub threshold_price: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct Quote {
    pub id: Option<i64>,
    pub number: Option<String>,
    /// Client de la base, ou None pour un client ponctuel (qui n'existe que dans ce devis).
    pub client_code: Option<String>,
    pub client_name: String,
    /// Remises et listes de prix du devis (copiées du client, modifiables).
    #[serde(flatten)]
    pub pricing: PricingContext,
    /// Contact chez le client et commercial.
    #[serde(flatten)]
    pub contact: QuoteContact,
    pub date: String,
    pub notes: Option<String>,
    /// Somme des lignes (remises de ligne comprises).
    pub total_ht: f64,
    /// Remise globale sur le total, en %.
    #[serde(default)]
    pub discount_pct: f64,
    /// Total après la remise globale (calculé à l'enregistrement).
    #[serde(default)]
    pub total_net: f64,
    /// Total des lignes en option (hors total HT).
    #[serde(default)]
    pub total_options: f64,
    pub lines: Vec<QuoteLine>,
}

/// Contact chez le client et commercial du devis (imprimés sur le PDF).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct QuoteContact {
    pub contact_name: String,
    pub contact_email: String,
    pub contact_phone: String,
    pub sales_rep: String,
}

#[tauri::command]
pub fn get_quote(state: State<AppState>, id: i64) -> CmdResult<Quote> {
    let conn = state.conn();
    load_quote(&conn, id)
}

/// En base, un client ponctuel a un code vide.
fn code_or_none(code: String) -> Option<String> {
    Some(code).filter(|c| !c.is_empty())
}

fn load_quote(conn: &Connection, id: i64) -> CmdResult<Quote> {
    let mut quote = conn
        .query_row(
            "SELECT id, number, client_code, client_name, date, notes, total_ht, discount_cfa, discount_cfo, price_lists,
                    discount_pct, forced_price_list, contact_name, contact_email, contact_phone, sales_rep
             FROM quotes WHERE id = ?1",
            [id],
            |r| {
                let lists: String = r.get(9)?;
                Ok(Quote {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    client_code: code_or_none(r.get(2)?),
                    client_name: r.get(3)?,
                    date: r.get(4)?,
                    notes: r.get(5)?,
                    total_ht: r.get(6)?,
                    discount_pct: r.get(10)?,
                    total_net: 0.0,
                    total_options: 0.0,
                    pricing: PricingContext {
                        discount_cfa: r.get(7)?,
                        discount_cfo: r.get(8)?,
                        price_lists: lists.split(',').filter(|l| !l.is_empty()).map(str::to_string).collect(),
                        forced_price_list: code_or_none(r.get(11)?),
                    },
                    contact: QuoteContact {
                        contact_name: r.get(12)?,
                        contact_email: r.get(13)?,
                        contact_phone: r.get(14)?,
                        sales_rep: r.get(15)?,
                    },
                    lines: Vec::new(),
                })
            },
        )
        .map_err(err)?;
    quote.lines = query_all(
        conn,
        "SELECT kind, product_ref, designation, quantity, unit_price, price_source, public_price, threshold_price,
                discount, is_option, enedis_code, discounted_price, lpn_price, lpn_list
         FROM quote_lines WHERE quote_id = ?1 ORDER BY position",
        [id],
        |r| {
            Ok(QuoteLine {
                kind: r.get(0)?,
                product_ref: r.get(1)?,
                designation: r.get(2)?,
                quantity: r.get(3)?,
                unit_price: r.get(4)?,
                price_source: r.get(5)?,
                public_price: r.get(6)?,
                threshold_price: r.get(7)?,
                discount: r.get(8)?,
                is_option: r.get(9)?,
                enedis_code: r.get(10)?,
                discounted_price: r.get(11)?,
                lpn_price: r.get(12)?,
                lpn_list: r.get(13)?,
            })
        },
    )?;
    let totals = quote_totals(&quote.lines, quote.discount_pct);
    quote.total_net = totals.total_net;
    quote.total_options = totals.options;
    Ok(quote)
}

fn setting(conn: &Connection, key: &str) -> CmdResult<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(err)
}

/// Numéro suivant au format `<année sur 2 chiffres>-<préfixe>-<n° sur 4 chiffres>` (ex. `26-JMOS-0001`).
/// Le compteur repart à 1 chaque année. Réglage `quote_last_number` : dernier numéro déjà utilisé
/// hors de l'appli (ex. `26-JMOS-0140`) ; la numérotation en prend la suite, pour cette année et ce
/// préfixe seulement.
fn next_number(conn: &Connection, date: &str) -> CmdResult<String> {
    let prefix = setting(conn, "quote_prefix")?
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "DEV".to_string());
    let year = date.get(2..4).unwrap_or("00");
    let stem = format!("{year}-{}-", prefix.trim());
    let last: Option<String> = conn
        .query_row(
            "SELECT number FROM quotes WHERE number LIKE ?1 || '%' ORDER BY number DESC LIMIT 1",
            [&stem],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    let counter = |number: &str| number.strip_prefix(stem.as_str()).and_then(|n| n.trim().parse::<u32>().ok());
    let in_app = last.as_deref().and_then(counter).unwrap_or(0);
    let outside = setting(conn, "quote_last_number")?.as_deref().map(str::trim).and_then(counter).unwrap_or(0);
    let n = in_app.max(outside) + 1;
    Ok(format!("{stem}{n:04}"))
}

/// Crée ou met à jour un devis et renvoie sa version enregistrée.
#[tauri::command]
pub fn save_quote(state: State<AppState>, mut quote: Quote) -> CmdResult<Quote> {
    let mut conn = state.conn();
    let tx = conn.transaction().map_err(err)?;
    // Tous les prix au centime.
    for l in &mut quote.lines {
        l.unit_price = round2(l.unit_price);
        l.public_price = l.public_price.map(round2);
        l.threshold_price = l.threshold_price.map(round2);
    }
    let total_of = |l: &QuoteLine| {
        if l.kind == "item" {
            line_total(l.quantity, l.unit_price, l.discount)
        } else if is_fee(&l.kind) {
            l.unit_price
        } else {
            0.0
        }
    };
    let totals = quote_totals(&quote.lines, quote.discount_pct);
    let (total, total_net) = (totals.total_ht, totals.total_net);
    let forced = quote.pricing.forced_price_list.clone().unwrap_or_default();
    let discount_pct = quote.discount_pct;

    let client_code = quote.client_code.clone().unwrap_or_default();
    let lists = quote.pricing.price_lists.join(",");
    let (cfa, cfo) = (quote.pricing.discount_cfa, quote.pricing.discount_cfo);
    let id = match quote.id {
        Some(id) => {
            tx.execute(
                "UPDATE quotes SET client_code = ?2, client_name = ?3, date = ?4, notes = ?5, total_ht = ?6,
                        discount_cfa = ?7, discount_cfo = ?8, price_lists = ?9, discount_pct = ?10,
                        forced_price_list = ?11, total_net = ?12, updated_at = datetime('now', 'localtime')
                 WHERE id = ?1",
                params![
                    id,
                    client_code,
                    quote.client_name,
                    quote.date,
                    quote.notes,
                    total,
                    cfa,
                    cfo,
                    lists,
                    discount_pct,
                    forced,
                    total_net
                ],
            )
            .map_err(err)?;
            tx.execute("DELETE FROM quote_lines WHERE quote_id = ?1", [id]).map_err(err)?;
            id
        }
        None => {
            let number = next_number(&tx, &quote.date)?;
            tx.execute(
                "INSERT INTO quotes
                 (number, client_code, client_name, date, notes, total_ht, discount_cfa, discount_cfo, price_lists,
                  discount_pct, forced_price_list, total_net)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    number,
                    client_code,
                    quote.client_name,
                    quote.date,
                    quote.notes,
                    total,
                    cfa,
                    cfo,
                    lists,
                    discount_pct,
                    forced,
                    total_net
                ],
            )
            .map_err(err)?;
            tx.last_insert_rowid()
        }
    };

    let c = &quote.contact;
    tx.execute(
        "UPDATE quotes SET contact_name = ?2, contact_email = ?3, contact_phone = ?4, sales_rep = ?5 WHERE id = ?1",
        params![id, c.contact_name.trim(), c.contact_email.trim(), c.contact_phone.trim(), c.sales_rep.trim()],
    )
    .map_err(err)?;

    for (i, l) in quote.lines.iter().enumerate() {
        tx.execute(
            "INSERT INTO quote_lines
             (quote_id, position, kind, product_ref, designation, quantity, unit_price, price_source, line_total,
              public_price, threshold_price, discount, is_option, enedis_code, discounted_price, lpn_price, lpn_list)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                id,
                i as i64,
                l.kind,
                l.product_ref,
                l.designation,
                l.quantity,
                l.unit_price,
                l.price_source,
                total_of(l),
                l.public_price,
                l.threshold_price,
                l.discount,
                l.is_option,
                l.enedis_code,
                l.discounted_price.map(round2),
                l.lpn_price.map(round2),
                l.lpn_list
            ],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    load_quote(&conn, id)
}

#[tauri::command]
pub fn delete_quote(state: State<AppState>, id: i64) -> CmdResult<()> {
    state.conn().execute("DELETE FROM quotes WHERE id = ?1", [id]).map(|_| ()).map_err(err)
}

/// Copie un devis (nouveau numéro, date du jour fournie par le front).
#[tauri::command]
pub fn duplicate_quote(state: State<AppState>, id: i64, date: String) -> CmdResult<Quote> {
    let mut quote = load_quote(&state.conn(), id)?;
    quote.id = None;
    quote.number = None;
    quote.date = date;
    let attachments = load_attachments(&state.conn(), id)?;
    let copy = save_quote(state.clone(), quote)?;
    // La copie reprend les documents joints.
    if let Some(new_id) = copy.id {
        store_attachments(&mut state.conn(), new_id, &attachments)?;
    }
    Ok(copy)
}

// ---------- Documents joints (PDF « Devis + Docs ») ----------

#[derive(Serialize, Deserialize, Clone)]
pub struct QuoteAttachment {
    /// `index` (documentation) ou `external` (fichier ajouté à la main).
    pub source: String,
    pub product_ref: String,
    pub path: String,
    /// Page de la documentation ; None : fichier entier.
    pub page_num: Option<i64>,
    pub included: bool,
}

fn load_attachments(conn: &Connection, quote_id: i64) -> CmdResult<Vec<QuoteAttachment>> {
    query_all(
        conn,
        "SELECT source, product_ref, path, page_num, included FROM quote_attachments
         WHERE quote_id = ?1 ORDER BY position",
        [quote_id],
        |r| {
            Ok(QuoteAttachment {
                source: r.get(0)?,
                product_ref: r.get(1)?,
                path: r.get(2)?,
                page_num: r.get(3)?,
                included: r.get(4)?,
            })
        },
    )
}

fn store_attachments(conn: &mut Connection, quote_id: i64, items: &[QuoteAttachment]) -> CmdResult<()> {
    let tx = conn.transaction().map_err(err)?;
    tx.execute("DELETE FROM quote_attachments WHERE quote_id = ?1", [quote_id]).map_err(err)?;
    for (i, a) in items.iter().enumerate() {
        tx.execute(
            "INSERT INTO quote_attachments (quote_id, position, source, product_ref, path, page_num, included)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![quote_id, i as i64, a.source, a.product_ref, a.path, a.page_num, a.included],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

#[tauri::command]
pub fn get_quote_attachments(state: State<AppState>, quote_id: i64) -> CmdResult<Vec<QuoteAttachment>> {
    load_attachments(&state.conn(), quote_id)
}

/// Enregistre les choix de documents d'un devis (remplace les précédents).
#[tauri::command]
pub fn save_quote_attachments(state: State<AppState>, quote_id: i64, attachments: Vec<QuoteAttachment>) -> CmdResult<()> {
    store_attachments(&mut state.conn(), quote_id, &attachments)
}

// ---------- Réglages / import ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> CmdResult<HashMap<String, String>> {
    let conn = state.conn();
    let rows = query_all(&conn, "SELECT key, COALESCE(value, '') FROM settings", [], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    Ok(rows.into_iter().collect())
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: HashMap<String, String>) -> CmdResult<()> {
    let mut conn = state.conn();
    let tx = conn.transaction().map_err(err)?;
    for (k, v) in settings {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![k, v],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

#[derive(Serialize)]
pub struct DbStats {
    pub path: String,
    pub products: i64,
    pub clients: i64,
    pub price_lists: i64,
    pub price_list_items: i64,
    pub quotes: i64,
}

#[tauri::command]
pub fn db_stats(state: State<AppState>) -> CmdResult<DbStats> {
    let conn = state.conn();
    let count = |table: &str| -> CmdResult<i64> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).map_err(err)
    };
    Ok(DbStats {
        path: state.db_path.display().to_string(),
        products: count("products")?,
        clients: count("clients")?,
        price_lists: count("price_lists")?,
        price_list_items: count("price_list_items")?,
        quotes: count("quotes")?,
    })
}

/// L'import lit un gros classeur : on le fait hors du thread principal pour ne pas figer l'interface.
#[tauri::command]
pub async fn import_lpn(state: State<'_, AppState>, path: String) -> CmdResult<ImportReport> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = db.lock().map_err(err)?;
        import::import_lpn(&mut conn, &PathBuf::from(path))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn save_file(path: String, contents: Vec<u8>) -> CmdResult<()> {
    std::fs::write(path, contents).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: &str, quantity: f64, unit_price: f64, is_option: bool) -> QuoteLine {
        QuoteLine {
            kind: kind.into(),
            product_ref: String::new(),
            enedis_code: None,
            designation: String::new(),
            quantity,
            unit_price,
            discount: 0.0,
            is_option,
            price_source: None,
            public_price: None,
            threshold_price: None,
            discounted_price: None,
            lpn_price: None,
            lpn_list: None,
        }
    }

    #[test]
    fn quote_numbers_are_year_prefix_counter() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('quote_prefix', 'JMOS')", []).unwrap();
        assert_eq!(next_number(&conn, "2026-10-09").unwrap(), "26-JMOS-0001");
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-JMOS-0007', '', '2026-10-09')", []).unwrap();
        assert_eq!(next_number(&conn, "2026-12-31").unwrap(), "26-JMOS-0008");
        // Nouvelle année : le compteur repart à 1.
        assert_eq!(next_number(&conn, "2027-01-02").unwrap(), "27-JMOS-0001");
    }

    #[test]
    fn client_price_list_attach_and_detach() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO clients (code, name, discount_cfa, discount_cfo) VALUES ('C1', 'Client 1', 0, 0);
             INSERT INTO price_lists (code, label) VALUES ('TL1', 'Liste 1');",
        )
        .unwrap();
        let count = || -> i64 {
            conn.query_row("SELECT COUNT(*) FROM client_price_lists WHERE client_code = 'C1' AND price_list_code = 'TL1'", [], |r| r.get(0))
                .unwrap()
        };
        link_client_price_list(&conn, "C1", "TL1", true).unwrap();
        link_client_price_list(&conn, "C1", "TL1", true).unwrap(); // deux fois : pas de doublon
        assert_eq!(count(), 1);
        link_client_price_list(&conn, "C1", "TL1", false).unwrap();
        assert_eq!(count(), 0);
    }

    #[test]
    fn quote_numbers_continue_after_last_number_used_outside() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('quote_prefix', 'JMOS')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('quote_last_number', ' 26-JMOS-0140 ')", []).unwrap();
        assert_eq!(next_number(&conn, "2026-10-09").unwrap(), "26-JMOS-0141");
        // Les devis de l'appli au-delà priment.
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-JMOS-0150', '', '2026-10-09')", []).unwrap();
        assert_eq!(next_number(&conn, "2026-10-10").unwrap(), "26-JMOS-0151");
        // Autre année ou autre préfixe : le réglage ne s'applique pas.
        assert_eq!(next_number(&conn, "2027-01-02").unwrap(), "27-JMOS-0001");
        conn.execute("UPDATE settings SET value = 'ABC' WHERE key = 'quote_prefix'", []).unwrap();
        assert_eq!(next_number(&conn, "2026-10-10").unwrap(), "26-ABC-0001");
    }

    #[test]
    fn attachments_round_trip() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute("INSERT INTO quotes (id, number, client_code, date) VALUES (1, 'D-1', '', '2026-10-09')", []).unwrap();
        let a = |path: &str, page: Option<i64>, included: bool| QuoteAttachment {
            source: "index".into(),
            product_ref: "00188".into(),
            path: path.into(),
            page_num: page,
            included,
        };
        store_attachments(&mut conn, 1, &[a("/b.pdf", Some(3), true), a("/a.pdf", None, false)]).unwrap();
        let back = load_attachments(&conn, 1).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!((back[0].path.as_str(), back[0].page_num, back[0].included), ("/b.pdf", Some(3), true));
        assert_eq!((back[1].path.as_str(), back[1].page_num, back[1].included), ("/a.pdf", None, false));
        // Supprimer le devis supprime ses documents.
        conn.execute("DELETE FROM quotes WHERE id = 1", []).unwrap();
        assert!(load_attachments(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn fees_are_in_total_but_not_discounted() {
        let lines = [
            line("item", 2.0, 50.0, false),     // 100 €
            line("item", 1.0, 30.0, true),      // option : hors total
            line("text", 0.0, 0.0, false),
            line("shipping", 1.0, 25.0, false), // frais de port
            line("billing", 1.0, 5.0, false),   // frais de facturation
        ];
        let t = quote_totals(&lines, 10.0);
        assert_eq!(t.total_ht, 130.0);
        // remise de 10 % sur les 100 € de produits seulement
        assert_eq!(t.total_net, 120.0);
        assert_eq!(t.options, 30.0);
    }
}
