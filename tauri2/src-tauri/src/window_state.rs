//! The main window's size, position and maximised state, and the interface
//! zoom, kept across starts on this computer in `window.json`. A saved place
//! that no longer fits a screen (a monitor unplugged, a smaller display) is
//! ignored, and the window opens in its configured place instead.

use std::{fs, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Monitor, PhysicalPosition, PhysicalSize, WebviewWindow, Window};

use crate::system::write_private;

/// The window outside a maximised state: its frame's top-left corner and
/// the size of its content, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub bounds: Bounds,
    pub maximized: bool,
}

/// The interface zoom levels offered, in percent.
pub const ZOOM_LEVELS: [u16; 6] = [80, 90, 100, 110, 125, 150];
const NO_ZOOM: u16 = 100;

/// The offered level nearest to `percent`.
pub fn zoom_level(percent: u16) -> u16 {
    ZOOM_LEVELS
        .into_iter()
        .min_by_key(|level| level.abs_diff(percent))
        .unwrap_or(NO_ZOOM)
}

/// What `window.json` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Document {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    place: Option<Saved>,
    #[serde(default = "no_zoom")]
    zoom: u16,
}

fn no_zoom() -> u16 {
    NO_ZOOM
}

impl Default for Document {
    fn default() -> Self {
        Self {
            place: None,
            zoom: NO_ZOOM,
        }
    }
}

/// A monitor's work area, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl From<&Monitor> for Area {
    fn from(monitor: &Monitor) -> Self {
        let area = monitor.work_area();
        Self {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        }
    }
}

// Windows places a snapped or maximised frame a few pixels outside the work
// area; the title bar must still be reachable to move the window.
const SLACK: i32 = 16;
const GRIP_WIDTH: i32 = 120;
const GRIP_HEIGHT: i32 = 40;

/// Whether the window would sit on one monitor with its title bar there.
pub fn fits(bounds: &Bounds, areas: &[Area]) -> bool {
    areas.iter().any(|area| {
        let right = area.x + area.width as i32;
        let bottom = area.y + area.height as i32;
        bounds.width <= area.width
            && bounds.height <= area.height
            && bounds.x >= area.x - SLACK
            && bounds.y >= area.y - SLACK
            && bounds.x + GRIP_WIDTH <= right
            && bounds.y + GRIP_HEIGHT <= bottom
    })
}

#[derive(Default)]
struct Tracked {
    now: Document,
    // The bounds before the latest change. Windows and GTK report a window's
    // maximised place before they report it maximised; that place is undone.
    before: Option<Bounds>,
    written: Document,
}

/// Follows the window while it runs and writes where it was when the app
/// closes, so nothing is written while the window is being dragged. A zoom
/// change is written straight away.
pub struct WindowState {
    path: Option<PathBuf>,
    tracked: Mutex<Tracked>,
}

impl WindowState {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            tracked: Mutex::new(Tracked::default()),
        }
    }

    /// Reads `window.json`. Returns the saved place, if there is one and it
    /// fits one of these screens; the zoom is kept for `apply_zoom`.
    pub fn load(&self, monitors: &[Monitor]) -> Option<Saved> {
        self.read(&monitors.iter().map(Area::from).collect::<Vec<_>>())
    }

    fn read(&self, areas: &[Area]) -> Option<Saved> {
        let document: Document = self
            .path
            .as_ref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        {
            let mut tracked = self.tracked.lock().expect("window state lock");
            tracked.now.zoom = zoom_level(document.zoom);
            tracked.written = document;
        }
        document.place.filter(|saved| fits(&saved.bounds, areas))
    }

    /// Zooms a window that is not shown yet as it was last time.
    pub fn apply_zoom(&self, window: &WebviewWindow) {
        let zoom = self.zoom();
        if zoom != NO_ZOOM {
            let _ = window.set_zoom(f64::from(zoom) / 100.0);
        }
    }

    pub fn zoom(&self) -> u16 {
        self.tracked.lock().expect("window state lock").now.zoom
    }

    /// Zooms the interface to the offered level nearest `percent`, and
    /// remembers it. Returns the level.
    pub fn set_zoom(&self, window: &WebviewWindow, percent: u16) -> u16 {
        let zoom = self.choose_zoom(percent);
        let _ = window.set_zoom(f64::from(zoom) / 100.0);
        zoom
    }

    fn choose_zoom(&self, percent: u16) -> u16 {
        let zoom = zoom_level(percent);
        self.tracked.lock().expect("window state lock").now.zoom = zoom;
        self.save();
        zoom
    }

    /// Puts a window that is not shown yet back where it was, then shows it.
    pub fn restore(&self, window: &WebviewWindow, saved: Saved) {
        let bounds = saved.bounds;
        let _ = window.set_size(PhysicalSize::new(bounds.width, bounds.height));
        let _ = window.set_position(PhysicalPosition::new(bounds.x, bounds.y));
        // Before showing it, so a maximised window never flashes at its own size.
        if saved.maximized {
            let _ = window.maximize();
        }
        let _ = window.show();
        self.tracked.lock().expect("window state lock").now.place = Some(saved);
    }

    /// The window as first shown, when there was nothing to restore.
    pub fn opened(&self, window: &WebviewWindow) {
        self.note(
            false,
            window.is_maximized().unwrap_or(false),
            bounds(window.outer_position(), window.inner_size()),
        );
    }

    /// Called when the window moves or changes size.
    pub fn track(&self, window: &Window) {
        self.note(
            window.is_minimized().unwrap_or(false),
            window.is_maximized().unwrap_or(false),
            bounds(window.outer_position(), window.inner_size()),
        );
    }

    fn note(&self, minimized: bool, maximized: bool, bounds: Option<Bounds>) {
        // A minimised window keeps the place it will come back to.
        if minimized {
            return;
        }
        let mut tracked = self.tracked.lock().expect("window state lock");
        if maximized {
            let before = tracked.before;
            if let Some(current) = tracked
                .now
                .place
                .as_mut()
                .filter(|current| !current.maximized)
            {
                current.bounds = before.unwrap_or(current.bounds);
                current.maximized = true;
            }
            return;
        }
        let Some(bounds) = bounds else {
            return;
        };
        let last = tracked.now.place.map(|current| current.bounds);
        if last != Some(bounds) {
            tracked.before = last;
        }
        tracked.now.place = Some(Saved {
            bounds,
            maximized: false,
        });
    }

    /// Writes the window's place and zoom, if they changed since they were
    /// read or written.
    pub fn save(&self) {
        let mut tracked = self.tracked.lock().expect("window state lock");
        let now = tracked.now;
        if tracked.written == now {
            return;
        }
        let (Some(path), Ok(data)) = (&self.path, serde_json::to_vec(&now)) else {
            return;
        };
        if write_private(path, &data).is_ok() {
            tracked.written = now;
        }
    }
}

fn bounds(
    position: tauri::Result<PhysicalPosition<i32>>,
    size: tauri::Result<PhysicalSize<u32>>,
) -> Option<Bounds> {
    let (position, size) = (position.ok()?, size.ok()?);
    Some(Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    const RIGHT: Area = Area {
        x: 1920,
        y: 0,
        width: 2560,
        height: 1400,
    };

    fn at(x: i32, y: i32, width: u32, height: u32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    fn written(state: &WindowState) -> Option<Document> {
        let data = fs::read(state.path.as_ref()?).ok()?;
        serde_json::from_slice(&data).ok()
    }

    fn place(state: &WindowState) -> Option<Saved> {
        written(state)?.place
    }

    #[test]
    fn a_saved_place_must_still_be_on_a_screen() {
        assert!(fits(&at(100, 80, 900, 700), &[SCREEN]));
        // Snapped windows sit a few pixels outside the work area.
        assert!(fits(&at(-8, -8, 960, 1040), &[SCREEN]));
        // On the second monitor while it is there, not after it is gone.
        assert!(fits(&at(2200, 100, 1200, 900), &[SCREEN, RIGHT]));
        assert!(!fits(&at(2200, 100, 1200, 900), &[SCREEN]));
        // Bigger than the screen now, or with the title bar out of reach.
        assert!(!fits(&at(0, 0, 2400, 900), &[SCREEN]));
        assert!(!fits(&at(1850, 100, 640, 600), &[SCREEN]));
        assert!(!fits(&at(100, -200, 640, 600), &[SCREEN]));
        assert!(!fits(&at(100, 80, 640, 600), &[]));
    }

    #[test]
    fn a_maximised_window_comes_back_maximised_over_its_own_place() {
        let directory = tempfile::tempdir().unwrap();
        let state = WindowState::new(Some(directory.path().join("window.json")));
        state.note(false, false, Some(at(370, 90, 1180, 860)));
        state.note(false, false, Some(at(200, 60, 1000, 800)));
        // Windows reports the maximised frame before it says "maximised".
        state.note(false, false, Some(at(-8, -8, 1920, 1009)));
        state.note(false, true, Some(at(-8, -8, 1920, 1009)));
        // Minimising says nothing about the place to come back to.
        state.note(true, false, Some(at(-32000, -32000, 160, 28)));
        state.save();
        assert_eq!(
            place(&state),
            Some(Saved {
                bounds: at(200, 60, 1000, 800),
                maximized: true,
            })
        );

        // Restored to its own size, then moved.
        state.note(false, false, Some(at(200, 60, 1000, 800)));
        state.note(false, false, Some(at(260, 70, 1000, 800)));
        state.note(false, false, Some(at(260, 70, 1000, 800)));
        state.save();
        assert_eq!(
            place(&state),
            Some(Saved {
                bounds: at(260, 70, 1000, 800),
                maximized: false,
            })
        );
    }

    #[test]
    fn nothing_is_written_until_the_window_is_known_or_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("window.json");
        let state = WindowState::new(Some(path.clone()));
        state.save();
        assert!(!path.exists());
        assert_eq!(state.read(&[]), None);

        let saved = Saved {
            bounds: at(10, 20, 800, 650),
            maximized: false,
        };
        fs::write(
            &path,
            serde_json::to_vec(&Document {
                place: Some(saved),
                zoom: NO_ZOOM,
            })
            .unwrap(),
        )
        .unwrap();
        let state = WindowState::new(Some(path.clone()));
        assert_eq!(state.read(&[SCREEN]), Some(saved));
        state.note(false, false, Some(saved.bounds));
        state.save();
        assert_eq!(state.tracked.lock().unwrap().written.place, Some(saved));
        state.note(false, false, Some(at(30, 20, 800, 650)));
        state.save();
        assert_eq!(place(&state).unwrap().bounds, at(30, 20, 800, 650));
        fs::write(&path, b"not json").unwrap();
        assert_eq!(state.read(&[SCREEN]), None);
        assert_eq!(state.zoom(), NO_ZOOM);
    }

    #[test]
    fn the_zoom_is_kept_even_when_the_place_is_not() {
        assert_eq!(zoom_level(100), 100);
        assert_eq!(zoom_level(0), 80);
        assert_eq!(zoom_level(118), 125);
        assert_eq!(zoom_level(500), 150);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("window.json");
        let state = WindowState::new(Some(path.clone()));
        assert_eq!(state.read(&[SCREEN]), None);
        // Written at once, before the window's place is even known.
        assert_eq!(state.choose_zoom(112), 110);
        assert_eq!(
            written(&state),
            Some(Document {
                place: None,
                zoom: 110,
            })
        );
        state.note(false, false, Some(at(2200, 100, 1200, 900)));
        state.save();

        // The second monitor is gone: the place is dropped, the zoom stays.
        let reopened = WindowState::new(Some(path));
        assert_eq!(reopened.read(&[SCREEN]), None);
        assert_eq!(reopened.zoom(), 110);
    }
}
