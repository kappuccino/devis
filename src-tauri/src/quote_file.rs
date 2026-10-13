//! Fichiers d'échange entre postes, en JSON : « .devis » pour les devis, « .devistype » pour les
//! devis types. L'export écrit un devis par fichier. L'import d'un fichier .devis (liste des devis)
//! ajoute ses devis et écrase un devis de même numéro ; celui d'un .devistype (onglet « Devis
//! types ») ajoute ses devis types. Chaque sorte de fichier n'est acceptée qu'à sa place.
use crate::commands::{load_quote, store_quote_numbered, Quote};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type Result<T> = std::result::Result<T, String>;

const FORMAT: &str = "devis-groupe-cahors";
const VERSION: u32 = 1;
/// Sorte de fichier (et extension) : devis ou devis types.
const KIND_QUOTES: &str = "devis";
const KIND_TEMPLATES: &str = "devistype";
/// Garde-fou : nombre de devis lus dans un fichier.
const MAX_QUOTES: usize = 1000;
const KINDS: [&str; 6] = ["item", "title", "text", "subtotal", "shipping", "billing"];

#[derive(Serialize, Deserialize)]
struct QuoteFile {
    format: String,
    version: u32,
    /// « devis » ou « devistype » ; absent dans les fichiers de la 0.2.3 et de la 0.2.4 (des devis).
    #[serde(default)]
    kind: String,
    #[serde(default)]
    app_version: String,
    quotes: Vec<Value>,
}

#[derive(Serialize)]
pub struct ExportedQuote {
    /// Nom proposé : comme le PDF (« numéro - client.devis »), ou « nom du type.devistype ».
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

fn or_default(s: String, default: &str) -> String {
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

/// Le devis `id` en fichier .devis, ou .devistype pour un devis type (sans ce qui ne concerne que
/// ce poste : id, versions, suivi).
pub fn export(conn: &Connection, id: i64) -> Result<ExportedQuote> {
    let quote = load_quote(conn, id)?;
    let (kind, file_name) = if quote.is_template {
        (KIND_TEMPLATES, format!("{}.{KIND_TEMPLATES}", or_default(safe_name(&quote.template_name), "Devis type")))
    } else {
        let client = if quote.client_name.is_empty() { quote.client_code.clone().unwrap_or_default() } else { quote.client_name.clone() };
        let number = safe_name(quote.number.as_deref().unwrap_or("Devis"));
        (KIND_QUOTES, format!("{number} - {}.{KIND_QUOTES}", or_default(safe_name(&client), "client")))
    };
    let mut value = serde_json::to_value(&quote).map_err(|e| e.to_string())?;
    if let Value::Object(map) = &mut value {
        for key in ["id", "versions", "version", "version_of", "won_at"] {
            map.remove(key);
        }
    }
    let file = QuoteFile {
        format: FORMAT.into(),
        version: VERSION,
        kind: kind.into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        quotes: vec![value],
    };
    let contents = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    Ok(ExportedQuote { file_name, contents })
}

/// Lit un fichier .devis (`templates` faux) ou .devistype (`templates` vrai), sans rien enregistrer ;
/// un fichier de l'autre sorte est refusé.
fn read(bytes: &[u8], templates: bool) -> Result<Vec<Quote>> {
    let expected = if templates { KIND_TEMPLATES } else { KIND_QUOTES };
    let file: QuoteFile = serde_json::from_slice(bytes)
        .map_err(|_| format!("Ce fichier n'est pas un fichier .{expected} lisible."))?;
    if file.format != FORMAT {
        return Err(format!("Ce fichier n'est pas un fichier .{expected} de l'appli."));
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
    let wrong_place = || {
        Err(if templates {
            "Ce fichier contient des devis, pas des devis types : importez-le depuis la liste des devis.".to_string()
        } else {
            "Ce fichier contient des devis types : importez-le depuis l'onglet « Devis types ».".to_string()
        })
    };
    let kind = if file.kind.is_empty() { KIND_QUOTES } else { file.kind.as_str() };
    if kind != expected {
        return wrong_place();
    }
    let quotes = file
        .quotes
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let quote: Quote = serde_json::from_value(v).map_err(|e| format!("Devis n° {} du fichier illisible : {e}", i + 1))?;
            if let Some(l) = quote.lines.iter().find(|l| !KINDS.contains(&l.kind.as_str())) {
                return Err(format!("Devis n° {} du fichier : type de ligne inconnu « {} ».", i + 1, l.kind));
            }
            Ok(quote)
        })
        .collect::<Result<Vec<_>>>()?;
    if quotes.iter().any(|q| q.is_template != templates) {
        return wrong_place();
    }
    Ok(quotes)
}

/// Devis d'un fichier prêts à enregistrer, avec l'id du devis de même numéro déjà en base (qu'ils
/// écraseront). Les devis types n'ont pas de numéro : ils sont toujours ajoutés.
fn prepare(conn: &Connection, bytes: &[u8], templates: bool) -> Result<Vec<(Quote, Option<i64>)>> {
    read(bytes, templates)?
        .into_iter()
        .map(|mut quote| {
            quote.id = None;
            quote.version = 1;
            quote.version_of = None;
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
pub fn preview(conn: &Connection, bytes: &[u8], templates: bool) -> Result<Vec<ImportedQuote>> {
    Ok(prepare(conn, bytes, templates)?.iter().map(|(q, existing)| summary(q, existing.is_some())).collect())
}

/// Enregistre les devis d'un fichier : un devis garde son numéro ; si ce numéro existe déjà, le
/// devis en place est écrasé par celui du fichier (son suivi, « obtenue » et commentaire, reste).
pub fn import(conn: &mut Connection, bytes: &[u8], templates: bool) -> Result<Vec<ImportedQuote>> {
    let mut imported = Vec::new();
    for (mut quote, existing) in prepare(conn, bytes, templates)? {
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

    fn template_json(name: &str) -> Value {
        serde_json::json!({
            "number": "TYPE-3", "client_code": null, "client_name": "", "price_lists": [], "discount_cfa": 0.0,
            "discount_cfo": 0.0, "date": "2026-10-13", "notes": null, "total_ht": 0.0, "is_template": true,
            "template_name": name,
            "lines": [{ "kind": "item", "product_ref": "0144204", "designation": "VDE", "quantity": 8.0, "unit_price": 0.0, "price_source": null }]
        })
    }

    /// Fichier sans « kind », comme ceux de la 0.2.3 et de la 0.2.4.
    fn file(quotes: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "version": 1, "quotes": quotes })).unwrap()
    }

    fn templates_file(quotes: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "version": 1, "kind": "devistype", "quotes": quotes })).unwrap()
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn import_keeps_numbers_and_overwrites_same_number() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('quote_prefix', 'JMOS')", []).unwrap();
        let bytes = file(vec![quote_json("26-JMOS-1758", "EIFFAGE"), quote_json("26-JMOS-1755-B", "CASSAGNE")]);
        let done = import(&mut conn, &bytes, false).unwrap();
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
        let seen = preview(&conn, &bytes, false).unwrap();
        assert_eq!(seen.iter().map(|q| q.replaced).collect::<Vec<_>>(), [true, false]);
        assert_eq!(count(&conn), 2, "l'aperçu n'enregistre rien");

        let again = import(&mut conn, &bytes, false).unwrap();
        assert_eq!((again[0].number.as_str(), again[0].replaced), ("26-JMOS-1758", true));
        assert_eq!(count(&conn), 3);
        let q = load_quote(&conn, 1).unwrap();
        assert_eq!((q.client_name.as_str(), q.lines.len()), ("EIFFAGE ÉNERGIE", 2));
        let (comment, won): (String, Option<String>) =
            conn.query_row("SELECT comment, won_at FROM quotes WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((comment.as_str(), won.as_deref()), ("relancé", Some("2026-10-14")), "suivi conservé");
    }

    #[test]
    fn quotes_and_templates_round_trip_in_their_own_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        import(&mut conn, &file(vec![quote_json("26-DEV-0007", "SONEPAR 40")]), false).unwrap();
        import(&mut conn, &templates_file(vec![template_json("Lotissement 24 lots")]), true).unwrap();

        let quote = export(&conn, 1).unwrap();
        assert_eq!(quote.file_name, "26-DEV-0007 - SONEPAR 40.devis");
        let template = export(&conn, 2).unwrap();
        assert_eq!(template.file_name, "Lotissement 24 lots.devistype");

        // Chaque fichier se réimporte à sa place : le devis écrase le sien, le devis type s'ajoute.
        assert!(import(&mut conn, quote.contents.as_bytes(), false).unwrap()[0].replaced);
        let added = import(&mut conn, template.contents.as_bytes(), true).unwrap();
        assert!(added[0].is_template && added[0].number.starts_with("TYPE-"));
        let t = load_quote(&conn, 3).unwrap();
        assert_eq!((t.template_name.as_str(), t.lines.len(), t.is_template), ("Lotissement 24 lots", 1, true));
        assert_eq!(count(&conn), 3);
    }

    #[test]
    fn each_kind_of_file_is_refused_in_the_other_place() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let quotes = file(vec![quote_json("26-X-0001", "A")]);
        let templates = templates_file(vec![template_json("Type")]);
        assert!(import(&mut conn, &quotes, true).unwrap_err().contains("liste des devis"));
        assert!(import(&mut conn, &templates, false).unwrap_err().contains("Devis types"));
        // Fichier « devis » qui cache un devis type, ou l'inverse : refusé aussi.
        assert!(import(&mut conn, &file(vec![template_json("Type")]), false).is_err());
        assert!(import(&mut conn, &templates_file(vec![quote_json("26-X-0002", "B")]), true).is_err());
        assert_eq!(count(&conn), 0);
    }

    #[test]
    fn refuses_foreign_or_newer_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        assert!(import(&mut conn, b"{}", false).is_err());
        let newer = serde_json::to_vec(&serde_json::json!({ "format": FORMAT, "version": 99, "quotes": [] })).unwrap();
        assert!(import(&mut conn, &newer, false).unwrap_err().contains("plus récente"));
        let mut bad = quote_json("26-X-0001", "A");
        bad["lines"][0]["kind"] = "section".into();
        assert!(import(&mut conn, &file(vec![bad]), false).unwrap_err().contains("inconnu"));
        assert_eq!(count(&conn), 0);
    }
}
