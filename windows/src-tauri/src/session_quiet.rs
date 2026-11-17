// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

//! Whether a reminder of a waiting session (#231) has to wait: the screen is
//! locked, asleep or off, or Do Not Disturb is on. Windows counts Focus
//! Assist (Do not disturb on Windows 11) as the latter; Linux reads what the
//! desktop exposes, where it exposes anything.

use serde::Serialize;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuiet {
    /// Locked, or its display off: nobody would see the island.
    pub locked: bool,
    /// Do Not Disturb, or none where the system does not tell.
    pub dnd: Option<bool>,
}

#[tauri::command]
pub async fn session_quiet() -> SessionQuiet {
    tauri::async_runtime::spawn_blocking(read)
        .await
        .unwrap_or_default()
}

fn read() -> SessionQuiet {
    #[cfg(windows)]
    {
        SessionQuiet {
            locked: crate::session_lock::screen_away(),
            dnd: windows_dnd::focus_assist(),
        }
    }
    #[cfg(target_os = "linux")]
    {
        SessionQuiet {
            locked: crate::linux::session_lock::locked_now(),
            dnd: crate::linux::session_lock::do_not_disturb(),
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        SessionQuiet::default()
    }
}

#[cfg(windows)]
mod windows_dnd {
    use std::ffi::c_void;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::core::{s, w};

    /// `WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED`: the Focus Assist (Do not
    /// disturb) profile in force. Windows offers no documented reading of it;
    /// the shell publishes it as this state, 0 when off, 1 for priority
    /// only, 2 for alarms only.
    const QUIET_HOURS: u64 = 0x0D83_063E_A3BF_1C75;

    type QueryWnfStateData = unsafe extern "system" fn(
        state: *const u64,
        type_id: *const c_void,
        scope: *const c_void,
        stamp: *mut u32,
        buffer: *mut c_void,
        size: *mut u32,
    ) -> i32;

    pub fn focus_assist() -> Option<bool> {
        unsafe {
            let ntdll = GetModuleHandleW(w!("ntdll.dll")).ok()?;
            let query = GetProcAddress(ntdll, s!("NtQueryWnfStateData"))?;
            let query: QueryWnfStateData = std::mem::transmute(query);
            let mut stamp = 0u32;
            let mut profile = 0u32;
            let mut size = std::mem::size_of::<u32>() as u32;
            let status = query(
                &QUIET_HOURS,
                std::ptr::null(),
                std::ptr::null(),
                &mut stamp,
                (&mut profile as *mut u32).cast(),
                &mut size,
            );
            if status < 0 {
                return None;
            }
            // Never written yet (a fresh profile): off.
            Some(profile_on(size, profile))
        }
    }

    pub(super) fn profile_on(size: u32, profile: u32) -> bool {
        size >= 4 && profile != 0
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn a_profile_other_than_off_is_do_not_disturb() {
            assert!(!super::profile_on(0, 0));
            assert!(!super::profile_on(4, 0));
            assert!(super::profile_on(4, 1));
            assert!(super::profile_on(4, 2));
            assert!(!super::profile_on(0, 2));
        }

        #[test]
        fn the_state_can_be_read() {
            // The machine's own state, whatever it is: the call must not fail.
            assert!(super::focus_assist().is_some());
        }
    }
}
