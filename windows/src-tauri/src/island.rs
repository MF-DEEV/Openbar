// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's window: a transparent, always-on-top layer over the top of a
//! display. The page draws the island inside it; everywhere else the window
//! lets the pointer through to whatever is underneath. The page reports the
//! island's shape every frame, and a pointer watcher turns mouse input on
//! only while the pointer is over that shape.

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "island";

/// How often the pointer watcher looks at the pointer. Fast enough that
/// hovering the closed island feels immediate.
const POINTER_POLL: Duration = Duration::from_millis(16);

/// A rectangle in the island page's CSS pixels, from the window's top-left,
/// with its top and bottom corners rounded by their own radii.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub top_radius: f64,
    #[serde(default)]
    pub bottom_radius: f64,
}

impl HitRect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        if px < self.x || py < self.y || px > self.x + self.width || py > self.y + self.height {
            return false;
        }
        let limit = (self.width / 2.0).min(self.height / 2.0);
        let top = self.top_radius.min(limit).max(0.0);
        let bottom = self.bottom_radius.min(limit).max(0.0);
        let (r, cy) = if py < self.y + top {
            (top, self.y + top)
        } else if py > self.y + self.height - bottom {
            (bottom, self.y + self.height - bottom)
        } else {
            return true;
        };
        // Only the corner squares can miss.
        let cx = if px < self.x + r {
            self.x + r
        } else if px > self.x + self.width - r {
            self.x + self.width - r
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
    /// Settings › Behavior › Show on, and the display the window is on now.
    display: Mutex<DisplayChoice>,
    placed_on: Mutex<Option<(i32, i32)>>,
    /// The window that had the keyboard before the island took it.
    #[cfg(windows)]
    previous_foreground: Mutex<isize>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DisplayChoice {
    #[default]
    Primary,
    Cursor,
    /// Every display shows the closed island; the open one follows the pointer.
    /// Not on Windows yet: reads as Primary.
    All,
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
        .inner_size(1000.0, 760.0)
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

/// Spans the chosen display, from its top
/// edge: room for the largest page, the floating buttons beside and below
/// it, and the island's swing. Transparent and click-through outside the
/// island, so its size costs nothing the person can notice, and it never
/// resizes during a motion (spec 01-shell §6.2).
pub fn place<R: Runtime>(window: &WebviewWindow<R>, choice: DisplayChoice) -> tauri::Result<()> {
    if let Some(state) = window.try_state::<IslandState>() {
        *state.display.lock().unwrap() = choice;
    }
    let monitor = match choice {
        DisplayChoice::Primary | DisplayChoice::All => window.primary_monitor()?,
        DisplayChoice::Cursor => {
            let p = window.cursor_position()?;
            window.monitor_from_point(p.x, p.y)?
        }
    };
    let Some(monitor) = monitor.or(window.current_monitor()?) else {
        return Ok(());
    };
    move_to(window, &monitor)
}

/// Sizes and moves the window onto `monitor`; never changes the choice.
fn move_to<R: Runtime>(window: &WebviewWindow<R>, monitor: &tauri::Monitor) -> tauri::Result<()> {
    let scale = monitor.scale_factor();
    let size = monitor.size();
    let width = size.width as f64 / scale;
    // Keep one stable transparent canvas. Opening a popup must not resize
    // the native window or trigger fullscreen transitions in the shell.
    let height = size.height as f64 / scale;
    window.set_size(LogicalSize::new(width, height))?;
    window.set_position(PhysicalPosition::new(
        monitor.position().x,
        monitor.position().y,
    ))?;
    if let Some(state) = window.try_state::<IslandState>() {
        *state.placed_on.lock().unwrap() = Some((monitor.position().x, monitor.position().y));
    }
    Ok(())
}

/// Follow pointer: the island moves to the display the pointer is on, but
/// never while the person is using it.
fn follow_pointer<R: Runtime>(window: &WebviewWindow<R>, state: &IslandState) {
    let following = || *state.display.lock().unwrap() == DisplayChoice::Cursor;
    if !following() {
        return;
    }
    // Only a closed island moves: an open one (taller than any strip or
    // notice) stays put until it closes.
    if state.rects.lock().unwrap().iter().any(|r| r.height > 80.0) {
        return;
    }
    let Ok(p) = window.cursor_position() else {
        return;
    };
    let Ok(Some(monitor)) = window.monitor_from_point(p.x, p.y) else {
        return;
    };
    let at = (monitor.position().x, monitor.position().y);
    // The choice may have changed while those calls waited on the event loop.
    if *state.placed_on.lock().unwrap() != Some(at) && following() {
        let _ = move_to(window, &monitor);
    }
}

/// Watches the pointer: lets the island window take mouse input only while
/// the pointer is over the island, tells the page when the pointer crosses
/// the island's edge, and when a button goes down anywhere else.
fn watch_pointer<R: Runtime>(app: AppHandle<R>) {
    thread::spawn(move || {
        let mut taking_input = false;
        let mut was_inside = false;
        let mut buttons_down = false;
        let mut ticks: u32 = 0;
        loop {
            ticks = ticks.wrapping_add(1);
            thread::sleep(POINTER_POLL);
            let Some(window) = app.get_webview_window(LABEL) else {
                break;
            };
            let Some((x, y)) = pointer_in_window(&window) else {
                continue;
            };
            let state = app.state::<IslandState>();
            let inside = state.rects.lock().unwrap().iter().any(|r| r.contains(x, y));
            let wants_input = inside || *state.capture.lock().unwrap();
            if wants_input != taking_input {
                taking_input = wants_input;
                let _ = window.set_ignore_cursor_events(!wants_input);
            }
            if inside != was_inside {
                was_inside = inside;
                let _ = window.emit_to(LABEL, "island://pointer", PointerEvent { inside, x, y });
            }
            let down = platform::mouse_buttons_down();
            if down && !buttons_down && !wants_input {
                let _ = window.emit_to(LABEL, "island://outside-click", ());
            }
            buttons_down = down;
            // About four times a second, only while the island is left alone.
            if ticks.is_multiple_of(16) && !wants_input && !down {
                follow_pointer(&window, &state);
            }
        }
    });
}

/// The pointer in the window's CSS pixels.
fn pointer_in_window<R: Runtime>(window: &WebviewWindow<R>) -> Option<(f64, f64)> {
    let pointer = window.cursor_position().ok()?;
    let origin = window.inner_position().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((
        (pointer.x - origin.x as f64) / scale,
        (pointer.y - origin.y as f64) / scale,
    ))
}

#[tauri::command]
pub fn island_set_hit_rects(state: tauri::State<'_, IslandState>, rects: Vec<HitRect>) {
    *state.rects.lock().unwrap() = rects;
}

#[tauri::command]
pub fn island_set_capture(state: tauri::State<'_, IslandState>, capture: bool) {
    *state.capture.lock().unwrap() = capture;
}

/// While open the island may take the keyboard (Esc, shortcuts, the reply
/// field); closed, it never does, and the app that had it gets it back.
#[tauri::command]
pub fn island_set_focusable<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, IslandState>,
    focusable: bool,
    take: bool,
) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("no island window")?;
    platform::set_focusable(&window, &state, focusable, take);
    Ok(())
}

/// Settings › Behavior › Show in screenshots.
pub fn show_in_captures<R: Runtime>(app: &AppHandle<R>, show: bool) {
    if let Some(window) = app.get_webview_window(LABEL) {
        platform::show_in_captures(&window, show);
    }
}

#[cfg(windows)]
mod platform {
    use super::IslandState;
    use tauri::{Runtime, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, IsWindow, SetForegroundWindow,
        SetWindowDisplayAffinity, SetWindowLongPtrW, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_APPWINDOW, WM_STYLECHANGING, STYLESTRUCT,
    };

    pub(super) fn overlay_style(style: u32) -> u32 {
        (style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0
    }

    /// Tao rebuilds extended styles when mouse hit testing changes. Preserve
    /// the tool-window flag *before* those styles reach Windows: otherwise an
    /// interactive, display-sized canvas looks opaque to browser occlusion
    /// tracking and video underneath can stop rendering.
    unsafe extern "system" fn preserve_overlay_style(
        hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM,
        _id: usize, _data: usize,
    ) -> LRESULT {
        if message == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
            let styles = lparam.0 as *mut STYLESTRUCT;
            if !styles.is_null() {
                unsafe { (*styles).styleNew = overlay_style((*styles).styleNew); }
            }
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// Left out of screenshots and recordings when the person asks.
    pub fn show_in_captures<R: Runtime>(window: &WebviewWindow<R>, show: bool) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let _ = SetWindowDisplayAffinity(
                hwnd,
                if show {
                    WDA_NONE
                } else {
                    WDA_EXCLUDEFROMCAPTURE
                },
            );
        }
    }

    /// No taskbar button, no Alt+Tab entry, and clicking the closed island
    /// does not take focus from the app the person is working in.
    pub fn style<R: Runtime>(window: &WebviewWindow<R>) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(preserve_overlay_style), 3, 0);
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                overlay_style(ex as u32) as isize | WS_EX_NOACTIVATE.0 as isize,
            );
        }
    }

    pub fn set_focusable<R: Runtime>(
        window: &WebviewWindow<R>,
        state: &IslandState,
        focusable: bool,
        take: bool,
    ) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let ex = if focusable {
                ex & !(WS_EX_NOACTIVATE.0 as isize)
            } else {
                ex | WS_EX_NOACTIVATE.0 as isize
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
            let foreground = GetForegroundWindow();
            if focusable && take {
                if foreground != hwnd {
                    *state.previous_foreground.lock().unwrap() = foreground.0 as isize;
                }
                let _ = SetForegroundWindow(hwnd);
                let _ = window.set_focus();
            } else if !focusable {
                let previous = HWND(std::mem::replace(
                    &mut *state.previous_foreground.lock().unwrap(),
                    0,
                ) as _);
                if foreground == hwnd && !previous.0.is_null() && IsWindow(Some(previous)).as_bool()
                {
                    let _ = SetForegroundWindow(previous);
                }
            }
        }
    }

    pub fn mouse_buttons_down() -> bool {
        unsafe {
            [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
                .iter()
                .any(|k| (GetAsyncKeyState(k.0 as i32) as u16 & 0x8000) != 0)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::IslandState;
    use tauri::{Runtime, WebviewWindow};

    pub fn style<R: Runtime>(_window: &WebviewWindow<R>) {}

    pub fn show_in_captures<R: Runtime>(window: &WebviewWindow<R>, show: bool) {
        let _ = window.set_content_protected(!show);
    }

    pub fn set_focusable<R: Runtime>(
        window: &WebviewWindow<R>,
        _state: &IslandState,
        focusable: bool,
        take: bool,
    ) {
        if focusable && take {
            let _ = window.set_focus();
        }
    }

    /// The Mac build is only for designing; outside clicks come from the page there.
    pub fn mouse_buttons_down() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::HitRect;

    #[cfg(windows)]
    #[test]
    fn mouse_and_focus_style_updates_keep_the_video_overlay_non_occluding() {
        use windows::Win32::UI::WindowsAndMessaging::{
            WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        };
        let base = WS_EX_TOPMOST.0 | WS_EX_NOREDIRECTIONBITMAP.0;
        // Closing, hovering, clicking, focusing an input and leaving the island
        // all rebuild this style in Tao; each must remain a tool window.
        for generated in [
            base | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0,
            base,
            base | WS_EX_APPWINDOW.0,
            base | WS_EX_NOACTIVATE.0,
        ] {
            let actual = super::platform::overlay_style(generated);
            assert_ne!(actual & WS_EX_TOOLWINDOW.0, 0);
            assert_eq!(actual & WS_EX_APPWINDOW.0, 0);
            assert_eq!(actual & !(WS_EX_TOOLWINDOW.0 | WS_EX_APPWINDOW.0),
                generated & !(WS_EX_TOOLWINDOW.0 | WS_EX_APPWINDOW.0));
        }
    }

    fn rect(top: f64, bottom: f64) -> HitRect {
        HitRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
            top_radius: top,
            bottom_radius: bottom,
        }
    }

    #[test]
    fn rounded_bottom_corners_let_the_pointer_through() {
        let r = rect(0.0, 20.0);
        assert!(r.contains(50.0, 20.0));
        assert!(
            r.contains(0.0, 0.0),
            "square top corner belongs to the island"
        );
        assert!(!r.contains(1.0, 39.0));
        assert!(!r.contains(99.0, 39.0));
        assert!(r.contains(50.0, 39.0));
        assert!(!r.contains(101.0, 20.0));
    }

    #[test]
    fn a_circle_is_a_rect_rounded_all_round() {
        let c = HitRect {
            x: 10.0,
            y: 10.0,
            width: 44.0,
            height: 44.0,
            top_radius: 22.0,
            bottom_radius: 22.0,
        };
        assert!(c.contains(32.0, 32.0));
        assert!(!c.contains(11.0, 11.0));
        assert!(!c.contains(53.0, 53.0));
        assert!(c.contains(32.0, 10.5));
    }

    #[test]
    fn deserializes_from_the_page() {
        let r: HitRect = serde_json::from_str(
            r#"{"x":1,"y":2,"width":3,"height":4,"topRadius":0,"bottomRadius":8}"#,
        )
        .unwrap();
        assert_eq!(r.bottom_radius, 8.0);
    }
}
