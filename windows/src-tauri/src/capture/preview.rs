// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! One owned image per preview. Closing a window releases its pixels and
//! temporary PNG; automatic saves are never removed by dismissing a preview.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use super::{Rect, clipboard, island_pref, snapshot::Frame};

#[derive(Default)]
pub struct Shared {
    images: Mutex<HashMap<String, Arc<Preview>>>,
    next: AtomicU64,
}

/// Release pinned/transient files as part of the app's normal exit path,
/// without depending on WebView destruction order.
pub fn stop<R: Runtime>(app: &AppHandle<R>) {
    let images = std::mem::take(&mut *app.state::<Shared>().images.lock().unwrap());
    for preview in images.values() {
        let _ = std::fs::remove_file(&preview.temporary);
    }
}

struct Preview {
    frame: Frame,
    png: Vec<u8>,
    saved: Option<PathBuf>,
    copied: bool,
    temporary: PathBuf,
    pinned: AtomicBool,
    busy: AtomicBool,
    ready: Mutex<Option<mpsc::Sender<Result<(), String>>>>,
    duration: u64,
    dismiss_on_blur: bool,
    monitor: Rect,
    embedded: AtomicBool,
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temporary);
    }
}

fn picture<R: Runtime>(
    window: &WebviewWindow<R>,
    id: Option<&str>,
) -> Result<Arc<Preview>, String> {
    let label = id.unwrap_or(window.label());
    if window.label() != label && window.label() != crate::island::LABEL {
        return Err("This window does not own the screenshot".into());
    }
    let p = window
        .state::<Shared>()
        .images
        .lock()
        .unwrap()
        .get(label)
        .cloned()
        .ok_or_else(|| "The screenshot preview has closed".to_string())?;
    if (window.label() == crate::island::LABEL) != p.embedded.load(Ordering::SeqCst) {
        return Err("The screenshot has moved to another window".into());
    }
    Ok(p)
}

fn close<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let _ = app.emit_to(crate::island::LABEL, "screenshot-preview://closed", id);
    if let Some(window) = app.get_webview_window(id) {
        let _ = window.destroy();
    }
    app.state::<Shared>().images.lock().unwrap().remove(id);
}

fn can_embed<R: Runtime>(app: &AppHandle<R>, monitor: Rect) -> bool {
    if island_pref(app, "enabled").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }
    let Some(window) = app.get_webview_window(crate::island::LABEL) else {
        return false;
    };
    let Ok(Some(display)) = window.current_monitor() else {
        return false;
    };
    display.position().x == monitor.x
        && display.position().y == monitor.y
        && !app
            .state::<Shared>()
            .images
            .lock()
            .unwrap()
            .values()
            .any(|p| {
                p.embedded.load(Ordering::SeqCst)
                    && (p.busy.load(Ordering::SeqCst) || p.pinned.load(Ordering::SeqCst))
            })
}

fn floating_window<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    monitor: Rect,
) -> Result<(), String> {
    let window = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App("screenshot-preview.html".into()),
    )
    .title(format!(
        "{} — Vorssaint",
        crate::locale::text(app, "screenshot")
    ))
    .decorations(false)
    .transparent(true)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .disable_drag_drop_handler()
    .build()
    .map_err(|e| e.to_string())?;
    let app_closed = app.clone();
    let closed_label = label.to_string();
    #[cfg(windows)]
    let share_hwnd = window.hwnd().ok().map(|hwnd| hwnd.0 as isize);
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            #[cfg(windows)]
            if let Some(hwnd) = share_hwnd {
                crate::files::shell::close_share(hwnd);
            }
            let shared = app_closed.state::<Shared>();
            let mut images = shared.images.lock().unwrap();
            // A failed Pin can restore the original island presentation.
            if images
                .get(&closed_label)
                .is_some_and(|p| !p.embedded.load(Ordering::SeqCst))
            {
                images.remove(&closed_label);
            }
        }
    });
    let (cx, cy) = monitor.center();
    let display = window
        .monitor_from_point(cx as f64, cy as f64)
        .map_err(|e| e.to_string())?
        .ok_or("The capture display is no longer available")?;
    let area = display.work_area();
    let (position, size) = placement(
        Rect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width as i32,
            height: area.size.height as i32,
        },
        display.scale_factor(),
    );
    window.set_position(position).map_err(|e| e.to_string())?;
    window.set_size(size).map_err(|e| e.to_string())?;
    super::platform::exclude_from_capture(&window);
    Ok(())
}

/// Physical desktop coordinates throughout, including work-area origins on
/// monitors to the left/above the primary display. UI dimensions are in DIPs.
fn placement(work: Rect, scale: f64) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let margin = (16.0 * scale).round() as i32;
    let width = (360.0 * scale)
        .round()
        .min((work.width - 2 * margin).max(1) as f64) as u32;
    let height = (320.0 * scale)
        .round()
        .min((work.height - 2 * margin).max(1) as f64) as u32;
    (
        PhysicalPosition::new(
            work.x + work.width - width as i32 - margin,
            work.y + work.height - height as i32 - margin,
        ),
        PhysicalSize::new(width, height),
    )
}

pub fn open<R: Runtime>(
    app: &AppHandle<R>,
    frame: Frame,
    png: Vec<u8>,
    saved: Option<PathBuf>,
    copied: bool,
    monitor: Rect,
) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let id = shared.next.fetch_add(1, Ordering::Relaxed);
    let label = format!("screenshot-preview-{id}");
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("screenshot-previews");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // A process-specific random name prevents collisions after a restart.
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let temporary = dir.join(format!(
        "Screenshot-{}.png",
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    if let Err(error) = std::fs::write(&temporary, &png) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("Could not prepare screenshot preview: {error}"));
    }
    let (tx, rx) = mpsc::channel();
    let preview = Arc::new(Preview {
        frame,
        png,
        saved,
        copied,
        temporary,
        pinned: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        ready: Mutex::new(Some(tx)),
        duration: island_pref(app, "screenshotPreviewDuration")
            .and_then(|v| v.as_u64())
            .unwrap_or(8)
            .min(300),
        dismiss_on_blur: island_pref(app, "screenshotPreviewDismissOnBlur")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        monitor,
        embedded: AtomicBool::new(false),
    });
    shared
        .images
        .lock()
        .unwrap()
        .insert(label.clone(), preview.clone());
    let result = (|| -> Result<(), String> {
        if can_embed(app, monitor) {
            preview.embedded.store(true, Ordering::SeqCst);
            if app
                .emit_to(crate::island::LABEL, "screenshot-preview://open", &label)
                .is_ok()
                && rx
                    .recv_timeout(Duration::from_secs(3))
                    .is_ok_and(|r| r.is_ok())
            {
                return Ok(());
            }
            // The island may be disabled/reloading: keep a real fallback.
            preview.embedded.store(false, Ordering::SeqCst);
            let _ = app.emit_to(crate::island::LABEL, "screenshot-preview://closed", &label);
        }
        let (tx, rx) = mpsc::channel();
        *preview.ready.lock().unwrap() = Some(tx);
        floating_window(app, &label, monitor)?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| "Screenshot preview did not load".to_string())?
    })();
    if result.is_err() {
        close(app, &label);
    } else {
        // A new capture replaces only idle, unpinned previews. Pinned images
        // and previews currently saving/recognizing/dragging remain intact.
        let stale: Vec<String> = shared
            .images
            .lock()
            .unwrap()
            .iter()
            .filter(|(key, p)| {
                key.strip_prefix("screenshot-preview-")
                    .and_then(|n| n.parse::<u64>().ok())
                    .is_some_and(|n| n < id)
                    && !p.pinned.load(Ordering::SeqCst)
                    && !p.busy.load(Ordering::SeqCst)
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale {
            close(app, &key);
        }
    }
    result
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    image: String,
    saved: Option<String>,
    copied: bool,
    duration: u64,
    dismiss_on_blur: bool,
    pinned: bool,
}

#[tauri::command]
pub fn preview_info<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
) -> Result<Info, String> {
    let p = picture(&window, id.as_deref())?;
    Ok(Info {
        image: crate::shell::png_data_url(&p.png),
        saved: p.saved.as_ref().map(|p| p.to_string_lossy().into_owned()),
        copied: p.copied,
        duration: p.duration,
        dismiss_on_blur: p.dismiss_on_blur,
        pinned: p.pinned.load(Ordering::SeqCst),
    })
}

#[tauri::command]
pub fn preview_ready<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
) -> Result<(), String> {
    let p = picture(&window, id.as_deref())?;
    let result = if p.embedded.load(Ordering::SeqCst) {
        Ok(())
    } else {
        window.show().map_err(|e| e.to_string())
    };
    if let Some(tx) = p.ready.lock().unwrap().take() {
        let _ = tx.send(result.clone());
    }
    result
}

#[tauri::command]
pub fn preview_close<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
) -> Result<(), String> {
    picture(&window, id.as_deref())?;
    close(window.app_handle(), id.as_deref().unwrap_or(window.label()));
    Ok(())
}

#[tauri::command]
pub async fn preview_pin<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
    pinned: bool,
) -> Result<(), String> {
    let p = picture(&window, id.as_deref())?;
    let _busy = begin(p.clone())?;
    let was_pinned = p.pinned.swap(pinned, Ordering::SeqCst);
    if !p.embedded.load(Ordering::SeqCst) || !pinned {
        return Ok(());
    }
    let label = id.ok_or("No screenshot selected")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (tx, rx) = mpsc::channel();
        *p.ready.lock().unwrap() = Some(tx);
        p.embedded.store(false, Ordering::SeqCst);
        let result = floating_window(&app, &label, p.monitor).and_then(|_| {
            rx.recv_timeout(Duration::from_secs(10))
                .map_err(|_| "Pinned preview did not load".to_string())?
        });
        if result.is_ok() {
            let _ = app.emit_to(crate::island::LABEL, "screenshot-preview://closed", &label);
        } else {
            p.embedded.store(true, Ordering::SeqCst);
            p.pinned.store(was_pinned, Ordering::SeqCst);
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.destroy();
            }
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

struct Busy(Arc<Preview>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }
}
fn begin(p: Arc<Preview>) -> Result<Busy, String> {
    if p.busy.swap(true, Ordering::SeqCst) {
        return Err("Another screenshot action is running".into());
    }
    Ok(Busy(p))
}

fn scan_qr(f: &Frame) -> Result<String, String> {
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(f.width as usize, f.height as usize, |x, y| {
            let i = (y * f.width as usize + x) * 4;
            ((u32::from(f.rgba[i]) * 299
                + u32::from(f.rgba[i + 1]) * 587
                + u32::from(f.rgba[i + 2]) * 114)
                / 1000) as u8
        });
    let readings: Vec<String> = image
        .detect_grids()
        .iter()
        .filter_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .collect();
    if readings.is_empty() {
        Err("No readable QR code found".into())
    } else {
        Ok(readings.join("\n\n"))
    }
}

#[tauri::command]
pub async fn preview_action<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
    action: String,
) -> Result<String, String> {
    let p = picture(&window, id.as_deref())?;
    let _busy = begin(p.clone())?;
    if action == "share" {
        // Sharing may consume the file asynchronously, after the sheet opens.
        // Keep this preview/file alive until the user explicitly closes it.
        p.pinned.store(true, Ordering::SeqCst);
        let path = p.temporary.to_string_lossy().into_owned();
        return if crate::files::shell::share_for_window(
            window.app_handle(),
            window.label(),
            vec![path],
        )
        .await
        {
            Ok("Windows Share opened. Preview pinned until you close it.".into())
        } else {
            Err("Windows Share is unavailable on this system. Drag the screenshot into another app instead.".into())
        };
    }
    tauri::async_runtime::spawn_blocking(move || match action.as_str() {
        "discard" => {
            if let Some(path) = &p.saved {
                match std::fs::read(path) {
                    Ok(bytes) if bytes == p.png => crate::files::shell::recycle(path.to_string_lossy().into_owned())?,
                    Ok(_) => return Err("The saved screenshot was changed by another app. Close the preview to keep that file.".into()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                    Err(e) => return Err(format!("Could not discard screenshot: {e}")),
                }
            }
            Ok("Screenshot discarded. Clipboard contents are unchanged.".into())
        }
        "copy" => clipboard::set_image(&p.frame.rgba, p.frame.width, p.frame.height, &p.png)
            .map(|_| "Image copied".into()),
        "text" => {
            let text = crate::ocr::recognize(&p.frame.rgba, p.frame.width, p.frame.height)
                .map_err(|e| e.to_string())?
                .text();
            if text.trim().is_empty() {
                return Err("No text found in this screenshot".into());
            }
            clipboard::set_text(&text)?;
            Ok("Text copied".into())
        }
        "qr" => {
            scan_qr(&p.frame)
        }
        "save" => {
            use tauri_plugin_dialog::DialogExt;
            let destination = window
                .app_handle()
                .dialog()
                .file()
                .set_parent(&window)
                .set_title(crate::locale::text(window.app_handle(), "saveScreenshot"))
                .set_file_name("Screenshot.png")
                .add_filter("PNG", &["png"])
                .blocking_save_file();
            let Some(destination) = destination else {
                return Ok("Save cancelled".into());
            };
            let path = destination.into_path().map_err(|e| e.to_string())?;
            std::fs::write(&path, &p.png).map_err(|e| format!("Could not save screenshot: {e}"))?;
            Ok(format!("Saved to {}", path.display()))
        }
        _ => Err("Unknown screenshot action".into()),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn preview_drag<R: Runtime>(
    window: WebviewWindow<R>,
    id: Option<String>,
) -> Result<bool, String> {
    let p = picture(&window, id.as_deref())?;
    let _busy = begin(p.clone())?;
    #[cfg(windows)]
    {
        let path = p.temporary.to_string_lossy().into_owned();
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        let (tx, rx) = mpsc::channel();
        let source = window.clone();
        window
            .app_handle()
            .run_on_main_thread(move || {
                let embedded = p.embedded.load(Ordering::SeqCst);
                let island = source.app_handle().clone();
                source.state::<crate::files::Files>().set_dragging_out(true);
                let accepted = crate::files::drag_out::run(
                    hwnd,
                    &[path],
                    &[],
                    false,
                    Box::new(move |x, y| {
                        !embedded || island.state::<crate::island::IslandState>().contains(x, y)
                    }),
                );
                let _ = source.set_ignore_cursor_events(false);
                source
                    .state::<crate::files::Files>()
                    .set_dragging_out(false);
                let _ = tx.send(accepted);
            })
            .map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("Native screenshot dragging is only available on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_detection_reads_the_captured_pixels() {
        // Machine-generated QR modules encoding rqrr's project URL, from its
        // upstream test_github fixture (https://github.com/WanzenBug/rqrr).
        let modules: Vec<&str> = include_str!("fixtures/qr-github.txt").lines().collect();
        let side = (modules.len() + 8) * 6;
        let mut frame = Frame {
            width: side as u32,
            height: side as u32,
            rgba: vec![255; side * side * 4],
        };
        for (y, row) in modules.iter().enumerate() {
            for (x, module) in row.bytes().enumerate() {
                if module != b'1' {
                    continue;
                }
                for py in (y + 4) * 6..(y + 5) * 6 {
                    for px in (x + 4) * 6..(x + 5) * 6 {
                        frame.rgba[(py * side + px) * 4..(py * side + px) * 4 + 3].fill(0);
                    }
                }
            }
        }
        assert_eq!(
            scan_qr(&frame).unwrap(),
            "https://github.com/WanzenBug/rqrr"
        );
        frame.rgba.fill(255);
        assert!(scan_qr(&frame).is_err());
    }

    #[test]
    fn closing_releases_temporary_file_but_keeps_automatic_save() {
        let dir = std::env::temp_dir().join(format!("vors-preview-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let temporary = dir.join("temporary.png");
        let saved = dir.join("saved.png");
        std::fs::write(&temporary, b"png").unwrap();
        std::fs::write(&saved, b"png").unwrap();
        let p = Arc::new(Preview {
            frame: Frame {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            },
            png: b"png".to_vec(),
            saved: Some(saved.clone()),
            copied: true,
            temporary: temporary.clone(),
            pinned: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            ready: Mutex::new(None),
            duration: 3,
            dismiss_on_blur: false,
            monitor: Rect::default(),
            embedded: AtomicBool::new(false),
        });
        let busy = begin(p.clone()).unwrap();
        assert!(begin(p.clone()).is_err());
        drop(p);
        assert!(temporary.exists()); // An in-flight action owns the file.
        drop(busy);
        assert!(!temporary.exists());
        assert!(saved.exists());
        std::fs::remove_file(saved).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn placement_uses_work_area_origin_and_dpi() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let work = Rect {
                x: -2560,
                y: -400,
                width: 2560,
                height: 1400,
            };
            let (pos, size) = placement(work, scale);
            assert_eq!(size.width, (360.0 * scale).round() as u32);
            assert_eq!(pos.x + size.width as i32, -(16.0 * scale).round() as i32);
            assert_eq!(
                pos.y + size.height as i32,
                1000 - (16.0 * scale).round() as i32
            );
            assert!(pos.x >= work.x && pos.y >= work.y);
        }
    }
    #[test]
    fn small_work_area_constrains_preview() {
        let (pos, size) = placement(
            Rect {
                x: 500,
                y: 0,
                width: 500,
                height: 400,
            },
            2.0,
        );
        assert_eq!(size, PhysicalSize::new(436, 336));
        assert_eq!(pos, PhysicalPosition::new(532, 32));
    }
}
