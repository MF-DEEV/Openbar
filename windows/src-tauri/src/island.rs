// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's window: a transparent, always-on-top strip along the top of
//! a display. The page draws the island inside it; everywhere else the window
//! lets the pointer through to whatever is underneath. The page reports the
//! island's shape, and a pointer watcher turns mouse input on only while the
//! pointer is over that shape.

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

pub const LABEL: &str = "island";

/// Large enough for the biggest page (the enlarged session page); the window
/// is see-through and click-through outside the island, so its size costs
/// nothing the person can notice.
const WINDOW_WIDTH: f64 = 1000.0;
const WINDOW_HEIGHT: f64 = 760.0;

/// How often the pointer watcher looks at the pointer. Fast enough that
/// hovering the closed island feels immediate.
const POINTER_POLL: Duration = Duration::from_millis(16);

/// A rectangle in the island page's CSS pixels, from the window's top-left.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Corner radius, so the pointer at a rounded corner falls through.
    #[serde(default)]
    pub radius: f64,
}

impl HitRect {
    fn contains(&self, px: f64, py: f64) -> bool {
        if px < self.x || py < self.y || px > self.x + self.width || py > self.y + self.height {
            return false;
        }
        let r = self.radius.min(self.width / 2.0).min(self.height / 2.0);
        if r <= 0.0 {
            return true;
        }
        // Only the four corner squares can miss.
        let cx = if px < self.x + r {
            self.x + r
        } else if px > self.x + self.width - r {
            self.x + self.width - r
        } else {
            return true;
        };
        let cy = if py < self.y + r {
            self.y + r
        } else if py > self.y + self.height - r {
            self.y + self.height - r
        } else {
            return true;
        };
        (px - cx).powi(2) + (py - cy).powi(2) <= r * r
    }
}

#[derive(Default)]
pub struct IslandState {
    rects: Mutex<Vec<HitRect>>,
    /// While the page holds the pointer (a drag, a slider, an open menu) the
    /// window keeps mouse input even outside the shape.
    capture: Mutex<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DisplayChoice {
    #[default]
    Primary,
    Cursor,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PointerEvent {
    inside: bool,
    x: f64,
    y: f64,
}

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("island.html".into()))
        .title("Vorssaint Island")
        .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;
    place(&window, DisplayChoice::Primary)?;
    window.set_ignore_cursor_events(true)?;
    platform::style(&window);
    window.show()?;
    watch_pointer(app.clone());
    Ok(window)
}

/// Puts the window at the top center of the chosen display.
pub fn place<R: Runtime>(window: &WebviewWindow<R>, choice: DisplayChoice) -> tauri::Result<()> {
    let monitor = match choice {
        DisplayChoice::Primary => window.primary_monitor()?,
        DisplayChoice::Cursor => {
            let p = window.cursor_position()?;
            window.monitor_from_point(p.x, p.y)?
        }
    }
    .or(window.current_monitor()?);
    let Some(monitor) = monitor else { return Ok(()) };
    let scale = monitor.scale_factor();
    window.set_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))?;
    let width = (WINDOW_WIDTH * scale).round() as i32;
    let origin = monitor.position();
    let size = monitor.size();
    let x = origin.x + (size.width as i32 - width) / 2;
    window.set_position(PhysicalPosition::new(x, origin.y))?;
    Ok(())
}

/// Watches the pointer and lets the island window take mouse input only
/// while the pointer is over the island.
fn watch_pointer<R: Runtime>(app: AppHandle<R>) {
    thread::spawn(move || {
        let mut taking_input = false;
        loop {
            thread::sleep(POINTER_POLL);
            let Some(window) = app.get_webview_window(LABEL) else { break };
            let Some((x, y)) = pointer_in_window(&window) else { continue };
            let state = app.state::<IslandState>();
            let inside = state.rects.lock().unwrap().iter().any(|r| r.contains(x, y));
            let wants_input = inside || *state.capture.lock().unwrap();
            if wants_input != taking_input {
                taking_input = wants_input;
                let _ = window.set_ignore_cursor_events(!wants_input);
                let _ = window.emit_to(LABEL, "island://pointer", PointerEvent { inside, x, y });
            }
        }
    });
}

/// The pointer in the window's CSS pixels.
fn pointer_in_window<R: Runtime>(window: &WebviewWindow<R>) -> Option<(f64, f64)> {
    let pointer = window.cursor_position().ok()?;
    let origin = window.outer_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some(((pointer.x - origin.x as f64) / scale, (pointer.y - origin.y as f64) / scale))
}

#[tauri::command]
pub fn island_set_hit_rects(state: tauri::State<'_, IslandState>, rects: Vec<HitRect>) {
    *state.rects.lock().unwrap() = rects;
}

#[tauri::command]
pub fn island_set_capture(state: tauri::State<'_, IslandState>, capture: bool) {
    *state.capture.lock().unwrap() = capture;
}

#[tauri::command]
pub fn island_place<R: Runtime>(app: AppHandle<R>, display: DisplayChoice) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("no island window")?;
    place(&window, display).map_err(|e| e.to_string())
}

#[cfg(windows)]
mod platform {
    use tauri::{Runtime, WebviewWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    /// No taskbar button, no Alt+Tab entry, and clicking the island does not
    /// take focus from the app the person is working in.
    pub fn style<R: Runtime>(window: &WebviewWindow<R>) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let ex = ex | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use tauri::{Runtime, WebviewWindow};
    pub fn style<R: Runtime>(_window: &WebviewWindow<R>) {}
}

#[cfg(test)]
mod tests {
    use super::HitRect;

    #[test]
    fn rounded_corners_let_the_pointer_through() {
        let r = HitRect { x: 0.0, y: 0.0, width: 100.0, height: 40.0, radius: 20.0 };
        assert!(r.contains(50.0, 20.0));
        assert!(r.contains(20.0, 0.0));
        assert!(!r.contains(1.0, 1.0));
        assert!(!r.contains(99.0, 39.0));
        assert!(r.contains(5.0, 20.0));
        assert!(!r.contains(101.0, 20.0));
    }

    #[test]
    fn square_rect_takes_its_corners() {
        let r = HitRect { x: 10.0, y: 10.0, width: 10.0, height: 10.0, radius: 0.0 };
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(20.0, 20.0));
        assert!(!r.contains(9.9, 10.0));
    }
}
