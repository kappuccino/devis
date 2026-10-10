//! Sauvegarde de la base des devis (`devis.db`) : copies datées dans un dossier choisi
//! (par défaut `sauvegardes/` à côté de la base), automatiques au démarrage (au plus une fois par
//! 20 h) ou à la demande, en gardant les N plus récentes ; restauration d'une copie.
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
pub const DEFAULT_KEEP: usize = 10;
/// Copie automatique si la plus récente a plus de 20 h (une par jour de travail).
const AUTO_INTERVAL: Duration = Duration::from_secs(20 * 3600);

const PREFIX: &str = "devis-";
const SAFETY_PREFIX: &str = "devis-avant-restauration-";

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
    /// Copie de sécurité faite avant une restauration.
    pub safety: bool,
}

fn setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(err)
}

/// Dossier des sauvegardes : réglage, sinon `sauvegardes/` à côté de la base.
pub fn dir(conn: &Connection, db_path: &Path) -> Result<PathBuf> {
    Ok(match setting(conn, DIR_KEY)?.filter(|d| !d.trim().is_empty()) {
        Some(d) => PathBuf::from(d.trim()),
        None => default_dir(db_path),
    })
}

pub fn default_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("sauvegardes")
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

/// Sauvegarde maintenant, puis ne garde que les N copies les plus récentes.
pub fn backup_now(conn: &Connection, db_path: &Path) -> Result<PathBuf> {
    let dir = dir(conn, db_path)?;
    let path = copy_to(conn, &dir, PREFIX)?;
    prune(&dir, keep(conn)?)?;
    Ok(path)
}

/// Sauvegarde au démarrage : si elle est activée et que la dernière copie a plus de 20 h.
pub fn auto_backup(conn: &Connection, db_path: &Path) -> Result<Option<PathBuf>> {
    if setting(conn, AUTO_KEY)?.as_deref() == Some("0") {
        return Ok(None);
    }
    let dir = dir(conn, db_path)?;
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
    backup_now(conn, db_path).map(Some)
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
                safety: name.starts_with(SAFETY_PREFIX),
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

/// Remplace la base par la copie `file` (après une copie de sécurité de la base actuelle).
pub fn restore(conn: &mut Connection, db_path: &Path, file: &Path) -> Result<PathBuf> {
    // La copie doit être une base de devis lisible.
    let src = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Copie illisible : {e}"))?;
    src.query_row("SELECT COUNT(*) FROM quotes", [], |r| r.get::<_, i64>(0))
        .map_err(|_| "Ce fichier n'est pas une sauvegarde de l'appli Devis.".to_string())?;
    drop(src);

    let safety = copy_to(conn, &dir(conn, db_path)?, SAFETY_PREFIX)?;
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
        let mut conn = crate::db::open(&db_path).unwrap();
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0001', '', '2026-10-10')", []).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_keep', '2')", []).unwrap();

        let first = backup_now(&conn, &db_path).unwrap();
        assert_eq!(first.parent().unwrap(), default_dir(&db_path));
        backup_now(&conn, &db_path).unwrap();
        backup_now(&conn, &db_path).unwrap();
        let files = list(&default_dir(&db_path)).unwrap();
        assert_eq!(files.len(), 2, "seules les 2 plus récentes sont gardées");
        assert!(files.iter().all(|f| f.size > 0 && !f.safety));

        // Un devis de plus, puis restauration d'une copie : on revient à un seul devis.
        conn.execute("INSERT INTO quotes (number, client_code, date) VALUES ('26-A-0002', '', '2026-10-10')", []).unwrap();
        let safety = restore(&mut conn, &db_path, Path::new(&files[0].path)).unwrap();
        assert_eq!(quote_count(&conn), 1);
        // La copie de sécurité contient l'état d'avant (2 devis) et n'est pas supprimée par le tri.
        let before = Connection::open(&safety).unwrap();
        assert_eq!(quote_count(&before), 2);
        backup_now(&conn, &db_path).unwrap();
        let files = list(&default_dir(&db_path)).unwrap();
        assert_eq!(files.iter().filter(|f| f.safety).count(), 1);
        assert_eq!(files.iter().filter(|f| !f.safety).count(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn auto_backup_once_per_day_and_can_be_disabled() {
        let root = temp_dir("auto");
        let db_path = root.join("devis.db");
        let conn = crate::db::open(&db_path).unwrap();
        assert!(auto_backup(&conn, &db_path).unwrap().is_some());
        assert!(auto_backup(&conn, &db_path).unwrap().is_none(), "déjà une copie récente");
        conn.execute("INSERT INTO settings (key, value) VALUES ('backup_auto', '0')", []).unwrap();
        std::fs::remove_dir_all(default_dir(&db_path)).unwrap();
        assert!(auto_backup(&conn, &db_path).unwrap().is_none(), "désactivée");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_refuses_a_foreign_file() {
        let root = temp_dir("foreign");
        let db_path = root.join("devis.db");
        let mut conn = crate::db::open(&db_path).unwrap();
        let other = root.join("autre.db");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE t (x)").unwrap();
        assert!(restore(&mut conn, &db_path, &other).unwrap_err().contains("pas une sauvegarde"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
