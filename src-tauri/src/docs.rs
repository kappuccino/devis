//! Documentation technique (ex-« PDF Finder ») : accès à l'index et aux fichiers.
//!
//! Toute la logique (extraction du texte, indexation, recherche) est en JS dans `src/docs/core`.
//! Ici seulement :
//! - une base SQLite séparée (`docs-index.db`, simple cache reconstructible), exposée au JS
//!   par trois commandes génériques `exec` / `run` / `all` (interface `DbAdapter` du cœur) ;
//! - la liste des fichiers d'un dossier, la lecture et la copie de fichiers ;
//! - un dossier temporaire pour le glisser-déposer de pages vers d'autres applications.

use rusqlite::types::{Value, ValueRef};
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use serde_json::{Map, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use tauri::{ipc::Response, AppHandle, Manager, State};

use crate::files::FileAccess;
use crate::AppState;

pub struct DocsState {
    pub db: Arc<Mutex<Connection>>,
    pub db_path: PathBuf,
    /// Fichiers temporaires du glisser-déposer (vidé à chaque lancement).
    pub drag_dir: PathBuf,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn open(app: &AppHandle) -> Result<DocsState, Box<dyn std::error::Error>> {
    let data = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data)?;
    let db_path = data.join("docs-index.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    // Le SQL vient de la page : pas d'autre base attachée (ex. la base des devis).
    conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0)?;

    let drag_dir = app.path().app_cache_dir()?.join("drag");
    let _ = std::fs::remove_dir_all(&drag_dir);
    std::fs::create_dir_all(&drag_dir)?;

    Ok(DocsState { db: Arc::new(Mutex::new(conn)), db_path, drag_dir })
}

/// Instructions refusées dans le SQL envoyé par la page : attacher une autre base, ou écrire une
/// copie de l'index ailleurs (`VACUUM INTO`).
fn check_sql(sql: &str) -> CmdResult<()> {
    let lower = sql.to_lowercase();
    let forbidden = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|w| ["attach", "detach", "vacuum", "load_extension"].contains(w));
    match forbidden {
        Some(w) => Err(format!("Instruction SQL refusée sur l'index de la documentation : {w}")),
        None => Ok(()),
    }
}

/// Exécute une requête SQL sur l'index, hors du thread principal.
async fn with_db<T: Send + 'static>(
    state: &State<'_, DocsState>,
    sql: &str,
    f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
) -> CmdResult<T> {
    check_sql(sql)?;
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.lock().unwrap_or_else(|p| p.into_inner());
        f(&conn).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Paramètre JSON → valeur SQLite.
fn to_sql(v: Json) -> Value {
    match v {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Integer(b as i64),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or(0.0)),
        },
        Json::String(s) => Value::Text(s),
        other => Value::Text(other.to_string()),
    }
}

/// Valeur SQLite → JSON.
fn to_json(v: ValueRef) -> Json {
    match v {
        ValueRef::Null => Json::Null,
        ValueRef::Integer(i) => Json::from(i),
        ValueRef::Real(f) => Json::from(f),
        ValueRef::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Json::from(b.to_vec()),
    }
}

#[tauri::command]
pub async fn docs_db_exec(state: State<'_, DocsState>, sql: String) -> CmdResult<()> {
    with_db(&state, &sql.clone(), move |c| c.execute_batch(&sql)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    last_insert_id: i64,
    changes: usize,
}

#[tauri::command]
pub async fn docs_db_run(state: State<'_, DocsState>, sql: String, params: Vec<Json>) -> CmdResult<RunResult> {
    with_db(&state, &sql.clone(), move |c| {
        let changes = c.prepare_cached(&sql)?.execute(params_from_iter(params.into_iter().map(to_sql)))?;
        Ok(RunResult { last_insert_id: c.last_insert_rowid(), changes })
    })
    .await
}

#[tauri::command]
pub async fn docs_db_all(state: State<'_, DocsState>, sql: String, params: Vec<Json>) -> CmdResult<Vec<Map<String, Json>>> {
    with_db(&state, &sql.clone(), move |c| query_rows(c, &sql, params)).await
}

fn query_rows(c: &Connection, sql: &str, params: Vec<Json>) -> rusqlite::Result<Vec<Map<String, Json>>> {
    let mut stmt = c.prepare_cached(sql)?;
    let names: Vec<String> = stmt.column_names().into_iter().map(str::to_string).collect();
    let mut rows = stmt.query(params_from_iter(params.into_iter().map(to_sql)))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut obj = Map::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            obj.insert(name.clone(), to_json(row.get_ref(i)?));
        }
        out.push(obj);
    }
    Ok(out)
}

#[tauri::command]
pub fn docs_index_path(state: State<'_, DocsState>) -> String {
    state.db_path.display().to_string()
}

// ---------- Fichiers ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocFile {
    path: String,
    /// Date de création (ms) ; date de modification si le système ne la fournit pas.
    created_at: Option<i64>,
}

const EXTENSIONS: [&str; 4] = ["pdf", "jpg", "jpeg", "png"];

/// PDF et images (jpg/png) du dossier, sous-dossiers compris, triés par nom ; fichiers cachés ignorés.
#[tauri::command]
pub async fn docs_list_files(
    state: State<'_, AppState>,
    files: State<'_, FileAccess>,
    dir: String,
) -> CmdResult<Vec<DocFile>> {
    files.check_folder(&state.conn(), &dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        walk(Path::new(&dir), &mut out).map_err(|e| format!("Lecture du dossier « {dir} » : {e}"))?;
        Ok(out)
    })
    .await
    .map_err(err)?
}

fn walk(dir: &Path, out: &mut Vec<DocFile>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            // Un sous-dossier illisible n'empêche pas d'indexer le reste.
            let _ = walk(&path, out);
        } else if meta.is_file() {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if EXTENSIONS.contains(&ext.as_str()) {
                let time = meta.created().or_else(|_| meta.modified()).ok();
                let created_at = time
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);
                out.push(DocFile { path: path.to_string_lossy().into_owned(), created_at });
            }
        }
    }
    Ok(())
}

/// Contenu brut d'un fichier (renvoyé en binaire, sans passer par du JSON).
#[tauri::command]
pub async fn read_file(
    state: State<'_, AppState>,
    files: State<'_, FileAccess>,
    docs: State<'_, DocsState>,
    path: String,
) -> CmdResult<Response> {
    files.check_read(&state.conn(), &path, &docs.drag_dir)?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::read(&path).map(Response::new).map_err(|e| format!("Lecture de « {path} » : {e}"))
    })
    .await
    .map_err(err)?
}

/// Indique, pour chaque chemin, si le fichier existe encore.
#[tauri::command]
pub fn files_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| Path::new(p).is_file()).collect()
}

#[tauri::command]
pub async fn copy_file(
    state: State<'_, AppState>,
    files: State<'_, FileAccess>,
    docs: State<'_, DocsState>,
    from: String,
    to: String,
) -> CmdResult<()> {
    files.check_read(&state.conn(), &from, &docs.drag_dir)?;
    let target = files.check_save_target(&to)?;
    let t = target.clone();
    tauri::async_runtime::spawn_blocking(move || std::fs::copy(&from, &t).map(|_| ()).map_err(err))
        .await
        .map_err(err)??;
    files.mark_written(&target);
    Ok(())
}

/// Écrit un fichier temporaire pour le glisser-déposer et renvoie son chemin.
#[tauri::command]
pub fn write_drag_file(state: State<'_, DocsState>, name: String, contents: Vec<u8>) -> CmdResult<String> {
    let safe: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c }).collect();
    let path = state.drag_dir.join(safe);
    std::fs::write(&path, contents).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Dossier choisi dans l'ancienne application PDF Finder, s'il existe sur ce poste
/// (repris au premier lancement de la documentation).
#[tauri::command]
pub fn docs_legacy_folder(app: AppHandle, files: State<'_, FileAccess>) -> Option<String> {
    let settings = app.path().data_dir().ok()?.join("org.kappuccino.pdfref").join("settings.json");
    let json: Json = serde_json::from_str(&std::fs::read_to_string(settings).ok()?).ok()?;
    let dir = json.get("pdfDir")?.as_str()?.to_string();
    // Choisi par l'utilisateur dans PDF Finder : repris comme s'il venait d'être choisi ici.
    Path::new(&dir).is_dir().then(|| files.allow_opened(Path::new(&dir)))?;
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sqlite_has_fts5_trigram() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE VIRTUAL TABLE t USING fts5(x, tokenize = 'trigram'); INSERT INTO t VALUES ('REFAB1234X');")
            .unwrap();
        let n: i64 = c.query_row("SELECT COUNT(*) FROM t WHERE x MATCH '\"1234\"'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn page_sql_cannot_attach_or_copy_the_index() {
        assert!(check_sql("SELECT * FROM docs WHERE path = ?").is_ok());
        assert!(check_sql("ATTACH DATABASE '/x/devis.db' AS d").is_err());
        assert!(check_sql("select 1; vacuum into '/tmp/x.db'").is_err());
        // Un nom de table ou de colonne qui contient le mot reste permis.
        assert!(check_sql("SELECT attachments, vacuum_at FROM t").is_ok());
        // Garde-fou supplémentaire : aucune base attachée possible sur la connexion.
        let c = Connection::open_in_memory().unwrap();
        c.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0).unwrap();
        assert!(c.execute_batch("ATTACH ':memory:' AS autre").is_err());
    }

    #[test]
    fn rows_and_params_round_trip() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE d (id INTEGER PRIMARY KEY, path TEXT, n REAL, created INTEGER)").unwrap();
        c.prepare("INSERT INTO d (path, n, created) VALUES (?, ?, ?)")
            .unwrap()
            .execute(params_from_iter([Json::from("/a é.pdf"), Json::from(1.5), Json::Null].map(to_sql)))
            .unwrap();
        let rows = query_rows(&c, "SELECT * FROM d WHERE path = ?", vec![Json::from("/a é.pdf")]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["path"], "/a é.pdf");
        assert_eq!(rows[0]["n"], 1.5);
        assert_eq!(rows[0]["created"], Json::Null);
        assert_eq!(rows[0]["id"], 1);
    }
}
