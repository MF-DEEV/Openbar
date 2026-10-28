// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Hides the island while an app is fullscreen (a game, a film, a
//! presentation) and brings it back after (spec 01-shell §6.2). The island
//! window registers with the shell as an app bar that reserves no space:
//! shell messages only trigger a fresh check of the foreground window.
//! The island's own transparent canvas also spans the monitor, so a shell
//! notification by itself is never evidence of an external fullscreen app.

use tauri::{AppHandle, Runtime};

#[derive(Clone, Copy, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionContext {
    pub can_auto_present: bool,
    pub fullscreen: bool,
    pub busy: bool,
}

fn desktop_or_own(class: &str, own: bool) -> bool {
    own || matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

#[tauri::command]
pub fn attention_context<R: Runtime>(app: AppHandle<R>) -> AttentionContext {
    #[cfg(windows)]
    {
        use tauri::Manager;
        app.get_webview_window(crate::island::LABEL)
            .map(|w| imp::context(&w))
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        AttentionContext {
            can_auto_present: true,
            fullscreen: false,
            busy: false,
        }
    }
}

#[tauri::command]
pub fn attention_compact_sessions(on: bool) {
    #[cfg(windows)]
    imp::set_compact(on);
    #[cfg(not(windows))]
    let _ = on;
}

fn fullscreen_candidate(eligible: bool, bounds: [i32; 4], display: [i32; 4]) -> bool {
    eligible
        && display[2] > display[0]
        && display[3] > display[1]
        && bounds[0] <= display[0]
        && bounds[1] <= display[1]
        && bounds[2] >= display[2]
        && bounds[3] >= display[3]
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::Shell::{
        ABM_NEW, ABM_REMOVE, ABN_FULLSCREENAPP, APPBARDATA, DefSubclassProc, SHAppBarMessage,
        SetWindowSubclass,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_SHOWNOACTIVATE, ShowWindow, WM_APP,
        WS_CAPTION,
    };

    use crate::island;

    const WM_FULLSCREEN: u32 = WM_APP + 2;
    static ON_FULLSCREEN: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
    /// Settings › Behavior › Hide in fullscreen apps.
    pub static HIDES: AtomicBool = AtomicBool::new(true);
    static COMPACT_SESSIONS: AtomicBool = AtomicBool::new(false);
    static STOPPED: AtomicBool = AtomicBool::new(false);
    static SAFE_SINCE: Mutex<Option<Instant>> = Mutex::new(None);

    pub fn context<R: Runtime>(window: &WebviewWindow<R>) -> super::AttentionContext {
        let Ok(Some(monitor)) = window.current_monitor() else {
            return super::AttentionContext::default();
        };
        let position = monitor.position();
        let size = monitor.size();
        let display = [
            position.x,
            position.y,
            position.x + size.width as i32,
            position.y + size.height as i32,
        ];
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.0.is_null() {
                *SAFE_SINCE.lock().unwrap() = None;
                return super::AttentionContext::default();
            }
            let mut pid = 0;
            GetWindowThreadProcessId(foreground, Some(&mut pid));
            let mut class = [0u16; 64];
            let count = GetClassNameW(foreground, &mut class).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..count]);
            let own = pid == std::process::id();
            let desktop = super::desktop_or_own(&class, own);
            let captioned = GetWindowLongPtrW(foreground, GWL_STYLE) & WS_CAPTION.0 as isize
                == WS_CAPTION.0 as isize;
            let mut bounds = RECT::default();
            let rect_ok = GetWindowRect(foreground, &mut bounds).is_ok();
            let exact = [bounds.left, bounds.top, bounds.right, bounds.bottom] == display;
            let eligible = !own
                && !desktop
                && (!captioned || exact)
                && rect_ok
                && IsWindowVisible(foreground).as_bool()
                && !IsIconic(foreground).as_bool();
            let mut since = SAFE_SINCE.lock().unwrap();
            let allowed = if desktop {
                since.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(750)
            } else {
                *since = None;
                false
            };
            super::AttentionContext {
                can_auto_present: allowed,
                fullscreen: super::fullscreen_candidate(
                    eligible,
                    [bounds.left, bounds.top, bounds.right, bounds.bottom],
                    display,
                ),
                busy: !desktop,
            }
        }
    }

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
        let previous = Mutex::new(None);
        let _ = ON_FULLSCREEN.set(Box::new(move || {
            if let Some(w) = handle.get_webview_window(island::LABEL) {
                // A show/hide can itself cause another shell broadcast. Read
                // the actual app on every broadcast, including "fullscreen off",
                // so our own visibility can never feed back into this decision.
                let context = context(&w);
                let mut before = previous.lock().unwrap();
                if *before != Some(context) {
                    *before = Some(context);
                    let _ = handle.emit("island://attention-context", context);
                }
                drop(before);
                let hide = HIDES.load(Ordering::Relaxed)
                    && context.fullscreen
                    && !COMPACT_SESSIONS.load(Ordering::Relaxed);
                if w.is_visible().unwrap_or(!hide) == hide {
                    if hide {
                        let _ = w.hide();
                    } else if context.fullscreen && COMPACT_SESSIONS.load(Ordering::Relaxed) {
                        // A game's pause-on-focus-loss must never fire just
                        // because a waiting indicator became visible.
                        if let Ok(hwnd) = w.hwnd() {
                            unsafe {
                                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                            }
                        }
                    } else {
                        let _ = w.show();
                    }
                    let _ = handle.emit("island://fullscreen", hide);
                }
            }
        }));
        unsafe {
            let _ = SetWindowSubclass(hwnd, Some(subclass), 2, 0);
            let mut d = data(hwnd);
            d.uCallbackMessage = WM_FULLSCREEN;
            // Registered but never positioned: no space is taken from other windows.
            SHAppBarMessage(ABM_NEW, &mut d);
        }
        // Borderless games do not consistently broadcast app-bar transitions.
        // Poll on the main thread as well, and classify the foreground afresh.
        let handle = app.clone();
        std::thread::spawn(move || {
            while !STOPPED.load(Ordering::Relaxed) {
                let _ = handle.run_on_main_thread(|| {
                    if let Some(notify) = ON_FULLSCREEN.get() {
                        notify();
                    }
                });
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }

    pub fn set_compact(on: bool) {
        if COMPACT_SESSIONS.swap(on, Ordering::Relaxed) != on
            && let Some(notify) = ON_FULLSCREEN.get()
        {
            notify();
        }
    }

    pub fn set_hides(hides: bool) {
        if HIDES.swap(hides, Ordering::Relaxed) == hides {
            return;
        }
        if let Some(notify) = ON_FULLSCREEN.get() {
            notify();
        }
    }

    pub fn stop<R: Runtime>(app: &AppHandle<R>) {
        STOPPED.store(true, Ordering::Relaxed);
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
                notify();
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

/// Settings › Behavior › Hide in full screen.
#[cfg(windows)]
pub fn set_hides(hides: bool) {
    imp::set_hides(hides)
}

#[cfg(not(windows))]
pub fn set_hides(_hides: bool) {}

#[cfg(not(windows))]
pub fn watch<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(not(windows))]
pub fn stop<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::fullscreen_candidate;

    const SCREEN: [i32; 4] = [0, 0, 2560, 1440];

    #[test]
    fn automatic_prompts_are_desktop_only_not_windowed_games_or_other_apps() {
        assert!(super::desktop_or_own("Progman", false));
        assert!(super::desktop_or_own("WorkerW", false));
        assert!(super::desktop_or_own("Tauri Window", true));
        assert!(!super::desktop_or_own("UnrealWindow", false));
        assert!(!super::desktop_or_own("Chrome_WidgetWin_1", false));
    }

    #[test]
    fn maximized_browser_above_taskbar_is_not_fullscreen() {
        assert!(!fullscreen_candidate(true, [-8, -8, 2568, 1392], SCREEN));
    }

    #[test]
    fn own_canvas_and_desktop_cannot_hide_the_island() {
        assert!(!fullscreen_candidate(false, SCREEN, SCREEN));
        // Excluded windows remain excluded even if their outer frame is larger.
        assert!(!fullscreen_candidate(false, [-8, -8, 2568, 1448], SCREEN));
    }

    #[test]
    fn real_fullscreen_must_cover_the_islands_display() {
        assert!(fullscreen_candidate(true, SCREEN, SCREEN));
        assert!(!fullscreen_candidate(true, [2560, 0, 5120, 1440], SCREEN));
        assert!(fullscreen_candidate(
            true,
            [-1920, 0, 0, 1080],
            [-1920, 0, 0, 1080]
        ));
        assert!(!fullscreen_candidate(true, [0, 0, 0, 0], [0, 0, 0, 0]));
    }
}
