// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Hides the island while an app is fullscreen (a game, a film, a
//! presentation) and brings it back after (spec 01-shell §6.2). The island
//! window registers with the shell as an app bar that reserves no space:
//! that is how Windows tells a window about fullscreen apps on its display.

use tauri::{AppHandle, Runtime};

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tauri::{AppHandle, Emitter, Manager, Runtime};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{
        ABM_NEW, ABM_REMOVE, ABN_FULLSCREENAPP, APPBARDATA, DefSubclassProc, SHAppBarMessage,
        SetWindowSubclass,
    };
    use windows::Win32::UI::WindowsAndMessaging::WM_APP;

    use crate::island;

    const WM_FULLSCREEN: u32 = WM_APP + 2;
    static ON_FULLSCREEN: OnceLock<Box<dyn Fn(bool) + Send + Sync>> = OnceLock::new();
    /// Settings › Behavior › Hide in fullscreen apps.
    pub static HIDES: AtomicBool = AtomicBool::new(true);
    /// Whether an app is fullscreen now, to show the island again when hiding is turned off.
    static FULLSCREEN: AtomicBool = AtomicBool::new(false);

    fn data(hwnd: HWND) -> APPBARDATA {
        APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            ..Default::default()
        }
    }

    pub fn watch<R: Runtime>(app: &AppHandle<R>) {
        let Some(window) = app.get_webview_window(island::LABEL) else {
            return;
        };
        let Ok(hwnd) = window.hwnd() else { return };
        let handle = app.clone();
        let _ = ON_FULLSCREEN.set(Box::new(move |on| {
            FULLSCREEN.store(on, Ordering::Relaxed);
            let hide = on && HIDES.load(Ordering::Relaxed);
            if let Some(w) = handle.get_webview_window(island::LABEL) {
                let _ = if hide { w.hide() } else { w.show() };
            }
            let _ = handle.emit("island://fullscreen", hide);
        }));
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(subclass), 2, 0);
            let mut d = data(hwnd);
            d.uCallbackMessage = WM_FULLSCREEN;
            // Registered but never positioned: no space is taken from other windows.
            SHAppBarMessage(ABM_NEW, &mut d);
        }
    }

    pub fn set_hides<R: Runtime>(app: &AppHandle<R>, hides: bool) {
        if HIDES.swap(hides, Ordering::Relaxed) == hides {
            return;
        }
        if FULLSCREEN.load(Ordering::Relaxed)
            && let Some(notify) = ON_FULLSCREEN.get()
        {
            let _ = app;
            notify(true);
        }
    }

    pub fn stop<R: Runtime>(app: &AppHandle<R>) {
        let Some(window) = app.get_webview_window(island::LABEL) else {
            return;
        };
        let Ok(hwnd) = window.hwnd() else { return };
        unsafe {
            let mut d = data(hwnd);
            SHAppBarMessage(ABM_REMOVE, &mut d);
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
        if msg == WM_FULLSCREEN {
            if wparam.0 as u32 == ABN_FULLSCREENAPP
                && let Some(notify) = ON_FULLSCREEN.get()
            {
                notify(lparam.0 != 0);
            }
            return LRESULT(0);
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }
}

#[cfg(windows)]
pub fn watch<R: Runtime>(app: &AppHandle<R>) {
    imp::watch(app)
}

#[cfg(windows)]
pub fn stop<R: Runtime>(app: &AppHandle<R>) {
    imp::stop(app)
}

#[cfg(windows)]
pub fn set_hides<R: Runtime>(app: &AppHandle<R>, hides: bool) {
    imp::set_hides(app, hides)
}

#[cfg(not(windows))]
pub fn set_hides<R: Runtime>(_app: &AppHandle<R>, _hides: bool) {}

#[cfg(not(windows))]
pub fn watch<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(not(windows))]
pub fn stop<R: Runtime>(_app: &AppHandle<R>) {}
