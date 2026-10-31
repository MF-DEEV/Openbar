// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The session being unlocked, which the Mac hears as
//! `com.apple.screenIsUnlocked`: the island's companion wakes up glad to see
//! you. `WTSRegisterSessionNotification` reports to a window, so a thread
//! keeps a hidden one for the app's life.

use tauri::AppHandle;

#[cfg(windows)]
pub const EVENT: &str = "session://unlocked";

#[cfg(windows)]
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

#[cfg(windows)]
pub fn start(app: AppHandle) {
    use windows::Win32::Foundation::{HINSTANCE, HWND};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::RemoteDesktop::{
        NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, MSG, RegisterClassW, WINDOW_EX_STYLE,
        WNDCLASSW, WS_OVERLAPPED,
    };
    use windows::core::w;
    if APP.set(app).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("session-lock".into())
        .spawn(|| unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).map(Into::into).unwrap_or_default();
            let class = w!("VorssaintSessionLock");
            let description = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&description);
            // Never shown. A message-only window would not hear session changes.
            let Ok(window): Result<HWND, _> = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("Vorssaint session"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            ) else {
                return;
            };
            if WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION).is_err() {
                return;
            }
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).as_bool() {
                DispatchMessageW(&message);
            }
        });
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}

#[cfg(windows)]
unsafe extern "system" fn window_proc(
    window: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use tauri::Emitter;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK,
    };
    if message == WM_WTSSESSION_CHANGE
        && wparam.0 == WTS_SESSION_UNLOCK as usize
        && let Some(app) = APP.get()
    {
        let _ = app.emit(EVENT, ());
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
