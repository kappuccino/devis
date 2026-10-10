use rusqlite::Connection;
use std::path::Path;

/// Lignes de devis. `kind` : `item` (article), `text` (texte libre dans `designation`)
/// ou `subtotal` (sous-total des articles depuis le sous-total précédent).
const QUOTE_LINES: &str = r#"
CREATE TABLE IF NOT EXISTS quote_lines (
    id           INTEGER PRIMARY KEY,
    quote_id     INTEGER NOT NULL REFERENCES quotes(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'item',
    product_ref  TEXT NOT NULL,
    -- Code ENEDIS du produit, copié au moment du devis (imprimé sur le PDF).
    enedis_code  TEXT,
    designation  TEXT NOT NULL DEFAULT '',
    quantity     REAL NOT NULL DEFAULT 1,
    unit_price   REAL NOT NULL DEFAULT 0,
    -- Remise supplémentaire de la ligne (en %), appliquée au prix unitaire.
    discount     REAL NOT NULL DEFAULT 0,
    -- 1 : ligne en option, comptée dans « Total options » et pas dans le total HT.
    is_option    INTEGER NOT NULL DEFAULT 0,
    price_source TEXT,
    line_total   REAL NOT NULL DEFAULT 0,
    -- Copies au moment du devis (affichage seulement, le prix seuil n'est jamais imprimé).
    public_price    REAL,
    threshold_price REAL,
    -- Affichage seulement : prix public remisé (CFA / CFO) et prix de la liste de prix (LPN).
    discounted_price REAL,
    lpn_price        REAL,
    lpn_list         TEXT
);"#;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS products (
    ref             TEXT PRIMARY KEY,
    enedis_code     TEXT,
    designation     TEXT NOT NULL DEFAULT '',
    public_price    REAL NOT NULL DEFAULT 0,
    eco_tax         REAL,
    eco_tax_code    TEXT,
    family          TEXT CHECK (family IN ('CFA', 'CFO')),
    threshold_price REAL
);

CREATE TABLE IF NOT EXISTS clients (
    code         TEXT PRIMARY KEY,
    name         TEXT NOT NULL DEFAULT '',
    group_name   TEXT,
    subgroup     TEXT,
    sales_rep    TEXT,
    discount_cfa REAL NOT NULL DEFAULT 0,
    discount_cfo REAL NOT NULL DEFAULT 0,
    email        TEXT,
    franco       TEXT,
    siren        TEXT
);

CREATE TABLE IF NOT EXISTS price_lists (
    code  TEXT PRIMARY KEY,
    label TEXT
);

CREATE TABLE IF NOT EXISTS price_list_items (
    id              INTEGER PRIMARY KEY,
    price_list_code TEXT NOT NULL REFERENCES price_lists(code) ON DELETE CASCADE,
    product_ref     TEXT NOT NULL,
    designation     TEXT,
    price           REAL NOT NULL,
    label           TEXT
);
CREATE INDEX IF NOT EXISTS idx_pli_list_ref ON price_list_items(price_list_code, product_ref);
CREATE INDEX IF NOT EXISTS idx_pli_ref ON price_list_items(product_ref);

CREATE TABLE IF NOT EXISTS client_price_lists (
    client_code     TEXT NOT NULL REFERENCES clients(code) ON DELETE CASCADE,
    price_list_code TEXT NOT NULL REFERENCES price_lists(code) ON DELETE CASCADE,
    PRIMARY KEY (client_code, price_list_code)
);

-- Les devis ne référencent pas les clients par clé étrangère : un réimport vide la table clients.
-- Chaque devis porte ses propres conditions (raison sociale, remises, listes de prix), copiées du
-- client à sa sélection puis modifiables. client_code vide : client ponctuel, absent de la base.
CREATE TABLE IF NOT EXISTS quotes (
    id          INTEGER PRIMARY KEY,
    number      TEXT NOT NULL UNIQUE,
    client_code TEXT NOT NULL,
    client_name TEXT NOT NULL DEFAULT '',
    discount_cfa REAL NOT NULL DEFAULT 0,
    discount_cfo REAL NOT NULL DEFAULT 0,
    -- Codes des listes de prix, séparés par des virgules.
    price_lists TEXT NOT NULL DEFAULT '',
    -- Remise globale sur les produits (en %).
    discount_pct REAL NOT NULL DEFAULT 0,
    -- Total après remise (frais de port et de facturation compris, non remisés).
    total_net    REAL NOT NULL DEFAULT 0,
    -- Liste de prix forcée (choisie parmi les favorites), prioritaire ; vide si aucune.
    forced_price_list TEXT NOT NULL DEFAULT '',
    -- Contact chez le client et commercial (repris du client, modifiables) : imprimés sur le PDF.
    contact_name  TEXT NOT NULL DEFAULT '',
    contact_email TEXT NOT NULL DEFAULT '',
    contact_phone TEXT NOT NULL DEFAULT '',
    sales_rep     TEXT NOT NULL DEFAULT '',
    date        TEXT NOT NULL,
    notes       TEXT,
    total_ht    REAL NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    -- Suivi : date à laquelle l'affaire a été marquée obtenue (devis devenu commande) ; NULL sinon.
    won_at      TEXT,
    -- Versions d'un devis : n° de version (1 pour l'original) et id de l'original (NULL pour lui).
    -- Une nouvelle version porte le numéro de l'original suivi de « -V2 », « -V3 »…
    version     INTEGER NOT NULL DEFAULT 1,
    version_of  INTEGER,
    -- Devis type (modèle) : nommé, sans client ni prix ; hors liste des devis, statistiques et
    -- numérotation (numéro technique « TYPE-… »).
    is_template   INTEGER NOT NULL DEFAULT 0,
    template_name TEXT NOT NULL DEFAULT '',
    -- Nom de l'affaire (chantier, opération…), imprimé sur le PDF.
    project_name  TEXT NOT NULL DEFAULT ''
);

{QUOTE_LINES}

-- Documents joints au PDF « Devis + Docs » : choix (inclus ou écarté) mémorisés par devis.
-- source 'index' : page (ou fichier entier si page_num NULL) trouvée dans la documentation ;
-- source 'external' : fichier ajouté à la main (PDF ou image), toujours en entier.
CREATE TABLE IF NOT EXISTS quote_attachments (
    id          INTEGER PRIMARY KEY,
    quote_id    INTEGER NOT NULL REFERENCES quotes(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    source      TEXT NOT NULL,
    product_ref TEXT NOT NULL DEFAULT '',
    path        TEXT NOT NULL,
    page_num    INTEGER,
    included    INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_quote_attachments ON quote_attachments(quote_id);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    conn.execute_batch(&SCHEMA.replace("{QUOTE_LINES}", QUOTE_LINES))?;
    migrate(conn)
}

/// Mises à jour des bases créées par une version précédente.
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    // Groupes de lignes : une ligne « section » porte le titre du groupe.
    add_column(conn, "quote_lines", "kind", "TEXT NOT NULL DEFAULT 'item'")?;
    // Prix public et prix seuil mémorisés sur chaque ligne.
    add_column(conn, "quote_lines", "public_price", "REAL")?;
    add_column(conn, "quote_lines", "threshold_price", "REAL")?;
    // Conditions de prix propres au devis : reprises du client pour les devis existants.
    if add_column(conn, "quotes", "discount_cfa", "REAL NOT NULL DEFAULT 0")? {
        add_column(conn, "quotes", "discount_cfo", "REAL NOT NULL DEFAULT 0")?;
        add_column(conn, "quotes", "price_lists", "TEXT NOT NULL DEFAULT ''")?;
        conn.execute_batch(
            "UPDATE quotes SET
                 discount_cfa = COALESCE((SELECT discount_cfa FROM clients WHERE code = quotes.client_code), 0),
                 discount_cfo = COALESCE((SELECT discount_cfo FROM clients WHERE code = quotes.client_code), 0),
                 price_lists = COALESCE((SELECT group_concat(price_list_code, ',') FROM client_price_lists
                                         WHERE client_code = quotes.client_code), '')",
        )?;
    }
    drop_sections(conn)?;
    // Remises supplémentaires : par ligne et sur le total du devis.
    add_column(conn, "quote_lines", "discount", "REAL NOT NULL DEFAULT 0")?;
    add_column(conn, "quotes", "discount_pct", "REAL NOT NULL DEFAULT 0")?;
    // Lignes en option et liste de prix forcée.
    add_column(conn, "quote_lines", "is_option", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(conn, "quotes", "forced_price_list", "TEXT NOT NULL DEFAULT ''")?;
    // Prix remisé et prix LPN mémorisés sur les lignes.
    add_column(conn, "quote_lines", "discounted_price", "REAL")?;
    add_column(conn, "quote_lines", "lpn_price", "REAL")?;
    add_column(conn, "quote_lines", "lpn_list", "TEXT")?;
    // Contact et commercial du devis.
    for column in ["contact_name", "contact_email", "contact_phone", "sales_rep"] {
        add_column(conn, "quotes", column, "TEXT NOT NULL DEFAULT ''")?;
    }
    // Total remisé enregistré (avant : calculé sur le total HT, sans frais).
    if add_column(conn, "quotes", "total_net", "REAL NOT NULL DEFAULT 0")? {
        conn.execute_batch("UPDATE quotes SET total_net = round(total_ht * (1 - discount_pct / 100.0), 2)")?;
    }
    // Code ENEDIS sur les lignes : repris du catalogue pour les devis existants.
    if add_column(conn, "quote_lines", "enedis_code", "TEXT")? {
        conn.execute_batch(
            "UPDATE quote_lines SET enedis_code = (SELECT enedis_code FROM products WHERE ref = quote_lines.product_ref)
             WHERE kind = 'item'",
        )?;
    }
    // Suivi « Affaire obtenue ».
    add_column(conn, "quotes", "won_at", "TEXT")?;
    // Versions d'un devis.
    add_column(conn, "quotes", "version", "INTEGER NOT NULL DEFAULT 1")?;
    add_column(conn, "quotes", "version_of", "INTEGER")?;
    conn.execute_batch("CREATE INDEX IF NOT EXISTS quotes_version_of ON quotes(version_of)")?;
    // Devis types.
    add_column(conn, "quotes", "is_template", "INTEGER NOT NULL DEFAULT 0")?;
    add_column(conn, "quotes", "template_name", "TEXT NOT NULL DEFAULT ''")?;
    // Nom de l'affaire.
    add_column(conn, "quotes", "project_name", "TEXT NOT NULL DEFAULT ''")?;
    Ok(())
}

/// Les groupes (lignes `section`) ont été remplacés par des lignes de texte et des sous-totaux :
/// les anciens titres deviennent des lignes de texte. On reconstruit aussi la table pour retirer
/// l'ancienne contrainte CHECK sur `kind` (SQLite ne sait pas la modifier).
fn drop_sections(conn: &Connection) -> rusqlite::Result<()> {
    let sql: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'quote_lines'",
        [],
        |r| r.get(0),
    )?;
    if !sql.contains("'section'") {
        return Ok(());
    }
    const COLUMNS: &str = "id, quote_id, position, kind, product_ref, designation, quantity, unit_price, \
                           price_source, line_total, public_price, threshold_price";
    conn.execute_batch(&format!(
        "BEGIN;
         ALTER TABLE quote_lines RENAME TO quote_lines_old;
         {create}
         INSERT INTO quote_lines ({COLUMNS})
             SELECT {COLUMNS} FROM quote_lines_old;
         UPDATE quote_lines SET kind = 'text', quantity = 0, unit_price = 0, line_total = 0 WHERE kind = 'section';
         DROP TABLE quote_lines_old;
         COMMIT;",
        create = QUOTE_LINES,
    ))
}

/// Ajoute la colonne si elle manque ; renvoie vrai si elle vient d'être ajoutée.
fn add_column(conn: &Connection, table: &str, column: &str, definition: &str) -> rusqlite::Result<bool> {
    if has_column(conn, table, column)? {
        return Ok(false);
    }
    conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))?;
    Ok(true)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?;
    stmt.exists([column])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_kind_column_to_existing_quote_lines() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE quote_lines (id INTEGER PRIMARY KEY, quote_id INTEGER, position INTEGER, product_ref TEXT);
             INSERT INTO quote_lines (quote_id, position, product_ref) VALUES (1, 0, 'A1');",
        )
        .unwrap();
        init(&conn).unwrap();
        let (kind, public): (String, Option<f64>) = conn
            .query_row("SELECT kind, public_price FROM quote_lines", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(kind, "item");
        assert_eq!(public, None);
        init(&conn).unwrap(); // idempotent
    }

    #[test]
    fn existing_quotes_get_their_client_conditions() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE quotes (id INTEGER PRIMARY KEY, number TEXT NOT NULL UNIQUE, client_code TEXT NOT NULL,
                 client_name TEXT NOT NULL DEFAULT '', date TEXT NOT NULL, notes TEXT, total_ht REAL NOT NULL DEFAULT 0,
                 created_at TEXT, updated_at TEXT);
             INSERT INTO quotes (id, number, client_code, date) VALUES (1, 'DEV-1', 'C1', '2026-10-08');",
        )
        .unwrap();
        init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO clients (code, name, discount_cfa, discount_cfo) VALUES ('C1', 'Client', 57.5, 60);
             INSERT INTO price_lists (code) VALUES ('T1'), ('T2');
             INSERT INTO client_price_lists VALUES ('C1', 'T1'), ('C1', 'T2');
             ALTER TABLE quotes DROP COLUMN discount_cfa;
             ALTER TABLE quotes DROP COLUMN discount_cfo;
             ALTER TABLE quotes DROP COLUMN price_lists;",
        )
        .unwrap();
        init(&conn).unwrap();
        let (cfa, cfo, lists): (f64, f64, String) = conn
            .query_row("SELECT discount_cfa, discount_cfo, price_lists FROM quotes", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((cfa, cfo), (57.5, 60.0));
        assert_eq!(lists.split(',').count(), 2);
    }

    #[test]
    fn turns_old_sections_into_text_lines() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE quotes (id INTEGER PRIMARY KEY, number TEXT NOT NULL UNIQUE, client_code TEXT NOT NULL,
                 client_name TEXT NOT NULL DEFAULT '', date TEXT NOT NULL, notes TEXT, total_ht REAL NOT NULL DEFAULT 0,
                 created_at TEXT, updated_at TEXT);
             INSERT INTO quotes (id, number, client_code, date) VALUES (1, 'DEV-1', 'C', '2026-10-08');
             CREATE TABLE quote_lines (id INTEGER PRIMARY KEY, quote_id INTEGER NOT NULL REFERENCES quotes(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL, kind TEXT NOT NULL DEFAULT 'item' CHECK (kind IN ('item', 'section')),
                 product_ref TEXT NOT NULL, designation TEXT NOT NULL DEFAULT '', quantity REAL NOT NULL DEFAULT 1,
                 unit_price REAL NOT NULL DEFAULT 0, price_source TEXT, line_total REAL NOT NULL DEFAULT 0,
                 public_price REAL, threshold_price REAL);
             INSERT INTO quote_lines (quote_id, position, kind, product_ref, designation, quantity) VALUES
                 (1, 0, 'section', '', 'Poste A', 0), (1, 1, 'item', '00188', 'E4R', 3);",
        )
        .unwrap();
        init(&conn).unwrap();
        let kinds: Vec<(String, String)> = conn
            .prepare("SELECT kind, designation FROM quote_lines ORDER BY position")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(kinds, [("text".into(), "Poste A".into()), ("item".into(), "E4R".into())]);
        // Plus de contrainte : un sous-total s'enregistre.
        conn.execute("INSERT INTO quote_lines (quote_id, position, kind, product_ref) VALUES (1, 2, 'subtotal', '')", [])
            .unwrap();
    }
}
