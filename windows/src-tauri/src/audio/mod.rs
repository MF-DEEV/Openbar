// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! System audio for Controls and the Mixer (spec 05 §1.3, §2.4): the default
//! output's and microphone's level and mute, the devices and which is
//! default, Mic Mute, and every app playing on any active output, grouped by
//! app. All of it runs on one MTA worker thread, which audio-session
//! notifications require; COM callbacks only queue work onto it. The whole
//! state goes to the island as `audio://state` whenever it changes.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod engine;
mod guard;
mod mic;
#[cfg(windows)]
mod policy;
mod priority;
mod roller;
mod switcher;
mod tree;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    /// Worn (form factor or name), for the headphone guard.
    #[serde(skip)]
    pub headphones: bool,
    /// Built in, plugged in or virtual, for where audio priority ranks it.
    #[serde(skip)]
    pub tier: Option<priority::Tier>,
}

/// One entry of a priority list, connected or remembered by its last name.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub id: String,
    pub name: String,
    pub available: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Priority {
    pub output_enabled: bool,
    pub input_enabled: bool,
    pub outputs: Vec<Ranked>,
    pub inputs: Vec<Ranked>,
}

/// A level and mute; None while unknown (no device, or it refused to say).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub volume: Option<f32>,
    pub muted: Option<bool>,
}

/// Every session of one app, as the mixer shows it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    /// The app's exe path in lower case (its persistence id), or "system".
    pub key: String,
    /// The exe whose icon the column shows.
    pub path: Option<String>,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
    pub system: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// False until the worker has read the devices once, or when it can't.
    pub supported: bool,
    pub output: Level,
    pub input: Level,
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    /// The default device can be switched (the IPolicyConfig probe passed).
    pub can_switch: bool,
    pub mic_muted: bool,
    pub apps: Vec<App>,
    pub priority: Priority,
    /// The output switcher is on but Windows refused its shortcut.
    pub switcher_refused: bool,
    /// The last switcher press found no chosen output that is connected.
    pub switch_failed: bool,
    /// Finer volume steps are on but the volume keys couldn't be hooked.
    pub roller_failed: bool,
}

/// The Mixer options the backend acts on itself (prefs `island.*`).
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub lower_on_disconnect: bool,
    pub lower_percent: u8,
    pub roller: bool,
    pub switcher: bool,
    pub shortcut: String,
    pub cycle: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            lower_on_disconnect: false,
            lower_percent: guard::DEFAULT_PERCENT,
            roller: false,
            switcher: false,
            shortcut: switcher::DEFAULT_SHORTCUT.into(),
            cycle: Vec::new(),
        }
    }
}

impl Options {
    fn from_prefs(value: &serde_json::Value) -> Self {
        let island = |key: &str| value.pointer(&format!("/island/{key}"));
        let flag = |key: &str| {
            island(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        let defaults = Options::default();
        Options {
            lower_on_disconnect: flag("mixerLowerVolumeOnHeadphonesDisconnect"),
            lower_percent: island("mixerHeadphonesDisconnectVolumePercent")
                .and_then(serde_json::Value::as_i64)
                .map_or(defaults.lower_percent, guard::sanitized_percent),
            roller: flag("preciseVolumeRollerEnabled"),
            switcher: flag("soundOutputSwitcherEnabled"),
            shortcut: island("soundOutputSwitcherShortcut")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map_or(defaults.shortcut, str::to_string),
            cycle: island("soundOutputSwitcherDevices")
                .and_then(serde_json::Value::as_array)
                .map(|ids| {
                    let ids: Vec<String> = ids
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                    priority::sanitized_list(&ids)
                })
                .unwrap_or_default(),
        }
    }
}

static OPTIONS: Mutex<Option<Options>> = Mutex::new(None);
static SWITCHER_REFUSED: AtomicBool = AtomicBool::new(false);
static ROLLER_FAILED: AtomicBool = AtomicBool::new(false);
/// The switcher shortcut held now, and whether a shortcut field is recording.
static SWITCHER_KEY: Mutex<Option<tauri_plugin_global_shortcut::Shortcut>> = Mutex::new(None);
static RECORDING: AtomicBool = AtomicBool::new(false);
/// The held shortcut's id (0 for none), for the shortcut handler.
static SWITCHER_ID: AtomicU32 = AtomicU32::new(0);

fn options() -> Options {
    OPTIONS.lock().unwrap().clone().unwrap_or_default()
}

/// One session's reading before grouping.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub key: String,
    pub path: Option<String>,
    pub name: String,
    pub system: bool,
    pub volume: f32,
    pub muted: bool,
    pub playing: bool,
}

/// One column per app: the loudest session's level, muted only when every
/// session is, playing when any is. Sorted by name, system sounds last.
pub fn group(readings: &[Reading]) -> Vec<App> {
    let mut apps: Vec<App> = Vec::new();
    for r in readings {
        match apps.iter_mut().find(|a| a.key == r.key) {
            Some(app) => {
                app.volume = app.volume.max(r.volume);
                app.muted &= r.muted;
                app.playing |= r.playing;
                if app.path.is_none() {
                    app.path = r.path.clone();
                }
            }
            None => apps.push(App {
                key: r.key.clone(),
                path: r.path.clone(),
                name: r.name.clone(),
                volume: r.volume,
                muted: r.muted,
                playing: r.playing,
                system: r.system,
            }),
        }
    }
    apps.sort_by(|a, b| {
        a.system
            .cmp(&b.system)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.key.cmp(&b.key))
    });
    apps
}

/// A volume from the webview: finite and within 0…1.
fn checked(volume: Option<f32>) -> Result<Option<f32>, String> {
    match volume {
        Some(v) if !v.is_finite() || !(0.0..=1.0).contains(&v) => {
            Err("Invalid volume level".into())
        }
        other => Ok(other),
    }
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);

static APP: OnceLock<(AppHandle, Shared)> = OnceLock::new();

/// Stores `next` and tells the island, only when something changed.
fn publish(next: State) {
    let Some((app, shared)) = APP.get() else {
        return;
    };
    let changed = {
        let mut current = shared.0.lock().unwrap();
        if *current == next {
            false
        } else {
            *current = next.clone();
            true
        }
    };
    if changed {
        // Settings shows the Mixer options too.
        for label in [crate::island::LABEL, crate::settings::LABEL] {
            let _ = app.emit_to(label, "audio://state", &next);
        }
    }
}

/// The output level changed from outside the island (keys, the system
/// flyout, another app): the island shows its volume notice.
fn level_from_outside(volume: f32, muted: bool) {
    if let Some((app, _)) = APP.get() {
        #[derive(serde::Serialize, Clone)]
        struct Payload {
            volume: f32,
            muted: bool,
        }
        let _ = app.emit_to(
            crate::island::LABEL,
            "audio://volume-changed",
            Payload { volume, muted },
        );
    }
}

/// A file of this module's own beside the preferences (Mic Mute's set).
fn saved_file(name: &str) -> Option<std::path::PathBuf> {
    let (app, _) = APP.get()?;
    app.path().app_config_dir().ok().map(|dir| dir.join(name))
}

pub fn start(app: AppHandle, shared: Shared) {
    if APP.set((app, shared)).is_err() {
        return;
    }
    #[cfg(windows)]
    engine::start();
}

/// The Mixer options from the preferences, at launch and on every change:
/// the headphone guard reads them on the next device change, the volume
/// keys are hooked or let go, and the switcher's shortcut is taken or freed.
pub fn apply_prefs<R: Runtime>(app: &AppHandle<R>, value: &serde_json::Value) {
    let next = Options::from_prefs(value);
    let before = OPTIONS.lock().unwrap().replace(next.clone());
    if before.as_ref().is_some_and(|b| *b == next) {
        return;
    }
    if before.as_ref().is_none_or(|b| b.roller != next.roller) {
        #[cfg(windows)]
        ROLLER_FAILED.store(roller::set_enabled(next.roller).is_err(), Ordering::SeqCst);
    }
    if before
        .as_ref()
        .is_none_or(|b| b.switcher != next.switcher || b.shortcut != next.shortcut)
    {
        sync_switcher(app);
    }
    #[cfg(windows)]
    {
        let cycle_changed = before.as_ref().is_some_and(|b| b.cycle != next.cycle);
        engine::post(move |e| {
            if cycle_changed {
                e.clear_switch_failure();
            }
        });
    }
}

/// Takes the switcher's shortcut while it is on and no field is recording.
/// A preference change and the end of a recording can arrive together, so
/// the whole decision is made under the lock from the newest options and
/// recording state: whichever call runs last leaves the right shortcut.
fn sync_switcher<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let shortcuts = app.global_shortcut();
    let mut held = SWITCHER_KEY.lock().unwrap();
    let options = options();
    let recording = RECORDING.load(Ordering::SeqCst);
    let wanted = (options.switcher && !recording)
        .then(|| options.shortcut.parse::<Shortcut>().ok())
        .flatten();
    if held.is_some() && *held == wanted {
        return;
    }
    if let Some(old) = held.take() {
        SWITCHER_ID.store(0, Ordering::SeqCst);
        let _ = shortcuts.unregister(old);
    }
    // The island's own toggle is never given away.
    let refused = match wanted {
        Some(key) if key == crate::toggle_shortcut() => true,
        Some(key) => match shortcuts.register(key) {
            Ok(()) => {
                *held = Some(key);
                SWITCHER_ID.store(key.id(), Ordering::SeqCst);
                false
            }
            Err(_) => true,
        },
        // On but unreadable; while a field records, nothing is refused.
        None => options.switcher && !recording,
    };
    SWITCHER_REFUSED.store(refused, Ordering::SeqCst);
}

/// The global shortcut handler's question: was this the switcher's key? It
/// runs on the main thread, which a registration holding SWITCHER_KEY may be
/// waiting for, so it reads the id alone.
pub fn is_switcher_shortcut(shortcut: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    let id = SWITCHER_ID.load(Ordering::SeqCst);
    id != 0 && id == shortcut.id()
}

/// The switcher's shortcut was pressed: the next chosen output plays.
pub fn switch_to_next_output() {
    #[cfg(windows)]
    engine::post(|e| e.switch_to_next(&options().cycle));
}

/// A finer volume key press (roller.rs), applied on the audio thread.
#[cfg(windows)]
pub(crate) fn roller_step(direction: roller::Direction) {
    engine::post(move |e| e.step_output(direction));
}

/// Lets go of the switcher's shortcut while a shortcut field records, so the
/// very combination it uses can be typed (SoundOutputSwitcher.suspendShortcut).
#[tauri::command]
pub fn audio_switcher_recording<R: Runtime>(app: AppHandle<R>, on: bool) {
    RECORDING.store(on, Ordering::SeqCst);
    sync_switcher(&app);
    #[cfg(windows)]
    engine::post(|_| {});
}

/// Audio priority: turns a list's automatic switching on or off, or saves
/// its new order (`output` picks the outputs' list, else the microphones').
#[tauri::command]
pub async fn audio_priority_set(
    output: bool,
    enabled: Option<bool>,
    ids: Option<Vec<String>>,
) -> Result<(), String> {
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| {
            e.set_priority(output, enabled, ids);
            Ok(())
        });
        #[cfg(not(windows))]
        {
            let _ = (output, enabled, ids);
            unavailable()
        }
    })
    .await
}

#[cfg(windows)]
use engine::with_engine;

#[cfg(not(windows))]
fn unavailable<T>() -> Result<T, String> {
    Err("System audio is available on Windows".into())
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}

/// A player's volume for the Music page (media_volume.rs): the loudest of
/// the sessions whose own exe `matches`, set first when `level` is given.
pub fn player_volume(
    matches: fn(&str, &str) -> bool,
    source: String,
    level: Option<f32>,
) -> Result<f32, String> {
    let level = checked(level)?;
    #[cfg(windows)]
    return with_engine(move |e| e.player_volume(&|exe| matches(&source, exe), level));
    #[cfg(not(windows))]
    {
        let _ = (matches, source, level);
        unavailable()
    }
}

/// The state once the audio thread has read it: an answer of "unsupported"
/// means it really failed, not that it hasn't started yet.
#[tauri::command]
pub async fn audio_state(state: tauri::State<'_, Shared>) -> Result<State, String> {
    #[cfg(windows)]
    let _ = blocking(|| with_engine(|_| Ok(()))).await;
    Ok(state.0.lock().unwrap().clone())
}

#[tauri::command]
pub async fn audio_set_output(volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    let volume = checked(volume)?;
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| e.set_level(false, volume, muted));
        #[cfg(not(windows))]
        {
            let _ = (volume, muted);
            unavailable()
        }
    })
    .await
}

#[tauri::command]
pub async fn audio_set_input(volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    let volume = checked(volume)?;
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| e.set_level(true, volume, muted));
        #[cfg(not(windows))]
        {
            let _ = (volume, muted);
            unavailable()
        }
    })
    .await
}

/// Makes `id` the default device for every role (console, multimedia, communications).
#[tauri::command]
pub async fn audio_set_default(id: String) -> Result<(), String> {
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| e.set_default(&id));
        #[cfg(not(windows))]
        {
            let _ = id;
            unavailable()
        }
    })
    .await
}

#[tauri::command]
pub async fn audio_set_app(
    key: String,
    volume: Option<f32>,
    muted: Option<bool>,
) -> Result<(), String> {
    let volume = checked(volume)?;
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| e.set_app(&key, volume, muted));
        #[cfg(not(windows))]
        {
            let _ = (key, volume, muted);
            unavailable()
        }
    })
    .await
}

/// While the mixer page shows, its apps are read four times a second with
/// their peak meters, for the playing dot.
#[tauri::command]
pub async fn audio_watch_apps(on: bool) -> Result<(), String> {
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| {
            e.watch(on);
            Ok(())
        });
        #[cfg(not(windows))]
        {
            let _ = on;
            unavailable()
        }
    })
    .await
}

/// Mic Mute: `muted` sets it, None toggles. Returns the new state.
#[tauri::command]
pub async fn mic_mute_set(muted: Option<bool>) -> Result<bool, String> {
    blocking(move || {
        #[cfg(windows)]
        return with_engine(move |e| e.set_mic_mute(muted));
        #[cfg(not(windows))]
        {
            let _ = muted;
            unavailable()
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(key: &str, name: &str, volume: f32, muted: bool, playing: bool) -> Reading {
        Reading {
            key: key.into(),
            path: Some(key.into()),
            name: name.into(),
            system: key == "system",
            volume,
            muted,
            playing,
        }
    }

    #[test]
    fn sessions_of_one_app_share_a_column() {
        let apps = group(&[
            reading("c:\\chrome.exe", "Google Chrome", 0.4, true, false),
            reading("system", "System sounds", 1.0, false, false),
            reading("c:\\chrome.exe", "Google Chrome", 0.8, false, true),
            reading("c:\\spotify.exe", "Spotify", 1.0, true, false),
        ]);
        assert_eq!(
            apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["Google Chrome", "Spotify", "System sounds"]
        );
        let chrome = &apps[0];
        assert_eq!(chrome.volume, 0.8);
        assert!(!chrome.muted && chrome.playing);
        assert!(apps[1].muted && !apps[1].playing);
        assert!(apps[2].system);
    }

    #[test]
    fn rejects_levels_outside_the_range() {
        for v in [f32::NAN, f32::INFINITY, -0.1, 1.01] {
            assert!(checked(Some(v)).is_err());
        }
        assert_eq!(checked(Some(0.5)), Ok(Some(0.5)));
        assert_eq!(checked(None), Ok(None));
    }

    #[test]
    fn the_state_serializes_for_the_island() {
        let state = State {
            can_switch: true,
            mic_muted: true,
            ..Default::default()
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["canSwitch"], true);
        assert_eq!(json["micMuted"], true);
        assert!(json["output"]["volume"].is_null());
    }

    #[test]
    fn the_mixer_options_state_serializes_for_the_island() {
        let state = State {
            outputs: vec![Device {
                id: "hp".into(),
                name: "Headphones".into(),
                is_default: true,
                headphones: true,
                tier: Some(priority::Tier::Hardware),
            }],
            priority: Priority {
                output_enabled: true,
                outputs: vec![Ranked {
                    id: "hp".into(),
                    name: "Headphones".into(),
                    available: false,
                }],
                ..Default::default()
            },
            switcher_refused: true,
            ..Default::default()
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["priority"]["outputEnabled"], true);
        assert_eq!(json["priority"]["inputEnabled"], false);
        assert_eq!(json["priority"]["outputs"][0]["available"], false);
        assert_eq!(json["switcherRefused"], true);
        assert_eq!(json["switchFailed"], false);
        assert_eq!(json["rollerFailed"], false);
        // What only the backend uses stays out of the page's state.
        assert!(json["outputs"][0].get("headphones").is_none());
        assert!(json["outputs"][0].get("tier").is_none());
    }

    #[test]
    fn mixer_options_read_the_island_prefs() {
        assert_eq!(
            Options::from_prefs(&serde_json::json!({})),
            Options::default()
        );
        let options = Options::from_prefs(&serde_json::json!({ "island": {
            "mixerLowerVolumeOnHeadphonesDisconnect": true,
            "mixerHeadphonesDisconnectVolumePercent": 140,
            "preciseVolumeRollerEnabled": true,
            "soundOutputSwitcherEnabled": true,
            "soundOutputSwitcherShortcut": "Ctrl+Alt+KeyO",
            "soundOutputSwitcherDevices": ["a", " ", "a", 3, "b"],
        }}));
        assert!(options.lower_on_disconnect && options.roller && options.switcher);
        assert_eq!(options.lower_percent, 100);
        assert_eq!(options.shortcut, "Ctrl+Alt+KeyO");
        assert_eq!(options.cycle, ["a", "b"]);
    }

    #[test]
    fn unreadable_mixer_options_fall_back_to_the_defaults() {
        let options = Options::from_prefs(&serde_json::json!({ "island": {
            "mixerLowerVolumeOnHeadphonesDisconnect": "yes",
            "mixerHeadphonesDisconnectVolumePercent": "low",
            "soundOutputSwitcherShortcut": "  ",
            "soundOutputSwitcherDevices": "a",
        }}));
        assert_eq!(options, Options::default());
        let negative = Options::from_prefs(&serde_json::json!({ "island": {
            "mixerHeadphonesDisconnectVolumePercent": -10,
        }}));
        assert_eq!(negative.lower_percent, 0);
    }
}
