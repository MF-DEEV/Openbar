// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! System audio for Controls and the Mixer (spec 05 §1.3, §2.4): the default
//! output's and microphone's level and mute, the devices and which is
//! default, Mic Mute, and every app playing on any active output, grouped by
//! app. All of it runs on one MTA worker thread, which audio-session
//! notifications require; COM callbacks only queue work onto it. The whole
//! state goes to the island as `audio://state` whenever it changes.

#![cfg_attr(not(windows), allow(dead_code))]

mod mic;
mod tree;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod policy;

use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
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
        Some(v) if !v.is_finite() || !(0.0..=1.0).contains(&v) => Err("Invalid volume level".into()),
        other => Ok(other),
    }
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);

static APP: OnceLock<(AppHandle, Shared)> = OnceLock::new();

/// Stores `next` and tells the island, only when something changed.
fn publish(next: State) {
    let Some((app, shared)) = APP.get() else { return };
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
        let _ = app.emit_to(crate::island::LABEL, "audio://state", &next);
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
        let _ = app.emit_to(crate::island::LABEL, "audio://volume-changed", Payload { volume, muted });
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

#[cfg(windows)]
use engine::with_engine;

#[cfg(not(windows))]
fn unavailable<T>() -> Result<T, String> {
    Err("System audio is available on Windows".into())
}

async fn blocking<T: Send + 'static>(job: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job).await.map_err(|e| e.to_string())?
}

/// A player's volume for the Music page (media_volume.rs): the loudest of
/// the sessions whose own exe `matches`, set first when `level` is given.
pub fn player_volume(matches: fn(&str, &str) -> bool, source: String, level: Option<f32>) -> Result<f32, String> {
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
pub async fn audio_set_app(key: String, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
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
        assert_eq!(apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Google Chrome", "Spotify", "System sounds"]);
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
        let state = State { can_switch: true, mic_muted: true, ..Default::default() };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["canSwitch"], true);
        assert_eq!(json["micMuted"], true);
        assert!(json["output"]["volume"].is_null());
    }
}
