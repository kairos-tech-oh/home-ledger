//! Keeping the app current. Releases on GitHub carry a `latest.json` and a
//! signature for each installer, made with the update key only the maintainer
//! holds. The updater checks that signature against the public key built into
//! this binary, so an installer that key did not sign is never run.

use ledger_app::{Answer, CommandError};
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

/// The update found by the last check, held so installing it installs exactly
/// what was offered rather than whatever a second check finds.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// The release notes, as written on GitHub.
    pub notes: String,
    pub date: String,
}

fn failed(e: impl std::fmt::Display) -> CommandError {
    CommandError::Message(format!("could not check for updates: {e}"))
}

/// A newer release, or None when this is the latest.
#[tauri::command]
pub async fn update_check<R: Runtime>(
    app: AppHandle<R>,
    pending: State<'_, Pending>,
) -> Answer<Option<UpdateInfo>> {
    let found = app
        .updater()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().unwrap_or_default(),
        date: u.date.map(|d| d.to_string()).unwrap_or_default(),
    });
    *pending.0.lock().await = found;
    Ok(info)
}

/// Downloads the update that was offered, checks its signature, installs it
/// and restarts. On Windows the installer closes the app itself.
#[tauri::command]
pub async fn update_install<R: Runtime>(
    app: AppHandle<R>,
    pending: State<'_, Pending>,
) -> Answer<()> {
    let Some(update) = pending.0.lock().await.take() else {
        return Err(CommandError::Message("check for an update first".into()));
    };
    let version = update.version.clone();
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| CommandError::Message(format!("could not install {version}: {e}")))?;
    tracing::info!(%version, "update installed; restarting");
    app.restart();
}

/// This build's own version, for the About section.
#[tauri::command]
pub fn app_version<R: Runtime>(app: AppHandle<R>) -> String {
    app.package_info().version.to_string()
}
