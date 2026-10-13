//! Sauvegarde de la base des devis (`devis.db`) : copies datées dans un dossier choisi
//! (par défaut `Documents/Devis Groupe Cahors/sauvegardes`, hors du dossier de l'appli qu'une
//! désinstallation avec « Supprimer les données » efface en entier), automatiques au démarrage
//! (au plus une fois par 20 h) et chaque soir à 22 h tant que l'appli tourne, à la demande et avant
//! chaque mise à jour, en gardant les N plus récentes ; restauration d'une copie. Une base sans
//! aucun devis n'est jamais copiée automatiquement et ne fait jamais supprimer d'anciennes copies.
//!
//! Copie : `VACUUM INTO`, cohérente même pendant que l'appli écrit. Restauration : la copie est
//! recopiée dans la connexion ouverte (API de sauvegarde SQLite), après une copie de sécurité
//! de la base actuelle (`devis-avant-restauration-…`, jamais supprimée automatiquement).

use rusqlite::{Connection, OpenFlags, OptionalExtension, MAIN_DB};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Réglages : dossier (vide : dossier par défaut), nombre de copies gardées, copie au démarrage.
pub const DIR_KEY: &str = "backup_dir";
pub const KEEP_KEY: &str = "backup_keep";
pub const AUTO_KEY: &str = "backup_auto";
/// Version de l'appli au dernier lancement (copie avant la mise à niveau de la base si elle change).
const VERSION_KEY: &str = "last_app_version";
pub const DEFAULT_KEEP: usize = 10;
/// Copie automatique si la plus récente a plus de 20 h (une par jour de travail).
const AUTO_INTERVAL: Duration = Duration::from_secs(20 * 3600);
/// Copie du soir, heure locale : l'appli reste ouverte des jours entiers, sans redémarrage.
const EVENING: &str = "22:00:00";
/// Au plus tard, une copie par 24 h tant que l'appli tourne (ordinateur en veille à 22 h…).
const MAX_GAP_MS: i64 = 24 * 3600 * 1000;

const PREFIX: &str = "devis-";
const SAFETY_PREFIX: &str = "devis-avant-restauration-";
/// Copie faite juste avant d'installer une mise à jour (les N dernières sont gardées à part).
const UPDATE_PREFIX: &str = "devis-avant-mise-a-jour-";
const UPDATE_KEEP: usize = 10;

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize, Debug)]
pub struct BackupFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    /// Date de la copie (ms depuis 1970).
    pub modified: Option<i64>,
    /// Copie de sécurité faite avant une restauration ou une mise à jour (hors du tri des N copies).
    pub safety: bool,
    /// Copie faite avant une mise à jour.
    pub before_update: bool,
}

fn setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(err)
}

/// Dossier des sauvegardes : réglage, sinon le dossier par défaut.
pub fn dir(conn: &Connection, default_dir: &Path) -> Result<PathBuf> {
    Ok(match setting(conn, DIR_KEY)?.filter(|d| !d.trim().is_empty()) {
        Some(d) => PathBuf::from(d.trim()),
        None => default_dir.to_path_buf(),
    })
}

/// Dossier par défaut : `Documents/Devis Groupe Cahors/sauvegardes` (à défaut, l'ancien dossier).
pub fn default_dir(documents: Option<PathBuf>, db_path: &Path) -> PathBuf {
    match documents {
        Some(d) => d.join("Devis Groupe Cahors").join("sauvegardes"),
        None => legacy_dir(db_path),
    }
}

/// Dossier par défaut jusqu'à la 0.2.2 : `sauvegardes/` à côté de la base.
fn legacy_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("sauvegardes")
}

/// Copie les sauvegardes de l'ancien dossier par défaut dans le nouveau (sans rien supprimer),
/// si aucun dossier n'a été choisi dans les Réglages. Renvoie le nombre de copies recopiées.
pub fn migrate_legacy(conn: &Connection, db_path: &Path, default_dir: &Path) -> Result<usize> {
    let legacy = legacy_dir(db_path);
    if setting(conn, DIR_KEY)?.is_some_and(|d| !d.trim().is_empty()) || legacy == default_dir {
        return Ok(0);
    }
    let mut copied = 0;
    for file in list(&legacy)? {
        let target = default_dir.join(&file.name);
        if target.exists() {
            continue;
        }
        std::fs::create_dir_all(default_dir)
            .map_err(|e| format!("Dossier de sauvegarde « {} » : {e}", default_dir.display()))?;
        std::fs::copy(&file.path, &target).map_err(|e| format!("Copie de « {} » : {e}", file.name))?;
        copied += 1;
    }
    Ok(copied)
}

/// La base contient-elle au moins un devis ?
fn has_quotes(conn: &Connection) -> Result<bool> {
    conn.query_row("SELECT EXISTS (SELECT 1 FROM quotes)", [], |r| r.get(0)).map_err(err)
}

fn keep(conn: &Connection) -> Result<usize> {
    Ok(setting(conn, KEEP_KEY)?
        .and_then(|k| k.trim().parse().ok())
        .filter(|&k: &usize| k > 0)
        .unwrap_or(DEFAULT_KEEP))
}

fn timestamp(conn: &Connection) -> Result<String> {
    conn.query_row("SELECT strftime('%Y-%m-%d_%Hh%M%S', 'now', 'localtime')", [], |r| r.get(0))
        .map_err(err)
}

/// Copie la base dans `dir` sous le nom `<prefix><date>.db` et renvoie son chemin.
fn copy_to(conn: &Connection, dir: &Path, prefix: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Dossier de sauvegarde « {} » : {e}", dir.display()))?;
    let base = format!("{prefix}{}", timestamp(conn)?);
    // Deux copies dans la même seconde : suffixe pour ne pas écraser.
    let mut path = dir.join(format!("{base}.db"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base}-{n}.db"));
        n += 1;
    }
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy()]).map_err(|e| format!("Sauvegarde : {e}"))?;
    Ok(path)
}

/// Sauvegarde maintenant, puis ne garde que les N copies les plus récentes (sauf si la base est
/// vide : ses copies ne doivent pas faire disparaître celles d'une base pleine).
pub fn backup_now(conn: &Connection, default_dir: &Path) -> Result<PathBuf> {
    let dir = dir(conn, default_dir)?;
    let path = copy_to(conn, &dir, PREFIX)?;
    if has_quotes(conn)? {
        prune(&dir, keep(conn)?)?;
    }
    Ok(path)
}

/// Copie juste avant d'installer une mise à jour (rien si la base n'a aucun devis) ; les
/// `UPDATE_KEEP` dernières sont gardées.
pub fn backup_before_update(conn: &Connection, default_dir: &Path) -> Result<Option<PathBuf>> {
    if !has_quotes(conn)? {
        return Ok(None);
    }
    update_copy(conn, &dir(conn, default_dir)?).map(Some)
}

fn update_copy(conn: &Connection, dir: &Path) -> Result<PathBuf> {
    let path = copy_to(conn, dir, UPDATE_PREFIX)?;
    for old in list(dir)?.into_iter().filter(|f| f.before_update).skip(UPDATE_KEEP) {
        std::fs::remove_file(&old.path).map_err(|e| format!("Suppression de « {} » : {e}", old.name))?;
    }
    Ok(path)
}

/// Au démarrage, **avant** la mise à niveau de la base (`db::open`) : si la version de l'appli a
/// changé depuis le dernier lancement (mise à jour automatique ou installeur lancé à la main),
/// copie de la base telle que l'ancienne version l'a laissée. Rien si la base est absente ou
/// sans devis (aucune table n'est créée ici).
pub fn backup_before_upgrade(db_path: &Path, default_dir: &Path, version: &str) -> Result<Option<PathBuf>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = Connection::open(db_path).map_err(err)?;
    // Base d'une très ancienne version : pas encore de table des réglages ou des devis.
    let last = setting(&conn, VERSION_KEY).ok().flatten();
    if last.as_deref() == Some(version) || !has_quotes(&conn).unwrap_or(false) {
        return Ok(None);
    }
    let dir = dir(&conn, default_dir).unwrap_or_else(|_| default_dir.to_path_buf());
    update_copy(&conn, &dir).map(Some)
}

/// Retient la version lancée (après la mise à niveau de la base).
pub fn record_version(conn: &Connection, version: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [VERSION_KEY, version],
    )
    .map(|_| ())
    .map_err(err)
}

/// Sauvegarde au démarrage : si elle est activée, que la base a des devis et que la dernière
/// copie a plus de 20 h.
pub fn auto_backup(conn: &Connection, default_dir: &Path) -> Result<Option<PathBuf>> {
    if setting(conn, AUTO_KEY)?.as_deref() == Some("0") || !has_quotes(conn)? {
        return Ok(None);
    }
    let dir = dir(conn, default_dir)?;
    let recent = list(&dir)?
        .iter()
        .filter(|f| !f.safety)
        .filter_map(|f| f.modified)
        .max()
        .is_some_and(|ms| {
            let age = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
            age.saturating_sub(Duration::from_millis(ms as u64)) < AUTO_INTERVAL
        });
    if recent {
        return Ok(None);
    }
    backup_now(conn, default_dir).map(Some)
}

/// Date (ms) de la copie automatique ou manuelle la plus récente du dossier.
fn latest_copy(dir: &Path) -> Result<Option<i64>> {
    Ok(list(dir)?.iter().filter(|f| !f.safety).filter_map(|f| f.modified).max())
}

/// Copie du soir due ? Après 22 h sans copie depuis 22 h, ou plus de 24 h sans copie.
fn evening_due(last_ms: Option<i64>, now_ms: i64, evening_ms: i64) -> bool {
    match last_ms {
        None => true,
        Some(last) => (now_ms >= evening_ms && last < evening_ms) || now_ms - last >= MAX_GAP_MS,
    }
}

/// Vérifiée chaque minute pendant que l'appli tourne : copie du soir (22 h, heure locale) si elle
/// est due, que la sauvegarde automatique est activée et que la base a des devis.
pub fn evening_backup(conn: &Connection, default_dir: &Path) -> Result<Option<PathBuf>> {
    if setting(conn, AUTO_KEY)?.as_deref() == Some("0") || !has_quotes(conn)? {
        return Ok(None);
    }
    // 22 h aujourd'hui, heure locale, en ms depuis 1970 (SQLite connaît le fuseau du poste).
    let evening_ms: i64 = conn
        .query_row(
            "SELECT CAST(strftime('%s', date('now', 'localtime') || ' ' || ?1, 'utc') AS INTEGER) * 1000",
            [EVENING],
            |r| r.get(0),
        )
        .map_err(err)?;
    let now_ms = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    if !evening_due(latest_copy(&dir(conn, default_dir)?)?, now_ms, evening_ms) {
        return Ok(None);
    }
    backup_now(conn, default_dir).map(Some)
}

/// Copies du dossier, les plus récentes d'abord (les noms datés se trient chronologiquement).
pub fn list(dir: &Path) -> Result<Vec<BackupFile>> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(Vec::new()) };
    let mut files: Vec<BackupFile> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_backup = name.starts_with(PREFIX) && name.ends_with(".db");
            let meta = e.metadata().ok().filter(|m| m.is_file() && is_backup)?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            Some(BackupFile {
                path: e.path().to_string_lossy().into_owned(),
                safety: name.starts_with(SAFETY_PREFIX) || name.starts_with(UPDATE_PREFIX),
                before_update: name.starts_with(UPDATE_PREFIX),
                name,
                size: meta.len(),
                modified,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| b.name.cmp(&a.name)));
    Ok(files)
}

/// Supprime les copies automatiques / manuelles au-delà des `keep` plus récentes
/// (les copies de sécurité d'avant restauration ne sont jamais supprimées).
fn prune(dir: &Path, keep: usize) -> Result<()> {
    for old in list(dir)?.into_iter().filter(|f| !f.safety).skip(keep) {
        std::fs::remove_file(&old.path).map_err(|e| format!("Suppression de « {} » : {e}", old.name))?;
    }
    Ok(())
}

/// La copie doit être une base de devis lisible, intacte, et sans trigger ni vue : l'appli n'en
/// crée jamais, et un trigger caché s'exécuterait à chaque enregistrement.
fn check_copy(file: &Path) -> Result<()> {
    crate::files::check_size(file, crate::files::MAX_BACKUP)?;
    let src = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Copie illisible : {e}"))?;
    src.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get::<_, i64>(0))
        .map_err(|_| "Ce fichier n'est pas une sauvegarde de l'appli Devis.".to_string())?;
    let integrity: String = src
        .query_row("PRAGMA integrity_check(1)", [], |r| r.get(0))
        .map_err(|e| format!("Copie endommagée : {e}"))?;
    if integrity != "ok" {
        return Err(format!("Copie endommagée, restauration annulée : {integrity}"));
    }
    let extra: i64 = src
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type IN ('trigger', 'view')", [], |r| r.get(0))
        .map_err(err)?;
    if extra > 0 {
        return Err("Cette copie contient des éléments que l'appli ne crée jamais (triggers ou vues) : restauration refusée.".into());
    }
    Ok(())
}

/// Base vide au démarrage : la copie la plus récente qui contient des devis, à proposer.
pub fn restore_candidate(conn: &Connection, default_dir: &Path) -> Result<Option<(BackupFile, i64)>> {
    if has_quotes(conn)? {
        return Ok(None);
    }
    for file in list(&dir(conn, default_dir)?)? {
        let path = Path::new(&file.path);
        if check_copy(path).is_err() {
            continue;
        }
        let quotes = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|c| c.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get::<_, i64>(0)))
            .unwrap_or(0);
        if quotes > 0 {
            return Ok(Some((file, quotes)));
        }
    }
    Ok(None)
}

/// Remplace la base par la copie `file` (après une copie de sécurité de la base actuelle).
pub fn restore(conn: &mut Connection, default_dir: &Path, file: &Path) -> Result<PathBuf> {
    check_copy(file)?;

    let safety = copy_to(conn, &dir(conn, default_dir)?, SAFETY_PREFIX)?;
    conn.restore(MAIN_DB, file, None::<fn(rusqlite::backup::Progress)>)
        .map_err(|e| format!("Restauration : {e}"))?;
    // Copie faite par une version plus ancienne : mêmes mises à niveau qu'au démarrage.
    crate::db::init(conn).map_err(err)?;
    Ok(safety)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("devis-backup-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn quote_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn backup_keeps_latest_and_restores() {
        let root = temp_dir("restore");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let mut conn = crate::db::open(&db_path).unwrap();
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_keep', '2')", []).unwrap();

        let first = backup_now(&conn, &backups).unwrap();
        assert_eq!(first.parent().unwrap(), backups);
        backup_now(&conn, &backups).unwrap();
        backup_now(&conn, &backups).unwrap();
        let files = list(&backups).unwrap();
        assert_eq!(files.len(), 2, "seules les 2 plus récentes sont gardées");
        assert!(files.iter().all(|f| f.size > 0 && !f.safety));

        // Un devis de plus, puis restauration d'une copie : on revient à un seul devis.
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0002', '', '2026-10-10')", []).unwrap();
        let safety = restore(&mut conn, &backups, Path::new(&files[0].path)).unwrap();
        assert_eq!(quote_count(&conn), 1);
        // La copie de sécurité contient l'état d'avant (2 devis) et n'est pas supprimée par le tri.
        let before = Connection::open(&safety).unwrap();
        assert_eq!(quote_count(&before), 2);
        backup_now(&conn, &backups).unwrap();
        let files = list(&backups).unwrap();
        assert_eq!(files.iter().filter(|f| f.safety).count(), 1);
        assert_eq!(files.iter().filter(|f| !f.safety).count(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn auto_backup_once_per_day_and_can_be_disabled() {
        let root = temp_dir("auto");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let conn = crate::db::open(&db_path).unwrap();
        assert!(auto_backup(&conn, &backups).unwrap().is_none(), "base sans devis : pas de copie");
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        assert!(auto_backup(&conn, &backups).unwrap().is_some());
        assert!(auto_backup(&conn, &backups).unwrap().is_none(), "déjà une copie récente");
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_auto', '0')", []).unwrap();
        std::fs::remove_dir_all(&backups).unwrap();
        assert!(auto_backup(&conn, &backups).unwrap().is_none(), "désactivée");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_refuses_a_copy_with_a_trigger_or_damaged() {
        let root = temp_dir("trigger");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let mut conn = crate::db::open(&db_path).unwrap();
        let copy = backup_now(&conn, &backups).unwrap();
        Connection::open(&copy)
            .unwrap()
            .execute_batch("CREATE TRIGGER t AFTER INSERT ON quotes BEGIN DELETE FROM clients; END;")
            .unwrap();
        assert!(restore(&mut conn, &backups, &copy).unwrap_err().contains("triggers ou vues"));

        // Fichier tronqué : refusé, la base en place n'est pas touchée.
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        let copy = backup_now(&conn, &backups).unwrap();
        let bytes = std::fs::read(&copy).unwrap();
        std::fs::write(&copy, &bytes[..bytes.len() / 2]).unwrap();
        assert!(restore(&mut conn, &backups, &copy).is_err());
        assert_eq!(quote_count(&conn), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_refuses_a_foreign_file() {
        let root = temp_dir("foreign");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let mut conn = crate::db::open(&db_path).unwrap();
        let other = root.join("autre.db");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE t (x)").unwrap();
        assert!(restore(&mut conn, &backups, &other).unwrap_err().contains("pas une sauvegarde"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_base_never_prunes_and_is_offered_a_restore() {
        let root = temp_dir("empty");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let mut conn = crate::db::open(&db_path).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_keep', '2')", []).unwrap();
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        backup_now(&conn, &backups).unwrap();
        assert!(backup_before_update(&conn, &backups).unwrap().is_some());

        // Base vidée (dossier de l'appli effacé) : ses copies ne suppriment pas les bonnes.
        conn.execute("DELETE FROM quotes", []).unwrap();
        assert!(backup_before_update(&conn, &backups).unwrap().is_none(), "base vide : pas de copie avant mise à jour");
        for _ in 0..3 {
            backup_now(&conn, &backups).unwrap();
        }
        assert_eq!(list(&backups).unwrap().len(), 5);

        let (file, quotes) = restore_candidate(&conn, &backups).unwrap().expect("copie à proposer");
        assert_eq!(quotes, 1);
        assert!(file.before_update, "la copie pleine la plus récente (avant mise à jour)");
        restore(&mut conn, &backups, Path::new(&file.path)).unwrap();
        assert_eq!(quote_count(&conn), 1);
        assert!(restore_candidate(&conn, &backups).unwrap().is_none(), "base pleine : rien à proposer");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_backups_are_copied_to_the_new_folder() {
        let root = temp_dir("legacy");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let conn = crate::db::open(&db_path).unwrap();
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        let old = backup_now(&conn, &legacy_dir(&db_path)).unwrap();
        assert_eq!(migrate_legacy(&conn, &db_path, &backups).unwrap(), 1);
        assert!(backups.join(old.file_name().unwrap()).exists());
        assert!(old.exists(), "l'ancienne copie reste en place");
        assert_eq!(migrate_legacy(&conn, &db_path, &backups).unwrap(), 0, "déjà recopiée");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_before_upgrade_when_the_version_changes() {
        let root = temp_dir("upgrade");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        assert!(backup_before_upgrade(&db_path, &backups, "0.2.3").unwrap().is_none(), "pas encore de base");
        assert!(!db_path.exists(), "la base n'est pas créée ici");

        let conn = crate::db::open(&db_path).unwrap();
        assert!(backup_before_upgrade(&db_path, &backups, "0.2.3").unwrap().is_none(), "base sans devis");
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        // Base laissée par une version qui ne retenait pas la sienne : copie.
        let copy = backup_before_upgrade(&db_path, &backups, "0.2.3").unwrap().expect("copie");
        assert_eq!(quote_count(&Connection::open(&copy).unwrap()), 1);
        assert!(list(&backups).unwrap()[0].before_update);

        record_version(&conn, "0.2.3").unwrap();
        assert!(backup_before_upgrade(&db_path, &backups, "0.2.3").unwrap().is_none(), "même version");
        assert!(backup_before_upgrade(&db_path, &backups, "0.2.4").unwrap().is_some(), "nouvelle version");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn evening_backup_after_22h_or_after_24h_without_copy() {
        const H: i64 = 3600 * 1000;
        let evening = 1_000 * H; // 22 h aujourd'hui
        assert!(evening_due(None, evening - 5 * H, evening), "aucune copie : tout de suite");
        assert!(!evening_due(Some(evening - 3 * H), evening - H, evening), "avant 22 h, copie récente");
        assert!(evening_due(Some(evening - 3 * H), evening + 60_000, evening), "22 h passées, dernière copie avant 22 h");
        assert!(!evening_due(Some(evening + 60_000), evening + 2 * H, evening), "déjà faite ce soir");
        // Ce matin à 8 h (22 h − 14 h), après une veille hier à 22 h : dernière copie avant-hier à 6 h.
        assert!(evening_due(Some(evening - 40 * H), evening - 14 * H, evening), "plus de 24 h sans copie");
        assert!(!evening_due(Some(evening - 24 * H), evening - 14 * H, evening), "copie d'hier soir : on attend ce soir");

        let root = temp_dir("evening");
        let db_path = root.join("devis.db");
        let backups = root.join("Documents").join("sauvegardes");
        let conn = crate::db::open(&db_path).unwrap();
        assert!(evening_backup(&conn, &backups).unwrap().is_none(), "base sans devis");
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        assert!(evening_backup(&conn, &backups).unwrap().is_some(), "aucune copie encore");
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_auto', '0')", []).unwrap();
        std::fs::remove_dir_all(&backups).unwrap();
        assert!(evening_backup(&conn, &backups).unwrap().is_none(), "désactivée");
        let _ = std::fs::remove_dir_all(&root);
    }
}
