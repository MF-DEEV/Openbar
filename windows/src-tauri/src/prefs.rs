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
    /// The file exists but couldn't be read or moved aside: saving would
    /// overwrite what the person had, so changes are refused instead.
    blocked: bool,
}

impl Prefs {
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|d| d.join("settings.json"));
        Self::at(path)
    }

    fn at(path: Option<PathBuf>) -> Self {
        let (value, blocked) = match path.as_deref().map(read) {
            Some(Ok(v)) => (v, false),
            Some(Err(_)) => (empty(), true),
            None => (empty(), false),
        };
        Prefs {
            path,
            value: Mutex::new(value),
            blocked,
        }
    }

    pub fn get(&self) -> Value {
        self.value.lock().unwrap().clone()
    }

    /// Merges `patch` and saves; returns the whole new value.
    pub fn set(&self, patch: &Value) -> Result<Value, String> {
        if self.blocked {
            return Err("settings.json can't be read, so changes aren't saved".into());
        }
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

/// A missing file reads as no changes. One that isn't a JSON object, or
/// can't be read at all (locked, not text), is moved aside so the next save
/// doesn't destroy it; if even that fails, it's an error and nothing saves.
fn read(path: &Path) -> std::io::Result<Value> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(empty()),
        Err(_) => return set_aside(path),
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return set_aside(path);
    };
    // Notepad and PowerShell 5 may write a byte-order mark.
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(v @ Value::Object(_)) => Ok(v),
        _ => set_aside(path),
    }
}

fn set_aside(path: &Path) -> std::io::Result<Value> {
    fs::rename(path, path.with_extension("json.broken"))?;
    Ok(empty())
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
                } else if v.is_object() {
                    // A new object: merged into nothing, so its nulls drop out.
                    let mut fresh = empty();
                    merge(&mut fresh, v);
                    t.insert(key.clone(), fresh);
                } else {
                    t.insert(key.clone(), v.clone());
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

/// Acts on what the backend owns. `before` is the value it replaces (None at
/// launch), so only what changed is redone: placing the island again or
/// re-reading transcripts on every slider step would be noticeable.
pub fn apply<R: Runtime>(app: &AppHandle<R>, before: Option<&Value>, value: &Value) {
    let display = |v: &Value| v.pointer("/island/display").cloned();
    if before.is_none_or(|b| display(b) != display(value)) {
        let choice = display(value)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if let Some(window) = app.get_webview_window(crate::island::LABEL) {
            let _ = crate::island::place(&window, choice);
        }
    }
    let flag = |pointer: &str, default: bool| {
        value
            .pointer(pointer)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    shortcut(app, flag("/island/shortcutEnabled", true));
    crate::fullscreen::set_hides(flag("/island/hideInFullscreen", true));
    let notifications_hidden = value
        .pointer("/island/hiddenModules")
        .and_then(Value::as_array)
        .is_some_and(|modules| modules.iter().any(|id| id.as_str() == Some("notifications")));
    crate::notifications::set_enabled(
        app,
        flag("/island/enabled", true)
            && flag("/island/sections/notifications", true)
            && !notifications_hidden,
    );
    crate::calendar::apply(app, before, value);
    let previews = value
        .pointer("/sessions/previews")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(service) = app.try_state::<crate::sessions::service::Shared>() {
        service.set_previews(previews);
    }
    crate::clipboard::apply_prefs(app, before, value);
    // Also applies Show in screenshots, which a watched display region overrides.
    crate::watch::apply_prefs(app);
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

/// False when the shortcut is on but another app holds Ctrl+Alt+Space.
#[tauri::command]
pub fn shortcut_registered<R: Runtime>(app: AppHandle<R>) -> bool {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    app.global_shortcut()
        .is_registered(crate::toggle_shortcut())
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
    let before = prefs.get();
    let next = prefs.set(&patch)?;
    apply(&app, Some(&before), &next);
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
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
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

    /// What the Run value holds for this exe: its quoted path.
    fn command() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!("\"{}\"", exe.display()))
    }

    /// On only when the value starts this very exe: a moved install or
    /// another build under the same name reads as off, and turning it on
    /// points it here.
    pub fn enabled() -> bool {
        let Some(key) = open(KEY_QUERY_VALUE) else {
            return false;
        };
        let name = HSTRING::from(NAME);
        let mut size = 0u32;
        let mut kind = REG_VALUE_TYPE::default();
        let mut stored = None;
        unsafe {
            if RegQueryValueExW(key, &name, None, Some(&mut kind), None, Some(&mut size)).is_ok()
                && kind == REG_SZ
                && size >= 2
            {
                // Whole UTF-16 units, and the registry told exactly how many bytes it may write.
                let mut buf = vec![0u16; (size as usize).div_ceil(2)];
                size = (buf.len() * 2) as u32;
                if RegQueryValueExW(
                    key,
                    &name,
                    None,
                    Some(&mut kind),
                    Some(buf.as_mut_ptr().cast()),
                    Some(&mut size),
                )
                .is_ok()
                    && kind == REG_SZ
                {
                    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    stored = Some(String::from_utf16_lossy(&buf[..len]));
                }
            }
            let _ = RegCloseKey(key);
        }
        match (stored, command()) {
            (Some(stored), Some(command)) => stored.eq_ignore_ascii_case(&command),
            _ => false,
        }
    }

    pub fn set(on: bool) -> Result<(), String> {
        let key = open(KEY_SET_VALUE).ok_or("couldn't open the Run key")?;
        let result = if on {
            let command = command().ok_or("couldn't find Vorssaint.exe")?;
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
        assert_eq!(read(&path).unwrap(), json!({}));
        assert!(path.with_extension("json.broken").exists());
        write(&path, &json!({"a": 1})).unwrap();
        assert_eq!(read(&path).unwrap(), json!({"a": 1}));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn patches_add_missing_objects_and_keep_their_siblings() {
        let mut v = json!({});
        merge(&mut v, &json!({"island": {"capsuleFit": {"width": 4}}}));
        merge(
            &mut v,
            &json!({"island": {"capsuleFit": {"drop": 2}, "outline": true}}),
        );
        assert_eq!(
            v,
            json!({"island": {"capsuleFit": {"width": 4, "drop": 2}, "outline": true}})
        );
        // Removing a key that was never set, inside an object that exists, changes nothing.
        merge(&mut v, &json!({"island": {"silhouette": null}}));
        assert_eq!(
            v,
            json!({"island": {"capsuleFit": {"width": 4, "drop": 2}, "outline": true}})
        );
    }

    #[test]
    fn set_saves_the_merged_value_and_a_reload_reads_it_back() {
        let dir = std::env::temp_dir().join(format!("vors-prefs-set-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let prefs = Prefs {
            path: Some(path.clone()),
            value: Mutex::new(empty()),
            blocked: false,
        };
        prefs
            .set(&json!({"island": {"outline": true, "display": "cursor"}}))
            .unwrap();
        let next = prefs.set(&json!({"island": {"outline": null}})).unwrap();
        assert_eq!(next, json!({"island": {"display": "cursor"}}));
        assert_eq!(prefs.get(), next);
        assert_eq!(read(&path).unwrap(), next);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_save_keeps_the_value_it_had() {
        // The folder is a file, so the write fails.
        let blocker =
            std::env::temp_dir().join(format!("vors-prefs-blocker-{}", std::process::id()));
        fs::write(&blocker, "").unwrap();
        let prefs = Prefs {
            path: Some(blocker.join("settings.json")),
            value: Mutex::new(json!({"island": {"outline": true}})),
            blocked: false,
        };
        assert!(prefs.set(&json!({"island": {"outline": false}})).is_err());
        assert_eq!(prefs.get(), json!({"island": {"outline": true}}));
        let _ = fs::remove_file(&blocker);
    }

    #[test]
    fn a_new_object_in_a_patch_loses_its_nulls() {
        let mut v = json!({});
        merge(
            &mut v,
            &json!({"sessions": {"live": null, "buddies": false}}),
        );
        assert_eq!(v, json!({"sessions": {"buddies": false}}));
    }

    #[test]
    fn a_byte_order_mark_is_read_and_text_that_isnt_utf8_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("vors-prefs-bom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "\u{feff}{\"island\":{\"outline\":true}}").unwrap();
        assert_eq!(read(&path).unwrap(), json!({"island": {"outline": true}}));
        fs::write(&path, b"{\"island\":{\"name\":\"caf\xe9\"}}").unwrap();
        assert_eq!(read(&path).unwrap(), json!({}));
        assert!(path.with_extension("json.broken").exists());
        assert!(!path.exists(), "the next save can't overwrite it");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cant_be_read_or_moved_blocks_saving() {
        let prefs = Prefs {
            path: Some(PathBuf::from("unused")),
            value: Mutex::new(empty()),
            blocked: true,
        };
        assert!(prefs.set(&json!({"island": {"outline": true}})).is_err());
        assert_eq!(prefs.get(), json!({}));
    }
}
