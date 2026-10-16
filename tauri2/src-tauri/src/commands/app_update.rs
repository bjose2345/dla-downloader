//! Updates for the app itself.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::{
    app_update::{AppUpdateView, AppUpdater, Installer},
    library::{DownloadState, Library},
};

type SharedUpdater<'a> = State<'a, Arc<AppUpdater>>;

#[tauri::command]
pub fn app_update(updater: SharedUpdater<'_>) -> AppUpdateView {
    updater.view()
}

#[tauri::command]
pub fn app_update_check(updater: SharedUpdater<'_>) -> AppUpdateView {
    updater.check()
}

/// Runs the verified installer and ends the app; the new version opens when
/// it is done. Refused while a download runs, so nothing is cut off halfway.
#[tauri::command]
pub fn app_update_install(
    app: AppHandle,
    updater: SharedUpdater<'_>,
    library: State<'_, Arc<Library>>,
) -> AppUpdateView {
    let busy = library
        .view()
        .downloads
        .iter()
        .any(|download| download.state == DownloadState::Running);
    match updater.begin_install(busy) {
        Ok(installer) => match run(&app, &installer) {
            Ok(()) => updater.view(),
            Err(detail) => updater.install_failed(detail),
        },
        Err(view) => *view,
    }
}

/// The NSIS installer replaces the app, then starts it again.
#[cfg(target_os = "windows")]
fn run(app: &AppHandle, installer: &Installer) -> Result<(), String> {
    let args: Vec<&str> = std::env::args()
        .skip(1)
        .any(|arg| arg == "--self-test")
        .then_some("--self-test")
        .into_iter()
        .collect();
    dla_app_update::installer_command(&installer.path, &args)
        .spawn()
        .map_err(|error| error.to_string())?;
    // The installer waits for this app to close.
    app.exit(0);
    Ok(())
}

/// The new AppImage takes the old one's place, and the app restarts from it.
#[cfg(not(target_os = "windows"))]
fn run(app: &AppHandle, installer: &Installer) -> Result<(), String> {
    let app_image = installer
        .app_image
        .as_ref()
        .ok_or("this copy is not an AppImage")?;
    dla_app_update::replace_app_image(app_image, &installer.path)
        .map_err(|error| error.to_string())?;
    crate::save_place(app);
    app.restart()
}
