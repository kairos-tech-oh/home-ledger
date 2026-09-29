//! The desktop shell. Owns no rules: it wires the store engine and the math
//! to a window, and every number it hands the UI comes from `ledger-math`.

mod audit;
mod clock;
mod commands;
mod dashboard;
#[cfg(test)]
mod history_sync_tests;
mod market;
mod planning;
mod plugin_history;
mod quotes;
mod snapshots;
mod spending;
mod state;
mod storage;
mod transactions;
mod updates;
mod views;

pub use state::AppState;

pub fn import_plugin_history(args: &[String]) -> i32 {
    plugin_history::run_cli(args)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "home_ledger=info,ledger_store=info".into()),
        )
        .init();

    use tauri::Manager;

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Pending::default())
        .setup(|app| {
            let state = AppState::bootstrap(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::stores,
            commands::sync_state,
            commands::flush,
            commands::apply,
            commands::import_preview,
            commands::import_apply,
            storage::setup,
            storage::test_store,
            storage::save_store,
            storage::promote_store,
            storage::remove_store,
            storage::rename_device,
            storage::set_retirement_target_year,
            storage::finish_setup,
            storage::set_opacity,
            storage::connect_drive,
            views::ledger,
            views::history,
            views::projection,
            planning::planning,
            spending::spending,
            spending::set_family_members,
            dashboard::dashboard,
            snapshots::take_snapshot,
            snapshots::plugin_snapshots_path,
            snapshots::snapshot_import_preview,
            snapshots::snapshot_import,
            updates::update_check,
            updates::update_install,
            updates::app_version,
            transactions::transactions_preview,
            market::holding_detail,
            quotes::refresh_prices,
            quotes::save_api_key,
            quotes::forget_api_key,
            quotes::has_api_key,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Home Ledger");
}
