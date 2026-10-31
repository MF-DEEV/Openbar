// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Owns the island's one global shortcut. Registration precedes replacement;
//! a refused candidate never removes the working key or reaches settings.json.

use serde_json::Value;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

pub const DEFAULT: &str = "Ctrl+Alt+Space";

#[derive(Default)]
pub struct Shared {
    state: Mutex<State>,
    id: AtomicU32,
    recording: AtomicBool,
}

#[derive(Default)]
struct State {
    held: Option<Shortcut>,
    recorder: Option<String>,
}

pub fn parse(raw: &str) -> Result<Shortcut, String> {
    let key: Shortcut = raw.parse().map_err(
        |_| "Unsupported shortcut. Use Ctrl or Alt with a letter, number or navigation key.",
    )?;
    let mods = key.mods;
    if !mods.intersects(Modifiers::CONTROL | Modifiers::ALT) {
        return Err("Include Ctrl or Alt with the key.".into());
    }
    // Windows owns Win chords and F12 (the debugger key). Never take over
    // task switching, window closing, the system menu or secure attention.
    if mods.contains(Modifiers::SUPER)
        || key.key == Code::F12
        || key.key == Code::Escape
        || (mods.contains(Modifiers::ALT)
            && matches!(key.key, Code::Tab | Code::F4 | Code::Space)
            && !mods.contains(Modifiers::CONTROL))
        || (mods.contains(Modifiers::CONTROL | Modifiers::ALT)
            && matches!(key.key, Code::Delete | Code::Tab))
    {
        return Err("This combination is reserved by Windows. Choose another shortcut.".into());
    }
    let code = key.key.to_string();
    let supported = (code.starts_with("Key") && code.len() == 4)
        || (code.starts_with("Digit") && code.len() == 6)
        || (code.starts_with("Numpad")
            && code.len() == 7
            && code.ends_with(|c: char| c.is_ascii_digit()))
        || matches!(
            key.key,
            Code::F1
                | Code::F2
                | Code::F3
                | Code::F4
                | Code::F5
                | Code::F6
                | Code::F7
                | Code::F8
                | Code::F9
                | Code::F10
                | Code::F11
                | Code::F13
                | Code::F14
                | Code::F15
                | Code::F16
                | Code::F17
                | Code::F18
                | Code::F19
                | Code::F20
                | Code::F21
                | Code::F22
                | Code::F23
                | Code::F24
                | Code::Backquote
                | Code::Backslash
                | Code::BracketLeft
                | Code::BracketRight
                | Code::Comma
                | Code::Equal
                | Code::Minus
                | Code::Period
                | Code::Quote
                | Code::Semicolon
                | Code::Slash
                | Code::Space
                | Code::Tab
                | Code::Enter
                | Code::Backspace
                | Code::Delete
                | Code::Insert
                | Code::Home
                | Code::End
                | Code::PageUp
                | Code::PageDown
                | Code::ArrowUp
                | Code::ArrowDown
                | Code::ArrowLeft
                | Code::ArrowRight
                | Code::Pause
                | Code::PrintScreen
        );
    if !supported {
        return Err("This key cannot be used as an island shortcut.".into());
    }
    if [Code::KeyY, Code::KeyN, Code::KeyT]
        .into_iter()
        .any(|code| key == Shortcut::new(Some(Modifiers::CONTROL), code))
    {
        return Err("This shortcut is used by Sessions approvals.".into());
    }
    Ok(key)
}

fn configured(value: &Value) -> Result<Option<Shortcut>, String> {
    let key = parse(
        value
            .pointer("/island/shortcut")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT),
    )?;
    if value
        .pointer("/island/soundOutputSwitcherEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && value
            .pointer("/island/soundOutputSwitcherShortcut")
            .and_then(Value::as_str)
            .unwrap_or("Ctrl+Alt+Shift+KeyS")
            .parse::<Shortcut>()
            .ok()
            == Some(key)
    {
        return Err("This shortcut is used by the sound output switcher.".into());
    }
    Ok(value
        .pointer("/island/shortcutEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
        .then_some(key))
}

trait Registry {
    fn register(&self, key: Shortcut) -> Result<(), String>;
    fn unregister(&self, key: Shortcut) -> Result<(), String>;
}

struct Plugin<'a, R: Runtime>(&'a AppHandle<R>);
impl<R: Runtime> Registry for Plugin<'_, R> {
    fn register(&self, key: Shortcut) -> Result<(), String> {
        let plugin = self.0.global_shortcut();
        if plugin.is_registered(key) {
            return Err("This shortcut is already used by another Vorssaint feature.".into());
        }
        plugin.register(key).map_err(|e| {
            format!("Windows couldn't register this shortcut. Another app may be using it. {e}")
        })
    }
    fn unregister(&self, key: Shortcut) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(key)
            .map_err(|e| format!("Couldn't release the previous shortcut: {e}"))
    }
}

impl State {
    fn save(
        &mut self,
        wanted: Option<Shortcut>,
        registry: &impl Registry,
        persist: impl FnOnce() -> Result<Value, String>,
    ) -> Result<Value, String> {
        let old = self.held;
        self.replace(wanted, registry)?;
        let result = persist();
        if let Err(error) = &result
            && let Err(rollback) = self.replace(old, registry)
        {
            return Err(format!(
                "{error}. Restoring the previous shortcut also failed: {rollback}"
            ));
        }
        result
    }

    fn replace(
        &mut self,
        wanted: Option<Shortcut>,
        registry: &impl Registry,
    ) -> Result<(), String> {
        if self.held == wanted {
            return Ok(());
        }
        if let Some(key) = wanted {
            registry.register(key)?;
        }
        if let Some(old) = self.held
            && let Err(error) = registry.unregister(old)
        {
            if let Some(key) = wanted {
                registry.unregister(key)?;
            }
            return Err(error);
        }
        self.held = wanted;
        Ok(())
    }
}

fn publish(shared: &Shared, state: &State) {
    shared
        .id
        .store(state.held.map_or(0, |k| k.id()), Ordering::SeqCst);
    shared
        .recording
        .store(state.recorder.is_some(), Ordering::SeqCst);
}

/// The handler never takes a lock: the plugin may wait for its main thread
/// while a preference command owns the state lock.
pub fn matches<R: Runtime>(app: &AppHandle<R>, key: &Shortcut) -> bool {
    let shared = app.state::<Shared>();
    !shared.recording.load(Ordering::SeqCst) && shared.id.load(Ordering::SeqCst) == key.id()
}

pub fn reserved<R: Runtime>(app: &AppHandle<R>, key: &Shortcut) -> bool {
    let value = app.state::<crate::prefs::Prefs>().get();
    parse(
        value
            .pointer("/island/shortcut")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT),
    )
    .ok()
    .as_ref()
        == Some(key)
}

/// Called only for changes to the shortcut preferences, under the prefs
/// transaction lock. Roll back registration if writing preferences fails.
pub fn save<R: Runtime>(
    app: &AppHandle<R>,
    value: &Value,
    persist: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    let wanted = configured(value)?;
    let shared = app.state::<Shared>();
    let mut state = shared.state.lock().unwrap();
    let result = state.save(wanted, &Plugin(app), persist);
    publish(&shared, &state);
    result
}

pub fn start<R: Runtime>(app: &AppHandle<R>, value: &Value) {
    let shared = app.state::<Shared>();
    let mut state = shared.state.lock().unwrap();
    // Invalid hand-edited preferences never register an unsafe key.
    if let Ok(wanted) = configured(value) {
        let _ = state.replace(wanted, &Plugin(app));
    }
    publish(&shared, &state);
}

pub fn recording<R: Runtime>(app: &AppHandle<R>, owner: &str, on: bool) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let mut state = shared.state.lock().unwrap();
    if on {
        if state
            .recorder
            .as_deref()
            .is_some_and(|label| label != owner)
        {
            return Err("Another window is recording a shortcut.".into());
        }
        state.replace(None, &Plugin(app))?;
        state.recorder = Some(owner.to_string());
    } else if state.recorder.as_deref() == Some(owner) {
        state.recorder = None;
        let value = app.state::<crate::prefs::Prefs>().get();
        let restored = configured(&value).and_then(|wanted| state.replace(wanted, &Plugin(app)));
        publish(&shared, &state);
        return restored;
    }
    publish(&shared, &state);
    Ok(())
}

pub fn recording_owner<R: Runtime>(app: &AppHandle<R>, owner: &str) -> bool {
    app.state::<Shared>()
        .state
        .lock()
        .unwrap()
        .recorder
        .as_deref()
        == Some(owner)
}

#[tauri::command]
pub fn shortcut_registered<R: Runtime>(app: AppHandle<R>) -> bool {
    let value = app.state::<crate::prefs::Prefs>().get();
    let shared = app.state::<Shared>();
    configured(&value).ok().flatten().is_some_and(|key| {
        !shared.recording.load(Ordering::SeqCst) && shared.id.load(Ordering::SeqCst) == key.id()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Fake {
        held: RefCell<Vec<Shortcut>>,
        refused: Option<Shortcut>,
        release_refused: Option<Shortcut>,
    }
    impl Registry for Fake {
        fn register(&self, key: Shortcut) -> Result<(), String> {
            if self.refused == Some(key) || self.held.borrow().contains(&key) {
                return Err("taken".into());
            }
            self.held.borrow_mut().push(key);
            Ok(())
        }
        fn unregister(&self, key: Shortcut) -> Result<(), String> {
            if self.release_refused == Some(key) {
                return Err("cannot release".into());
            }
            self.held.borrow_mut().retain(|k| *k != key);
            Ok(())
        }
    }
    #[test]
    fn a_failed_registration_never_persists_and_a_failed_save_restores_the_old_key() {
        let old = parse(DEFAULT).unwrap();
        let new = parse("Ctrl+Shift+KeyX").unwrap();
        let mut state = State::default();
        let registry = Fake {
            refused: Some(new),
            ..Fake::default()
        };
        state.replace(Some(old), &registry).unwrap();
        assert!(
            state
                .save(Some(new), &registry, || panic!(
                    "a refused key must not be persisted"
                ))
                .is_err()
        );
        let registry = Fake {
            held: RefCell::new(vec![old]),
            ..Fake::default()
        };
        assert!(
            state
                .save(Some(new), &registry, || Err("disk full".into()))
                .is_err()
        );
        assert_eq!(state.held, Some(old));
        assert_eq!(*registry.held.borrow(), vec![old]);
        let registry = Fake {
            held: RefCell::new(vec![old]),
            release_refused: Some(old),
            ..Fake::default()
        };
        assert!(
            state
                .save(Some(new), &registry, || panic!(
                    "failed cleanup must not be persisted"
                ))
                .is_err()
        );
        assert_eq!(*registry.held.borrow(), vec![old]);
    }
    #[test]
    fn rejects_unsafe_and_unsupported_keys() {
        for key in [
            "KeyX",
            "Shift+KeyX",
            "Ctrl+Alt+Delete",
            "Alt+Tab",
            "Alt+F4",
            "Alt+Space",
            "Ctrl+Escape",
            "Ctrl+Shift+Escape",
            "Super+KeyL",
            "Ctrl+F12",
            "Ctrl+KeyY",
            "Ctrl+AudioVolumeUp",
            "Ctrl+IntlBackslash",
        ] {
            assert!(parse(key).is_err(), "{key}");
        }
        for key in [
            DEFAULT,
            "Ctrl+Shift+KeyX",
            "Alt+F8",
            "Ctrl+Numpad7",
            "Ctrl+ArrowUp",
        ] {
            assert!(parse(key).is_ok(), "{key}");
        }
    }
    #[test]
    fn default_saved_disabled_and_switcher_conflicts() {
        use serde_json::json;
        assert_eq!(
            configured(&json!({})).unwrap(),
            Some(parse(DEFAULT).unwrap())
        );
        assert_eq!(
            configured(&json!({"island":{"shortcut":"Ctrl+Shift+KeyX"}})).unwrap(),
            Some(parse("Ctrl+Shift+KeyX").unwrap())
        );
        assert_eq!(
            configured(&json!({"island":{"shortcutEnabled":false}})).unwrap(),
            None
        );
        assert!(configured(&json!({"island":{"shortcut":"Ctrl+Alt+Shift+KeyS", "soundOutputSwitcherEnabled":true}})).is_err());
    }
    #[test]
    fn replacement_failure_keeps_old_and_success_leaves_one_registration() {
        let old = parse(DEFAULT).unwrap();
        let new = parse("Ctrl+Shift+KeyX").unwrap();
        let registry = Fake {
            refused: Some(new),
            ..Fake::default()
        };
        let mut state = State::default();
        state.replace(Some(old), &registry).unwrap();
        assert!(state.replace(Some(new), &registry).is_err());
        assert_eq!(state.held, Some(old));
        assert_eq!(*registry.held.borrow(), vec![old]);
        let registry = Fake {
            held: RefCell::new(vec![old]),
            ..Fake::default()
        };
        state.replace(Some(new), &registry).unwrap();
        state.replace(Some(new), &registry).unwrap();
        assert_eq!(*registry.held.borrow(), vec![new]);
        state.replace(None, &registry).unwrap();
        assert!(registry.held.borrow().is_empty());
        state.replace(Some(new), &registry).unwrap();
        assert_eq!(*registry.held.borrow(), vec![new]);
    }
}
