//! The interface zoom, kept with the window's place.

use tauri::{State, WebviewWindow};

use crate::window_state::WindowState;

#[tauri::command]
pub fn window_zoom(place: State<'_, WindowState>) -> u16 {
    place.zoom()
}

/// Returns the level set: the offered one nearest `percent`.
#[tauri::command]
pub fn set_window_zoom(window: WebviewWindow, place: State<'_, WindowState>, percent: u16) -> u16 {
    place.set_zoom(&window, percent)
}
