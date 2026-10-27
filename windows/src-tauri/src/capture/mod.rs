// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Screen capture for the Tools section (screenshot, Copy text from screen,
//! colour picker) and for Watch (spec 05 §4–5, §7, §15.2).
//!
//! A selection freezes the display under the pointer first ([`snapshot`]:
//! DXGI duplication, else `BitBlt`), then covers that display with the
//! `capture` window (`capture.html`, transparent, topmost, left out of
//! captures) showing the frozen picture dimmed. The person drags an area
//! (Enter or Space takes the whole display, Escape cancels) or, picking a
//! colour, clicks a pixel under a loupe. The island is collapsed by the page
//! before it asks, so it's out of the way.
//!
//! ## API for other sections
//!
//! ```ignore
//! // Blocks until the person chooses or cancels; run off the main thread.
//! let selection: Option<capture::Selection> = capture::select_area(&app, capture::Purpose::Watch).await;
//! // selection.rect   — the area, in desktop (virtual-screen) physical pixels
//! // selection.monitor — the display it was chosen on, same units
//! // selection.window — the top-level window under the area's centre, not
//! //                    Vorssaint's own; 0 over the desktop or nothing
//! let text = ocr::recognize(&rgba, width, height)?.text(); // see ocr.rs
//! ```
//!
//! Only one selection runs at a time; a second asks get `None` at once.
//! `clipboard::set_text` / `clipboard::set_image` write the clipboard.

pub mod clipboard;
pub mod color;
pub mod snapshot;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl, WebviewWindowBuilder};

use self::color::ColorFormat;
use self::snapshot::Frame;

pub const LABEL: &str = "capture";

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    Screenshot,
    Text,
    Color,
    Watch,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub purpose: Purpose,
    pub rect: Rect,
    pub monitor: Rect,
    pub window: isize,
}

/// What the overlay page answers, in the frozen picture's pixels.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Pick {
    Area { x: i32, y: i32, width: i32, height: i32 },
    Point { x: i32, y: i32 },
}

struct Session {
    id: u64,
    purpose: Purpose,
    frame: Arc<Frame>,
    reply: Option<mpsc::Sender<Option<Pick>>>,
}

#[derive(Default)]
pub struct Shared {
    session: Mutex<Option<Session>>,
    next: Mutex<u64>,
}

/// What the overlay page needs to draw.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    id: u64,
    purpose: Purpose,
    width: u32,
    height: u32,
    color_format: ColorFormat,
    bare_hex: bool,
}

/// What a tool did, for the island's notice.
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Outcome {
    Cancelled,
    /// Saved (when `path` is set) and copied (when `copied`).
    Screenshot {
        path: Option<String>,
        folder: Option<String>,
        copied: bool,
        thumbnail: Option<String>,
    },
    Text {
        text: String,
    },
    NoText,
    NoLanguage,
    Color {
        value: String,
        hex: String,
    },
    Failed {
        message: String,
    },
}

/// Waits for the person to choose an area (or a point when picking a
/// colour) on the display under the pointer. See the module docs.
#[allow(dead_code, reason = "the Watch section's way in")]
pub async fn select_area<R: Runtime>(app: &AppHandle<R>, purpose: Purpose) -> Option<Selection> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || select(&app, purpose).map(|(s, _)| s))
        .await
        .ok()
        .flatten()
}

/// The selection and the frozen picture it was made on. Blocking.
fn select<R: Runtime>(app: &AppHandle<R>, purpose: Purpose) -> Option<(Selection, Arc<Frame>)> {
    let shared = app.state::<Shared>();
    if shared.session.lock().unwrap().is_some() {
        return None;
    }
    let (handle, monitor) = platform::monitor_under_cursor()?;
    let frame = Arc::new(snapshot::capture(handle, monitor)?);
    let (tx, rx) = mpsc::channel();
    let id = {
        let mut next = shared.next.lock().unwrap();
        *next += 1;
        *next
    };
    *shared.session.lock().unwrap() = Some(Session {
        id,
        purpose,
        frame: frame.clone(),
        reply: Some(tx),
    });
    if open_overlay(app, monitor).is_err() {
        *shared.session.lock().unwrap() = None;
        return None;
    }
    // Escape cancels even while another app keeps the keyboard.
    listen_escape(app, true);
    // A closed overlay drops the sender, which ends the wait with nothing.
    let pick = rx.recv().ok().flatten();
    listen_escape(app, false);
    *shared.session.lock().unwrap() = None;
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
    let rect = match pick? {
        Pick::Area { x, y, width, height } => {
            let r = clamp_area(Rect { x, y, width, height }, frame.width, frame.height)?;
            Rect { x: monitor.x + r.x, y: monitor.y + r.y, ..r }
        }
        Pick::Point { x, y } => {
            frame.pixel(x, y)?;
            Rect { x: monitor.x + x, y: monitor.y + y, width: 1, height: 1 }
        }
    };
    let (cx, cy) = rect.center();
    let window = platform::window_at(cx, cy);
    Some((Selection { purpose, rect, monitor, window }, frame))
}

/// The area inside a `width` × `height` picture, or none when empty.
fn clamp_area(r: Rect, width: u32, height: u32) -> Option<Rect> {
    let x0 = r.x.clamp(0, width as i32);
    let y0 = r.y.clamp(0, height as i32);
    let x1 = (r.x + r.width).clamp(0, width as i32);
    let y1 = (r.y + r.height).clamp(0, height as i32);
    (x1 > x0 && y1 > y0).then(|| Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
}

fn open_overlay<R: Runtime>(app: &AppHandle<R>, monitor: Rect) -> tauri::Result<()> {
    if let Some(stale) = app.get_webview_window(LABEL) {
        let _ = stale.destroy();
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("capture.html".into()))
        .title("Vorssaint Capture")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    window.set_position(PhysicalPosition::new(monitor.x, monitor.y))?;
    window.set_size(PhysicalSize::new(monitor.width as u32, monitor.height as u32))?;
    platform::exclude_from_capture(&window);
    let app = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            cancel(&app);
        }
    });
    Ok(())
}

fn cancel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(session) = app.state::<Shared>().session.lock().unwrap().as_mut()
        && let Some(reply) = session.reply.take()
    {
        let _ = reply.send(None);
    }
}

fn listen_escape<R: Runtime>(app: &AppHandle<R>, on: bool) {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
    let escape = Shortcut::new(None, Code::Escape);
    let shortcuts = app.global_shortcut();
    if on {
        let _ = shortcuts.on_shortcut(escape, |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                cancel(app);
            }
        });
    } else if shortcuts.is_registered(escape) {
        let _ = shortcuts.unregister(escape);
    }
}

fn island_pref(app: &AppHandle<impl Runtime>, key: &str) -> Option<Value> {
    app.try_state::<crate::prefs::Prefs>()?.get().pointer(&format!("/island/{key}")).cloned()
}

fn color_prefs(app: &AppHandle<impl Runtime>) -> (ColorFormat, bool) {
    let format = ColorFormat::from_pref(island_pref(app, "colorPickerFormat").as_ref().and_then(Value::as_str));
    let bare = island_pref(app, "colorPickerBareHex").and_then(|v| v.as_bool()).unwrap_or(false);
    (format, bare)
}

// MARK: Commands for the overlay page

#[tauri::command]
pub fn capture_session<R: Runtime>(app: AppHandle<R>) -> Option<SessionInfo> {
    let (color_format, bare_hex) = color_prefs(&app);
    let shared = app.state::<Shared>();
    let session = shared.session.lock().unwrap();
    session.as_ref().map(|s| SessionInfo {
        id: s.id,
        purpose: s.purpose,
        width: s.frame.width,
        height: s.frame.height,
        color_format,
        bare_hex,
    })
}

/// The frozen picture as raw RGBA, sent as bytes rather than JSON.
#[tauri::command]
pub fn capture_pixels(shared: tauri::State<'_, Shared>, id: u64) -> Result<tauri::ipc::Response, String> {
    let session = shared.session.lock().unwrap();
    let session = session.as_ref().filter(|s| s.id == id).ok_or("no capture")?;
    Ok(tauri::ipc::Response::new(session.frame.rgba.clone()))
}

/// The page drew the picture: show it over the display and take the keyboard.
#[tauri::command]
pub fn capture_ready<R: Runtime>(app: AppHandle<R>, id: u64) {
    let current = app.state::<Shared>().session.lock().unwrap().as_ref().map(|s| s.id);
    if current != Some(id) {
        return;
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.show();
        platform::take_foreground(&window);
        let _ = window.set_focus();
    }
}

/// The person's choice, or `None` to cancel.
#[tauri::command]
pub fn capture_commit(shared: tauri::State<'_, Shared>, id: u64, pick: Option<Pick>) {
    if let Some(session) = shared.session.lock().unwrap().as_mut().filter(|s| s.id == id)
        && let Some(reply) = session.reply.take()
    {
        let _ = reply.send(pick);
    }
}

// MARK: The Tools

/// Runs a tool from start to finish: select, then save and copy, read the
/// text, or copy the colour. `name` is the screenshot's file name without
/// extension, in the person's local time (the page knows it).
#[tauri::command]
pub async fn capture_run<R: Runtime>(app: AppHandle<R>, purpose: Purpose, name: String) -> Outcome {
    tauri::async_runtime::spawn_blocking(move || run(&app, purpose, &name))
        .await
        .unwrap_or(Outcome::Failed { message: "capture stopped".into() })
}

fn run<R: Runtime>(app: &AppHandle<R>, purpose: Purpose, name: &str) -> Outcome {
    let Some((selection, frame)) = select(app, purpose) else {
        return Outcome::Cancelled;
    };
    let local = Rect {
        x: selection.rect.x - selection.monitor.x,
        y: selection.rect.y - selection.monitor.y,
        ..selection.rect
    };
    match purpose {
        Purpose::Screenshot | Purpose::Watch => screenshot(&frame.crop(local), name),
        Purpose::Text => {
            let crop = frame.crop(local);
            match crate::ocr::recognize(&crop.rgba, crop.width, crop.height) {
                Err(crate::ocr::OcrError::NoLanguage) => Outcome::NoLanguage,
                Err(e) => Outcome::Failed { message: e.to_string() },
                Ok(result) => match result.text() {
                    text if text.trim().is_empty() => Outcome::NoText,
                    text => match clipboard::set_text(&text) {
                        Ok(()) => Outcome::Text { text },
                        Err(message) => Outcome::Failed { message },
                    },
                },
            }
        }
        Purpose::Color => {
            let Some([r, g, b]) = frame.pixel(local.x, local.y) else {
                return Outcome::Cancelled;
            };
            let (format, bare) = color_prefs(app);
            let value = color::format(r, g, b, format, bare);
            match clipboard::set_text(&value) {
                Ok(()) => Outcome::Color { value, hex: color::format(r, g, b, ColorFormat::Hex, false) },
                Err(message) => Outcome::Failed { message },
            }
        }
    }
}

/// Saves to the Screenshots folder and copies; either alone still counts.
fn screenshot(crop: &Frame, name: &str) -> Outcome {
    let Some(png) = crate::shell::encode_png(&crop.rgba, crop.width, crop.height) else {
        return Outcome::Failed { message: "the picture could not be encoded".into() };
    };
    let copied = clipboard::set_image(&crop.rgba, crop.width, crop.height, &png).is_ok();
    let folder = platform::screenshots_folder();
    let path = folder.as_deref().and_then(|dir| {
        std::fs::create_dir_all(dir).ok()?;
        let path = free_path(dir, &file_stem(name));
        std::fs::write(&path, &png).ok()?;
        Some(path)
    });
    if path.is_none() && !copied {
        return Outcome::Failed { message: "the screenshot could not be saved or copied".into() };
    }
    Outcome::Screenshot {
        path: path.map(|p| p.to_string_lossy().into_owned()),
        folder: folder.map(|p| p.to_string_lossy().into_owned()),
        copied,
        thumbnail: thumbnail(crop),
    }
}

/// A small picture for the notice, 44 px tall at most.
fn thumbnail(crop: &Frame) -> Option<String> {
    let image = image::RgbaImage::from_raw(crop.width, crop.height, crop.rgba.clone())?;
    let scale = (44.0 / crop.height as f64).min(88.0 / crop.width as f64).min(1.0);
    let (w, h) = (((crop.width as f64 * scale).round() as u32).max(1), ((crop.height as f64 * scale).round() as u32).max(1));
    let small = image::imageops::thumbnail(&image, w, h);
    crate::shell::encode_png(small.as_raw(), w, h).map(|png| crate::shell::png_data_url(&png))
}

/// A file name Windows accepts: no path or reserved characters, not empty.
fn file_stem(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '-' } else { c })
        .collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() { "Screenshot".into() } else { clean }
}

/// `stem.png`, or `stem (2).png` and so on when taken.
fn free_path(dir: &Path, stem: &str) -> PathBuf {
    let mut path = dir.join(format!("{stem}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).png"));
        n += 1;
    }
    path
}

/// Copies text for the page (the scratchpad's Copy all).
#[tauri::command]
pub async fn tools_copy_text(text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || clipboard::set_text(&text))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use super::Rect;
    use std::path::PathBuf;
    use tauri::{Runtime, WebviewWindow};
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId};
    use windows::Win32::UI::Shell::{FOLDERID_Pictures, FOLDERID_Screenshots, KF_FLAG_CREATE, SHGetKnownFolderPath};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GW_HWNDNEXT, GetClassNameW, GetCursorPos, GetForegroundWindow, GetTopWindow, GetWindow,
        GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

    pub fn monitor_under_cursor() -> Option<(isize, Rect)> {
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            GetMonitorInfoW(monitor, &mut info).ok().ok()?;
            let r = info.rcMonitor;
            Some((monitor.0 as isize, Rect { x: r.left, y: r.top, width: r.right - r.left, height: r.bottom - r.top }))
        }
    }

    /// The overlay is never part of a capture, whatever the island does.
    pub fn exclude_from_capture<R: Runtime>(window: &WebviewWindow<R>) {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            }
        }
    }

    /// The island gave the keyboard back when it collapsed, so Windows'
    /// foreground lock would keep it from the overlay; joining the
    /// foreground thread's input for the call is allowed.
    pub fn take_foreground<R: Runtime>(window: &WebviewWindow<R>) {
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let foreground = GetForegroundWindow();
            let theirs = GetWindowThreadProcessId(foreground, None);
            let ours = GetWindowThreadProcessId(hwnd, None);
            let attach = theirs != 0 && theirs != ours;
            if attach {
                let _ = AttachThreadInput(theirs, ours, true);
            }
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            if attach {
                let _ = AttachThreadInput(theirs, ours, false);
            }
        }
    }

    /// The topmost visible, uncloaked top-level window at the point that
    /// isn't ours or the desktop; 0 when there is none.
    pub fn window_at(x: i32, y: i32) -> isize {
        unsafe {
            let ours = GetCurrentProcessId();
            let mut next = GetTopWindow(None).ok();
            while let Some(hwnd) = next {
                next = GetWindow(hwnd, GW_HWNDNEXT).ok();
                if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
                    continue;
                }
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == ours || cloaked(hwnd) {
                    continue;
                }
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r).is_err() || x < r.left || x >= r.right || y < r.top || y >= r.bottom {
                    continue;
                }
                let mut class = [0u16; 32];
                let n = GetClassNameW(hwnd, &mut class).max(0) as usize;
                let class = String::from_utf16_lossy(&class[..n]);
                return if class == "Progman" || class == "WorkerW" { 0 } else { hwnd.0 as isize };
            }
            0
        }
    }

    fn cloaked(hwnd: HWND) -> bool {
        let mut value = 0u32;
        unsafe {
            DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut value as *mut u32).cast(), 4).is_ok() && value != 0
        }
    }

    /// Pictures\Screenshots wherever it lives (OneDrive often moves it).
    pub fn screenshots_folder() -> Option<PathBuf> {
        let known = |id| unsafe {
            let path = SHGetKnownFolderPath(id, KF_FLAG_CREATE, None).ok()?;
            let out = path.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(path.0 as *const _));
            out
        };
        known(&FOLDERID_Screenshots).or_else(|| known(&FOLDERID_Pictures).map(|p| p.join("Screenshots")))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Rect;
    use std::path::PathBuf;
    use tauri::{Runtime, WebviewWindow};

    pub fn monitor_under_cursor() -> Option<(isize, Rect)> {
        Some((0, Rect { x: 0, y: 0, width: 1440, height: 900 }))
    }

    pub fn exclude_from_capture<R: Runtime>(window: &WebviewWindow<R>) {
        let _ = window.set_content_protected(true);
    }

    pub fn take_foreground<R: Runtime>(_window: &WebviewWindow<R>) {}

    pub fn window_at(_x: i32, _y: i32) -> isize {
        0
    }

    pub fn screenshots_folder() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Pictures").join("Screenshots"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_clamp_to_the_picture_and_empty_ones_drop() {
        let r = clamp_area(Rect { x: -10, y: 5, width: 50, height: 2000 }, 100, 100).unwrap();
        assert_eq!(r, Rect { x: 0, y: 5, width: 40, height: 95 });
        assert_eq!(clamp_area(Rect { x: 10, y: 10, width: 0, height: 5 }, 100, 100), None);
        assert_eq!(clamp_area(Rect { x: 120, y: 10, width: 5, height: 5 }, 100, 100), None);
    }

    #[test]
    fn picks_deserialize_from_the_page() {
        let area: Pick = serde_json::from_str(r#"{"kind":"area","x":1,"y":2,"width":3,"height":4}"#).unwrap();
        assert_eq!(area, Pick::Area { x: 1, y: 2, width: 3, height: 4 });
        let point: Pick = serde_json::from_str(r#"{"kind":"point","x":7,"y":8}"#).unwrap();
        assert_eq!(point, Pick::Point { x: 7, y: 8 });
    }

    #[test]
    fn file_names_lose_reserved_characters() {
        assert_eq!(file_stem("Screenshot 2026-10-27 140322"), "Screenshot 2026-10-27 140322");
        assert_eq!(file_stem("a/b:c?"), "a-b-c-");
        assert_eq!(file_stem("  ..  "), "Screenshot");
    }

    #[test]
    fn taken_names_get_a_number() {
        let dir = std::env::temp_dir().join(format!("vors-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shot.png"), b"x").unwrap();
        assert_eq!(free_path(&dir, "shot"), dir.join("shot (2).png"));
        assert_eq!(free_path(&dir, "other"), dir.join("other.png"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn outcomes_serialize_with_a_kind() {
        let json = serde_json::to_value(Outcome::Color { value: "#FFFFFF".into(), hex: "#FFFFFF".into() }).unwrap();
        assert_eq!(json["kind"], "color");
        assert_eq!(serde_json::to_value(Outcome::NoLanguage).unwrap()["kind"], "noLanguage");
    }
}
