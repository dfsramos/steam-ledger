// pub, not mod: needed by src/bin/steam_import_debug.rs (a throwaway
// diagnostic tool, not part of the shipped app — see that file's doc
// comment). Revert to private once the live Steam-import path is confirmed
// working end-to-end and the debug tool is removed.
pub mod appinfo;
mod commands;
pub mod credentials;
mod currency;
pub mod date_infer;
pub mod db;
pub mod help_wizard;
pub mod inventory;
mod models;
mod progress;
pub mod reconcile;
pub mod steam;
pub mod steam_history;
pub mod steam_login;
pub mod steamid;
pub mod store_history;
pub mod store_reconcile;
pub mod ua;

use tauri::Manager;

/// Installs an app-wide panic hook that appends the panic message and
/// location to `~/.steamledger/crash.log` before falling through to the
/// default hook (which still prints to stderr when one is attached). The
/// release build has no console on Windows (see `main.rs`'s
/// `windows_subsystem = "windows"`), so a startup panic — like a schema
/// migration failure in `db::connect()` — would otherwise leave no trace
/// the user could report back. Best-effort: if the log file itself can't be
/// written, this silently falls back to just the default hook rather than
/// panicking-while-handling-a-panic.
fn install_panic_hook() {
    let log_path = db::crash_log_path();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = format!("[unix {timestamp}] {info}\n");
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            use std::io::Write;
            let _ = file.write_all(entry.as_bytes());
        }
        default_hook(info);
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_hook();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let conn = db::connect().expect("open vault db");
            app.manage(db::DbConnection(std::sync::Mutex::new(conn)));
            app.manage(commands::RefreshCancelFlag(std::sync::atomic::AtomicBool::new(false)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_items,
            commands::get_item,
            commands::sell_item,
            commands::remove_item,
            commands::get_price_history,
            commands::get_settings,
            commands::update_settings,
            commands::refresh_item_price_command,
            commands::refresh_prices_command,
            commands::cancel_price_refresh,
            commands::export_items_csv,
            commands::export_items_json,
            commands::wipe_vault,
            commands::get_vault_file_size,
            commands::save_steam_cookie_command,
            commands::has_steam_cookie_command,
            commands::clear_steam_cookie_command,
            commands::save_steam_store_cookie_command,
            commands::has_steam_store_cookie_command,
            commands::clear_steam_store_cookie_command,
            commands::save_steam_help_cookie_command,
            commands::has_steam_help_cookie_command,
            commands::clear_steam_help_cookie_command,
            commands::connect_steam_account,
            commands::preview_steam_import,
            commands::commit_steam_import,
            commands::backfill_game_names,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
