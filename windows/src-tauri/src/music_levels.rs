// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Demand-driven music spectrum, following the existing SMTC selection.
//! The frontend renews a lease so a destroyed/reloaded webview cannot leave
//! audio capture running. Only normalized levels are emitted, at most 30 Hz.
#[cfg(windows)]
mod capture;
#[cfg(any(windows, test))]
mod spectrum;

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Demand(Mutex<Option<Instant>>);
impl Demand {
    fn watch(&self, on: bool) {
        *self.0.lock().unwrap() = on.then(|| Instant::now() + Duration::from_secs(15));
    }
    #[cfg(any(windows, test))]
    fn active(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .is_some_and(|until| until > Instant::now())
    }
}

#[tauri::command]
pub fn music_watch_levels(on: bool, demand: tauri::State<'_, Demand>) {
    demand.watch(on);
}

#[cfg(any(windows, test))]
fn playing_source(state: &Option<crate::media::NowPlaying>) -> Option<&str> {
    state
        .as_ref()?
        .playback
        .as_ref()
        .filter(|playback| playback.is_playing)
        .map(|playback| playback.source.as_str())
}

#[cfg(windows)]
pub fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>, media: crate::media::Shared) {
    use capture::Capture;
    use spectrum::{BANDS, Smoother};
    use tauri::{Emitter, Manager};

    #[derive(Clone, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Frame<'a> {
        source: &'a str,
        mode: &'static str,
        levels: [f32; BANDS],
    }

    let _ = std::thread::Builder::new()
        .name("music-spectrum".into())
        .spawn(move || {
            let apartment = crate::com::Apartment::mta();
            let mut identity: Option<String> = None;
            let mut streams: Vec<Capture> = Vec::new();
            let mut pids = Vec::new();
            let mut endpoint = String::new();
            let mut mode = "unavailable";
            let mut smoother = Smoother::default();
            let mut inspect_at = Instant::now();
            let mut process_retry = Instant::now();
            let mut emit_at = Instant::now();
            let mut last_sent: Option<([f32; BANDS], &'static str)> = None;
            let mut sent_at = Instant::now();
            loop {
                let change = {
                    let state = media.lock().unwrap();
                    let desired = if app.state::<Demand>().active() {
                        playing_source(&state.now)
                    } else {
                        None
                    };
                    let previous = identity.as_deref();
                    // Allocate a source ID only on a real selection/playback change.
                    (desired != previous).then(|| desired.map(str::to_owned))
                };
                if let Some(desired) = change {
                    streams.clear();
                    pids.clear();
                    endpoint.clear();
                    if let Some(source) = &identity {
                        let _ = app.emit(
                            "music://levels",
                            Frame {
                                source,
                                mode: "inactive",
                                levels: [0.; BANDS],
                            },
                        );
                    }
                    identity = desired;
                    smoother = Smoother::default();
                    mode = "unavailable";
                    last_sent = None;
                    inspect_at = Instant::now();
                    process_retry = Instant::now();
                }
                let Some(source) = identity.as_ref() else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                let now = Instant::now();
                if now >= inspect_at && apartment.is_ok() {
                    inspect_at = now + Duration::from_secs(1);
                    let next_pids =
                        crate::audio::player_processes(source.clone()).unwrap_or_default();
                    let device = capture::default_device();
                    let processes_changed = pids != next_pids;
                    let changed = (mode == "process" && processes_changed)
                        || (mode == "system"
                            && device.as_ref().map(|(_, id)| id.as_str()).ok()
                                != Some(endpoint.as_str()));
                    if changed {
                        streams.clear();
                        smoother = Smoother::default();
                        process_retry = now;
                    }
                    if processes_changed {
                        process_retry = now;
                    }
                    if streams.is_empty() || (mode != "process" && now >= process_retry) {
                        // Process loopback is runtime-probed; older Windows versions
                        // reject activation and use render-device loopback instead.
                        let isolated: windows::core::Result<Vec<_>> = if next_pids.is_empty() {
                            Err(windows::core::Error::empty())
                        } else {
                            next_pids.iter().map(|&pid| Capture::process(pid)).collect()
                        };
                        process_retry = now + Duration::from_secs(30);
                        if let Ok(captures) = isolated {
                            streams = captures;
                            mode = "process";
                            smoother = Smoother::default();
                        } else if streams.is_empty() {
                            mode = "unavailable";
                            if let Ok((device, id)) = device {
                                if let Ok(capture) = Capture::system(&device) {
                                    streams.push(capture);
                                    endpoint = id;
                                    mode = "system";
                                }
                            }
                        }
                        pids = next_pids;
                    }
                }
                let mut failed = false;
                for stream in &mut streams {
                    if stream.drain().is_err() {
                        failed = true;
                        break;
                    }
                }
                if failed {
                    streams.clear();
                    mode = "unavailable";
                    inspect_at = Instant::now() + Duration::from_secs(2);
                }
                if Instant::now() >= emit_at {
                    let mut levels = [0f32; BANDS];
                    for stream in &streams {
                        for (level, next) in levels.iter_mut().zip(stream.levels()) {
                            *level = level.max(next);
                        }
                    }
                    let levels = smoother.next(levels);
                    if last_sent != Some((levels, mode))
                        || sent_at.elapsed() >= Duration::from_millis(200)
                    {
                        let _ = app.emit(
                            "music://levels",
                            Frame {
                                source,
                                mode,
                                levels,
                            },
                        );
                        last_sent = Some((levels, mode));
                        sent_at = Instant::now();
                    }
                    emit_at = Instant::now() + Duration::from_secs_f64(1. / 30.);
                }
                std::thread::sleep(Duration::from_millis(if streams.is_empty() {
                    100
                } else {
                    8
                }));
            }
        });
}

#[cfg(not(windows))]
pub fn start<R: tauri::Runtime>(_app: tauri::AppHandle<R>, _media: crate::media::Shared) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_follows_the_media_source_and_stops_for_paused_or_missing_playback() {
        use crate::media::{NowPlaying, Playback};
        let mut now = Some(NowPlaying {
            playback: Some(Playback {
                source: "Spotify.exe".into(),
                is_playing: true,
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(playing_source(&now), Some("Spotify.exe"));
        now.as_mut().unwrap().playback.as_mut().unwrap().source = "other.exe".into();
        assert_eq!(playing_source(&now), Some("other.exe"));
        now.as_mut().unwrap().playback.as_mut().unwrap().is_playing = false;
        assert_eq!(playing_source(&now), None);
        now.as_mut().unwrap().playback = None;
        assert_eq!(playing_source(&now), None);
        assert_eq!(playing_source(&None), None);
    }
    #[test]
    fn monitoring_needs_a_live_lease() {
        let demand = Demand::default();
        assert!(!demand.active());
        demand.watch(true);
        assert!(demand.active());
        demand.watch(false);
        assert!(!demand.active());
        *demand.0.lock().unwrap() = Some(Instant::now() - Duration::from_secs(1));
        assert!(!demand.active());
    }
}
