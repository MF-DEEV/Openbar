// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The person's preferences, one JSON file in the app's config folder
//! (`%APPDATA%\com.vorssaint.island\settings.json`). The windows own the
//! schema and its defaults; this keeps what was changed, merges patches
//! (objects merge, anything else replaces, null removes), writes atomically
//! and tells every window. The few preferences the backend acts on itself
//! (display, content previews) are applied here, so they hold even before a
//! window asks.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const CHANGED: &str = "prefs://changed";

pub struct Prefs {
    path: Option<PathBuf>,
    value: Mutex<Value>,
}

impl Prefs {
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|d| d.join("settings.json"));
        let value = path.as_deref().map(read).unwrap_or_else(empty);
        Prefs {
            path,
            value: Mutex::new(value),
        }
    }

    pub fn get(&self) -> Value {
        self.value.lock().unwrap().clone()
    }

    /// Merges `patch` and saves; returns the whole new value.
    pub fn set(&self, patch: &Value) -> Result<Value, String> {
        let mut value = self.value.lock().unwrap();
        let mut next = value.clone();
        merge(&mut next, patch);
        if let Some(path) = &self.path {
            write(path, &next).map_err(|e| format!("couldn't save settings: {e}"))?;
        }
        *value = next.clone();
        Ok(next)
    }
}

fn empty() -> Value {
    Value::Object(Map::new())
}

/// A missing or broken file reads as no changes; a broken one is kept aside.
fn read(path: &Path) -> Value {
    let Ok(text) = fs::read_to_string(path) else {
        return empty();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            let _ = fs::rename(path, path.with_extension("json.broken"));
            empty()
        }
    }
}

fn write(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&tmp, path)
}

pub fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (key, v) in p {
                if v.is_null() {
                    t.remove(key);
                } else if let Some(existing) = t.get_mut(key)
                    && existing.is_object()
                    && v.is_object()
                {
                    merge(existing, v);
                } else {
                    t.insert(key.clone(), v.clone());
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

/// Acts on what the backend owns, from the whole value.
pub fn apply<R: Runtime>(app: &AppHandle<R>, value: &Value) {
    let display = value
        .pointer("/island/display")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    if let Some(window) = app.get_webview_window(crate::island::LABEL) {
        let _ = crate::island::place(&window, display);
    }
    let flag = |pointer: &str, default: bool| {
        value
            .pointer(pointer)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    shortcut(app, flag("/island/shortcutEnabled", true));
    crate::fullscreen::set_hides(app, flag("/island/hideInFullscreen", true));
    crate::island::show_in_captures(app, flag("/island/showInCaptures", true));
    let previews = value
        .pointer("/sessions/previews")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(service) = app.try_state::<crate::sessions::service::Shared>() {
        service.set_previews(previews);
    }
}

/// Registers Ctrl+Alt+Space or lets it go. Taken by another app, the island
/// still opens by clicking it.
fn shortcut<R: Runtime>(app: &AppHandle<R>, on: bool) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcuts = app.global_shortcut();
    let key = crate::toggle_shortcut();
    match (on, shortcuts.is_registered(key)) {
        (true, false) => {
            let _ = shortcuts.register(key);
        }
        (false, true) => {
            let _ = shortcuts.unregister(key);
        }
        _ => {}
    }
}

#[tauri::command]
pub fn prefs_get(prefs: tauri::State<'_, Prefs>) -> Value {
    prefs.get()
}

#[tauri::command]
pub fn prefs_set<R: Runtime>(
    app: AppHandle<R>,
    prefs: tauri::State<'_, Prefs>,
    patch: Value,
) -> Result<Value, String> {
    let next = prefs.set(&patch)?;
    apply(&app, &next);
    let _ = app.emit(CHANGED, &next);
    Ok(next)
}

/// Starts Vorssaint when the person signs in to Windows (the per-user Run key).
#[tauri::command]
pub fn launch_at_login_get() -> bool {
    autostart::enabled()
}

#[tauri::command]
pub fn launch_at_login_set(on: bool) -> Result<bool, String> {
    autostart::set(on)?;
    Ok(autostart::enabled())
}

#[cfg(windows)]
mod autostart {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey,
        RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows::core::{HSTRING, PCWSTR};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "Vorssaint";

    fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
        let mut key = HKEY::default();
        let ok = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &HSTRING::from(RUN),
                None,
                access,
                &mut key,
            )
        };
        ok.is_ok().then_some(key)
    }

    pub fn enabled() -> bool {
        let Some(key) = open(KEY_QUERY_VALUE) else {
            return false;
        };
        let found =
            unsafe { RegQueryValueExW(key, &HSTRING::from(NAME), None, None, None, None).is_ok() };
        unsafe {
            let _ = RegCloseKey(key);
        }
        found
    }

    pub fn set(on: bool) -> Result<(), String> {
        let key = open(KEY_SET_VALUE).ok_or("couldn't open the Run key")?;
        let result = if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let command = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
            let bytes =
                unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
            unsafe { RegSetValueExW(key, &HSTRING::from(NAME), None, REG_SZ, Some(bytes)) }
        } else {
            let r = unsafe { RegDeleteValueW(key, PCWSTR(HSTRING::from(NAME).as_ptr())) };
            // Already off is fine.
            if r == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                windows::Win32::Foundation::ERROR_SUCCESS
            } else {
                r
            }
        };
        unsafe {
            let _ = RegCloseKey(key);
        }
        result.ok().map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod autostart {
    pub fn enabled() -> bool {
        false
    }
    pub fn set(_on: bool) -> Result<(), String> {
        Err("launch at login is Windows only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patches_merge_objects_and_replace_the_rest() {
        let mut v = json!({"island": {"display": "primary", "hiddenModules": ["camera"]}, "sessions": {"previews": true}});
        merge(
            &mut v,
            &json!({"island": {"display": "cursor", "hiddenModules": ["watch"]}, "sessions": {"previews": null}}),
        );
        assert_eq!(
            v,
            json!({"island": {"display": "cursor", "hiddenModules": ["watch"]}, "sessions": {}})
        );
    }

    #[test]
    fn a_broken_file_reads_as_empty_and_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("vors-prefs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(read(&path), json!({}));
        assert!(path.with_extension("json.broken").exists());
        write(&path, &json!({"a": 1})).unwrap();
        assert_eq!(read(&path), json!({"a": 1}));
        let _ = fs::remove_dir_all(&dir);
    }
}
