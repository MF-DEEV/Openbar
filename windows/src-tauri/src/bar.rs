// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The menu bar imitation along the top of the primary display (Windows
//! only): an acrylic strip registered as a shell app bar, so maximized
//! windows start below it the way they start below the Mac's menu bar. It
//! names the app in front and hides, with the island, while an app is
//! fullscreen.

#[cfg(not(windows))]
use tauri::{AppHandle, Runtime};

#[cfg_attr(not(windows), allow(dead_code))]
pub const LABEL: &str = "bar";
/// The bar's height in DIPs; the island's strip matches it (geometry.ts).
#[cfg_attr(not(windows), allow(dead_code))]
pub const HEIGHT: f64 = 24.0;

#[cfg(windows)]
pub use imp::{create, own_island, remove};

#[cfg(not(windows))]
pub fn create<R: Runtime>(_app: &AppHandle<R>) -> tauri::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn remove<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(not(windows))]
pub fn own_island<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::thread;
    use std::time::Duration;

    use tauri::window::{Effect, EffectsBuilder};
    use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Shell::{
        ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABN_FULLSCREENAPP, ABN_POSCHANGED,
        APPBARDATA, DefSubclassProc, SHAppBarMessage, SetWindowSubclass,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWLP_HWNDPARENT, GetForegroundWindow, GetWindowLongPtrW,
        GetWindowThreadProcessId, MoveWindow, SetWindowLongPtrW, WM_APP, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR, w};

    use super::{HEIGHT, LABEL};
    use crate::island;

    const WM_APPBAR: u32 = WM_APP + 1;

    /// Reacts to the shell's app-bar notices: fullscreen apps and layout changes.
    static ON_NOTICE: OnceLock<Box<dyn Fn(Notice) + Send + Sync>> = OnceLock::new();

    enum Notice {
        Fullscreen(bool),
        Moved,
    }

    /// Makes the island a window the bar owns, so Windows keeps it above the
    /// bar whichever of the two topmost windows was clicked last.
    pub fn own_island<R: Runtime>(app: &AppHandle<R>) {
        let (Some(bar), Some(island)) = (
            app.get_webview_window(LABEL),
            app.get_webview_window(island::LABEL),
        ) else {
            return;
        };
        let (Ok(bar), Ok(island)) = (bar.hwnd(), island.hwnd()) else {
            return;
        };
        unsafe {
            SetWindowLongPtrW(island, GWLP_HWNDPARENT, bar.0 as isize);
        }
    }

    pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
        let Some(monitor) = app.primary_monitor()? else {
            return Ok(());
        };
        let scale = monitor.scale_factor();
        let width = monitor.size().width as f64 / scale;
        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("bar.html".into()))
            .title("Vorssaint Bar")
            .inner_size(width, HEIGHT)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .effects(EffectsBuilder::new().effect(Effect::Acrylic).build())
            .build()?;
        let hwnd = window.hwnd()?;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | WS_EX_TOOLWINDOW.0 as isize | WS_EX_NOACTIVATE.0 as isize,
            );
        }

        let handle = app.clone();
        let _ = ON_NOTICE.set(Box::new(move |notice| match notice {
            Notice::Fullscreen(on) => {
                for label in [LABEL, island::LABEL] {
                    if let Some(w) = handle.get_webview_window(label) {
                        let _ = if on { w.hide() } else { w.show() };
                    }
                }
                let _ = handle.emit("island://fullscreen", on);
            }
            Notice::Moved => {
                if let Some(w) = handle.get_webview_window(LABEL)
                    && let Ok(hwnd) = w.hwnd()
                {
                    reserve(hwnd, &handle);
                }
            }
        }));

        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(subclass), 1, 0);
            let mut data = appbar_data(hwnd);
            data.uCallbackMessage = WM_APPBAR;
            SHAppBarMessage(ABM_NEW, &mut data);
        }
        reserve(hwnd, app);
        watch_foreground(app.clone());
        Ok(())
    }

    /// Gives the bar's strip back to the desktop; without this, the space
    /// stays reserved until Explorer restarts.
    pub fn remove<R: Runtime>(app: &AppHandle<R>) {
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let mut data = appbar_data(hwnd);
            SHAppBarMessage(ABM_REMOVE, &mut data);
        }
    }

    fn appbar_data(hwnd: HWND) -> APPBARDATA {
        APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            ..Default::default()
        }
    }

    /// Asks the shell for the top strip of the primary display and moves the bar there.
    fn reserve<R: Runtime>(hwnd: HWND, app: &AppHandle<R>) {
        let Ok(Some(monitor)) = app.primary_monitor() else {
            return;
        };
        let origin = monitor.position();
        let size = monitor.size();
        let height = (HEIGHT * monitor.scale_factor()).round() as i32;
        unsafe {
            let mut data = appbar_data(hwnd);
            data.uEdge = ABE_TOP;
            data.rc = RECT {
                left: origin.x,
                top: origin.y,
                right: origin.x + size.width as i32,
                bottom: origin.y + height,
            };
            SHAppBarMessage(ABM_QUERYPOS, &mut data);
            data.rc.bottom = data.rc.top + height;
            SHAppBarMessage(ABM_SETPOS, &mut data);
            let rc = data.rc;
            let _ = MoveWindow(
                hwnd,
                rc.left,
                rc.top,
                rc.right - rc.left,
                rc.bottom - rc.top,
                true,
            );
        }
    }

    unsafe extern "system" fn subclass(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_APPBAR {
            if let Some(notify) = ON_NOTICE.get() {
                match wparam.0 as u32 {
                    ABN_FULLSCREENAPP => notify(Notice::Fullscreen(lparam.0 != 0)),
                    ABN_POSCHANGED => notify(Notice::Moved),
                    _ => {}
                }
            }
            return LRESULT(0);
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Names the app in front, as the Mac's bar does, every few tenths of a second.
    fn watch_foreground<R: Runtime>(app: AppHandle<R>) {
        thread::spawn(move || {
            let own = std::process::id();
            let mut last = String::new();
            loop {
                thread::sleep(Duration::from_millis(300));
                let Some((pid, name)) = foreground_app() else {
                    continue;
                };
                if pid == own || name == last {
                    continue;
                }
                last = name.clone();
                let _ = app.emit_to(LABEL, "bar://foreground-app", name);
            }
        });
    }

    fn foreground_app() -> Option<(u32, String)> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(process);
            ok.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            Some((pid, describe(&path)))
        }
    }

    /// The executable's FileDescription ("Windows Terminal"), else its file name.
    fn describe(path: &str) -> String {
        let fallback = || {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        unsafe {
            let file = HSTRING::from(path);
            let size = GetFileVersionInfoSizeW(&file, None);
            if size == 0 {
                return fallback();
            }
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(&file, Some(0), size, data.as_mut_ptr().cast()).is_err() {
                return fallback();
            }
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            if !VerQueryValueW(
                data.as_ptr().cast(),
                w!("\\VarFileInfo\\Translation"),
                &mut ptr,
                &mut len,
            )
            .as_bool()
                || len < 4
            {
                return fallback();
            }
            let pair = *(ptr as *const [u16; 2]);
            let key = HSTRING::from(format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
                pair[0], pair[1]
            ));
            if !VerQueryValueW(
                data.as_ptr().cast(),
                PCWSTR(key.as_ptr()),
                &mut ptr,
                &mut len,
            )
            .as_bool()
                || len == 0
            {
                return fallback();
            }
            let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let text = String::from_utf16_lossy(text)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if text.is_empty() { fallback() } else { text }
        }
    }
}
