mod access;
mod app_update;
mod commands;
mod connection;
#[cfg(test)]
mod go_services;
mod library;
mod local_store;
mod mode;
mod session;
mod system;
mod window_state;

use std::{path::PathBuf, sync::Arc};

use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::{
    app_update::{AppUpdateView, AppUpdater},
    connection::{Connection, ConnectionNotifier, ConnectionView},
    library::{Library, LibraryView},
    local_store::LocalStore,
    mode::Mode,
    session::{Session, SessionView},
    window_state::WindowState,
};

pub const SESSION_CHANGED: &str = "session://changed";
pub const CONNECTION_CHANGED: &str = "connection://changed";
pub const LIBRARY_CHANGED: &str = "library://changed";
pub const APP_UPDATE_CHANGED: &str = "app-update://changed";

pub fn run() {
    let mode = match mode::from_environment() {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("DLA Downloader: {error}");
            std::process::exit(2);
        }
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| start(app, mode))
        .invoke_handler(tauri::generate_handler![
            commands::account::app_info,
            commands::account::connection_status,
            commands::account::session,
            commands::account::sign_in,
            commands::account::sign_out,
            commands::account::set_remember_account,
            commands::account::open_password_page,
            commands::account::change_password,
            commands::account::account_email,
            commands::account::set_account_email,
            commands::account::send_reset_code,
            commands::account::reset_password,
            commands::library::library,
            commands::library::library_folders,
            commands::library::library_search,
            commands::library::library_folder,
            commands::library::library_refresh,
            commands::library::download_files,
            commands::library::pause_download,
            commands::library::resume_download,
            commands::library::remove_download,
            commands::library::clear_downloads,
            commands::library::show_download,
            commands::library::choose_download_folder,
            commands::library::clear_download_folder,
            commands::library::set_ask_every_time,
            commands::window::window_zoom,
            commands::window::set_window_zoom,
            commands::app_update::app_update,
            commands::app_update::app_update_check,
            commands::app_update::app_update_install,
        ])
        .on_window_event(remember_place)
        .build(tauri::generate_context!())
        .expect("DLA Downloader could not start")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                save_place(app);
                if let Some(library) = app.try_state::<Arc<Library>>() {
                    library.shutdown();
                }
                if let Some(connection) = app.try_state::<Arc<Connection>>() {
                    connection.shutdown();
                }
            }
        });
}

/// Creates the app's state, then its window: the page can never call a
/// command before the state behind it exists.
fn start(app: &mut tauri::App, mode: Mode) -> Result<(), Box<dyn std::error::Error>> {
    let events = app.handle().clone();
    let connection_notify: ConnectionNotifier = Arc::new(move |view: &ConnectionView| {
        let _ = events.emit(CONNECTION_CHANGED, view);
    });
    let (endpoint, connection) = match mode {
        Mode::SelfTest(endpoint) => (endpoint, Connection::self_test(connection_notify)),
        Mode::Private => Connection::private(connection_notify),
    };
    app.manage(Arc::new(connection));
    let data_dir = app.path().app_local_data_dir().ok();
    let handle = app.handle().clone();
    let notify = Arc::new(move |view: &SessionView| {
        let _ = handle.emit(SESSION_CHANGED, view);
    });
    app.manage(Arc::new(Session::new(
        endpoint.clone(),
        LocalStore::new(
            data_dir
                .as_ref()
                .map(|directory| directory.join("account.json")),
        ),
        notify,
    )));
    let handle = app.handle().clone();
    let notify = Arc::new(move |view: &LibraryView| {
        let _ = handle.emit(LIBRARY_CHANGED, view);
    });
    app.manage(Arc::new(Library::new(
        endpoint.clone(),
        data_dir.clone(),
        notify,
    )));
    let handle = app.handle().clone();
    let notify = Arc::new(move |view: &AppUpdateView| {
        let _ = handle.emit(APP_UPDATE_CHANGED, view);
    });
    // The public half of the DLA update key, embedded by build.rs.
    let updater = Arc::new(AppUpdater::new(
        endpoint,
        data_dir.clone(),
        app.package_info().version.to_string(),
        option_env!("DLA_UPDATE_KEY"),
        // Set by the AppImage runtime; an update replaces that file.
        std::env::var_os("APPIMAGE").map(PathBuf::from),
        notify,
    ));
    updater.start();
    app.manage(updater);
    let window = app
        .config()
        .app
        .windows
        .first()
        .cloned()
        .expect("tauri.conf.json defines the main window");
    let place = WindowState::new(data_dir.map(|directory| directory.join("window.json")));
    let saved = place.load(&app.available_monitors().unwrap_or_default());
    // Shown only once it is back in its place.
    let window = WebviewWindowBuilder::from_config(app.handle(), &window)?
        .visible(saved.is_none())
        .build()?;
    place.apply_zoom(&window);
    match saved {
        Some(saved) => place.restore(&window, saved),
        None => {
            fit_to_screen(&window);
            place.opened(&window);
        }
    }
    app.manage(place);
    Ok(())
}

fn remember_place(window: &tauri::Window, event: &WindowEvent) {
    let Some(place) = window.try_state::<WindowState>() else {
        return;
    };
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => place.track(window),
        WindowEvent::CloseRequested { .. } => place.save(),
        _ => {}
    }
}

/// Also called before the app restarts into an update, which skips the
/// usual exit.
pub(crate) fn save_place(app: &tauri::AppHandle) {
    if let Some(place) = app.try_state::<WindowState>() {
        place.save();
    }
}

/// The configured size suits most screens. On smaller ones, such as
/// 1366×768 laptops, the window shrinks to the work area and is centred in
/// it, so nothing ends up under the taskbar.
fn fit_to_screen(window: &WebviewWindow) {
    let (Ok(Some(monitor)), Ok(outer), Ok(inner)) = (
        window.current_monitor(),
        window.outer_size(),
        window.inner_size(),
    ) else {
        return;
    };
    let area = monitor.work_area();
    if outer.width <= area.size.width && outer.height <= area.size.height {
        return;
    }
    // The title bar and borders are outside the configured size.
    let frame_width = outer.width.saturating_sub(inner.width);
    let frame_height = outer.height.saturating_sub(inner.height);
    let width = inner.width.min(area.size.width.saturating_sub(frame_width));
    let height = inner
        .height
        .min(area.size.height.saturating_sub(frame_height));
    let _ = window.set_size(PhysicalSize::new(width, height));
    let left = (area.size.width.saturating_sub(width + frame_width) / 2) as i32;
    let top = (area.size.height.saturating_sub(height + frame_height) / 2) as i32;
    let _ = window.set_position(PhysicalPosition::new(
        area.position.x + left,
        area.position.y + top,
    ));
}
