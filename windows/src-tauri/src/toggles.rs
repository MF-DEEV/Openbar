// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Quick toggles, the one-click system actions the Tools section hosts
//! (QuickTogglesService.swift; spec 05 §7.3). Nothing here runs, polls or
//! observes until the page asks: the state is read when the list shows, an
//! action runs when clicked, and each reports success or failure for its row.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    DarkMode,
    EmptyTrash,
    EjectDisks,
    HiddenFiles,
    DesktopIcons,
    LockScreen,
    DisplayOff,
    ScreenSaver,
}

/// What each row shows now. `None` where Windows wouldn't say.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub dark: Option<bool>,
    pub hidden_files_shown: Option<bool>,
    /// None when the desktop's icon view can't be found: the row hides.
    pub desktop_icons_shown: Option<bool>,
    pub ejectable: usize,
    pub recycle_items: Option<i64>,
}

/// A volume "Eject all disks" might offer.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// "E:"
    pub drive: String,
    pub label: String,
    pub removable: bool,
    pub usb: bool,
    pub system: bool,
}

/// Which volumes "Eject all disks" offers (QuickTogglesSupport.shouldOfferEject):
/// removable media, or a fixed drive on the USB or SD bus (most desk drives
/// say fixed); never the system drive or one the person excluded.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn offers_eject(volume: &Volume, excluded: &[String]) -> bool {
    !volume.system && (volume.removable || volume.usb) && !is_excluded(volume, excluded)
}

/// An exclusion matches the label, or the drive as "E", "E:" or "E:\",
/// ignoring case and spaces (QuickTogglesSupport.isExcluded).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_excluded(volume: &Volume, excluded: &[String]) -> bool {
    let drive = volume.drive.trim_end_matches(':').to_lowercase();
    excluded.iter().any(|entry| {
        let entry = entry.trim().to_lowercase();
        if entry.is_empty() {
            return false;
        }
        let as_drive = entry.trim_end_matches('\\').trim_end_matches(':');
        entry == volume.label.trim().to_lowercase() || as_drive == drive
    })
}

/// Empties the Recycle Bin. An empty bin is already the outcome asked for:
/// it isn't touched (`SHEmptyRecycleBinW` fails on one), and an error that
/// leaves the bin empty after all still counts as done.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn empty_recycle_bin(
    count: impl Fn() -> Option<i64>,
    empty: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if count() == Some(0) {
        return Ok(());
    }
    match empty() {
        Ok(()) => Ok(()),
        Err(_) if count() == Some(0) => Ok(()),
        Err(e) => Err(e),
    }
}

fn excluded_volumes<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    app.try_state::<crate::prefs::Prefs>()
        .and_then(|p| p.get().pointer("/island/ejectExcluded").cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

#[tauri::command]
pub async fn toggles_state<R: Runtime>(app: AppHandle<R>) -> State {
    let excluded = excluded_volumes(&app);
    tauri::async_runtime::spawn_blocking(move || imp::state(&excluded))
        .await
        .unwrap_or_default()
}

/// Runs one action; `Err` turns its row to "Could not complete."
#[tauri::command]
pub async fn toggles_run<R: Runtime>(app: AppHandle<R>, action: Action) -> Result<(), String> {
    let excluded = excluded_volumes(&app);
    tauri::async_runtime::spawn_blocking(move || imp::run(action, &excluded))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod imp {
    use super::{Action, State, Volume, empty_recycle_bin, offers_eject};
    use std::thread::sleep;
    use std::time::Duration;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Parent, CM_Request_Device_EjectW, CR_SUCCESS, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        PNP_VETO_TYPE, PNP_VetoTypeUnknown, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW,
    };
    use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, WPARAM};
    use windows::Win32::Storage::FileSystem::{
        BusTypeSd, BusTypeUsb, CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_QUERY_PROPERTY,
        PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty,
    };
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
    };
    use windows::Win32::System::Shutdown::LockWorkStation;
    use windows::Win32::System::Variant::{VARIANT, VT_I4};
    use windows::Win32::UI::Shell::{
        IShellWindows, IWebBrowser2, SHELLSTATEA, SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
        SHERB_NOSOUND, SHGetSetSettings, SHQUERYRBINFO, SHQueryRecycleBinW, SSF_SHOWALLOBJECTS, ShellWindows,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetDesktopWindow, HWND_BROADCAST, PostMessageW, SMTO_ABORTIFHUNG,
        SendMessageTimeoutW, WM_COMMAND, WM_SETTINGCHANGE, WM_SYSCOMMAND,
    };
    use windows::core::{HSTRING, Interface, PCWSTR, w};

    const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    const ADVANCED: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced");
    const SC_MONITORPOWER: usize = 0xF170;
    const SC_SCREENSAVE: usize = 0xF140;
    /// Explorer's "Show desktop icons" command (undocumented, spec 05 §7.3).
    const TOGGLE_DESKTOP_ICONS: usize = 0x7402;
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;

    pub fn state(excluded: &[String]) -> State {
        let _apartment = crate::com::Apartment::sta();
        State {
            dark: read_dword(PERSONALIZE, w!("AppsUseLightTheme")).map(|light| light == 0).or(Some(false)),
            hidden_files_shown: Some(hidden_files_shown()),
            desktop_icons_shown: desktop_view().map(|_| read_dword(ADVANCED, w!("HideIcons")) != Some(1)),
            ejectable: volumes().iter().filter(|v| offers_eject(v, excluded)).count(),
            recycle_items: recycle_count(),
        }
    }

    pub fn run(action: Action, excluded: &[String]) -> Result<(), String> {
        let _apartment = crate::com::Apartment::sta();
        match action {
            Action::DarkMode => toggle_dark_mode(),
            Action::EmptyTrash => empty_recycle_bin(recycle_count, || unsafe {
                SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
                    .map_err(|e| e.to_string())
            }),
            Action::EjectDisks => eject_all(excluded),
            Action::HiddenFiles => toggle_hidden_files(),
            Action::DesktopIcons => {
                let view = desktop_view().ok_or("the desktop isn't available")?;
                // A hung Explorer must not hold the row in "running" for good.
                let sent = unsafe {
                    SendMessageTimeoutW(
                        view,
                        WM_COMMAND,
                        WPARAM(TOGGLE_DESKTOP_ICONS),
                        LPARAM(0),
                        SMTO_ABORTIFHUNG,
                        2000,
                        None,
                    )
                };
                if sent.0 == 0 { Err("the desktop didn't answer".into()) } else { Ok(()) }
            }
            Action::LockScreen => unsafe { LockWorkStation().map_err(|e| e.to_string()) },
            Action::DisplayOff => {
                // The pointer's own movement from the click would wake it at once.
                sleep(Duration::from_millis(500));
                unsafe {
                    PostMessageW(Some(HWND_BROADCAST), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER), LPARAM(2))
                        .map_err(|e| e.to_string())
                }
            }
            Action::ScreenSaver => {
                // With none chosen, Windows quietly does nothing: say so instead.
                read_string(w!("Control Panel\\Desktop"), w!("SCRNSAVE.EXE"))
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("no screen saver is set")?;
                unsafe {
                    SendMessageTimeoutW(
                        GetDesktopWindow(),
                        WM_SYSCOMMAND,
                        WPARAM(SC_SCREENSAVE),
                        LPARAM(0),
                        SMTO_ABORTIFHUNG,
                        1000,
                        None,
                    );
                }
                Ok(())
            }
        }
    }

    // MARK: Registry

    fn read_dword(key: PCWSTR, name: PCWSTR) -> Option<u32> {
        let mut value = 0u32;
        let mut size = 4u32;
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            )
        }
        .ok()
        .ok()?;
        Some(value)
    }

    fn write_dword(key: PCWSTR, name: PCWSTR, value: u32) -> Result<(), String> {
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some((&value as *const u32).cast()), 4)
        }
        .ok()
        .map_err(|e| e.to_string())
    }

    fn read_string(key: PCWSTR, name: PCWSTR) -> Option<String> {
        let mut buffer = [0u16; 520];
        let mut size = (buffer.len() * 2) as u32;
        unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        }
        .ok()
        .ok()?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..len]))
    }

    // MARK: Appearance

    /// Apps and the shell both follow; the broadcast tells running apps now.
    fn toggle_dark_mode() -> Result<(), String> {
        let light = if read_dword(PERSONALIZE, w!("AppsUseLightTheme")) == Some(0) { 1 } else { 0 };
        write_dword(PERSONALIZE, w!("AppsUseLightTheme"), light)?;
        write_dword(PERSONALIZE, w!("SystemUsesLightTheme"), light)?;
        let area = HSTRING::from("ImmersiveColorSet");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(area.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                100,
                None,
            );
        }
        Ok(())
    }

    // MARK: Explorer

    fn hidden_files_shown() -> bool {
        let mut state = SHELLSTATEA::default();
        unsafe { SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, false) };
        state._bitfield1 & 1 != 0
    }

    /// The documented route, then a refresh of every open folder window,
    /// which `SHChangeNotify` alone doesn't do.
    fn toggle_hidden_files() -> Result<(), String> {
        let mut state = SHELLSTATEA::default();
        unsafe { SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, false) };
        let show = state._bitfield1 & 1 == 0;
        state._bitfield1 = (state._bitfield1 & !1) | show as i32;
        unsafe { SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, true) };
        if hidden_files_shown() != show {
            return Err("Explorer kept the old setting".into());
        }
        refresh_folder_windows();
        Ok(())
    }

    fn refresh_folder_windows() {
        unsafe {
            let Ok(windows) = CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) else { return };
            let count = windows.Count().unwrap_or(0);
            for i in 0..count {
                let mut index = VARIANT::default();
                (*index.Anonymous.Anonymous).vt = VT_I4;
                (*index.Anonymous.Anonymous).Anonymous.lVal = i;
                if let Ok(browser) = windows.Item(&index).and_then(|d| d.cast::<IWebBrowser2>()) {
                    let _ = browser.Refresh();
                }
            }
        }
    }

    /// The desktop's icon view: under Progman, or under a WorkerW once a
    /// wallpaper slideshow has split the desktop.
    fn desktop_view() -> Option<windows::Win32::Foundation::HWND> {
        unsafe {
            let progman = FindWindowW(w!("Progman"), PCWSTR::null()).ok()?;
            if let Ok(view) = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), PCWSTR::null()) {
                return Some(view);
            }
            let mut worker = None;
            while let Ok(next) = FindWindowExW(None, worker, w!("WorkerW"), PCWSTR::null()) {
                if let Ok(view) = FindWindowExW(Some(next), None, w!("SHELLDLL_DefView"), PCWSTR::null()) {
                    return Some(view);
                }
                worker = Some(next);
            }
            None
        }
    }

    // MARK: Recycle Bin

    fn recycle_count() -> Option<i64> {
        let mut info = SHQUERYRBINFO { cbSize: size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
        unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.ok()?;
        Some(info.i64NumItems)
    }

    // MARK: Disks

    fn volumes() -> Vec<Volume> {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .filter_map(|i| {
                let drive = format!("{}:", (b'A' + i) as char);
                let root = HSTRING::from(format!("{drive}\\"));
                let kind = unsafe { GetDriveTypeW(&root) };
                if kind != DRIVE_REMOVABLE && kind != DRIVE_FIXED {
                    return None;
                }
                let mut label = [0u16; 261];
                // An empty card slot says removable too, but has no volume to read.
                unsafe { GetVolumeInformationW(&root, Some(&mut label), None, None, None, None) }.ok()?;
                let len = label.iter().position(|&c| c == 0).unwrap_or(0);
                Some(Volume {
                    usb: kind == DRIVE_FIXED && external_bus(&drive),
                    label: String::from_utf16_lossy(&label[..len]),
                    removable: kind == DRIVE_REMOVABLE,
                    system: drive == system,
                    drive,
                })
            })
            .collect()
    }

    /// Opens `\\.\E:` without asking for read or write access.
    fn open_volume(drive: &str) -> Option<HANDLE> {
        unsafe {
            CreateFileW(
                &HSTRING::from(format!("\\\\.\\{drive}")),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
            .ok()
        }
    }

    fn external_bus(drive: &str) -> bool {
        let Some(handle) = open_volume(drive) else { return false };
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            ..Default::default()
        };
        let mut descriptor = [0u8; 1024];
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some((&query as *const STORAGE_PROPERTY_QUERY).cast()),
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(descriptor.as_mut_ptr().cast()),
                descriptor.len() as u32,
                None,
                None,
            )
            .is_ok()
        };
        unsafe {
            let _ = CloseHandle(handle);
        }
        if !ok {
            return false;
        }
        let descriptor = unsafe { descriptor.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>().read_unaligned() };
        descriptor.BusType == BusTypeUsb || descriptor.BusType == BusTypeSd
    }

    fn device_number(handle: HANDLE) -> Option<u32> {
        let mut number = STORAGE_DEVICE_NUMBER::default();
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                None,
                0,
                Some((&mut number as *mut STORAGE_DEVICE_NUMBER).cast()),
                size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                None,
                None,
            )
            .ok()?;
        }
        Some(number.DeviceNumber)
    }

    /// The disk device node holding a volume, matched by disk number.
    fn disk_devinst(drive: &str) -> Option<u32> {
        let volume = open_volume(drive)?;
        let wanted = device_number(volume);
        unsafe {
            let _ = CloseHandle(volume);
        }
        let wanted = wanted?;
        unsafe {
            let set = SetupDiGetClassDevsW(
                Some(&GUID_DEVINTERFACE_DISK),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
            .ok()?;
            let mut found = None;
            for index in 0.. {
                let mut interface =
                    SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
                if SetupDiEnumDeviceInterfaces(set, None, &GUID_DEVINTERFACE_DISK, index, &mut interface).is_err() {
                    break;
                }
                // The detail is a size field then the path, in one u32-aligned buffer.
                let mut buffer = [0u32; 512];
                let detail = buffer.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
                (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
                if SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &interface,
                    Some(detail),
                    (buffer.len() * 4) as u32,
                    None,
                    Some(&mut info),
                )
                .is_err()
                {
                    continue;
                }
                let path = PCWSTR((*detail).DevicePath.as_ptr());
                let Ok(disk) =
                    CreateFileW(path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None)
                else {
                    continue;
                };
                let number = device_number(disk);
                let _ = CloseHandle(disk);
                if number == Some(wanted) {
                    found = Some(info.DevInst);
                    break;
                }
            }
            let _ = SetupDiDestroyDeviceInfoList(set);
            found
        }
    }

    /// The device to eject for a volume: the disk's parent (the USB device).
    fn eject_target(drive: &str) -> Option<u32> {
        let disk = disk_devinst(drive)?;
        let mut parent = 0u32;
        (unsafe { CM_Get_Parent(&mut parent, disk, 0) } == CR_SUCCESS).then_some(parent)
    }

    /// Asks Plug and Play to eject a device, with a veto buffer so Windows
    /// doesn't show its own dialog; a veto (files open) is a failure. Three
    /// tries, as the device may be busy.
    fn eject(device: u32) -> bool {
        for _ in 0..3 {
            let mut veto = PNP_VETO_TYPE::default();
            let mut name = [0u16; 260];
            let result = unsafe { CM_Request_Device_EjectW(device, Some(&mut veto), Some(&mut name), 0) };
            if result == CR_SUCCESS && veto == PNP_VetoTypeUnknown {
                return true;
            }
            sleep(Duration::from_millis(300));
        }
        false
    }

    fn eject_all(excluded: &[String]) -> Result<(), String> {
        // Every device once: two partitions of one stick are one eject.
        let mut devices = Vec::new();
        let mut failed = 0;
        for volume in volumes().into_iter().filter(|v| offers_eject(v, excluded)) {
            match eject_target(&volume.drive) {
                Some(device) if !devices.contains(&device) => devices.push(device),
                Some(_) => {}
                None => failed += 1,
            }
        }
        failed += devices.into_iter().filter(|&device| !eject(device)).count();
        if failed == 0 { Ok(()) } else { Err(format!("{failed} disk(s) could not be ejected")) }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Action, State};

    pub fn state(_excluded: &[String]) -> State {
        State { dark: Some(true), hidden_files_shown: Some(false), desktop_icons_shown: Some(true), ejectable: 1, recycle_items: Some(3) }
    }

    pub fn run(_action: Action, _excluded: &[String]) -> Result<(), String> {
        Err("quick toggles need Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn volume(drive: &str, label: &str, removable: bool, usb: bool) -> Volume {
        Volume { drive: drive.into(), label: label.into(), removable, usb, system: false }
    }

    #[test]
    fn removable_and_usb_volumes_are_offered() {
        assert!(offers_eject(&volume("E:", "STICK", true, false), &[]));
        assert!(offers_eject(&volume("F:", "Backup", false, true), &[]));
        assert!(!offers_eject(&volume("D:", "Data", false, false), &[]), "an internal fixed drive stays");
    }

    #[test]
    fn the_system_drive_is_never_offered() {
        let mut c = volume("C:", "Windows", false, true);
        c.system = true;
        assert!(!offers_eject(&c, &[]));
    }

    #[test]
    fn exclusions_match_label_or_drive_in_any_spelling() {
        let v = volume("F:", "Backup", false, true);
        for entry in ["backup", " Backup ", "F", "f:", "F:\\"] {
            assert!(!offers_eject(&v, &[entry.to_string()]), "{entry} should exclude F:");
        }
        assert!(offers_eject(&v, &["G:".into(), "".into(), "Back".into()]));
    }

    #[test]
    fn an_already_empty_bin_is_left_alone_and_succeeds() {
        let called = Cell::new(false);
        let result = empty_recycle_bin(
            || Some(0),
            || {
                called.set(true);
                Err("E_UNEXPECTED".into())
            },
        );
        assert_eq!(result, Ok(()));
        assert!(!called.get());
    }

    #[test]
    fn an_error_that_still_empties_the_bin_counts_as_done() {
        let items = Cell::new(4);
        let result = empty_recycle_bin(
            || Some(items.get()),
            || {
                items.set(0);
                Err("S_FALSE".into())
            },
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_real_failure_is_reported() {
        assert_eq!(empty_recycle_bin(|| Some(2), || Err("denied".into())), Err("denied".into()));
        // The count unknown: the call decides.
        assert_eq!(empty_recycle_bin(|| None, || Ok(())), Ok(()));
    }

    #[test]
    fn actions_and_state_speak_camel_case() {
        let action: Action = serde_json::from_str("\"emptyTrash\"").unwrap();
        assert_eq!(action, Action::EmptyTrash);
        let json = serde_json::to_value(State::default()).unwrap();
        assert!(json.get("hiddenFilesShown").is_some());
    }
}
