// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The music spectrum on Linux, where Windows records a process's loopback:
//! `parec` records the stream the player plays (the sound server can monitor
//! one stream of an output) and the same analysis turns it into bars.
//! `music_levels.rs` includes this file as `imp`, since it keeps its lease
//! and helpers private.
//!
//! Like Windows it follows the player the island shows, only while the page
//! renews its lease, and never keeps the audio: only seven levels leave this
//! thread. A player whose streams cannot be found (or told apart) is heard
//! through the monitor of the default output instead.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use super::spectrum::{BANDS, Smoother, Spectrum};
use super::{Demand, playing_source};

const RATE: u32 = 48_000;
/// What `parec` is asked for: 16-bit stereo, which is what it can always give.
const FRAME_BYTES: usize = 4;

/// Mono samples from 16-bit stereo, `carry` holding what is left of a frame
/// when a read ends in the middle of one.
fn mono(bytes: &[u8], carry: &mut Vec<u8>) -> Vec<f32> {
    carry.extend_from_slice(bytes);
    let whole = carry.len() / FRAME_BYTES * FRAME_BYTES;
    let samples = carry[..whole]
        .as_chunks::<FRAME_BYTES>()
        .0
        .iter()
        .map(|frame| {
            let left = f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32768.;
            let right = f32::from(i16::from_le_bytes([frame[2], frame[3]])) / 32768.;
            (left + right) / 2.
        })
        .collect();
    carry.drain(..whole);
    samples
}

struct Heard {
    spectrum: Mutex<Spectrum>,
    last: Mutex<Instant>,
    ended: AtomicBool,
}

/// One recording of what a stream, or an output, plays.
pub struct Capture {
    heard: Arc<Heard>,
    child: Option<Child>,
}

impl Capture {
    /// Reads `source` on its own thread until it ends.
    fn from_reader(mut source: impl Read + Send + 'static, child: Option<Child>) -> Self {
        let heard = Arc::new(Heard {
            spectrum: Mutex::new(Spectrum::new(RATE)),
            last: Mutex::new(Instant::now()),
            ended: AtomicBool::new(false),
        });
        let reading = heard.clone();
        let _ = std::thread::Builder::new()
            .name("music-capture".into())
            .spawn(move || {
                let mut buffer = [0u8; 4096];
                let mut carry = Vec::new();
                while let Ok(count) = source.read(&mut buffer) {
                    if count == 0 {
                        break;
                    }
                    let samples = mono(&buffer[..count], &mut carry);
                    let mut spectrum = reading.spectrum.lock().unwrap();
                    for sample in samples {
                        spectrum.push(sample);
                    }
                    *reading.last.lock().unwrap() = Instant::now();
                }
                reading.ended.store(true, Ordering::SeqCst);
            });
        Capture { heard, child }
    }

    /// `parec` on `source` (the monitor of an output), narrowed to one stream.
    fn record(source: &str, stream: Option<u32>) -> Option<Self> {
        let mut command = Command::new("parec");
        command
            .args([
                "--raw",
                "--format=s16le",
                "--channels=2",
                "--latency-msec=20",
                "--client-name=Openbar levels",
                "--stream-name=music levels",
            ])
            .arg(format!("--rate={RATE}"))
            .arg(format!("--device={source}"));
        if let Some(stream) = stream {
            command.arg(format!("--monitor-stream={stream}"));
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let out = child.stdout.take()?;
        Some(Self::from_reader(out, Some(child)))
    }

    pub fn stream(index: u32, monitor: &str) -> Option<Self> {
        Self::record(monitor, Some(index))
    }

    pub fn system() -> Option<Self> {
        Self::record("@DEFAULT_MONITOR@", None)
    }

    /// Fails once the recording has ended (the server went, or `parec` did).
    pub fn drain(&mut self) -> Result<(), ()> {
        if self.heard.ended.load(Ordering::SeqCst) {
            return Err(());
        }
        // Some servers send nothing on silence. Never reuse old audio.
        if self.heard.last.lock().unwrap().elapsed() > Duration::from_millis(100) {
            self.heard.spectrum.lock().unwrap().clear();
        }
        Ok(())
    }

    pub fn levels(&self) -> [f32; BANDS] {
        self.heard.spectrum.lock().unwrap().levels()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Mode {
    Unavailable,
    Process,
    System,
}

impl Mode {
    fn name(&self) -> &'static str {
        match self {
            Mode::Unavailable => "unavailable",
            Mode::Process => "process",
            Mode::System => "system",
        }
    }
}

/// What to record for a player whose streams are `found`, given what is
/// recorded now: one capture per stream, or the default output's monitor
/// when none was found.
fn wanted(found: &[(u32, String)]) -> Mode {
    if found.is_empty() {
        Mode::System
    } else {
        Mode::Process
    }
}

pub fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>, media: crate::media::Shared) {
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
            let mut identity: Option<String> = None;
            let mut streams: Vec<Capture> = Vec::new();
            let mut indexes: Vec<u32> = Vec::new();
            let mut mode = Mode::Unavailable;
            let mut smoother = Smoother::default();
            let mut inspect_at = Instant::now();
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
                    indexes.clear();
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
                    mode = Mode::Unavailable;
                    last_sent = None;
                    inspect_at = Instant::now();
                }
                let Some(source) = identity.as_ref() else {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                };
                let now = Instant::now();
                if now >= inspect_at {
                    inspect_at = now + Duration::from_secs(1);
                    let found = crate::audio::player_streams(
                        crate::media_volume::matches_player,
                        source.clone(),
                    )
                    .unwrap_or_default();
                    let next: Vec<u32> = found.iter().map(|(index, _)| *index).collect();
                    let target = wanted(&found);
                    // A new set of streams, or streams where there were none,
                    // or none where there were: record again.
                    if streams.is_empty()
                        || mode != target
                        || (mode == Mode::Process && indexes != next)
                    {
                        streams.clear();
                        smoother = Smoother::default();
                        indexes = next;
                        mode = Mode::Unavailable;
                        match target {
                            Mode::Process => {
                                let captures: Vec<Capture> = found
                                    .iter()
                                    .filter_map(|(index, monitor)| Capture::stream(*index, monitor))
                                    .collect();
                                if captures.len() == found.len() {
                                    streams = captures;
                                    mode = Mode::Process;
                                }
                            }
                            _ => {
                                if let Some(capture) = Capture::system() {
                                    streams.push(capture);
                                    mode = Mode::System;
                                }
                            }
                        }
                        if streams.is_empty() {
                            // Nothing could be recorded; try again in a moment.
                            inspect_at = now + Duration::from_secs(2);
                        }
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
                    mode = Mode::Unavailable;
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
                    let name = mode.name();
                    if last_sent != Some((levels, name))
                        || sent_at.elapsed() >= Duration::from_millis(200)
                    {
                        let _ = app.emit(
                            "music://levels",
                            Frame {
                                source,
                                mode: name,
                                levels,
                            },
                        );
                        last_sent = Some((levels, name));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 16-bit stereo of `count` frames of a sine at `hz`, as `parec` writes it.
    fn tone(hz: f32, count: usize, amplitude: f32) -> Vec<u8> {
        (0..count)
            .flat_map(|n| {
                let value = (amplitude
                    * 32767.
                    * (std::f32::consts::TAU * hz * n as f32 / RATE as f32).sin())
                    as i16;
                let [a, b] = value.to_le_bytes();
                [a, b, a, b]
            })
            .collect()
    }

    #[test]
    fn stereo_frames_become_mono_and_a_split_frame_waits_for_its_rest() {
        let mut carry = Vec::new();
        // Left full, right zero: half. Then 3 bytes: one whole frame and a piece of the next.
        let loud = [0xFF, 0x7F, 0x00, 0x00];
        let first = mono(
            &[loud[0], loud[1], loud[2], loud[3], 0x00, 0x80, 0x00],
            &mut carry,
        );
        assert_eq!(first.len(), 1);
        assert!((first[0] - 0.5).abs() < 0.001);
        assert_eq!(carry, [0x00, 0x80, 0x00]);
        // Its last byte arrives: left -1, right 0.
        let second = mono(&[0x00], &mut carry);
        assert_eq!(second, [-0.5]);
        assert!(carry.is_empty());
        assert!(mono(&[], &mut carry).is_empty());
    }

    #[test]
    fn a_tone_shows_in_its_band_and_the_others_stay_low() {
        // 1 kHz is in the fifth band (1000..2000 Hz), as the spectrum's edges say.
        let capture = Capture::from_reader(Cursor::new(tone(1500., RATE as usize / 4, 0.5)), None);
        std::thread::sleep(Duration::from_millis(200));
        let levels = capture.levels();
        let loudest = levels
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(loudest, 4, "{levels:?}");
        assert!(levels[4] > 0.5 && levels[0] < levels[4] && levels[6] < levels[4]);
    }

    #[test]
    fn silence_is_zero_and_an_ended_recording_is_an_error() {
        let mut capture = Capture::from_reader(Cursor::new(vec![0u8; 4096 * 8]), None);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(capture.levels(), [0.; BANDS]);
        // The reader ran out of input: the recording is over.
        assert_eq!(capture.drain(), Err(()));
    }

    #[test]
    fn a_player_with_streams_is_recorded_by_stream_and_one_without_by_the_default_output() {
        assert!(wanted(&[]) == Mode::System);
        assert!(wanted(&[(7, "out.monitor".into())]) == Mode::Process);
        assert_eq!(Mode::Unavailable.name(), "unavailable");
        assert_eq!(Mode::Process.name(), "process");
        assert_eq!(Mode::System.name(), "system");
    }

    #[test]
    #[ignore = "records on this desktop's sound server; OPENBAR_LIVE_MONITOR names a monitor with streams ToneA (1500 Hz) and ToneB (100 Hz) playing to it"]
    fn one_stream_of_an_output_is_heard_without_the_other() {
        use crate::linux::pulse::{Pactl, Pulse};
        let monitor = std::env::var("OPENBAR_LIVE_MONITOR").expect("OPENBAR_LIVE_MONITOR");
        let streams = Pactl::default().streams().unwrap();
        let index = |app: &str| {
            streams
                .iter()
                .find(|s| s.app == app)
                .unwrap_or_else(|| panic!("no stream {app}"))
                .index
        };
        let loudest = |capture: &Capture| {
            std::thread::sleep(Duration::from_millis(700));
            let levels = capture.levels();
            let band = levels
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            (band, levels)
        };
        let a = Capture::stream(index("ToneA"), &monitor).expect("parec for ToneA");
        let b = Capture::stream(index("ToneB"), &monitor).expect("parec for ToneB");
        let (band_a, levels_a) = loudest(&a);
        let (band_b, levels_b) = loudest(&b);
        println!("ToneA {levels_a:?}\nToneB {levels_b:?}");
        assert_eq!(band_a, 4, "ToneA is 1500 Hz");
        assert_eq!(band_b, 0, "ToneB is 100 Hz, in 50..120");
        // What each one does not play stays well under what it does.
        assert!(levels_a[0] < levels_a[4] * 0.6 && levels_b[4] < levels_b[0] * 0.6);
        // The whole output has both.
        let both = Capture::record(&monitor, None).expect("parec for the monitor");
        let (_, levels) = loudest(&both);
        println!("both {levels:?}");
        assert!(levels[0] > 0.3 && levels[4] > 0.3);
    }
}
