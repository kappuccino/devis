mod backup;
mod commands;
mod db;
mod docs;
mod files;
mod import;
mod licence;
mod pricing;
mod quote_file;
mod shipping;
mod stats;
mod xlsx_export;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::Manager;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub db_path: PathBuf,
    /// Dossier des sauvegardes par défaut (Documents), si aucun n'est choisi dans les Réglages.
    pub backup_dir: PathBuf,
}

impl AppState {
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Sans licence valide pour ce poste, seules les commandes `licence_*` (écran de connexion) sont
/// acceptées ; les autres sont refusées avant d'atteindre la base.
fn licensed<R: tauri::Runtime>(
    handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        let allowed = invoke.message.command().starts_with("licence_")
            || invoke.message.webview().state::<licence::Licence>().is_valid();
        if !allowed {
            invoke.resolver.reject("Licence requise : reconnectez-vous.");
            return true;
        }
        handler(invoke)
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
            // Sauvegardes hors du dossier de l'appli (une désinstallation peut l'effacer en entier) ;
            // celles de l'ancien dossier y sont recopiées.
            let backup_dir = backup::default_dir(app.path().document_dir().ok(), &db_path);
            // Nouvelle version : copie de la base avant que `db::open` ne la mette à niveau.
            let version = app.package_info().version.to_string();
            if let Err(e) = backup::backup_before_upgrade(&db_path, &backup_dir, &version) {
                eprintln!("Sauvegarde avant mise à niveau : {e}");
            }
            let conn = db::open(&db_path)?;
            if let Err(e) = backup::record_version(&conn, &version) {
                eprintln!("Version de l'appli : {e}");
            }
            if let Err(e) = backup::migrate_legacy(&conn, &db_path, &backup_dir) {
                eprintln!("Sauvegardes de l'ancien dossier : {e}");
            }
            let db = Arc::new(Mutex::new(conn));
            // Sauvegarde automatique (au plus une par 20 h), sans retarder l'ouverture.
            {
                let (db, backup_dir) = (db.clone(), backup_dir.clone());
                std::thread::spawn(move || {
                    {
                        let conn = db.lock().unwrap_or_else(|p| p.into_inner());
                        if let Err(e) = backup::auto_backup(&conn, &backup_dir) {
                            eprintln!("Sauvegarde automatique : {e}");
                        }
                    }
                    // L'appli reste ouverte des jours entiers : copie du soir, vérifiée chaque minute.
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        let conn = db.lock().unwrap_or_else(|p| p.into_inner());
                        if let Err(e) = backup::evening_backup(&conn, &backup_dir) {
                            eprintln!("Sauvegarde du soir : {e}");
                        }
                    }
                });
            }
            app.manage(AppState { db, db_path, backup_dir });
            // Licence du poste (ticket enregistré dans le même dossier).
            app.manage(licence::Licence::open(&dir));
            // Chemins choisis par l'utilisateur pendant la session (seuls accessibles depuis la page).
            app.manage(files::FileAccess::default());
            // Index de la documentation technique (base séparée, reconstructible).
            app.manage(docs::open(app.handle())?);
            Ok(())
        })
        .invoke_handler(licensed(tauri::generate_handler![
            licence::licence_status,
            licence::licence_ticket,
            licence::licence_login,
            licence::licence_refresh,
            licence::licence_logout,
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
            files::pick_open,
            files::pick_save,
            files::open_file,
            commands::list_products,
            commands::search_products,
            commands::list_clients,
            commands::get_client,
            commands::update_client,
            commands::set_client_price_list,
            commands::export_price_list,
            commands::quote_stats,
            commands::product_price_history,
            commands::search_quoted_refs,
            commands::backup_info,
            commands::backup_now,
            commands::restore_backup,
            commands::backup_before_update,
            commands::restore_candidate,
            commands::export_quote,
            commands::preview_quote_files,
            commands::import_quotes,
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
            commands::set_quote_comment,
            commands::new_quote_version,
            commands::list_quote_templates,
            commands::save_as_template,
            commands::get_settings,
            commands::save_settings,
            commands::db_stats,
            commands::import_lpn,
            commands::read_shipping_rates,
            commands::save_file,
        ]))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
