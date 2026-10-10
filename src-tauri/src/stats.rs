//! Statistiques des devis (écran Statistiques) : chiffres clés, devis par mois, meilleurs
//! clients, commerciaux et produits, sur une année ou sur toute la base.
//!
//! Montants : total HT après remise globale, frais compris (`quotes.total_net`) ; pour les
//! produits, total des lignes (remise de ligne comprise, avant remise globale). Les lignes en
//! option ne sont pas comptées. Ce sont des montants devisés ; les affaires obtenues (coche de la
//! liste des devis) sont comptées à part (`won_count`, `won_total`).
//!
//! Versions : un devis compte une fois, par sa version obtenue s'il y en a une, sinon par sa
//! dernière version (voir `QUOTES`).

use rusqlite::{params, Connection};
use serde::Serialize;

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Month {
    /// `AAAA-MM`
    pub month: String,
    pub count: i64,
    pub total: f64,
    pub won_count: i64,
    pub won_total: f64,
}

/// Regroupement (client ou commercial) : nombre de devis et montant.
#[derive(Serialize, Debug, PartialEq)]
pub struct Group {
    pub label: String,
    /// Code client (vide pour un client ponctuel ou un commercial).
    pub code: String,
    pub count: i64,
    pub total: f64,
    pub won_count: i64,
    pub won_total: f64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ProductStat {
    pub product_ref: String,
    pub designation: String,
    pub quantity: f64,
    pub total: f64,
    /// Nombre de devis où le produit apparaît.
    pub quotes: i64,
}

#[derive(Serialize, Debug)]
pub struct Stats {
    /// Années présentes dans la base (les plus récentes d'abord), pour le filtre.
    pub years: Vec<String>,
    pub count: i64,
    pub total: f64,
    pub clients: i64,
    /// Affaires obtenues : nombre et montant.
    pub won_count: i64,
    pub won_total: f64,
    pub by_month: Vec<Month>,
    pub top_clients: Vec<Group>,
    pub by_sales_rep: Vec<Group>,
    pub top_products: Vec<ProductStat>,
}

/// Les devis comptés : une version par devis (l'obtenue, sinon la plus récente).
const QUOTES: &str = "(SELECT * FROM quotes q0 WHERE q0.id = (
        SELECT q1.id FROM quotes q1 WHERE COALESCE(q1.version_of, q1.id) = COALESCE(q0.version_of, q0.id)
        ORDER BY q1.won_at IS NULL, q1.version DESC LIMIT 1))";

/// Nombre de lignes des classements.
const TOP: i64 = 10;

/// Statistiques de l'année `year` (`"2026"`), ou de toute la base si `None`.
pub fn quote_stats(conn: &Connection, year: Option<&str>) -> Result<Stats> {
    // Filtre commun : `?1` NULL = toutes les années.
    const IN_YEAR: &str = "(?1 IS NULL OR substr(q.date, 1, 4) = ?1)";
    // Client ponctuel (sans code) : regroupé par son nom.
    const CLIENT_KEY: &str = "CASE WHEN q.client_code <> '' THEN q.client_code ELSE 'nom:' || q.client_name END";

    // Nombre et montant des affaires obtenues (à la suite d'un COUNT et d'une SUM).
    const WON: &str = "COUNT(q.won_at), COALESCE(SUM(CASE WHEN q.won_at IS NOT NULL THEN q.total_net END), 0)";

    let years = all(conn, "SELECT DISTINCT substr(date, 1, 4) FROM quotes ORDER BY 1 DESC", params![], |r| r.get(0))?;

    let (count, total, clients, won_count, won_total) = conn
        .query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(q.total_net), 0), COUNT(DISTINCT {CLIENT_KEY}), {WON}
                 FROM {QUOTES} q WHERE {IN_YEAR}"
            ),
            [year],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .map_err(err)?;

    let by_month = all(
        conn,
        &format!(
            "SELECT substr(q.date, 1, 7), COUNT(*), COALESCE(SUM(q.total_net), 0), {WON} FROM {QUOTES} q
             WHERE {IN_YEAR} GROUP BY 1 ORDER BY 1"
        ),
        params![year],
        |r| {
            Ok(Month { month: r.get(0)?, count: r.get(1)?, total: r.get(2)?, won_count: r.get(3)?, won_total: r.get(4)? })
        },
    )?;

    let top_clients = all(
        conn,
        &format!(
            "SELECT MAX(q.client_name), q.client_code, COUNT(*), COALESCE(SUM(q.total_net), 0), {WON} FROM {QUOTES} q
             WHERE {IN_YEAR} GROUP BY {CLIENT_KEY} ORDER BY 4 DESC, 3 DESC LIMIT ?2"
        ),
        params![year, TOP],
        |r| {
            Ok(Group {
                label: r.get(0)?,
                code: r.get(1)?,
                count: r.get(2)?,
                total: r.get(3)?,
                won_count: r.get(4)?,
                won_total: r.get(5)?,
            })
        },
    )?;

    let by_sales_rep = all(
        conn,
        &format!(
            "SELECT trim(q.sales_rep), COUNT(*), COALESCE(SUM(q.total_net), 0), {WON} FROM {QUOTES} q
             WHERE {IN_YEAR} GROUP BY trim(q.sales_rep) ORDER BY 3 DESC"
        ),
        params![year],
        |r| {
            Ok(Group {
                label: r.get(0)?,
                code: String::new(),
                count: r.get(1)?,
                total: r.get(2)?,
                won_count: r.get(3)?,
                won_total: r.get(4)?,
            })
        },
    )?;

    let top_products = all(
        conn,
        &format!(
            "SELECT l.product_ref, MAX(l.designation), SUM(l.quantity), SUM(l.line_total), COUNT(DISTINCT l.quote_id)
             FROM quote_lines l JOIN {QUOTES} q ON q.id = l.quote_id
             WHERE {IN_YEAR} AND l.kind = 'item' AND l.is_option = 0 AND l.product_ref <> ''
             GROUP BY l.product_ref ORDER BY 4 DESC LIMIT ?2"
        ),
        params![year, TOP],
        |r| {
            Ok(ProductStat {
                product_ref: r.get(0)?,
                designation: r.get(1)?,
                quantity: r.get(2)?,
                total: r.get(3)?,
                quotes: r.get(4)?,
            })
        },
    )?;

    Ok(Stats { years, count, total, clients, won_count, won_total, by_month, top_clients, by_sales_rep, top_products })
}

// ---------- Évolution du chiffrage d'une référence ----------

/// Produit du catalogue (en-tête de l'écran).
#[derive(Serialize, Debug)]
pub struct ProductInfo {
    pub product_ref: String,
    pub designation: String,
    pub enedis_code: Option<String>,
    pub public_price: f64,
    pub threshold_price: Option<f64>,
    pub family: Option<String>,
}

/// La référence dans un devis.
#[derive(Serialize, Debug, PartialEq)]
pub struct HistoryLine {
    pub quote_id: i64,
    pub number: String,
    pub date: String,
    pub client_code: String,
    pub client_name: String,
    pub sales_rep: String,
    pub quantity: f64,
    /// Prix unitaire HT saisi sur la ligne.
    pub unit_price: f64,
    /// Remise supplémentaire de la ligne et remise globale du devis (en %).
    pub discount: f64,
    pub quote_discount: f64,
    /// Prix unitaire réellement devisé : remise de ligne, puis remise globale (sauf option).
    pub net_unit_price: f64,
    /// Prix public et prix de liste au moment du devis.
    pub public_price: Option<f64>,
    pub lpn_price: Option<f64>,
    pub is_option: bool,
}

#[derive(Serialize, Debug)]
pub struct ProductHistory {
    /// None : référence absente du catalogue actuel (encore présente dans d'anciens devis).
    pub product: Option<ProductInfo>,
    /// Du plus ancien au plus récent.
    pub lines: Vec<HistoryLine>,
}

/// Toutes les lignes de devis de la référence `product_ref`.
pub fn product_history(conn: &Connection, product_ref: &str) -> Result<ProductHistory> {
    let product = conn
        .query_row(
            "SELECT ref, designation, enedis_code, public_price, threshold_price, family FROM products WHERE ref = ?1",
            [product_ref],
            |r| {
                Ok(ProductInfo {
                    product_ref: r.get(0)?,
                    designation: r.get(1)?,
                    enedis_code: r.get(2)?,
                    public_price: r.get(3)?,
                    threshold_price: r.get(4)?,
                    family: r.get(5)?,
                })
            },
        )
        .map(Some)
        .or_else(|e| if e == rusqlite::Error::QueryReturnedNoRows { Ok(None) } else { Err(e) })
        .map_err(err)?;

    let lines = all(
        conn,
        "SELECT q.id, q.number, q.date, q.client_code, q.client_name, q.sales_rep, l.quantity, l.unit_price,
                l.discount, q.discount_pct, l.public_price, l.lpn_price, l.is_option
         FROM quote_lines l JOIN quotes q ON q.id = l.quote_id
         WHERE l.kind = 'item' AND l.product_ref = ?1
         ORDER BY q.date, q.number, l.position",
        params![product_ref],
        |r| {
            let (unit_price, discount, quote_discount, is_option): (f64, f64, f64, bool) =
                (r.get(7)?, r.get(8)?, r.get(9)?, r.get(12)?);
            let line_net = crate::pricing::net_unit_price(unit_price, discount);
            // Les options ne sont pas concernées par la remise globale (hors total).
            let net_unit_price =
                if is_option { line_net } else { crate::pricing::round2(line_net * (1.0 - quote_discount / 100.0)) };
            Ok(HistoryLine {
                quote_id: r.get(0)?,
                number: r.get(1)?,
                date: r.get(2)?,
                client_code: r.get(3)?,
                client_name: r.get(4)?,
                sales_rep: r.get(5)?,
                quantity: r.get(6)?,
                unit_price,
                discount,
                quote_discount,
                net_unit_price,
                public_price: r.get(10)?,
                lpn_price: r.get(11)?,
                is_option,
            })
        },
    )?;
    Ok(ProductHistory { product, lines })
}

fn all<T>(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    map: impl FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql).map_err(err)?;
    let rows = stmt.query_map(params, map).map_err(err)?;
    rows.collect::<std::result::Result<_, _>>().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(conn: &Connection, id: i64, date: &str, client: (&str, &str), rep: &str, total: f64) {
        conn.execute(
            "INSERT INTO quotes (id, number, client_code, client_name, sales_rep, date, total_net) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, format!("Q{id}"), client.0, client.1, rep, date, total],
        )
        .unwrap();
    }

    fn line(conn: &Connection, quote: i64, product: &str, qty: f64, total: f64, option: bool) {
        conn.execute(
            "INSERT INTO quote_lines (quote_id, position, kind, product_ref, designation, quantity, line_total, is_option)
             VALUES (?1, 0, 'item', ?2, ?2, ?3, ?4, ?5)",
            params![quote, product, qty, total, option],
        )
        .unwrap();
    }

    #[test]
    fn product_history_net_prices_in_date_order() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute(
            "INSERT INTO products (ref, designation, public_price, family) VALUES ('P1', 'Coffret', 339.29, 'CFO')",
            [],
        )
        .unwrap();
        quote(&conn, 1, "2026-05-01", ("C1", "ALPHA"), "Julie", 0.0);
        quote(&conn, 2, "2026-02-01", ("", "Ponctuel"), "", 0.0);
        conn.execute("UPDATE quotes SET discount_pct = 10 WHERE id = 1", []).unwrap();
        for (quote_id, price, discount, option) in [(1, 100.0, 5.0, false), (2, 120.0, 0.0, false), (1, 80.0, 0.0, true)] {
            conn.execute(
                "INSERT INTO quote_lines (quote_id, position, kind, product_ref, quantity, unit_price, discount, is_option)
                 VALUES (?1, 0, 'item', 'P1', 2, ?2, ?3, ?4)",
                params![quote_id, price, discount, option],
            )
            .unwrap();
        }
        let h = product_history(&conn, "P1").unwrap();
        assert_eq!(h.product.unwrap().designation, "Coffret");
        // Ordre chronologique ; prix net = remise de ligne puis remise globale (pas pour une option).
        let nets: Vec<(&str, f64, bool)> = h.lines.iter().map(|l| (l.date.as_str(), l.net_unit_price, l.is_option)).collect();
        assert_eq!(nets, [("2026-02-01", 120.0, false), ("2026-05-01", 85.5, false), ("2026-05-01", 80.0, true)]);
        assert!(product_history(&conn, "INCONNU").unwrap().product.is_none());
    }

    #[test]
    fn stats_by_year_month_client_rep_and_product() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        quote(&conn, 1, "2026-03-10", ("C1", "ALPHA"), "Julie", 100.0);
        quote(&conn, 2, "2026-03-20", ("C1", "ALPHA"), "Julie", 300.0);
        quote(&conn, 3, "2026-05-02", ("", "Ponctuel SARL"), "", 50.0);
        quote(&conn, 4, "2025-12-01", ("C2", "BRAVO"), "Jacques", 1000.0);
        conn.execute("UPDATE quotes SET won_at = '2026-04-01' WHERE id IN (2, 4)", []).unwrap();
        line(&conn, 1, "P1", 2.0, 80.0, false);
        line(&conn, 2, "P1", 1.0, 40.0, false);
        line(&conn, 2, "P2", 5.0, 250.0, false);
        line(&conn, 2, "P3", 1.0, 999.0, true); // option : pas comptée

        let s = quote_stats(&conn, Some("2026")).unwrap();
        assert_eq!(s.years, ["2026", "2025"]);
        assert_eq!((s.count, s.total, s.clients), (3, 450.0, 2));
        assert_eq!((s.won_count, s.won_total), (1, 300.0));
        assert_eq!(
            s.by_month,
            [
                Month { month: "2026-03".into(), count: 2, total: 400.0, won_count: 1, won_total: 300.0 },
                Month { month: "2026-05".into(), count: 1, total: 50.0, won_count: 0, won_total: 0.0 },
            ]
        );
        assert_eq!(
            s.top_clients[0],
            Group { label: "ALPHA".into(), code: "C1".into(), count: 2, total: 400.0, won_count: 1, won_total: 300.0 }
        );
        assert_eq!(s.top_clients[1].label, "Ponctuel SARL");
        assert_eq!(s.by_sales_rep.iter().map(|g| (g.label.as_str(), g.total)).collect::<Vec<_>>(), [("Julie", 400.0), ("", 50.0)]);
        assert_eq!(
            s.top_products,
            [
                ProductStat { product_ref: "P2".into(), designation: "P2".into(), quantity: 5.0, total: 250.0, quotes: 1 },
                ProductStat { product_ref: "P1".into(), designation: "P1".into(), quantity: 3.0, total: 120.0, quotes: 2 },
            ]
        );

        // Toutes les années.
        let all = quote_stats(&conn, None).unwrap();
        assert_eq!((all.count, all.total, all.clients), (4, 1450.0, 3));
        assert_eq!((all.won_count, all.won_total), (2, 1300.0));
        assert_eq!(all.top_clients[0].label, "BRAVO");
    }

    #[test]
    fn stats_count_one_version_per_quote() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        // Devis 1 en trois versions (la V2 obtenue) ; devis 4 en deux versions, aucune obtenue.
        quote(&conn, 1, "2026-03-01", ("C1", "ALPHA"), "", 100.0);
        quote(&conn, 2, "2026-03-05", ("C1", "ALPHA"), "", 200.0);
        quote(&conn, 3, "2026-04-01", ("C1", "ALPHA"), "", 300.0);
        quote(&conn, 4, "2026-03-01", ("C2", "BRAVO"), "", 10.0);
        quote(&conn, 5, "2026-05-01", ("C2", "BRAVO"), "", 20.0);
        conn.execute_batch(
            "UPDATE quotes SET version = 2, version_of = 1 WHERE id = 2;
             UPDATE quotes SET version = 3, version_of = 1 WHERE id = 3;
             UPDATE quotes SET version = 2, version_of = 4 WHERE id = 5;
             UPDATE quotes SET won_at = '2026-03-10' WHERE id = 2;",
        )
        .unwrap();
        line(&conn, 1, "P1", 1.0, 100.0, false);
        line(&conn, 2, "P1", 2.0, 200.0, false);
        line(&conn, 3, "P1", 3.0, 300.0, false);
        let s = quote_stats(&conn, None).unwrap();
        assert_eq!((s.count, s.total, s.won_count, s.won_total), (2, 220.0, 1, 200.0));
        assert_eq!(s.by_month.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(), ["2026-03", "2026-05"]);
        assert_eq!((s.top_products[0].quantity, s.top_products[0].quotes), (2.0, 1));
    }
}
