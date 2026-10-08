use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Conditions de prix d'un devis, figées dans le devis : copiées du client à sa sélection,
/// modifiables ensuite sans toucher à la fiche client (ou saisies pour un client ponctuel).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PricingContext {
    pub price_lists: Vec<String>,
    pub discount_cfa: f64,
    pub discount_cfo: f64,
    /// Liste de prix forcée sur le devis (choisie parmi les favorites) : prioritaire sur les autres.
    #[serde(default)]
    pub forced_price_list: Option<String>,
}

impl PricingContext {
    /// Codes de toutes les listes du devis (forcée comprise) en JSON,
    /// pour `IN (SELECT value FROM json_each(?))`.
    fn lists_json(&self) -> String {
        let mut all = self.price_lists.clone();
        all.extend(self.forced_price_list.clone());
        serde_json::to_string(&all).unwrap_or_else(|_| "[]".into())
    }
}

/// Prix le plus bas d'un produit parmi des listes (JSON), avec la liste et sa désignation.
fn cheapest_in(conn: &Connection, lists_json: &str, product_ref: &str) -> rusqlite::Result<Option<(f64, String, Option<String>)>> {
    conn.query_row(
        "SELECT pli.price, pli.price_list_code, pli.designation
         FROM price_list_items pli
         WHERE pli.price_list_code IN (SELECT value FROM json_each(?1)) AND pli.product_ref = ?2
         ORDER BY pli.price ASC
         LIMIT 1",
        params![lists_json, product_ref],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ResolvedPrice {
    pub product_ref: String,
    pub designation: String,
    pub unit_price: f64,
    /// Code de la liste de prix, ou « Public -X% CFO » quand on retombe sur le prix public.
    pub source: String,
    pub family: Option<String>,
    pub public_price: Option<f64>,
    pub threshold_price: Option<f64>,
}

/// Prix d'un produit dans les conditions d'un devis :
/// 1. le prix de la liste forcée, si le produit y figure (même si une autre liste est moins chère) ;
/// 2. sinon le prix le plus bas parmi les listes de prix du devis ;
/// 3. sinon le prix public moins la remise CFA ou CFO du devis selon la famille du produit.
pub fn resolve_price(conn: &Connection, ctx: &PricingContext, product_ref: &str) -> Result<ResolvedPrice, String> {
    let lists = ctx.lists_json();
    let typed = product_ref.trim();
    let canonical = canonical_ref(conn, &lists, typed).map_err(|e| e.to_string())?;
    let product_ref = canonical.as_deref().unwrap_or(typed);
    let product: Option<(String, f64, Option<String>, Option<f64>)> = conn
        .query_row(
            "SELECT designation, public_price, family, threshold_price FROM products WHERE ref = ?1",
            [product_ref],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let forced = match &ctx.forced_price_list {
        Some(code) => {
            let one = serde_json::to_string(&[code]).map_err(|e| e.to_string())?;
            cheapest_in(conn, &one, product_ref)
                .map_err(|e| e.to_string())?
                .map(|(price, list, designation)| (price, format!("{list} (forcée)"), designation))
        }
        None => None,
    };
    let best = match forced {
        Some(hit) => Some(hit),
        None => {
            let usual = serde_json::to_string(&ctx.price_lists).map_err(|e| e.to_string())?;
            cheapest_in(conn, &usual, product_ref).map_err(|e| e.to_string())?
        }
    };

    if let Some((price, list_code, list_designation)) = best {
        let (designation, family, public_price, threshold_price) = match &product {
            Some((d, p, f, t)) => (d.clone(), f.clone(), Some(*p), *t),
            None => (list_designation.unwrap_or_default(), None, None, None),
        };
        return Ok(ResolvedPrice {
            product_ref: product_ref.to_string(),
            designation,
            unit_price: round2(price),
            source: list_code,
            family,
            public_price: public_price.map(round2),
            threshold_price: threshold_price.map(round2),
        });
    }

    let Some((designation, public_price, family, threshold_price)) = product else {
        return Err(format!("Produit « {product_ref} » introuvable"));
    };

    let discount = match family.as_deref() {
        Some("CFA") => ctx.discount_cfa,
        Some("CFO") => ctx.discount_cfo,
        _ => 0.0,
    };
    let unit_price = round2(public_price * (1.0 - discount / 100.0));
    let source = match family.as_deref() {
        Some(f) => format!("Public -{}% {f}", fmt_pct(discount)),
        None => "Public".to_string(),
    };

    Ok(ResolvedPrice {
        product_ref: product_ref.to_string(),
        designation,
        unit_price,
        source,
        family,
        public_price: Some(round2(public_price)),
        threshold_price: threshold_price.map(round2),
    })
}

/// Retrouve la référence exacte à partir de ce qui a été tapé :
/// tel quel, puis sans tenir compte de la casse, puis sans les zéros initiaux (« 188 » → « 00188 »).
/// On cherche dans le catalogue et dans les listes de prix du devis ; il faut une seule correspondance.
fn canonical_ref(conn: &Connection, lists_json: &str, typed: &str) -> rusqlite::Result<Option<String>> {
    if typed.is_empty() {
        return Ok(None);
    }
    let candidates = |condition: &str| -> rusqlite::Result<Vec<String>> {
        let sql = format!(
            "SELECT ref FROM products WHERE {c}
             UNION
             SELECT pli.product_ref FROM price_list_items pli
             WHERE pli.price_list_code IN (SELECT value FROM json_each(?2)) AND {pc}",
            c = condition.replace("{col}", "ref"),
            pc = condition.replace("{col}", "pli.product_ref"),
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![typed, lists_json], |r| r.get(0))?;
        rows.collect()
    };
    for condition in [
        "{col} = ?1",
        "{col} = ?1 COLLATE NOCASE",
        "ltrim({col}, '0') = ltrim(?1, '0') COLLATE NOCASE",
    ] {
        let found = candidates(condition)?;
        match found.len() {
            0 => continue,
            1 => return Ok(found.into_iter().next()),
            // Plusieurs références possibles : on ne devine pas.
            _ => return Ok(None),
        }
    }
    Ok(None)
}

/// Prix unitaire après la remise supplémentaire de la ligne (en %), au centime.
pub fn net_unit_price(unit_price: f64, discount_pct: f64) -> f64 {
    round2(unit_price * (1.0 - discount_pct / 100.0))
}

/// Total d'une ligne : quantité × prix unitaire remisé.
pub fn line_total(quantity: f64, unit_price: f64, discount_pct: f64) -> f64 {
    round2(quantity * net_unit_price(unit_price, discount_pct))
}

/// Total après la remise globale du devis (en %).
pub fn discounted_total(total: f64, discount_pct: f64) -> f64 {
    round2(total * (1.0 - discount_pct / 100.0))
}

/// Arrondi au centime, sans les erreurs du binaire (1,005 → 1,01 et non 1,00).
pub fn round2(v: f64) -> f64 {
    let cents = (v * 1e9).round() / 1e7;
    cents.round() / 100.0
}

fn fmt_pct(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        v.to_string().replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO products (ref, designation, public_price, family) VALUES
                ('A1', 'Produit CFO', 100, 'CFO'),
                ('B1', 'Produit CFA', 200, 'CFA'),
                ('C1', 'Produit en liste', 50, 'CFO');
             INSERT INTO price_lists (code) VALUES ('T1'), ('T2'), ('T3');
             INSERT INTO price_list_items (price_list_code, product_ref, price) VALUES
                ('T1', 'C1', 12.5), ('T2', 'C1', 10.0), ('T3', 'C1', 1.0),
                ('T1', 'X9', 7.0);",
        )
        .unwrap();
        conn
    }

    /// Conditions du devis de test : listes T1 et T2, remises 57,5 % CFA et 60 % CFO.
    fn ctx() -> PricingContext {
        PricingContext {
            price_lists: vec!["T1".into(), "T2".into()],
            discount_cfa: 57.5,
            discount_cfo: 60.0,
            forced_price_list: None,
        }
    }

    #[test]
    fn cheapest_attached_list_wins() {
        let conn = setup();
        let r = resolve_price(&conn, &ctx(), "C1").unwrap();
        // T3 est moins cher mais ne fait pas partie des listes du devis.
        assert_eq!(r.unit_price, 10.0);
        assert_eq!(r.source, "T2");
    }

    #[test]
    fn falls_back_to_public_price_with_cfo_discount() {
        let conn = setup();
        let r = resolve_price(&conn, &ctx(), "A1").unwrap();
        assert_eq!(r.unit_price, 40.0);
        assert_eq!(r.source, "Public -60% CFO");
    }

    #[test]
    fn falls_back_to_public_price_with_cfa_discount() {
        let conn = setup();
        let r = resolve_price(&conn, &ctx(), "B1").unwrap();
        assert_eq!(r.unit_price, 85.0);
        assert_eq!(r.source, "Public -57,5% CFA");
    }

    #[test]
    fn list_only_product_is_priced() {
        let conn = setup();
        let r = resolve_price(&conn, &ctx(), "X9").unwrap();
        assert_eq!(r.unit_price, 7.0);
        assert_eq!(r.source, "T1");
    }

    #[test]
    fn prices_are_rounded_to_cents() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO products (ref, public_price, family, threshold_price) VALUES ('R1', 10.0, 'CFO', 1.377472);
             INSERT INTO price_list_items (price_list_code, product_ref, price) VALUES ('T1', 'R2', 1.005);",
        )
        .unwrap();
        // 10 × (1 - 0,60) = 4 ; seuil 1,377472 → 1,38
        let r = resolve_price(&conn, &ctx(), "R1").unwrap();
        assert_eq!(r.threshold_price, Some(1.38));
        assert_eq!(resolve_price(&conn, &ctx(), "R2").unwrap().unit_price, 1.01);
        assert_eq!(round2(2.8389280000000001), 2.84);
        assert_eq!(round2(0.125), 0.13);
    }

    #[test]
    fn finds_ref_despite_case_and_missing_leading_zeros() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO products (ref, designation, public_price, family) VALUES
                ('00188', 'E4R', 10, 'CFO'), ('06PFAD0003', 'Coffret', 20, 'CFO');",
        )
        .unwrap();
        assert_eq!(resolve_price(&conn, &ctx(), "188").unwrap().product_ref, "00188");
        assert_eq!(resolve_price(&conn, &ctx(), " 06pfad0003 ").unwrap().product_ref, "06PFAD0003");
        // Référence de liste de prix absente du catalogue.
        assert_eq!(resolve_price(&conn, &ctx(), "x9").unwrap().product_ref, "X9");
    }

    #[test]
    fn ambiguous_ref_is_not_guessed() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO products (ref, public_price, family) VALUES ('0188', 1, 'CFO'), ('00188', 1, 'CFO');",
        )
        .unwrap();
        assert!(resolve_price(&conn, &ctx(), "188").is_err());
    }

    #[test]
    fn quote_discounts_and_lists_are_used() {
        let conn = setup();
        // Remise CFO modifiée sur le devis, aucune liste (client ponctuel sans liste).
        let custom = PricingContext { price_lists: vec![], discount_cfa: 0.0, discount_cfo: 50.0, forced_price_list: None };
        let r = resolve_price(&conn, &custom, "C1").unwrap();
        assert_eq!((r.unit_price, r.source.as_str()), (25.0, "Public -50% CFO"));
        // Une seule liste.
        let single = PricingContext { price_lists: vec!["T3".into()], ..custom };
        assert_eq!(resolve_price(&conn, &single, "C1").unwrap().unit_price, 1.0);
    }

    #[test]
    fn line_and_quote_discounts() {
        // 12,35 € remisé de 10 % = 11,115 → 11,12 € ; × 3 = 33,36 €
        assert_eq!(net_unit_price(12.35, 10.0), 11.12);
        assert_eq!(line_total(3.0, 12.35, 10.0), 33.36);
        assert_eq!(line_total(3.0, 12.35, 0.0), 37.05);
        assert_eq!(discounted_total(1000.0, 5.0), 950.0);
        assert_eq!(discounted_total(1243.78, 3.5), 1200.25);
    }

    #[test]
    fn forced_list_has_priority() {
        let conn = setup();
        // T1 (12,50) est forcée : elle passe devant T2 (10,00), pourtant moins chère.
        let forced = PricingContext { forced_price_list: Some("T1".into()), ..ctx() };
        let r = resolve_price(&conn, &forced, "C1").unwrap();
        assert_eq!((r.unit_price, r.source.as_str()), (12.5, "T1 (forcée)"));
        // Liste forcée hors des listes du client : elle s'ajoute aux recherches.
        let outside = PricingContext { forced_price_list: Some("T3".into()), ..ctx() };
        assert_eq!(resolve_price(&conn, &outside, "C1").unwrap().unit_price, 1.0);
        // Produit absent de la liste forcée : règle habituelle.
        let r = resolve_price(&conn, &outside, "X9").unwrap();
        assert_eq!((r.unit_price, r.source.as_str()), (7.0, "T1"));
    }

    #[test]
    fn unknown_product_is_an_error() {
        let conn = setup();
        assert!(resolve_price(&conn, &ctx(), "NOPE").is_err());
    }
}
