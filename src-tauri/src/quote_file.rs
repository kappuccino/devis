//! Fichier « .devis » : un ou plusieurs devis (ou devis types) en JSON, pour les échanger entre
//! postes ou les remettre dans une base. L'export écrit un devis par fichier ; l'import ajoute les
//! devis du fichier, et écrase un devis de même numéro.
use crate::commands::{as_template, load_quote, store_quote_numbered, Quote};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type Result<T> = std::result::Result<T, String>;

const FORMAT: &str = "devis-groupe-cahors";
const VERSION: u32 = 1;
/// Garde-fou : nombre de devis lus dans un fichier.
const MAX_QUOTES: usize = 1000;
const KINDS: [&str; 6] = ["item", "title", "text", "subtotal", "shipping", "billing"];

#[derive(Serialize, Deserialize)]
struct QuoteFile {
    format: String,
    version: u32,
    #[serde(default)]
    app_version: String,
    quotes: Vec<Value>,
}

#[derive(Serialize)]
pub struct ExportedQuote {
    /// Nom proposé : comme le PDF (« numéro - client »), ou le nom du devis type.
    pub file_name: String,
    pub contents: String,
}

#[derive(Serialize, Default, Debug)]
pub struct ImportedQuote {
    pub number: String,
    pub name: String,
    pub is_template: bool,
    /// Un devis de même numéro existait : il a été (ou sera) écrasé.
    pub replaced: bool,
}

fn safe_name(name: &str) -> String {
    name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c }).collect::<String>().trim().to_string()
}

/// Le devis `id` au format .devis (sans ce qui ne concerne que ce poste : id, versions, suivi).
pub fn export(conn: &Connection, id: i64) -> Result<ExportedQuote> {
    let quote = load_quote(conn, id)?;
    let file_name = if quote.is_template {
        format!("{}.devis", safe_name(&quote.template_name).if_empty("Devis type"))
    } else {
        let client = if quote.client_name.is_empty() { quote.client_code.clone().unwrap_or_default() } else { quote.client_name.clone() };
        format!("{} - {}.devis", safe_name(quote.number.as_deref().unwrap_or("Devis")), safe_name(&client).if_empty("client"))
    };
    let mut value = serde_json::to_value(&quote).map_err(|e| e.to_string())?;
    if let Value::Object(map) = &mut value {
        for key in ["id", "versions", "version", "version_of", "won_at"] {
            map.remove(key);
        }
    }
    let file = QuoteFile { format: FORMAT.into(), version: VERSION, app_version: env!("CARGO_PKG_VERSION").into(), quotes: vec![value] };
    let contents = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    Ok(ExportedQuote { file_name, contents })
}

trait IfEmpty {
    fn if_empty(self, default: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, default: &str) -> String {
        if self.is_empty() {
            default.to_string()
        } else {
            self
        }
    }
}

/// Lit un fichier .devis (sans rien enregistrer).
fn read(bytes: &[u8]) -> Result<Vec<Quote>> {
    let file: QuoteFile =
        serde_json::from_slice(bytes).map_err(|_| "Ce fichier n'est pas un fichier .devis lisible.".to_string())?;
    if file.format != FORMAT {
        return Err("Ce fichier n'est pas un fichier .devis de l'appli.".into());
    }
    if file.version > VERSION {
        return Err(format!(
            "Fichier créé par une version plus récente de l'appli ({}) : mettez l'appli à jour.",
            file.app_version
        ));
    }
    if file.quotes.len() > MAX_QUOTES {
        return Err(format!("Trop de devis dans le fichier (au plus {MAX_QUOTES})."));
    }
    file.quotes
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let quote: Quote = serde_json::from_value(v).map_err(|e| format!("Devis n° {} du fichier illisible : {e}", i + 1))?;
            if let Some(l) = quote.lines.iter().find(|l| !KINDS.contains(&l.kind.as_str())) {
                return Err(format!("Devis n° {} du fichier : type de ligne inconnu « {} ».", i + 1, l.kind));
            }
            Ok(quote)
        })
        .collect()
}

/// Devis d'un fichier .devis prêts à enregistrer, avec l'id du devis de même numéro déjà en base
/// (qu'ils écraseront). `as_templates` : import depuis l'onglet « Devis types », tout devis y
/// devient un devis type (sans client ni prix) ; les devis types sont toujours ajoutés.
fn prepare(conn: &Connection, bytes: &[u8], as_templates: bool, today: &str) -> Result<Vec<(Quote, Option<i64>)>> {
    read(bytes)?
        .into_iter()
        .map(|mut quote| {
            quote.id = None;
            quote.version = 1;
            quote.version_of = None;
            if as_templates && !quote.is_template {
                let name = [quote.project_name.as_str(), quote.client_name.as_str(), quote.number.as_deref().unwrap_or("")]
                    .into_iter()
                    .find(|s| !s.trim().is_empty())
                    .map_or_else(|| "Devis importé".to_string(), str::to_string);
                quote = as_template(quote, name, today.to_string());
            }
            quote.number = quote.number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty() && !quote.is_template);
            let existing = match &quote.number {
                Some(n) => conn
                    .query_row("SELECT id FROM quotes WHERE number = ?1 AND is_template = 0", [n], |r| r.get(0))
                    .optional()
                    .map_err(|e| e.to_string())?,
                None => None,
            };
            Ok((quote, existing))
        })
        .collect()
}

fn summary(quote: &Quote, replaced: bool) -> ImportedQuote {
    ImportedQuote {
        number: quote.number.clone().unwrap_or_default(),
        name: if quote.is_template { quote.template_name.clone() } else { quote.client_name.clone() },
        is_template: quote.is_template,
        replaced,
    }
}

/// Ce que l'import ajouterait ou écraserait, sans rien enregistrer (confirmation avant l'import).
pub fn preview(conn: &Connection, bytes: &[u8], as_templates: bool, today: &str) -> Result<Vec<ImportedQuote>> {
    Ok(prepare(conn, bytes, as_templates, today)?.iter().map(|(q, existing)| summary(q, existing.is_some())).collect())
}

/// Enregistre les devis d'un fichier .devis : un devis garde son numéro ; si ce numéro existe déjà,
/// le devis en place est écrasé par celui du fichier (son suivi, « obtenue » et commentaire, reste).
pub fn import(conn: &mut Connection, bytes: &[u8], as_templates: bool, today: &str) -> Result<Vec<ImportedQuote>> {
    let mut imported = Vec::new();
    for (mut quote, existing) in prepare(conn, bytes, as_templates, today)? {
        quote.id = existing;
        let number = quote.number.clone();
        let saved = store_quote_numbered(conn, quote, number)?;
        imported.push(summary(&saved, existing.is_some()));
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote_json(number: &str, client: &str) -> Value {
        serde_json::json!({
            "number": number, "client_code": "490200", "client_name": client,
            "price_lists": [], "discount_cfa": 0.0, "discount_cfo": 0.0,
            "contact_name": "Jean", "sales_rep": "Jacques",
            "date": "2026-10-13", "notes": "Dispo usine", "total_ht": 0.0, "discount_pct": 5.0,
            "project_name": "Chantier",
            "lines": [
                { "kind": "title", "product_ref": "", "designation": "Bâtiment 1", "quantity": 0.0, "unit_price": 0.0, "price_source": null },
                { "kind": "item", "product_ref": "0144204", "designation": "VDE", "quantity": 2.0, "unit_price": 6.02, "public_price": 15.04, "price_source": null },
                { "kind": "shipping", "product_ref": "", "designation": "Frais de port", "quantity": 1.0, "unit_price": 31.0, "price_source": null }
            ]
        })
    }

    fn file(quotes: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "version": 1, "quotes": quotes })).unwrap()
    }

    #[test]
    fn import_keeps_numbers_and_overwrites_same_number() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('quote_prefix', 'JMOS')", []).unwrap();
        let bytes = file(vec![quote_json("26-JMOS-1758", "EIFFAGE"), quote_json("26-JMOS-1755-B", "CASSAGNE")]);
        let done = import(&mut conn, &bytes, false, "2026-10-13").unwrap();
        assert_eq!(done.iter().map(|q| q.number.as_str()).collect::<Vec<_>>(), ["26-JMOS-1758", "26-JMOS-1755-B"]);

        let q = load_quote(&conn, 1).unwrap();
        // 2 × 6,02 = 12,04 ; remise 5 % sur les produits ; frais de port non remisés.
        assert_eq!((q.total_ht, q.total_net), (43.04, 42.44));
        assert_eq!((q.lines.len(), q.notes.as_deref(), q.project_name.as_str()), (3, Some("Dispo usine"), "Chantier"));

        conn.execute("UPDATE quotes SET comment = 'relancé', won_at = '2026-10-14' WHERE id = 1", []).unwrap();

        // Réimport d'une autre version du 1758 : annoncé, puis écrase le devis en place.
        let mut changed = quote_json("26-JMOS-1758", "EIFFAGE ÉNERGIE");
        changed["lines"].as_array_mut().unwrap().remove(0);
        let bytes = file(vec![changed, quote_json("26-JMOS-1760", "REXEL")]);
        let seen = preview(&conn, &bytes, false, "2026-10-13").unwrap();
        assert_eq!(seen.iter().map(|q| q.replaced).collect::<Vec<_>>(), [true, false]);
        let count = |c: &Connection| c.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(count(&conn), 2, "l'aperçu n'enregistre rien");

        let again = import(&mut conn, &bytes, false, "2026-10-13").unwrap();
        assert_eq!((again[0].number.as_str(), again[0].replaced), ("26-JMOS-1758", true));
        assert_eq!(count(&conn), 3);
        let q = load_quote(&conn, 1).unwrap();
        assert_eq!((q.client_name.as_str(), q.lines.len()), ("EIFFAGE ÉNERGIE", 2));
        let (comment, won): (String, Option<String>) =
            conn.query_row("SELECT comment, won_at FROM quotes WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((comment.as_str(), won.as_deref()), ("relancé", Some("2026-10-14")), "suivi conservé");
    }

    #[test]
    fn export_then_import_round_trip_and_as_template() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        import(&mut conn, &file(vec![quote_json("26-DEV-0007", "SONEPAR 40")]), false, "2026-10-13").unwrap();
        let exported = export(&conn, 1).unwrap();
        assert_eq!(exported.file_name, "26-DEV-0007 - SONEPAR 40.devis");

        // Depuis l'onglet « Devis types » : devis type sans client ni prix ni frais.
        let done = import(&mut conn, exported.contents.as_bytes(), true, "2026-10-13").unwrap();
        assert!(done[0].is_template);
        let t = load_quote(&conn, 2).unwrap();
        assert!(t.is_template && t.client_name.is_empty());
        assert_eq!(t.template_name, "Chantier");
        assert_eq!(t.lines.len(), 2, "frais retirés");
        assert!(t.lines.iter().all(|l| l.unit_price == 0.0));
        assert_eq!(export(&conn, 2).unwrap().file_name, "Chantier.devis");
    }

    #[test]
    fn refuses_foreign_or_newer_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        assert!(import(&mut conn, b"{}", false, "2026-10-13").is_err());
        let newer = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "version": 99, "quotes": [] })).unwrap();
        assert!(import(&mut conn, &newer, false, "2026-10-13").unwrap_err().contains("plus récente"));
        let mut bad = quote_json("26-X-0001", "A");
        bad["lines"][0]["kind"] = "section".into();
        assert!(import(&mut conn, &file(vec![bad]), false, "2026-10-13").unwrap_err().contains("inconnu"));
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }
}
