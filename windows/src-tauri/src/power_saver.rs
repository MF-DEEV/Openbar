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

use std::sync::{Mutex, MutexGuard, OnceLock};
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "power://saver";

static APP: OnceLock<AppHandle> = OnceLock::new();
/// One lock for both settings, so callbacks arriving together can't send a stale state last.
static STATE: Mutex<Saver> = Mutex::new(Saver {
    energy_saver: 0,
    battery_saver: 0,
    on: false,
});

struct Saver {
    /// ENERGY_SAVER_OFF (0), _STANDARD (1) or _HIGH_SAVINGS (2).
    energy_saver: u32,
    /// Battery Saver off (0) or on (1), before Energy saver replaced it.
    battery_saver: u32,
    /// The state last sent.
    on: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
enum Setting {
    EnergySaver,
    BatterySaver,
}

fn state() -> MutexGuard<'static, Saver> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
pub fn power_saver_on() -> bool {
    state().on
}

/// Takes a setting's new value and returns the state to send when it changed.
/// Either one saving energy counts, in any of Energy saver's modes.
#[cfg_attr(not(windows), allow(dead_code))]
fn update(saver: &mut Saver, setting: Setting, value: u32) -> Option<bool> {
    match setting {
        Setting::EnergySaver => saver.energy_saver = value,
        Setting::BatterySaver => saver.battery_saver = value,
    }
    let on = saver.energy_saver != 0 || saver.battery_saver != 0;
    (saver.on != on).then(|| {
        saver.on = on;
        on
    })
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
    let setting = if guid == GUID_ENERGY_SAVER_STATUS {
        Setting::EnergySaver
    } else if guid == GUID_POWER_SAVING_STATUS {
        Setting::BatterySaver
    } else {
        return 0;
    };
    let mut saver = state();
    if let Some(on) = update(&mut saver, setting, value)
        && let Some(app) = APP.get()
    {
        // Sent under the lock, so the windows hear the changes in order.
        let _ = app.emit(EVENT, on);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{Saver, Setting, update};

    #[test]
    fn any_saver_mode_counts_and_only_changes_are_sent() {
        let mut saver = Saver {
            energy_saver: 0,
            battery_saver: 0,
            on: false,
        };
        assert_eq!(update(&mut saver, Setting::BatterySaver, 0), None);
        assert_eq!(
            update(&mut saver, Setting::EnergySaver, 1),
            Some(true),
            "standard"
        );
        assert_eq!(
            update(&mut saver, Setting::EnergySaver, 2),
            None,
            "high savings"
        );
        assert_eq!(update(&mut saver, Setting::BatterySaver, 1), None);
        assert_eq!(
            update(&mut saver, Setting::EnergySaver, 0),
            None,
            "Battery Saver still on"
        );
        assert_eq!(update(&mut saver, Setting::BatterySaver, 0), Some(false));
        assert!(!saver.on);
    }
}
