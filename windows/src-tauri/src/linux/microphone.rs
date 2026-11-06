// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Who is recording on Linux: an app with an open recording stream on a
//! microphone, as the sound server lists it. `microphone.rs` includes this
//! file as `imp`, since it keeps its snapshot private.
//!
//! Like the Windows reader this never records: it reads the server's lists
//! when the server says a recording started or stopped, so `level` (a peak
//! meter, which needs a stream of its own) stays 0, and Discord's own mute
//! (read from its window on Windows) is not known.

use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::{Shared, Snapshot};
use crate::linux::pulse::{self, Event, Node, Pactl, Pulse, Stream};

/// A recording by an app, from a microphone: not one that is paused, one of
/// a level meter (pavucontrol's "Peak detect"), or one reading the monitor
/// of an output (a screen recorder, the music levels), whose source is not
/// among the microphones.
fn recording_from<'a>(
    recordings: &'a [Stream],
    microphones: &'a [Node],
) -> Vec<(&'a Stream, &'a Node)> {
    recordings
        .iter()
        .filter(|r| !r.corked && r.media != "Peak detect")
        .filter_map(|r| {
            microphones
                .iter()
                .find(|m| m.index == r.source)
                .map(|m| (r, m))
        })
        .collect()
}

/// Whether something records, and whether every microphone it records from is muted.
fn snapshot_of(recordings: &[Stream], microphones: &[Node]) -> Snapshot {
    let active = recording_from(recordings, microphones);
    Snapshot {
        recording: !active.is_empty(),
        system_muted: !active.is_empty() && active.iter().all(|(r, m)| m.muted || r.muted),
        level: 0.0,
        discord: None,
    }
}

fn read(pulse: &dyn Pulse) -> Snapshot {
    match (pulse.recordings(), pulse.inputs()) {
        (Some(recordings), Some(microphones)) => snapshot_of(&recordings, &microphones),
        _ => Snapshot::default(),
    }
}

/// What the probe writes: the snapshot as of now.
pub fn read_now() -> Snapshot {
    read(&Pactl::default())
}

pub fn start(app: AppHandle, shared: Shared) {
    // The server reports a recording, or a microphone muted or gone.
    let (tx, rx) = mpsc::channel::<()>();
    let tx = Mutex::new(tx);
    pulse::subscribe(move |event| {
        if matches!(event, Event::Recordings | Event::Devices) {
            let _ = tx.lock().unwrap().send(());
        }
    });
    let _ = std::thread::Builder::new()
        .name("openbar-microphone".into())
        .spawn(move || {
            let pactl = Pactl::default();
            loop {
                let prefs = app.state::<crate::prefs::Prefs>().get();
                let enabled = prefs
                    .pointer("/island/microphoneStatus")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                    && prefs
                        .pointer("/island/enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                let next = if enabled {
                    read(&pactl)
                } else {
                    Snapshot::default()
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
                    let _ = app.emit_to(crate::island::LABEL, "microphone://snapshot", &next);
                }
                // Until the server says something changed, and for a while after,
                // since a recording opens with a few events in a row.
                if rx.recv_timeout(Duration::from_secs(5)).is_ok() {
                    std::thread::sleep(Duration::from_millis(150));
                    while rx.try_recv().is_ok() {}
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mic(index: u32, muted: bool) -> Node {
        Node {
            index,
            id: format!("mic{index}"),
            muted,
            ..Default::default()
        }
    }

    fn recording(source: u32) -> Stream {
        Stream {
            index: 100 + source,
            app: "Discord".into(),
            source,
            ..Default::default()
        }
    }

    #[test]
    fn an_app_recording_from_a_microphone_is_recording() {
        let mics = [mic(5, false), mic(6, false)];
        let snapshot = snapshot_of(&[recording(5)], &mics);
        assert!(snapshot.recording && !snapshot.system_muted);
        assert_eq!(snapshot_of(&[], &mics), Snapshot::default());
    }

    #[test]
    fn it_is_muted_only_when_every_microphone_in_use_is() {
        let mics = [mic(5, true), mic(6, false)];
        assert!(snapshot_of(&[recording(5)], &mics).system_muted);
        let both = snapshot_of(&[recording(5), recording(6)], &mics);
        assert!(both.recording && !both.system_muted);
        // A mute of the stream itself counts too.
        let mut muted = recording(6);
        muted.muted = true;
        assert!(snapshot_of(&[muted], &mics).system_muted);
    }

    #[test]
    fn monitors_meters_and_paused_streams_are_not_recording() {
        let mics = [mic(5, false)];
        // Source 9 is the monitor of an output: not among the microphones.
        assert!(!snapshot_of(&[recording(9)], &mics).recording);
        let mut meter = recording(5);
        meter.media = "Peak detect".into();
        assert!(!snapshot_of(&[meter], &mics).recording);
        let mut paused = recording(5);
        paused.corked = true;
        assert!(!snapshot_of(&[paused], &mics).recording);
    }

    #[test]
    #[ignore = "reads this desktop's sound server"]
    fn what_this_desktop_records_now() {
        println!("recording now: {:?}", read_now());
    }
}
