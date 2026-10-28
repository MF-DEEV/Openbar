// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Display brightness for the Controls page (BrightnessService on the Mac;
//! spec 05 §1.3, §15.5). A laptop's own panel goes through WMI
//! (`root\WMI` WmiMonitorBrightness / WmiMonitorBrightnessMethods); an
//! external display through DDC/CI, VCP code 0x10, which is slow (50–200 ms
//! a call), so slider moves are coalesced to about ten writes a second. Every
//! call runs on one MTA worker. Displays are read when the page asks, never
//! polled, and only while the person has turned brightness on in Settings.

#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::com::Worker;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    /// "wmi:<instance>" or "ddc:<GDI device>#<physical monitor>".
    pub id: String,
    pub name: String,
    /// 0…1.
    pub brightness: f32,
    pub built_in: bool,
}

/// Writes at most this often: DDC/CI monitors drop commands sent faster.
const WRITE_INTERVAL: Duration = Duration::from_millis(100);

fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(|| Worker::spawn("vorssaint-brightness", false))
}

fn pending() -> &'static Mutex<HashMap<String, f32>> {
    static P: OnceLock<Mutex<HashMap<String, f32>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Each DDC display's maximum, as last read; the write scales to it.
fn maxima() -> &'static Mutex<HashMap<String, u32>> {
    static M: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

static FLUSHING: AtomicBool = AtomicBool::new(false);

/// A 0…1 level as the display's own 0…max value.
pub fn to_device(value: f32, max: u32) -> u32 {
    (value.clamp(0.0, 1.0) * max as f32).round() as u32
}

/// The display's value as 0…1; a display reporting no range reads as full.
pub fn from_device(current: u32, max: u32) -> f32 {
    if max == 0 {
        1.0
    } else {
        (current.min(max) as f32) / max as f32
    }
}

/// Applies the newest level of each display, then waits out the interval
/// before the next batch, until no level is waiting.
fn flush() {
    loop {
        let batch: Vec<(String, f32)> = pending().lock().unwrap().drain().collect();
        if batch.is_empty() {
            FLUSHING.store(false, Ordering::SeqCst);
            // A level that arrived between the drain and the flag goes now.
            if pending().lock().unwrap().is_empty() || FLUSHING.swap(true, Ordering::SeqCst) {
                return;
            }
            continue;
        }
        let started = Instant::now();
        for (id, value) in batch {
            if let Err(e) = imp::set(&id, value) {
                eprintln!("brightness: {id}: {e}");
            }
        }
        std::thread::sleep(WRITE_INTERVAL.saturating_sub(started.elapsed()));
    }
}

#[tauri::command]
pub async fn brightness_displays() -> Vec<Display> {
    tauri::async_runtime::spawn_blocking(|| worker().run(imp::displays).unwrap_or_default())
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn brightness_set(id: String, value: f32) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("Invalid brightness".into());
    }
    pending().lock().unwrap().insert(id, value);
    if !FLUSHING.swap(true, Ordering::SeqCst) {
        worker().post(flush);
    }
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::{Display, from_device, maxima, to_device};
    use std::collections::HashMap;
    use windows::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
        DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
        DISPLAYCONFIG_TARGET_DEVICE_NAME, DestroyPhysicalMonitors, DisplayConfigGetDeviceInfo,
        GetDisplayConfigBufferSizes, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, PHYSICAL_MONITOR,
        QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig, SetVCPFeature,
    };
    use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoSetProxyBlanket, EOAC_NONE,
        RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_UI1};
    use windows::Win32::System::Wmi::{
        IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY,
        WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WbemLocator,
    };
    use windows::core::{BOOL, BSTR, IUnknown, PCWSTR};

    const BRIGHTNESS: u8 = 0x10;
    /// RPC_C_AUTHN_WINNT and RPC_C_AUTHZ_NONE.
    const AUTHN_WINNT: u32 = 10;
    const AUTHZ_NONE: u32 = 0;

    fn wide_str(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    /// GDI device name ("\\.\DISPLAY1") → the monitor's own name and
    /// whether it is the built-in panel.
    fn targets() -> HashMap<String, (String, bool)> {
        let mut map = HashMap::new();
        unsafe {
            let (mut paths, mut modes) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes)
                != ERROR_SUCCESS
            {
                return map;
            }
            let mut path_info = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
            let mut mode_info = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
            if QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths,
                path_info.as_mut_ptr(),
                &mut modes,
                mode_info.as_mut_ptr(),
                None,
            ) != ERROR_SUCCESS
            {
                return map;
            }
            for path in &path_info[..paths as usize] {
                let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
                source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                source.header.adapterId = path.sourceInfo.adapterId;
                source.header.id = path.sourceInfo.id;
                let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
                target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                target.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                target.header.adapterId = path.targetInfo.adapterId;
                target.header.id = path.targetInfo.id;
                if DisplayConfigGetDeviceInfo(&mut source.header) != 0
                    || DisplayConfigGetDeviceInfo(&mut target.header) != 0
                {
                    continue;
                }
                let tech = path.targetInfo.outputTechnology;
                let built_in = tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED;
                map.insert(
                    wide_str(&source.viewGdiDeviceName).to_lowercase(),
                    (wide_str(&target.monitorFriendlyDeviceName), built_in),
                );
            }
        }
        map
    }

    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        unsafe {
            let list = &mut *(data.0 as *mut Vec<(HMONITOR, String)>);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
                list.push((monitor, wide_str(&info.szDevice)));
            }
            BOOL(1)
        }
    }

    fn monitors() -> Vec<(HMONITOR, String)> {
        let mut list: Vec<(HMONITOR, String)> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(collect),
                LPARAM(&mut list as *mut _ as isize),
            );
        }
        list
    }

    /// The physical monitors behind one display, released after `use_them`.
    fn with_physical<T>(
        monitor: HMONITOR,
        use_them: impl FnOnce(&[PHYSICAL_MONITOR]) -> T,
    ) -> Option<T> {
        unsafe {
            let mut count = 0u32;
            GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count).ok()?;
            if count == 0 {
                return None;
            }
            let mut list = vec![PHYSICAL_MONITOR::default(); count as usize];
            GetPhysicalMonitorsFromHMONITOR(monitor, &mut list).ok()?;
            let result = use_them(&list);
            let _ = DestroyPhysicalMonitors(&list);
            Some(result)
        }
    }

    pub fn displays() -> Vec<Display> {
        let targets = targets();
        let mut out = wmi_displays(&targets);
        let has_panel = !out.is_empty();
        for (monitor, device) in monitors() {
            let (name, built_in) = targets
                .get(&device.to_lowercase())
                .cloned()
                .unwrap_or_default();
            // The panel answers through WMI, and rarely over DDC/CI.
            if built_in && has_panel {
                continue;
            }
            let found = with_physical(monitor, |list| {
                list.iter()
                    .enumerate()
                    .filter_map(|(i, physical)| {
                        let (mut current, mut max) = (0u32, 0u32);
                        let ok = unsafe {
                            GetVCPFeatureAndVCPFeatureReply(
                                physical.hPhysicalMonitor,
                                BRIGHTNESS,
                                None,
                                &mut current,
                                Some(&mut max),
                            )
                        } != 0;
                        if !ok {
                            return None;
                        }
                        // A monitor that reports no range takes the usual 0…100.
                        let max = if max == 0 { 100 } else { max };
                        let id = format!("ddc:{device}#{i}");
                        maxima().lock().unwrap().insert(id.clone(), max);
                        // PHYSICAL_MONITOR is packed: copy the name out before reading it.
                        let description = physical.szPhysicalMonitorDescription;
                        let description = wide_str(&description);
                        let name = if !name.is_empty() {
                            name.clone()
                        } else if !description.is_empty() {
                            description
                        } else {
                            device.trim_start_matches(r"\\.\").to_string()
                        };
                        Some(Display {
                            id,
                            name,
                            brightness: from_device(current, max),
                            built_in,
                        })
                    })
                    .collect::<Vec<_>>()
            });
            out.extend(found.unwrap_or_default());
        }
        out
    }

    pub fn set(id: &str, value: f32) -> Result<(), String> {
        if let Some(instance) = id.strip_prefix("wmi:") {
            return wmi_set(instance, value);
        }
        let rest = id.strip_prefix("ddc:").ok_or("Unknown display")?;
        let (device, index) = rest.rsplit_once('#').ok_or("Unknown display")?;
        let index: usize = index.parse().map_err(|_| "Unknown display")?;
        let max = maxima().lock().unwrap().get(id).copied().unwrap_or(100);
        let (monitor, _) = monitors()
            .into_iter()
            .find(|(_, d)| d.eq_ignore_ascii_case(device))
            .ok_or("Display unavailable")?;
        with_physical(monitor, |list| {
            let physical = list.get(index).ok_or("Display unavailable")?;
            let ok = unsafe {
                SetVCPFeature(physical.hPhysicalMonitor, BRIGHTNESS, to_device(value, max))
            } != 0;
            if ok {
                Ok(())
            } else {
                Err("The display refused the change".to_string())
            }
        })
        .ok_or("Display unavailable")?
    }

    // MARK: WMI

    fn services() -> Result<IWbemServices, String> {
        unsafe {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.message())?;
            let services = locator
                .ConnectServer(
                    &BSTR::from(r"ROOT\WMI"),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
                .map_err(|e| e.message())?;
            blanket(&services.clone().into());
            Ok(services)
        }
    }

    /// Lets WMI act for this user on the proxy, without touching the
    /// process-wide COM security the rest of the app shares.
    fn blanket(proxy: &IUnknown) {
        unsafe {
            let _ = CoSetProxyBlanket(
                proxy,
                AUTHN_WINNT,
                AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            );
        }
    }

    fn query(services: &IWbemServices, wql: &str) -> Vec<IWbemClassObject> {
        let mut out = Vec::new();
        unsafe {
            let flags =
                WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0);
            let Ok(rows) = services.ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), flags, None)
            else {
                return out;
            };
            blanket(&rows.clone().into());
            loop {
                let mut row = [None];
                let mut returned = 0u32;
                // A stuck WMI service must not hold the brightness worker for good.
                let hr = rows.Next(5000, &mut row, &mut returned);
                if hr.is_err() || returned == 0 {
                    break;
                }
                if let Some(row) = row[0].take() {
                    out.push(row);
                }
            }
        }
        out
    }

    fn property(object: &IWbemClassObject, name: &str) -> Option<VARIANT> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut value = VARIANT::default();
        unsafe { object.Get(PCWSTR(wide.as_ptr()), 0, &mut value, None, None) }.ok()?;
        Some(value)
    }

    fn text(object: &IWbemClassObject, name: &str) -> Option<String> {
        property(object, name)
            .and_then(|v| BSTR::try_from(&v).ok())
            .map(|b| b.to_string())
    }

    fn wmi_displays(targets: &HashMap<String, (String, bool)>) -> Vec<Display> {
        let Ok(services) = services() else {
            return Vec::new();
        };
        let panel = targets
            .values()
            .find(|(_, built_in)| *built_in)
            .map(|(name, _)| name.clone());
        query(
            &services,
            "SELECT InstanceName, CurrentBrightness FROM WmiMonitorBrightness WHERE Active = TRUE",
        )
        .iter()
        .filter_map(|row| {
            let instance = text(row, "InstanceName")?;
            let level = property(row, "CurrentBrightness").and_then(|v| u32::try_from(&v).ok())?;
            Some(Display {
                id: format!("wmi:{instance}"),
                name: panel
                    .clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Built-in display".into()),
                brightness: from_device(level, 100),
                built_in: true,
            })
        })
        .collect()
    }

    /// A VT_UI1 VARIANT: WmiSetBrightness takes its level as a uint8.
    fn byte(value: u8) -> VARIANT {
        VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_UI1,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { bVal: value },
                }),
            },
        }
    }

    fn put(object: &IWbemClassObject, name: &str, value: &VARIANT) -> Result<(), String> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe { object.Put(PCWSTR(wide.as_ptr()), 0, value, 0) }.map_err(|e| e.message())
    }

    fn wmi_set(instance: &str, value: f32) -> Result<(), String> {
        let services = services()?;
        let target = query(
            &services,
            "SELECT InstanceName FROM WmiMonitorBrightnessMethods WHERE Active = TRUE",
        )
        .into_iter()
        .find(|row| text(row, "InstanceName").as_deref() == Some(instance))
        .ok_or("Display unavailable")?;
        let path = text(&target, "__PATH").ok_or("Display unavailable")?;
        unsafe {
            let mut class = None;
            services
                .GetObject(
                    &BSTR::from("WmiMonitorBrightnessMethods"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    Some(&mut class),
                    None,
                )
                .map_err(|e| e.message())?;
            let class: IWbemClassObject = class.ok_or("WMI has no brightness methods")?;
            let mut signature = None;
            let name: Vec<u16> = "WmiSetBrightness".encode_utf16().chain(Some(0)).collect();
            class
                .GetMethod(
                    PCWSTR(name.as_ptr()),
                    0,
                    &mut signature,
                    std::ptr::null_mut(),
                )
                .map_err(|e| e.message())?;
            let signature: IWbemClassObject = signature.ok_or("WMI has no WmiSetBrightness")?;
            let input = signature.SpawnInstance(0).map_err(|e| e.message())?;
            put(&input, "Timeout", &VARIANT::from(1i32))?;
            put(&input, "Brightness", &byte(to_device(value, 100) as u8))?;
            services
                .ExecMethod(
                    &BSTR::from(path),
                    &BSTR::from("WmiSetBrightness"),
                    WBEM_GENERIC_FLAG_TYPE(0),
                    None,
                    &input,
                    None,
                    None,
                )
                .map_err(|e| e.message())
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn displays() -> Vec<super::Display> {
        Vec::new()
    }
    pub fn set(_id: &str, _value: f32) -> Result<(), String> {
        Err("Brightness is available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_scale_to_the_display_range() {
        assert_eq!(to_device(0.5, 100), 50);
        assert_eq!(to_device(1.0, 80), 80);
        assert_eq!(to_device(-1.0, 100), 0);
        assert_eq!(to_device(2.0, 100), 100);
        assert_eq!(from_device(40, 80), 0.5);
        assert_eq!(from_device(120, 100), 1.0);
        assert_eq!(from_device(0, 0), 1.0);
    }

    #[test]
    fn rejects_invalid_levels() {
        assert!(brightness_set("ddc:x#0".into(), f32::NAN).is_err());
        assert!(brightness_set("ddc:x#0".into(), 1.5).is_err());
    }
}
