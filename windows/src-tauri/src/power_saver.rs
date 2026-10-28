// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Battery Saver, the Mac's Low Power Mode (`isLowPowerModeEnabled` and
//! `NSProcessInfoPowerStateDidChange`): whether Windows is saving energy,
//! read from `PowerManager.EnergySaverStatus` and followed through its change
//! event. Windows 11's Energy saver reports there too, also on PCs without a
//! battery. Every window hears it, so a Clawd in the island or in Settings
//! halves its steps from the moment it changes.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "power://saver";

static ON: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn power_saver_on() -> bool {
    ON.load(Ordering::Relaxed)
}

pub fn start(app: AppHandle) {
    ON.store(read(), Ordering::Relaxed);
    watch(app);
}

/// Sends the state to every window when it differs from the last one sent.
#[cfg_attr(not(windows), allow(dead_code))]
fn changed(app: &AppHandle) {
    let on = read();
    if ON.swap(on, Ordering::Relaxed) != on {
        let _ = app.emit(EVENT, on);
    }
}

#[cfg(windows)]
fn read() -> bool {
    use windows::System::Power::{EnergySaverStatus, PowerManager};
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    match PowerManager::EnergySaverStatus() {
        Ok(status) => status == EnergySaverStatus::On,
        // Without the WinRT reading, the flag Battery Saver sets.
        Err(_) => {
            let mut status = SYSTEM_POWER_STATUS::default();
            unsafe { GetSystemPowerStatus(&mut status) }.is_ok() && status.SystemStatusFlag == 1
        }
    }
}

#[cfg(not(windows))]
fn read() -> bool {
    false
}

#[cfg(windows)]
fn watch(app: AppHandle) {
    use windows::Foundation::EventHandler;
    use windows::System::Power::PowerManager;
    use windows::core::IInspectable;
    // Kept for the app's life, so the token is never removed.
    let _ =
        PowerManager::EnergySaverStatusChanged(&EventHandler::<IInspectable>::new(move |_, _| {
            changed(&app);
            Ok(())
        }));
}

#[cfg(not(windows))]
fn watch(_app: AppHandle) {}
