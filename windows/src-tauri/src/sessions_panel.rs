// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The panel the sessions' tray icon opens (#331): the island's Sessions page
//! in a window of its own, next to the taskbar's corner where the icon sits.
//! On Windows a click on the icon opens it beside the icon; Linux has no
//! click, so the icon's menu opens it in the corner by the panel that holds
//! the tray. It closes when it loses the keyboard, as a flyout does.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

pub const LABEL: &str = "sessions-panel";
/// The panel in CSS pixels: the island's Sessions page with its top row.
const SIZE: (f64, f64) = (460.0, 440.0);
/// Enlarged on a session's page, as the island enlarges.
const ENLARGED: (f64, f64) = (720.0, 640.0);
/// The shortest it fits a list to: a row with room around it.
const SHORTEST: f64 = 172.0;
/// Room kept from the taskbar and the screen's edges.
const MARGIN: f64 = 10.0;
/// A click on the icon that closed the panel by taking its keyboard must
/// not open it again a moment later.
const RECLICK: Duration = Duration::from_millis(350);
/// The keyboard can come and go while the window maps (a window manager
/// placing it, a Wayland surface set up): that is not the person leaving.
const SETTLE: Duration = Duration::from_millis(600);

/// The session to open on, sent to the panel's page.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Focus {
    pub session: String,
    pub request: Option<String>,
}

/// A rectangle on screen in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    fn right(&self) -> f64 {
        self.x + self.width
    }
    fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// The screen edge the taskbar or panel holding the tray sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Which edge the bar holds, by what it takes from the monitor; `fallback`
/// when it takes nothing (hidden, or a Wayland desktop that does not say).
pub fn bar_edge(monitor: Area, work: Area, fallback: Edge) -> Edge {
    let taken = [
        (Edge::Top, work.y - monitor.y),
        (Edge::Bottom, monitor.bottom() - work.bottom()),
        (Edge::Left, work.x - monitor.x),
        (Edge::Right, monitor.right() - work.right()),
    ];
    taken
        .into_iter()
        .filter(|(_, room)| *room > 0.5)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(fallback, |(edge, _)| edge)
}

/// Where the panel's top-left corner goes: against the bar, beside the
/// icon when its place is known, else at the bar's far end where the tray
/// is, and always inside the work area.
pub fn place(
    size: (f64, f64),
    icon: Option<Area>,
    work: Area,
    edge: Edge,
    margin: f64,
) -> (f64, f64) {
    let (width, height) = (
        size.0.min(work.width - margin * 2.0),
        size.1.min(work.height - margin * 2.0),
    );
    let clamp_x = |x: f64| {
        x.clamp(
            work.x + margin,
            (work.right() - margin - width).max(work.x + margin),
        )
    };
    let clamp_y = |y: f64| {
        y.clamp(
            work.y + margin,
            (work.bottom() - margin - height).max(work.y + margin),
        )
    };
    let center = icon.map(|icon| (icon.x + icon.width / 2.0, icon.y + icon.height / 2.0));
    match edge {
        Edge::Bottom | Edge::Top => {
            let x = center.map_or(work.right() - margin - width, |(cx, _)| cx - width / 2.0);
            let y = if edge == Edge::Top {
                work.y + margin
            } else {
                work.bottom() - margin - height
            };
            (clamp_x(x), clamp_y(y))
        }
        Edge::Left | Edge::Right => {
            let y = center.map_or(work.bottom() - margin - height, |(_, cy)| cy - height / 2.0);
            let x = if edge == Edge::Left {
                work.x + margin
            } else {
                work.right() - margin - width
            };
            (clamp_x(x), clamp_y(y))
        }
    }
}

#[derive(Default)]
struct State {
    enlarged: bool,
    /// The height the page asked for around its list; None for the full panel.
    fitted: Option<f64>,
    pinned: bool,
    visible: bool,
    /// The session the panel was last opened on, for a page that loads
    /// after the event telling it went by (the window's first showing).
    focus: Option<Focus>,
    shown_at: Option<Instant>,
    hidden_at: Option<Instant>,
    /// The icon the panel opened from, kept to place it again as it resizes.
    icon: Option<Area>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Mutex::default)
}

/// The panel's size now, in CSS pixels.
fn size() -> (f64, f64) {
    let state = state()
        .lock()
        .map(|s| (s.enlarged, s.fitted))
        .unwrap_or((false, None));
    match state {
        (true, _) => ENLARGED,
        (false, Some(height)) => (SIZE.0, height.clamp(SHORTEST, SIZE.1)),
        (false, None) => SIZE,
    }
}

/// The bar's edge where it cannot be read: Windows keeps its taskbar at the
/// bottom, Linux desktops their top bar (GNOME, sway's bar) unless KDE.
fn default_edge() -> Edge {
    #[cfg(target_os = "linux")]
    {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if desktop.to_ascii_uppercase().contains("KDE") {
            return Edge::Bottom;
        }
        Edge::Top
    }
    #[cfg(not(target_os = "linux"))]
    Edge::Bottom
}

/// Places the window by its icon, or by the bar of the monitor the pointer is on.
fn position<R: Runtime>(window: &WebviewWindow<R>) {
    let app = window.app_handle();
    let icon = state().lock().ok().and_then(|s| s.icon);
    let monitor = icon
        .and_then(|i| {
            app.monitor_from_point(i.x + i.width / 2.0, i.y + i.height / 2.0)
                .ok()
                .flatten()
        })
        .or_else(|| {
            app.cursor_position()
                .ok()
                .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let area = Area {
        x: f64::from(monitor.position().x),
        y: f64::from(monitor.position().y),
        width: f64::from(monitor.size().width),
        height: f64::from(monitor.size().height),
    };
    let rect = monitor.work_area();
    let work = Area {
        x: f64::from(rect.position.x),
        y: f64::from(rect.position.y),
        width: f64::from(rect.size.width),
        height: f64::from(rect.size.height),
    };
    let (w, h) = size();
    let physical = (w * scale, h * scale);
    let edge = bar_edge(area, work, default_edge());
    let (x, y) = place(physical, icon, work, edge, MARGIN * scale);
    let width = physical.0.min(work.width - MARGIN * scale * 2.0);
    let height = physical.1.min(work.height - MARGIN * scale * 2.0);
    let _ = window.set_size(PhysicalSize::new(
        width.round() as u32,
        height.round() as u32,
    ));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    #[cfg(target_os = "linux")]
    crate::linux::sessions_panel::set_size(window, width / scale, height / scale);
}

fn build<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return Some(window);
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("panel.html".into()))
        .title("Openbar Sessions")
        .inner_size(SIZE.0, SIZE.1)
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
        .build()
        .ok()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            let keeps = state()
                .lock()
                .map(|s| s.pinned || s.shown_at.is_some_and(|at| at.elapsed() < SETTLE))
                .unwrap_or(false);
            if !keeps {
                hide(&handle);
            }
        }
    });
    Some(window)
}

/// Shows the panel, by the tray icon when its place is known, on a
/// session's page when `focus` names one. Built on a worker: on Windows a
/// webview built on the event loop's thread never loads.
pub fn show<R: Runtime>(
    app: &AppHandle<R>,
    icon: Option<Area>,
    focus: Option<Focus>,
    take_focus: bool,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(window) = build(&app) else {
            crate::diagnostics::log::error(
                "sessions-panel",
                "the panel's window could not be made",
            );
            return;
        };
        if let Ok(mut state) = state().lock() {
            if icon.is_some() {
                state.icon = icon;
            }
            state.visible = true;
            state.focus = focus.clone();
            state.shown_at = Some(Instant::now());
            state.hidden_at = None;
        }
        #[cfg(target_os = "linux")]
        crate::linux::sessions_panel::prepare(&window, default_edge() == Edge::Top);
        position(&window);
        if let Some(focus) = focus {
            let _ = window.emit_to(LABEL, "sessions-panel://focus", Some(focus));
        }
        let shown = window.show();
        if take_focus {
            let _ = window.set_focus();
        }
        crate::diagnostics::log::info(
            "sessions-panel",
            &format!(
                "shown {} ({}, visible {:?}, size {:?})",
                if icon.is_some() {
                    "by the tray icon"
                } else {
                    "in the bar's corner"
                },
                if shown.is_ok() { "ok" } else { "show failed" },
                window.is_visible().ok(),
                window.inner_size().ok()
            ),
        );
        let _ = app.emit("sessions-panel://visible", true);
    });
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    let was = state().lock().map(|mut s| {
        let was = s.visible;
        s.visible = false;
        s.enlarged = false;
        s.focus = None;
        if was {
            s.hidden_at = Some(Instant::now());
        }
        was
    });
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
    if was.unwrap_or(false) {
        crate::diagnostics::log::info("sessions-panel", "hidden");
        let _ = app.emit("sessions-panel://visible", false);
    }
}

/// A click on the tray icon: opens the panel beside it, or closes it.
pub fn toggle_from_tray<R: Runtime>(app: &AppHandle<R>, rect: tauri::Rect) {
    let (visible, just_hidden) = state()
        .lock()
        .map(|s| {
            (
                s.visible,
                s.hidden_at.is_some_and(|at| at.elapsed() < RECLICK),
            )
        })
        .unwrap_or((false, false));
    if visible {
        hide(app);
        return;
    }
    // The click took the keyboard from the open panel, which closed: it stays closed.
    if just_hidden {
        return;
    }
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    let icon = Area {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    show(app, Some(icon), None, true);
}

/// What the panel's page asks as it loads: whether it shows, and on which session.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Current {
    pub visible: bool,
    pub focus: Option<Focus>,
}

#[tauri::command]
pub fn sessions_panel_current() -> Current {
    state()
        .lock()
        .map(|s| Current {
            visible: s.visible,
            focus: s.focus.clone(),
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn sessions_panel_show<R: Runtime>(app: AppHandle<R>, focus: Option<Focus>, take_focus: bool) {
    show(&app, None, focus, take_focus);
}

#[tauri::command]
pub fn sessions_panel_hide<R: Runtime>(app: AppHandle<R>) {
    hide(&app);
}

/// The page's list wants this tall a window (CSS pixels); None for the full panel.
#[tauri::command]
pub fn sessions_panel_fit<R: Runtime>(app: AppHandle<R>, height: Option<f64>) {
    let height = height.filter(|h| h.is_finite() && *h > 0.0);
    let changed = state()
        .lock()
        .map(|mut s| {
            let changed = s.fitted != height;
            s.fitted = height;
            changed && s.visible
        })
        .unwrap_or(false);
    if changed && let Some(window) = app.get_webview_window(LABEL) {
        position(&window);
    }
}

#[tauri::command]
pub fn sessions_panel_set_enlarged<R: Runtime>(app: AppHandle<R>, on: bool) {
    let changed = state()
        .lock()
        .map(|mut s| {
            let changed = s.enlarged != on;
            s.enlarged = on;
            changed && s.visible
        })
        .unwrap_or(false);
    if changed && let Some(window) = app.get_webview_window(LABEL) {
        position(&window);
    }
}

/// Kept open while a dialog of the panel's own (a file picker) holds the keyboard.
#[tauri::command]
pub fn sessions_panel_set_pinned(on: bool) {
    if let Ok(mut state) = state().lock() {
        state.pinned = on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Area = Area {
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
    };

    fn area(x: f64, y: f64, width: f64, height: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_bar_is_where_the_work_area_gives_way() {
        let bottom = area(0.0, 0.0, 1920.0, 1032.0);
        assert_eq!(bar_edge(MONITOR, bottom, Edge::Top), Edge::Bottom);
        let top = area(0.0, 32.0, 1920.0, 1048.0);
        assert_eq!(bar_edge(MONITOR, top, Edge::Bottom), Edge::Top);
        let left = area(64.0, 0.0, 1856.0, 1080.0);
        assert_eq!(bar_edge(MONITOR, left, Edge::Bottom), Edge::Left);
        let right = area(0.0, 0.0, 1856.0, 1080.0);
        assert_eq!(bar_edge(MONITOR, right, Edge::Bottom), Edge::Right);
        // GNOME's top bar and a dock on the left: the larger strip is the bar's.
        let both = area(72.0, 32.0, 1848.0, 1048.0);
        assert_eq!(bar_edge(MONITOR, both, Edge::Bottom), Edge::Left);
        // A bar that hides itself, or a desktop that says nothing.
        assert_eq!(bar_edge(MONITOR, MONITOR, Edge::Top), Edge::Top);
        assert_eq!(bar_edge(MONITOR, MONITOR, Edge::Bottom), Edge::Bottom);
        // A second monitor to the right.
        let second = area(1920.0, 0.0, 2560.0, 1440.0);
        let second_work = area(1920.0, 0.0, 2560.0, 1392.0);
        assert_eq!(bar_edge(second, second_work, Edge::Top), Edge::Bottom);
    }

    #[test]
    fn the_panel_rises_from_the_taskbar_beside_its_icon() {
        let work = area(0.0, 0.0, 1920.0, 1032.0);
        let icon = area(1300.0, 1040.0, 24.0, 32.0);
        let (x, y) = place((460.0, 440.0), Some(icon), work, Edge::Bottom, 10.0);
        assert_eq!(x, 1312.0 - 230.0, "centered on the icon");
        assert_eq!(y, 1032.0 - 10.0 - 440.0, "standing on the taskbar");
        // An icon by the screen's corner keeps the panel inside it.
        let corner = area(1890.0, 1040.0, 24.0, 32.0);
        let (x, _) = place((460.0, 440.0), Some(corner), work, Edge::Bottom, 10.0);
        assert_eq!(x, 1920.0 - 10.0 - 460.0);
        // Opened from the menu: the bar's far end, where the tray is.
        let (x, y) = place((460.0, 440.0), None, work, Edge::Bottom, 10.0);
        assert_eq!((x, y), (1920.0 - 10.0 - 460.0, 1032.0 - 10.0 - 440.0));
    }

    #[test]
    fn the_panel_hangs_from_a_top_bar_and_stands_beside_a_side_one() {
        let top = area(0.0, 32.0, 1920.0, 1048.0);
        let (x, y) = place((460.0, 440.0), None, top, Edge::Top, 10.0);
        assert_eq!((x, y), (1920.0 - 10.0 - 460.0, 42.0));
        let left = area(64.0, 0.0, 1856.0, 1080.0);
        let icon = area(20.0, 900.0, 24.0, 24.0);
        let (x, y) = place((460.0, 440.0), Some(icon), left, Edge::Left, 10.0);
        assert_eq!(x, 74.0);
        assert_eq!(y, 1080.0 - 10.0 - 440.0, "kept above the screen's bottom");
        let right = area(0.0, 0.0, 1856.0, 1080.0);
        let (x, _) = place((460.0, 440.0), None, right, Edge::Right, 10.0);
        assert_eq!(x, 1856.0 - 10.0 - 460.0);
    }

    #[test]
    fn a_small_screen_shrinks_nothing_out_of_view() {
        let work = area(0.0, 0.0, 400.0, 300.0);
        let (x, y) = place((460.0, 440.0), None, work, Edge::Bottom, 10.0);
        assert_eq!((x, y), (10.0, 10.0));
    }

    #[test]
    fn a_focus_reaches_the_page_in_its_own_words() {
        let focus = Focus {
            session: "opencode:s1".into(),
            request: Some("per_1".into()),
        };
        assert_eq!(
            serde_json::to_value(&focus).unwrap(),
            serde_json::json!({"session": "opencode:s1", "request": "per_1"})
        );
    }
}
