// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The closed island needs only SYSTEM_POWER_STATUS, never the full System
//! monitor's process, device or battery-health probes. Windows notifies us
//! when charge, supply or charging state changes; no polling runs at rest.

use crate::system_monitor::Battery;

#[tauri::command]
pub fn battery_status() -> Option<Battery> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
        crate::system_monitor::math::battery_from_status(
            status.ACLineStatus,
            status.BatteryFlag,
            status.BatteryLifePercent,
            status.BatteryLifeTime,
        )
    }
    #[cfg(not(windows))]
    None
}

#[cfg(windows)]
pub fn start(app: tauri::AppHandle) {
    use std::sync::{Mutex, OnceLock};
    use tauri::Emitter;
    use windows::{Foundation::EventHandler, System::Power::PowerManager, core::IInspectable};

    static WORKER: OnceLock<crate::com::Worker> = OnceLock::new();
    if WORKER.get().is_some() {
        return;
    }
    let worker = WORKER.get_or_init(|| crate::com::Worker::spawn("island-battery", false));
    worker.post(move || {
        // Keep concurrent callbacks ordered, reading the latest Windows state
        // under the same lock before sending it to every island window.
        let last = std::sync::Arc::new(Mutex::new(battery_status()));
        let handler = EventHandler::<IInspectable>::new(move |_, _| {
            let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
            let next = battery_status();
            if *last != next {
                let _ = app.emit("power://battery", &next);
                *last = next;
            }
            Ok(())
        });
        // Static events retain the handler for the app's life. The worker's
        // COM apartment stays alive too, even when no System page is open.
        let _ = PowerManager::RemainingChargePercentChanged(&handler);
        let _ = PowerManager::PowerSupplyStatusChanged(&handler);
        let _ = PowerManager::BatteryStatusChanged(&handler);
    });
}

#[cfg(not(windows))]
pub fn start(_app: tauri::AppHandle) {}
