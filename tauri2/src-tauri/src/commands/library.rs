//! The archive catalog and downloads.

use std::sync::Arc;

use dla_library::{CatalogEntry, FolderSummary};
use tauri::{AppHandle, State, async_runtime::spawn_blocking};
use tauri_plugin_opener::OpenerExt;

use super::{blocking, pick_folder};
use crate::{
    app_update::{AppUpdater, UPDATE_REQUIRED},
    library::{FolderFile, Library, LibraryNotice, LibraryView, RecentFile, RequestedFile},
};

type SharedLibrary<'a> = State<'a, Arc<Library>>;

#[tauri::command]
pub fn library(library: SharedLibrary<'_>) -> LibraryView {
    library.view()
}

#[tauri::command]
pub fn library_folders(library: SharedLibrary<'_>) -> Vec<FolderSummary> {
    library.folders()
}

#[tauri::command]
pub fn library_search(library: SharedLibrary<'_>, query: String) -> Vec<CatalogEntry> {
    library.search(&query)
}

#[tauri::command]
pub fn library_recent(library: SharedLibrary<'_>, days: u32) -> Vec<RecentFile> {
    library.recent(days)
}

#[tauri::command]
pub fn library_added_months(library: SharedLibrary<'_>, offset: i32) -> Vec<FolderSummary> {
    library.added_months(offset)
}

#[tauri::command]
pub fn library_added(library: SharedLibrary<'_>, month: String, offset: i32) -> Vec<RecentFile> {
    library.added(&month, offset)
}

#[tauri::command]
pub async fn library_folder(
    library: SharedLibrary<'_>,
    name: String,
) -> Result<Vec<FolderFile>, LibraryNotice> {
    let library = Arc::clone(&library);
    spawn_blocking(move || library.folder(&name))
        .await
        .unwrap_or_else(|_| Ok(Vec::new()))
}

#[tauri::command]
pub fn library_refresh(library: SharedLibrary<'_>, thorough: bool) -> LibraryView {
    library.refresh(thorough)
}

/// Asks where to save with the system folder picker; the chosen path stays
/// in Rust.
#[tauri::command]
pub async fn download_files(
    app: AppHandle,
    library: SharedLibrary<'_>,
    updater: State<'_, Arc<AppUpdater>>,
    files: Vec<RequestedFile>,
) -> Result<LibraryView, String> {
    // Refused while this version is below the newest release's minimum.
    if updater.required() {
        return Err(UPDATE_REQUIRED.into());
    }
    blocking(&library, move |library| {
        if files.is_empty() {
            return library.view();
        }
        let start = match library.destination() {
            Ok(directory) => return library.queue(directory, files),
            Err(start) => start,
        };
        match pick_folder(&app, "Choose where to save the download", start) {
            Some(directory) => library.queue(directory, files),
            None => library.view(),
        }
    })
    .await
}

#[tauri::command]
pub fn pause_download(library: SharedLibrary<'_>, id: String) -> LibraryView {
    library.pause(&id)
}

#[tauri::command]
pub fn resume_download(library: SharedLibrary<'_>, id: String) -> LibraryView {
    library.resume(&id)
}

#[tauri::command]
pub fn remove_download(library: SharedLibrary<'_>, id: String) -> LibraryView {
    library.remove(&id)
}

#[tauri::command]
pub fn clear_downloads(library: SharedLibrary<'_>) -> LibraryView {
    library.clear_finished()
}

#[tauri::command]
pub fn show_download(app: AppHandle, library: SharedLibrary<'_>, id: String) -> Result<(), String> {
    let path = library
        .saved_path(&id)
        .ok_or("This download is no longer listed.")?;
    let opener = app.opener();
    if path.is_file() {
        opener.reveal_item_in_dir(&path)
    } else {
        opener.open_path(path.display().to_string(), None::<&str>)
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn choose_download_folder(
    app: AppHandle,
    library: SharedLibrary<'_>,
) -> Result<LibraryView, String> {
    blocking(&library, move |library| {
        let start = library.destination().map_or_else(|start| start, Some);
        match pick_folder(&app, "Choose your download folder", start) {
            Some(directory) => library.set_default_directory(Some(directory)),
            None => library.view(),
        }
    })
    .await
}

#[tauri::command]
pub fn clear_download_folder(library: SharedLibrary<'_>) -> LibraryView {
    library.set_default_directory(None)
}

#[tauri::command]
pub fn set_ask_every_time(library: SharedLibrary<'_>, ask: bool) -> LibraryView {
    library.set_ask_every_time(ask)
}
