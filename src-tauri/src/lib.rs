mod backup;
mod commands;
mod db;
mod docs;
mod import;
mod pricing;
mod stats;
mod xlsx_export;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::Manager;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub db_path: PathBuf,
}

impl AppState {
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Mémorise taille, position et état (plein écran…) de la fenêtre d'un lancement à l'autre.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // Glisser une page ou un fichier de la documentation vers une autre application.
        .plugin(tauri_plugin_drag::init())
        // Mises à jour depuis les releases GitHub (signées), puis redémarrage.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // ~/Library/Application Support/fr.devis.app/devis.db sur macOS
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("devis.db");
            let conn = db::open(&db_path)?;
            let db = Arc::new(Mutex::new(conn));
            // Sauvegarde automatique (au plus une par 20 h), sans retarder l'ouverture.
            {
                let (db, db_path) = (db.clone(), db_path.clone());
                std::thread::spawn(move || {
                    let conn = db.lock().unwrap_or_else(|p| p.into_inner());
                    if let Err(e) = backup::auto_backup(&conn, &db_path) {
                        eprintln!("Sauvegarde automatique : {e}");
                    }
                });
            }
            app.manage(AppState { db, db_path });
            // Index de la documentation technique (base séparée, reconstructible).
            app.manage(docs::open(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            docs::docs_db_exec,
            docs::docs_db_run,
            docs::docs_db_all,
            docs::docs_index_path,
            docs::docs_list_files,
            docs::docs_legacy_folder,
            docs::read_file,
            docs::copy_file,
            docs::files_exist,
            commands::get_quote_attachments,
            commands::save_quote_attachments,
            docs::write_drag_file,
            commands::list_products,
            commands::search_products,
            commands::list_clients,
            commands::get_client,
            commands::update_client,
            commands::set_client_price_list,
            commands::export_price_list,
            commands::quote_stats,
            commands::product_price_history,
            commands::backup_info,
            commands::backup_now,
            commands::restore_backup,
            commands::list_price_lists,
            commands::get_price_list_items,
            commands::get_price_list_clients,
            commands::resolve_price,
            commands::list_quotes,
            commands::get_quote,
            commands::save_quote,
            commands::delete_quote,
            commands::duplicate_quote,
            commands::set_quote_won,
            commands::get_settings,
            commands::save_settings,
            commands::db_stats,
            commands::import_lpn,
            commands::save_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
