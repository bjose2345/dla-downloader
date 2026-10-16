//! The commands the interface can call, one file per part of the app.
//! Paths picked in a system dialog stay in Rust; the interface gets views.

pub mod account;
pub mod app_update;
pub mod library;
pub mod window;

use std::{path::PathBuf, sync::Arc};

use tauri::{AppHandle, async_runtime::spawn_blocking};
use tauri_plugin_dialog::DialogExt;

/// Runs slow work (disk, network, a system dialog) off the interface thread.
async fn blocking<S: Send + Sync + 'static, T: Send + 'static>(
    state: &Arc<S>,
    work: impl FnOnce(&Arc<S>) -> T + Send + 'static,
) -> Result<T, String> {
    let state = Arc::clone(state);
    spawn_blocking(move || work(&state))
        .await
        .map_err(|error| error.to_string())
}

/// The system folder picker; the chosen path stays in Rust.
pub(super) fn pick_folder(app: &AppHandle, title: &str, start: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = app.dialog().file().set_title(title);
    if let Some(directory) = start {
        dialog = dialog.set_directory(directory);
    }
    dialog
        .blocking_pick_folder()
        .and_then(|folder| folder.into_path().ok())
}
