// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's window: a transparent, always-on-top layer over the top of a
//! display. The page draws the island inside it; everywhere else the window
//! lets the pointer through to whatever is underneath. The page reports the
//! island's shape every frame, and a pointer watcher turns mouse input on
//! only while the pointer is over that shape.
//!
//! With "All displays" every other display gets a small mirror window showing
//! the closed island (NotchMirrorView; spec 01-shell §6.1). The island's page
//! sends what to draw; a click on a mirror brings the island there, open.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

pub const LABEL: &str = "island";
const MIRROR_LABEL: &str = "island-mirror-";
/// Room for the closed island and the companion's hops above nothing else.
const MIRROR_HEIGHT: f64 = 120.0;
/// A display change comes as a burst of messages (spec 01-shell §6.1).
const DISPLAY_SETTLE: Duration = Duration::from_millis(100);

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

/// What a mirror draws: the closed island's markup, the page's styles, and
/// its shape in CSS pixels from the frame's left and the window's top.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MirrorFrame {
    pub html: String,
    pub styles: Vec<String>,
    pub css: String,
    pub width: f64,
    pub hit: HitRect,
}

/// A display in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Display {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f64,
}

impl From<&tauri::Monitor> for Display {
    fn from(monitor: &tauri::Monitor) -> Self {
        Display {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
            scale: monitor.scale_factor(),
        }
    }
}

impl Display {
    fn at(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// Whether a screen point in physical pixels is on the frame centred at this display's top.
    fn over(&self, frame: &MirrorFrame, px: f64, py: f64) -> bool {
        let x = (px - self.x as f64) / self.scale;
        let y = (py - self.y as f64) / self.scale;
        let left = (self.width as f64 / self.scale - frame.width) / 2.0;
        frame.hit.contains(x - left, y)
    }
}

#[derive(Clone, Debug)]
struct Mirror {
    label: String,
    display: Display,
    shown: bool,
    fresh: bool,
}

/// The displays that get a mirror: with "All displays", all but the island's.
fn mirrored(
    choice: DisplayChoice,
    displays: &[Display],
    island: Option<(i32, i32)>,
) -> Vec<Display> {
    if choice != DisplayChoice::All {
        return Vec::new();
    }
    displays
        .iter()
        .filter(|d| Some(d.at()) != island)
        .copied()
        .collect()
}

/// Waits for a burst of display messages to end before acting once on it.
#[derive(Default)]
struct DisplaySettle {
    seen: u32,
    since: Option<Instant>,
}

impl DisplaySettle {
    fn poll(&mut self, count: u32, now: Instant) -> bool {
        if count != self.seen {
            self.seen = count;
            self.since = Some(now);
            return false;
        }
        if self
            .since
            .is_some_and(|since| now.duration_since(since) >= DISPLAY_SETTLE)
        {
            self.since = None;
            return true;
        }
        false
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
    /// Changed only by the pointer watcher, and never held across a window call.
    mirrors: Mutex<Vec<Mirror>>,
    mirror_frame: Mutex<Option<MirrorFrame>>,
    /// Whether the page should send frames: some display has a mirror.
    mirroring: AtomicBool,
    hidden_from_captures: AtomicBool,
    next_mirror: AtomicU32,
    /// The window that had the keyboard before the island took it.
    #[cfg(windows)]
    previous_foreground: Mutex<isize>,
}

impl IslandState {
    /// Whether a point in the window's CSS pixels is on the island's shape.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.rects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|r| r.contains(x, y))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DisplayChoice {
    #[default]
    Primary,
    Cursor,
    /// Every display shows the closed island; the real one follows the pointer.
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
        // The Files shelf registers its own drop target, which also takes
        // text and links (files::drop_target; spec 05 §6.4).
        .disable_drag_drop_handler()
        .build()?;
    place(&window, DisplayChoice::Primary)?;
    window.set_ignore_cursor_events(true)?;
    platform::style(&window);
    platform::watch_displays(&window);
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
        DisplayChoice::Primary => window.primary_monitor()?,
        DisplayChoice::Cursor | DisplayChoice::All => {
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
    // Physical sizes, position first: a logical size would be converted at
    // the scale of the display the window is leaving. Keep one stable
    // transparent canvas: opening a popup must not resize the native window
    // or trigger fullscreen transitions in the shell.
    window.set_position(PhysicalPosition::new(
        monitor.position().x,
        monitor.position().y,
    ))?;
    window.set_size(PhysicalSize::new(
        monitor.size().width,
        monitor.size().height,
    ))?;
    if let Some(state) = window.try_state::<IslandState>() {
        *state.placed_on.lock().unwrap() = Some((monitor.position().x, monitor.position().y));
    }
    Ok(())
}

/// Follow pointer: the island moves to the display the pointer is on, but
/// never while the person is using it.
fn follow_pointer<R: Runtime>(window: &WebviewWindow<R>, state: &IslandState) {
    let following = || {
        matches!(
            *state.display.lock().unwrap(),
            DisplayChoice::Cursor | DisplayChoice::All
        )
    };
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

/// Places the window again after a display change, on the same choice.
fn resync<R: Runtime>(window: &WebviewWindow<R>, state: &IslandState) {
    let choice = *state.display.lock().unwrap();
    let _ = place(window, choice);
    let _ = window.emit_to(LABEL, "island://display-changed", ());
}

/// Brings the mirror windows in line with the displays: one per display but
/// the island's, kept hidden rather than closed while the island is there.
fn sync_mirrors<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>, state: &IslandState) {
    let choice = *state.display.lock().unwrap();
    let displays: Vec<Display> = if choice == DisplayChoice::All {
        window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(Display::from)
            .collect()
    } else {
        Vec::new()
    };
    let before = state.mirrors.lock().unwrap().clone();
    if before.is_empty() && displays.len() < 2 {
        set_mirroring(app, state, false);
        return;
    }
    let island = *state.placed_on.lock().unwrap();
    let wanted = mirrored(choice, &displays, island);
    let has_frame = state.mirror_frame.lock().unwrap().is_some();
    let mut after = Vec::with_capacity(displays.len());
    // A display's mirror stays, hidden, while the island visits that display.
    for mirror in before {
        if displays.iter().any(|d| d.at() == mirror.display.at()) {
            after.push(mirror);
        } else if let Some(gone) = app.get_webview_window(&mirror.label) {
            let _ = gone.destroy();
        }
    }
    for display in &displays {
        let index = after.iter().position(|m| m.display.at() == display.at());
        let index = match index {
            Some(index) => index,
            None if wanted.contains(display) => match open_mirror(app, state, display) {
                Ok(mirror) => {
                    after.push(mirror);
                    after.len() - 1
                }
                Err(_) => continue,
            },
            None => continue,
        };
        let mirror = &mut after[index];
        let Some(handle) = app.get_webview_window(&mirror.label) else {
            continue;
        };
        if mirror.fresh || mirror.display != *display {
            let _ = position_mirror(&handle, display);
            mirror.display = *display;
            mirror.fresh = false;
        }
        let show = has_frame
            && wanted.iter().any(|d| d.at() == display.at())
            && !crate::fullscreen::hides_on(&handle);
        if show != mirror.shown {
            mirror.shown = show;
            let _ = if show { handle.show() } else { handle.hide() };
        }
    }
    let any = !after.is_empty();
    *state.mirrors.lock().unwrap() = after;
    set_mirroring(app, state, any);
}

fn set_mirroring<R: Runtime>(app: &AppHandle<R>, state: &IslandState, on: bool) {
    if state.mirroring.swap(on, Ordering::Relaxed) != on {
        let _ = app.emit_to(LABEL, "island://mirroring", on);
    }
}

fn position_mirror<R: Runtime>(window: &WebviewWindow<R>, display: &Display) -> tauri::Result<()> {
    window.set_position(PhysicalPosition::new(display.x, display.y))?;
    window.set_size(PhysicalSize::new(
        display.width,
        (MIRROR_HEIGHT * display.scale).round() as u32,
    ))
}

fn open_mirror<R: Runtime>(
    app: &AppHandle<R>,
    state: &IslandState,
    display: &Display,
) -> tauri::Result<Mirror> {
    let label = format!(
        "{MIRROR_LABEL}{}",
        state.next_mirror.fetch_add(1, Ordering::Relaxed)
    );
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("mirror.html".into()))
        .title("Vorssaint Island")
        .inner_size(display.width as f64 / display.scale, MIRROR_HEIGHT)
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
        .disable_drag_drop_handler()
        .build()?;
    window.set_ignore_cursor_events(true)?;
    platform::style(&window);
    platform::show_in_captures(&window, !state.hidden_from_captures.load(Ordering::Relaxed));
    Ok(Mirror {
        label,
        display: *display,
        shown: false,
        fresh: true,
    })
}

/// The mirror under the pointer, which takes its click.
fn mirror_under_pointer<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &IslandState,
) -> Option<String> {
    let frame = state.mirror_frame.lock().unwrap().clone()?;
    let mirrors = state.mirrors.lock().unwrap().clone();
    if !mirrors.iter().any(|m| m.shown) {
        return None;
    }
    let pointer = window.cursor_position().ok()?;
    mirrors
        .iter()
        .find(|m| m.shown && m.display.over(&frame, pointer.x, pointer.y))
        .map(|m| m.label.clone())
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
        let mut settle = DisplaySettle::default();
        let mut taking_mirror: Option<String> = None;
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
            let display_changed = settle.poll(platform::display_changes(), Instant::now());
            if display_changed {
                resync(&window, &state);
            }
            // A screen selection covers the display: the island stays out of it.
            let selecting = app.get_webview_window(crate::capture::LABEL).is_some();
            let inside = !selecting && state.rects.lock().unwrap().iter().any(|r| r.contains(x, y));
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
            if display_changed || ticks.is_multiple_of(16) {
                sync_mirrors(&app, &window, &state);
            }
            let over_mirror = if selecting {
                None
            } else {
                mirror_under_pointer(&window, &state)
            };
            if over_mirror != taking_mirror {
                for (label, ignore) in [(&taking_mirror, true), (&over_mirror, false)] {
                    if let Some(mirror) = label.as_ref().and_then(|l| app.get_webview_window(l)) {
                        let _ = mirror.set_ignore_cursor_events(ignore);
                    }
                }
                taking_mirror = over_mirror;
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

/// What the mirrors draw; None draws nothing (the island is off or hidden).
#[tauri::command]
pub fn island_mirror_update<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, IslandState>,
    frame: Option<MirrorFrame>,
) {
    *state.mirror_frame.lock().unwrap() = frame.clone();
    let labels: Vec<String> = state
        .mirrors
        .lock()
        .unwrap()
        .iter()
        .map(|m| m.label.clone())
        .collect();
    for label in labels {
        let _ = app.emit_to(&label, "island://mirror-frame", &frame);
    }
}

/// A mirror page that just loaded asks for the current frame.
#[tauri::command]
pub fn island_mirror_frame(state: tauri::State<'_, IslandState>) -> Option<MirrorFrame> {
    state.mirror_frame.lock().unwrap().clone()
}

#[tauri::command]
pub fn island_mirroring(state: tauri::State<'_, IslandState>) -> bool {
    state.mirroring.load(Ordering::Relaxed)
}

/// A click on a mirror: the island moves to that display and answers as if
/// it had been clicked there.
#[tauri::command]
pub async fn island_mirror_open<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
) -> Result<(), String> {
    let main = app.get_webview_window(LABEL).ok_or("no island window")?;
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no display")?;
    move_to(&main, &monitor).map_err(|e| e.to_string())?;
    main.emit_to(LABEL, "island://open-here", ())
        .map_err(|e| e.to_string())
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

/// The window that had the keyboard before the island took it (0 for none),
/// where the clipboard pastes an entry.
#[cfg(windows)]
pub fn previous_foreground<R: Runtime>(app: &AppHandle<R>) -> isize {
    app.try_state::<IslandState>()
        .map_or(0, |state| *state.previous_foreground.lock().unwrap())
}

/// Settings › Behavior › Show in screenshots.
pub fn show_in_captures<R: Runtime>(app: &AppHandle<R>, show: bool) {
    if let Some(window) = app.get_webview_window(LABEL) {
        platform::show_in_captures(&window, show);
    }
    let Some(state) = app.try_state::<IslandState>() else {
        return;
    };
    state.hidden_from_captures.store(!show, Ordering::Relaxed);
    let mirrors = state.mirrors.lock().unwrap().clone();
    for mirror in mirrors {
        if let Some(window) = app.get_webview_window(&mirror.label) {
            platform::show_in_captures(&window, show);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::IslandState;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tauri::{Runtime, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
    };
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, IsWindow, SPI_SETWORKAREA,
        STYLESTRUCT, SetForegroundWindow, SetWindowDisplayAffinity, SetWindowLongPtrW,
        WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE,
        WM_STYLECHANGING, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    static DISPLAY_CHANGES: AtomicU32 = AtomicU32::new(0);

    pub fn display_changes() -> u32 {
        DISPLAY_CHANGES.load(Ordering::Relaxed)
    }

    /// A monitor plugged in or out, a new resolution or scale, or a taskbar
    /// that moved: the island's own window hears them all.
    unsafe extern "system" fn notice_display_change(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if message == WM_DISPLAYCHANGE
            || message == WM_DPICHANGED
            || (message == WM_SETTINGCHANGE && wparam.0 as u32 == SPI_SETWORKAREA.0)
        {
            DISPLAY_CHANGES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    pub fn watch_displays<R: Runtime>(window: &WebviewWindow<R>) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(notice_display_change), 4, 0);
        }
    }

    pub(super) fn overlay_style(style: u32) -> u32 {
        (style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0
    }

    /// Tao rebuilds extended styles when mouse hit testing changes. Preserve
    /// the tool-window flag *before* those styles reach Windows: otherwise an
    /// interactive, display-sized canvas looks opaque to browser occlusion
    /// tracking and video underneath can stop rendering.
    unsafe extern "system" fn preserve_overlay_style(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if message == WM_STYLECHANGING && wparam.0 as i32 == GWL_EXSTYLE.0 {
            let styles = lparam.0 as *mut STYLESTRUCT;
            if !styles.is_null() {
                unsafe {
                    (*styles).styleNew = overlay_style((*styles).styleNew);
                }
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

    pub fn watch_displays<R: Runtime>(_window: &WebviewWindow<R>) {}

    pub fn display_changes() -> u32 {
        0
    }

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
    use super::{Display, DisplayChoice, DisplaySettle, HitRect, MirrorFrame, mirrored};
    use std::time::{Duration, Instant};

    fn display(x: i32, width: u32, scale: f64) -> Display {
        Display {
            x,
            y: 0,
            width,
            height: 1080,
            scale,
        }
    }

    #[test]
    fn only_all_displays_has_mirrors_and_never_on_the_islands_display() {
        let displays = [
            display(0, 1920, 1.0),
            display(1920, 2560, 1.5),
            display(-1280, 1280, 1.0),
        ];
        assert!(mirrored(DisplayChoice::Primary, &displays, Some((0, 0))).is_empty());
        assert!(mirrored(DisplayChoice::Cursor, &displays, Some((0, 0))).is_empty());
        assert_eq!(
            mirrored(DisplayChoice::All, &displays, Some((1920, 0))),
            [displays[0], displays[2]]
        );
        assert!(mirrored(DisplayChoice::All, &displays[..1], Some((0, 0))).is_empty());
        // The island's display just went away: every display left has a mirror
        // until the island lands on one.
        assert_eq!(
            mirrored(DisplayChoice::All, &displays[..1], Some((1920, 0))).len(),
            1
        );
    }

    #[test]
    fn a_mirror_answers_on_its_own_display_at_that_displays_scale() {
        let frame = MirrorFrame {
            width: 200.0,
            hit: HitRect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 30.0,
                top_radius: 0.0,
                bottom_radius: 0.0,
            },
            ..Default::default()
        };
        // 2560 physical at 150 % is 1706.67 CSS wide, its frame centred there.
        let right = display(1920, 2560, 1.5);
        let centre = 1920.0 + 1280.0;
        assert!(right.over(&frame, centre, 10.0));
        assert!(right.over(&frame, centre + 149.0, 44.0));
        assert!(!right.over(&frame, centre + 151.0, 10.0));
        assert!(!right.over(&frame, centre, 46.0));
        assert!(
            !right.over(&frame, 960.0, 10.0),
            "another display's pointer"
        );
        let left = display(-1280, 1280, 1.0);
        assert!(left.over(&frame, -640.0, 10.0));
    }

    #[test]
    fn a_burst_of_display_messages_acts_once_after_it_ends() {
        let mut settle = DisplaySettle::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        assert!(!settle.poll(0, at(0)), "nothing happened yet");
        assert!(!settle.poll(1, at(0)));
        assert!(!settle.poll(1, at(50)));
        assert!(!settle.poll(3, at(60)), "still arriving");
        assert!(!settle.poll(3, at(159)));
        assert!(settle.poll(3, at(160)));
        assert!(!settle.poll(3, at(500)), "only once");
    }

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
            assert_eq!(
                actual & !(WS_EX_TOOLWINDOW.0 | WS_EX_APPWINDOW.0),
                generated & !(WS_EX_TOOLWINDOW.0 | WS_EX_APPWINDOW.0)
            );
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
