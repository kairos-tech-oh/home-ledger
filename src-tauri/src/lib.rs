//! The desktop shell. Owns no rules: every figure and every edit comes from
//! `ledger-app`, shared with the `hl` command line; this crate puts a window
//! on it, and adds what only a window needs (price charts, self-updating).

mod bridge;
mod market;
mod updates;

pub use ledger_app::AppState;

pub fn import_plugin_history(args: &[String]) -> i32 {
    ledger_app::plugin_history::run_cli(args)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "home_ledger=info,ledger_app=info,ledger_store=info".into()),
        )
        .init();

    use tauri::Manager;

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Pending::default())
        .setup(|app| {
            let state = AppState::headless(ledger_app::Client::Desktop)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bridge::overview,
            bridge::stores,
            bridge::sync_state,
            bridge::flush,
            bridge::apply,
            bridge::import_preview,
            bridge::import_apply,
            bridge::setup,
            bridge::test_store,
            bridge::save_store,
            bridge::promote_store,
            bridge::remove_store,
            bridge::rename_device,
            bridge::set_retirement_target_year,
            bridge::finish_setup,
            bridge::set_opacity,
            bridge::connect_drive,
            bridge::ledger,
            bridge::history,
            bridge::projection,
            bridge::planning,
            bridge::spending,
            bridge::set_family_members,
            bridge::dashboard,
            bridge::take_snapshot,
            bridge::plugin_snapshots_path,
            bridge::snapshot_import_preview,
            bridge::snapshot_import,
            bridge::transactions_preview,
            bridge::encryption_status,
            bridge::encryption_enable,
            bridge::encryption_unlock,
            bridge::encryption_disable,
            bridge::refresh_prices,
            bridge::save_api_key,
            bridge::forget_api_key,
            bridge::has_api_key,
            bridge::bank_status,
            bridge::bank_save_keys,
            bridge::bank_forget_keys,
            bridge::bank_connect,
            bridge::bank_connect_check,
            bridge::bank_link,
            bridge::bank_disconnect,
            bridge::bank_fetch,
            bridge::bank_preview,
            bridge::bank_import,
            bridge::bank_balances,
            bridge::bank_apply_balances,
            market::holding_detail,
            updates::update_check,
            updates::update_install,
            updates::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Home Ledger");
}
