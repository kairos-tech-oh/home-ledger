//! The window's commands. Each forwards to `ledger_app`, which does the work
//! for the desktop app, the `hl` command line and the phone app alike; this
//! file is the only place that knows the work is reached through Tauri.

use ledger_app::{Answer, AppState};
use ledger_app::{
    commands as cmd, dashboard as dash, encryption as enc, planning as plan, quotes as q,
    snapshots as snap, spending as spend, storage as st, transactions as tx, views as v,
};
use ledger_config::{Secret, StoreConfig};
use ledger_store::{StoreStatus, SyncState};
use ledger_writer::bank_csv::Mapping;
use ledger_writer::import::ImportReport;
use serde_json::Value;
use tauri::State;

/// One command: its name, its arguments, what it returns, and the function in
/// `ledger_app` it forwards to with the app's state first.
macro_rules! forward {
    ($( $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty => $target:path; )*) => {
        $(
            #[tauri::command]
            pub async fn $name(state: State<'_, AppState> $(, $arg: $ty)*) -> Answer<$ret> {
                $target(state.inner() $(, $arg)*).await
            }
        )*
    };
}

forward! {
    overview() -> cmd::Overview => cmd::overview;
    apply(op: Value) -> Option<cmd::Applied> => cmd::apply;
    import_preview(path: String) -> cmd::ImportPreview => cmd::import_preview;
    import_apply(path: String, replace: bool) -> ImportReport => cmd::import_apply;
    stores() -> Vec<StoreStatus> => cmd::stores;
    sync_state() -> SyncState => cmd::sync_state;
    flush() -> SyncState => cmd::flush;

    dashboard(today: String, offset_minutes: i64) -> dash::DashboardView => dash::dashboard;
    planning(from: String, to: String) -> plan::PlanningView => plan::planning;
    spending(
        period: String,
        from: String,
        to: String,
        status: String,
        today: String,
        offset_minutes: i64
    ) -> spend::SpendingView => spend::spending;
    set_family_members(names: Vec<String>) -> Vec<String> => spend::set_family_members;
    transactions_preview(id: String, text: String, mapping: Option<Mapping>)
        -> tx::ImportPreview => tx::transactions_preview;

    encryption_status() -> enc::Status => enc::encryption_status;
    encryption_enable(passphrase: String) -> enc::Enabled => enc::encryption_enable;
    encryption_unlock(secret: String) -> bool => enc::encryption_unlock;
    encryption_disable(passphrase: String) -> Vec<String> => enc::encryption_disable;

    save_api_key(key: String) -> bool => q::save_api_key;
    forget_api_key() -> bool => q::forget_api_key;
    has_api_key() -> bool => q::has_api_key;
    refresh_prices() -> q::Refreshed => q::refresh_prices;

    take_snapshot(today: String) -> bool => snap::take_snapshot;
    snapshot_import_preview(path: String) -> snap::SnapshotImport => snap::snapshot_import_preview;
    snapshot_import(path: String) -> usize => snap::snapshot_import;

    setup() -> st::Setup => st::setup;
    save_store(store: StoreConfig, secret: Option<Secret>) -> st::Setup => st::save_store;
    promote_store(id: String, force: bool) -> st::PromoteReport => st::promote_store;
    remove_store(id: String) -> st::Setup => st::remove_store;
    rename_device(name: String) -> st::Setup => st::rename_device;
    set_retirement_target_year(year: Option<i32>) -> st::Setup => st::set_retirement_target_year;
    set_opacity(value: f64) -> st::Setup => st::set_opacity;
    finish_setup() -> st::Setup => st::finish_setup;
    connect_drive(client_id: String, file_name: String, label: String)
        -> st::Connected => st::connect_drive;

    ledger() -> v::LedgerView => v::ledger;
    history() -> v::HistoryView => v::history;
    projection() -> v::ProjectionView => v::projection;
}

// The two that need no state.

#[tauri::command]
pub async fn test_store(store: StoreConfig, secret: Option<Secret>) -> Answer<String> {
    st::test_store(store, secret).await
}

#[tauri::command]
pub async fn plugin_snapshots_path() -> Answer<Option<String>> {
    snap::plugin_snapshots_path().await
}
