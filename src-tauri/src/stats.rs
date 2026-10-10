//! Statistiques des devis (écran Statistiques) : chiffres clés, devis par mois, meilleurs
//! clients, commerciaux et produits, sur une année ou sur toute la base.
//!
//! Montants : total HT après remise globale, frais compris (`quotes.total_net`) ; pour les
//! produits, total des lignes (remise de ligne comprise, avant remise globale). Les lignes en
//! option ne sont pas comptées. Ce sont des montants devisés, pas commandés.

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
}

/// Regroupement (client ou commercial) : nombre de devis et montant.
#[derive(Serialize, Debug, PartialEq)]
pub struct Group {
    pub label: String,
    /// Code client (vide pour un client ponctuel ou un commercial).
    pub code: String,
    pub count: i64,
    pub total: f64,
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
    pub by_month: Vec<Month>,
    pub top_clients: Vec<Group>,
    pub by_sales_rep: Vec<Group>,
    pub top_products: Vec<ProductStat>,
}

/// Nombre de lignes des classements.
const TOP: i64 = 10;

/// Statistiques de l'année `year` (`"2026"`), ou de toute la base si `None`.
pub fn quote_stats(conn: &Connection, year: Option<&str>) -> Result<Stats> {
    // Filtre commun : `?1` NULL = toutes les années.
    const IN_YEAR: &str = "(?1 IS NULL OR substr(q.date, 1, 4) = ?1)";
    // Client ponctuel (sans code) : regroupé par son nom.
    const CLIENT_KEY: &str = "CASE WHEN q.client_code <> '' THEN q.client_code ELSE 'nom:' || q.client_name END";

    let years = all(conn, "SELECT DISTINCT substr(date, 1, 4) FROM quotes ORDER BY 1 DESC", params![], |r| r.get(0))?;

    let (count, total, clients) = conn
        .query_row(
            &format!("SELECT COUNT(*), COALESCE(SUM(q.total_net), 0), COUNT(DISTINCT {CLIENT_KEY}) FROM quotes q WHERE {IN_YEAR}"),
            [year],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(err)?;

    let by_month = all(
        conn,
        &format!(
            "SELECT substr(q.date, 1, 7), COUNT(*), COALESCE(SUM(q.total_net), 0) FROM quotes q
             WHERE {IN_YEAR} GROUP BY 1 ORDER BY 1"
        ),
        params![year],
        |r| Ok(Month { month: r.get(0)?, count: r.get(1)?, total: r.get(2)? }),
    )?;

    let top_clients = all(
        conn,
        &format!(
            "SELECT MAX(q.client_name), q.client_code, COUNT(*), COALESCE(SUM(q.total_net), 0) FROM quotes q
             WHERE {IN_YEAR} GROUP BY {CLIENT_KEY} ORDER BY 4 DESC, 3 DESC LIMIT ?2"
        ),
        params![year, TOP],
        |r| Ok(Group { label: r.get(0)?, code: r.get(1)?, count: r.get(2)?, total: r.get(3)? }),
    )?;

    let by_sales_rep = all(
        conn,
        &format!(
            "SELECT trim(q.sales_rep), COUNT(*), COALESCE(SUM(q.total_net), 0) FROM quotes q
             WHERE {IN_YEAR} GROUP BY trim(q.sales_rep) ORDER BY 3 DESC"
        ),
        params![year],
        |r| Ok(Group { label: r.get(0)?, code: String::new(), count: r.get(1)?, total: r.get(2)? }),
    )?;

    let top_products = all(
        conn,
        &format!(
            "SELECT l.product_ref, MAX(l.designation), SUM(l.quantity), SUM(l.line_total), COUNT(DISTINCT l.quote_id)
             FROM quote_lines l JOIN quotes q ON q.id = l.quote_id
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

    Ok(Stats { years, count, total, clients, by_month, top_clients, by_sales_rep, top_products })
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
    fn stats_by_year_month_client_rep_and_product() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        quote(&conn, 1, "2026-03-10", ("C1", "ALPHA"), "Julie", 100.0);
        quote(&conn, 2, "2026-03-20", ("C1", "ALPHA"), "Julie", 300.0);
        quote(&conn, 3, "2026-05-02", ("", "Ponctuel SARL"), "", 50.0);
        quote(&conn, 4, "2025-12-01", ("C2", "BRAVO"), "Jacques", 1000.0);
        line(&conn, 1, "P1", 2.0, 80.0, false);
        line(&conn, 2, "P1", 1.0, 40.0, false);
        line(&conn, 2, "P2", 5.0, 250.0, false);
        line(&conn, 2, "P3", 1.0, 999.0, true); // option : pas comptée

        let s = quote_stats(&conn, Some("2026")).unwrap();
        assert_eq!(s.years, ["2026", "2025"]);
        assert_eq!((s.count, s.total, s.clients), (3, 450.0, 2));
        assert_eq!(
            s.by_month,
            [
                Month { month: "2026-03".into(), count: 2, total: 400.0 },
                Month { month: "2026-05".into(), count: 1, total: 50.0 },
            ]
        );
        assert_eq!(s.top_clients[0], Group { label: "ALPHA".into(), code: "C1".into(), count: 2, total: 400.0 });
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
        assert_eq!(all.top_clients[0].label, "BRAVO");
    }
}
