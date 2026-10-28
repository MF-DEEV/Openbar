// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Energy saver, the Mac's Low Power Mode (`isLowPowerModeEnabled` and
//! `NSProcessInfoPowerStateDidChange`). Windows 11 24H2 and later report it
//! through `GUID_ENERGY_SAVER_STATUS`, also when it runs on AC power, where
//! `PowerManager.EnergySaverStatus` says Disabled and `SystemStatusFlag` 0;
//! older versions report Battery Saver through `GUID_POWER_SAVING_STATUS`.
//! Both are followed with power setting callbacks, which also hand over the
//! current value as they're registered. The island's Clawds halve their
//! steps from the moment it changes.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "power://saver";

static APP: OnceLock<AppHandle> = OnceLock::new();
/// ENERGY_SAVER_OFF (0), _STANDARD (1) or _HIGH_SAVINGS (2).
static ENERGY_SAVER: AtomicU32 = AtomicU32::new(0);
/// Battery Saver off (0) or on (1), before Energy saver replaced it.
static BATTERY_SAVER: AtomicU32 = AtomicU32::new(0);
static ON: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn power_saver_on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Either one saving energy counts, in any of Energy saver's modes.
fn saving(energy_saver: u32, battery_saver: u32) -> bool {
    energy_saver != 0 || battery_saver != 0
}

/// Sends the state to every window when it differs from the last one sent.
#[cfg_attr(not(windows), allow(dead_code))]
fn changed() {
    let on = saving(
        ENERGY_SAVER.load(Ordering::Relaxed),
        BATTERY_SAVER.load(Ordering::Relaxed),
    );
    if ON.swap(on, Ordering::Relaxed) != on
        && let Some(app) = APP.get()
    {
        let _ = app.emit(EVENT, on);
    }
}

#[cfg(windows)]
pub fn start(app: AppHandle) {
    use windows::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerSettingRegisterNotification,
    };
    use windows::Win32::System::SystemServices::GUID_POWER_SAVING_STATUS;
    use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_CALLBACK;
    if APP.set(app).is_err() {
        return;
    }
    for guid in [GUID_ENERGY_SAVER_STATUS, GUID_POWER_SAVING_STATUS] {
        // Registered for the app's life: the parameters and handle are never freed.
        let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(notified),
            Context: std::ptr::null_mut(),
        }));
        let mut handle = std::ptr::null_mut();
        // A Windows without Energy saver refuses its GUID; Battery Saver still reports.
        let _ = unsafe {
            PowerSettingRegisterNotification(
                &guid,
                DEVICE_NOTIFY_CALLBACK,
                windows::Win32::Foundation::HANDLE(params as *mut _ as *mut _),
                &mut handle,
            )
        };
    }
}

#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}

/// In WinNT.h since the 24H2 SDK; windows-rs 0.62 doesn't have it yet.
#[cfg(windows)]
const GUID_ENERGY_SAVER_STATUS: windows::core::GUID =
    windows::core::GUID::from_u128(0x550e8400_e29b_41d4_a716_446655440000);

#[cfg(windows)]
unsafe extern "system" fn notified(
    _context: *const core::ffi::c_void,
    _type: u32,
    setting: *const core::ffi::c_void,
) -> u32 {
    use windows::Win32::System::Power::POWERBROADCAST_SETTING;
    use windows::Win32::System::SystemServices::GUID_POWER_SAVING_STATUS;
    let setting = setting as *const POWERBROADCAST_SETTING;
    if setting.is_null() || unsafe { (*setting).DataLength } < 4 {
        return 0;
    }
    // A DWORD in `Data`, which the struct declares as one byte.
    let value = unsafe {
        std::ptr::addr_of!((*setting).Data)
            .cast::<u32>()
            .read_unaligned()
    };
    let guid = unsafe { (*setting).PowerSetting };
    if guid == GUID_ENERGY_SAVER_STATUS {
        ENERGY_SAVER.store(value, Ordering::Relaxed);
    } else if guid == GUID_POWER_SAVING_STATUS {
        BATTERY_SAVER.store(value, Ordering::Relaxed);
    } else {
        return 0;
    }
    changed();
    0
}

#[cfg(test)]
mod tests {
    use super::saving;

    #[test]
    fn any_saver_mode_counts() {
        assert!(!saving(0, 0));
        assert!(saving(1, 0), "Energy saver, standard");
        assert!(saving(2, 0), "Energy saver, high savings");
        assert!(saving(0, 1), "Battery Saver");
    }
}
