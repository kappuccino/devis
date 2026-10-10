//! Accès aux fichiers depuis l'interface : l'appli ne lit, n'écrit et n'ouvre que des fichiers
//! choisis par l'utilisateur.
//!
//! Les boîtes de dialogue « Ouvrir » et « Enregistrer sous » sont affichées ici, côté Rust (et non
//! plus depuis la page) : les chemins choisis sont mémorisés pour la session, et les commandes qui
//! touchent au disque n'acceptent que ceux-là, ou ceux déjà enregistrés dans les réglages (dossier
//! de la documentation, gabarit Excel, PDF des CGV…) et dans les documents joints aux devis.
//! Ainsi, même un script injecté dans la page ne pourrait ni écrire où il veut, ni lire un fichier
//! quelconque, ni lancer un programme.

use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

type CmdResult<T> = Result<T, String>;

/// Réglages qui contiennent un chemin (fichier ou dossier).
pub const PATH_SETTINGS: [&str; 4] = ["docs_folder", "backup_dir", "price_list_template", "cgv_pdf_path"];

/// Fichiers que l'appli ouvre avec l'application par défaut (jamais un programme ou un script).
const OPENABLE: [&str; 6] = ["pdf", "xlsx", "xlsm", "png", "jpg", "jpeg"];

/// Chemins autorisés pendant la session.
#[derive(Default)]
pub struct FileAccess {
    /// Choisis dans « Ouvrir » (fichiers ou dossiers) : lecture.
    opened: Mutex<HashSet<PathBuf>>,
    /// Choisis dans « Enregistrer sous » : écriture.
    save_targets: Mutex<HashSet<PathBuf>>,
    /// Écrits par l'appli : peuvent être ouverts ensuite (PDF du devis, export Excel…).
    written: Mutex<HashSet<PathBuf>>,
}

fn set(m: &Mutex<HashSet<PathBuf>>) -> std::sync::MutexGuard<'_, HashSet<PathBuf>> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl FileAccess {
    fn was_opened(&self, path: &Path) -> bool {
        set(&self.opened).contains(&normalize(path))
    }

    /// Chemin d'écriture choisi dans « Enregistrer sous » ; erreur sinon.
    pub fn check_save_target(&self, path: &str) -> CmdResult<PathBuf> {
        let p = normalize(Path::new(path));
        if set(&self.save_targets).contains(&p) {
            Ok(p)
        } else {
            Err(format!("Écriture refusée : « {path} » n'a pas été choisi dans « Enregistrer sous »."))
        }
    }

    /// Fichier choisi dans « Ouvrir » ; erreur sinon.
    pub fn check_opened(&self, path: &str) -> CmdResult<PathBuf> {
        if self.was_opened(Path::new(path)) {
            Ok(PathBuf::from(path))
        } else {
            Err(format!("Accès refusé : « {path} » n'a pas été choisi dans une boîte de dialogue."))
        }
    }

    /// Autorise un chemin connu de l'appli elle-même (ex. dossier de l'ancienne appli PDF Finder).
    pub fn allow_opened(&self, path: &Path) {
        set(&self.opened).insert(normalize(path));
    }

    pub fn mark_written(&self, path: &Path) {
        set(&self.written).insert(normalize(path));
    }

    /// Lecture autorisée : fichier choisi, dossier de la documentation, fichier des réglages,
    /// document joint à un devis, ou dossier `extra` (glisser-déposer).
    pub fn check_read(&self, conn: &Connection, path: &str, extra: &Path) -> CmdResult<PathBuf> {
        let p = normalize(Path::new(path));
        let allowed = set(&self.opened).iter().any(|o| p.starts_with(o))
            || p.starts_with(normalize(extra))
            || setting_path(conn, "docs_folder").is_some_and(|d| p.starts_with(d))
            || ["price_list_template", "cgv_pdf_path"].iter().any(|k| setting_path(conn, k).as_ref() == Some(&p))
            || attachment_paths(conn).iter().any(|a| normalize(Path::new(a)) == p);
        if allowed {
            Ok(PathBuf::from(path))
        } else {
            Err(format!("Lecture refusée : « {path} » n'est pas un fichier choisi dans l'appli."))
        }
    }

    /// Dossier à parcourir : celui de la documentation, ou un dossier choisi.
    pub fn check_folder(&self, conn: &Connection, dir: &str) -> CmdResult<PathBuf> {
        let p = normalize(Path::new(dir));
        if self.was_opened(&p) || setting_path(conn, "docs_folder").as_ref() == Some(&p) {
            Ok(PathBuf::from(dir))
        } else {
            Err(format!("Accès refusé au dossier « {dir} »."))
        }
    }

    /// Nouvelle valeur d'un réglage de chemin : vide, inchangée, ou choisie dans une boîte de dialogue.
    pub fn check_setting(&self, conn: &Connection, key: &str, value: &str) -> CmdResult<()> {
        if !PATH_SETTINGS.contains(&key) || value.trim().is_empty() {
            return Ok(());
        }
        let current: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if current.as_deref() == Some(value) || self.was_opened(Path::new(value.trim())) {
            Ok(())
        } else {
            Err(format!("Réglage « {key} » refusé : choisissez le chemin avec le bouton prévu."))
        }
    }

    /// Document externe joint à un devis : choisi dans « Ouvrir », ou déjà joint à un devis.
    pub fn check_attachment(&self, conn: &Connection, path: &str) -> CmdResult<()> {
        let p = normalize(Path::new(path));
        if self.was_opened(&p) || attachment_paths(conn).iter().any(|a| normalize(Path::new(a)) == p) {
            Ok(())
        } else {
            Err(format!("Document refusé : « {path} » n'a pas été choisi dans l'appli."))
        }
    }
}

/// Chemin absolu sans `.` ni `..`, liens résolus quand le fichier (ou son dossier) existe, pour
/// comparer des chemins écrits différemment.
pub fn normalize(path: &Path) -> PathBuf {
    if let Ok(p) = std::fs::canonicalize(path) {
        return p;
    }
    // Fichier pas encore créé (« Enregistrer sous ») : dossier résolu + nom.
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(p) = std::fs::canonicalize(parent) {
            return p.join(name);
        }
    }
    // Sinon : nettoyage lexical (`..` retiré avec le composant précédent).
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn setting_path(conn: &Connection, key: &str) -> Option<PathBuf> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get::<_, Option<String>>(0))
        .optional()
        .ok()
        .flatten()
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| normalize(Path::new(&v)))
}

fn attachment_paths(conn: &Connection) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare("SELECT DISTINCT path FROM quote_attachments WHERE source = 'external'") else {
        return Vec::new();
    };
    stmt.query_map([], |r| r.get(0)).map(|rows| rows.filter_map(Result::ok).collect()).unwrap_or_default()
}

// ---------- Boîtes de dialogue ----------

#[derive(Deserialize)]
pub struct Filter {
    name: String,
    extensions: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PickOptions {
    title: Option<String>,
    /// Dossier de départ, ou chemin proposé (« Enregistrer sous » : dossier + nom de fichier).
    default_path: Option<String>,
    filters: Vec<Filter>,
    directory: bool,
    multiple: bool,
}

fn builder(window: &WebviewWindow, o: &PickOptions, save: bool) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    let mut b = window.dialog().file().set_parent(window);
    if let Some(t) = &o.title {
        b = b.set_title(t);
    }
    for f in &o.filters {
        let exts: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
        b = b.add_filter(&f.name, &exts);
    }
    if let Some(d) = o.default_path.as_deref().filter(|d| !d.is_empty()) {
        let p = Path::new(d);
        if save {
            if let Some(name) = p.file_name() {
                b = b.set_file_name(name.to_string_lossy());
            }
            if let Some(parent) = p.parent().filter(|p| p.is_dir()) {
                b = b.set_directory(parent);
            }
        } else if p.is_dir() {
            b = b.set_directory(p);
        }
    }
    b
}

fn to_path(f: tauri_plugin_dialog::FilePath) -> Option<PathBuf> {
    f.into_path().ok()
}

/// « Ouvrir » : fichier(s) ou dossier ; chemins choisis (vide si annulé).
#[tauri::command]
pub async fn pick_open(window: WebviewWindow, files: State<'_, FileAccess>, options: PickOptions) -> CmdResult<Vec<String>> {
    let w = window.clone();
    let picked: Vec<PathBuf> = tauri::async_runtime::spawn_blocking(move || {
        let b = builder(&w, &options, false);
        if options.directory {
            b.blocking_pick_folder().and_then(to_path).into_iter().collect()
        } else if options.multiple {
            b.blocking_pick_files().unwrap_or_default().into_iter().filter_map(to_path).collect()
        } else {
            b.blocking_pick_file().and_then(to_path).into_iter().collect()
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut opened = set(&files.opened);
    for p in &picked {
        opened.insert(normalize(p));
    }
    Ok(picked.iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

/// « Enregistrer sous » ; None si annulé.
#[tauri::command]
pub async fn pick_save(window: WebviewWindow, files: State<'_, FileAccess>, options: PickOptions) -> CmdResult<Option<String>> {
    let w = window.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || builder(&w, &options, true).blocking_save_file().and_then(to_path))
        .await
        .map_err(|e| e.to_string())?;
    if let Some(p) = &picked {
        set(&files.save_targets).insert(normalize(p));
    }
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

/// Ouvre avec l'application par défaut un fichier écrit par l'appli (PDF, Excel…), ou le
/// dossier des sauvegardes. Jamais un programme ni un script.
#[tauri::command]
pub fn open_file(app: AppHandle, files: State<'_, FileAccess>, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let p = normalize(Path::new(&path));
    let backup_dir = crate::backup::dir(&state.conn(), &state.db_path).map(|d| normalize(&d)).ok();
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let allowed = (set(&files.written).contains(&p) && OPENABLE.contains(&ext.as_str()))
        || (backup_dir.as_ref() == Some(&p) && p.is_dir());
    if !allowed {
        return Err(format!("Ouverture refusée : « {path} »."));
    }
    app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::init(&c).unwrap();
        c
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("devis-files-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("docs/sous")).unwrap();
        std::fs::write(d.join("docs/sous/a.pdf"), b"%PDF").unwrap();
        std::fs::write(d.join("secret.txt"), b"x").unwrap();
        d
    }

    #[test]
    fn normalize_removes_dot_dot() {
        let d = temp_dir("norm");
        let twisted = d.join("docs/sous/../../secret.txt");
        assert_eq!(normalize(&twisted), normalize(&d.join("secret.txt")));
        // Fichier à créer : dossier résolu + nom.
        assert_eq!(normalize(&d.join("docs/../nouveau.pdf")), normalize(&d).join("nouveau.pdf"));
    }

    #[test]
    fn reads_only_chosen_or_configured_files() {
        let d = temp_dir("read");
        let c = conn();
        let files = FileAccess::default();
        let drag = d.join("drag");
        let doc = d.join("docs/sous/a.pdf");
        let secret = d.join("secret.txt");
        let s = |p: &Path| p.to_string_lossy().into_owned();

        assert!(files.check_read(&c, &s(&doc), &drag).is_err());
        // Dossier de la documentation (réglage) : ses fichiers, pas au-delà (même avec « .. »).
        c.execute("INSERT INTO settings (key, value) VALUES ('docs_folder', ?1)", [s(&d.join("docs"))]).unwrap();
        assert!(files.check_read(&c, &s(&doc), &drag).is_ok());
        assert!(files.check_read(&c, &s(&d.join("docs/../secret.txt")), &drag).is_err());
        assert!(files.check_folder(&c, &s(&d.join("docs"))).is_ok());
        assert!(files.check_folder(&c, &s(&d)).is_err());
        // Fichier choisi dans « Ouvrir ».
        set(&files.opened).insert(normalize(&secret));
        assert!(files.check_read(&c, &s(&secret), &drag).is_ok());
    }

    #[test]
    fn writes_only_to_save_targets_and_settings_need_a_dialog() {
        let d = temp_dir("write");
        let c = conn();
        let files = FileAccess::default();
        let target = d.join("Devis.pdf");
        let s = |p: &Path| p.to_string_lossy().into_owned();
        assert!(files.check_save_target(&s(&target)).is_err());
        set(&files.save_targets).insert(normalize(&target));
        assert!(files.check_save_target(&s(&target)).is_ok());
        assert!(files.check_save_target(&s(&d.join("autre.pdf"))).is_err());

        // Réglage de chemin : refusé s'il n'a pas été choisi ; vide ou inchangé, accepté.
        assert!(files.check_setting(&c, "backup_dir", &s(&d)).is_err());
        assert!(files.check_setting(&c, "backup_dir", "").is_ok());
        assert!(files.check_setting(&c, "company_name", "n'importe quoi").is_ok());
        set(&files.opened).insert(normalize(&d));
        assert!(files.check_setting(&c, "backup_dir", &s(&d)).is_ok());
        c.execute("INSERT INTO settings (key, value) VALUES ('cgv_pdf_path', '/ancien/cgv.pdf')", []).unwrap();
        assert!(files.check_setting(&c, "cgv_pdf_path", "/ancien/cgv.pdf").is_ok());
    }
}
