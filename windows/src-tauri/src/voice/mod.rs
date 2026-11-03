// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Voice replies (VoiceReplyRecorder.swift on the Mac): the microphone, at
//! 16 kHz mono, feeds whisper.cpp on this PC and nothing else. Windows' own
//! dictation (`SpeechRecognitionTopicConstraint`) is a web-service grammar
//! that needs "Online speech recognition" and sends the audio to Microsoft,
//! so it is not used (docs/voice-replies.md). The model is downloaded once,
//! when the person asks, and checked against its SHA-256.
//!
//! This side only listens and transcribes. When to stop (silence, the cap,
//! Escape, the session going away) is the island's: sessions/voice.ts.

pub mod model;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod whisper;

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use tauri::Emitter;
use tauri::{AppHandle, Manager, Runtime};

/// The audio rate whisper wants.
pub const SAMPLE_RATE: u32 = 16_000;
/// Longer than the island's 60 s cap, so a stuck page can't keep it open.
pub const HARD_CAP_SECONDS: u32 = 75;
/// How often the words heard so far are transcribed again.
pub const PARTIAL_EVERY_MS: u64 = 1_200;
/// A voice is louder than this (RMS, about -42 dBFS); quieter audio is never
/// transcribed, since whisper makes words up from a quiet room.
pub const SPEECH_RMS: f32 = 0.008;

/// Why a recording could not start or went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Problem {
    /// No input device at all.
    NoMicrophone,
    /// Settings → Privacy & security → Microphone keeps desktop apps out.
    MicrophoneDenied,
    /// The speech model is not on this PC yet.
    ModelMissing,
    /// The model would not load, or recognition failed.
    Failed,
    /// Another reply is being spoken.
    Busy,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    pub id: u64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub id: u64,
    pub level: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub id: u64,
    pub problem: Problem,
}

/// The recording that runs, if any; only one at a time.
struct Recording {
    id: u64,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Shared {
    recording: Mutex<Option<Recording>>,
    next: AtomicU64,
    pub model: model::Shared,
}

/// Starts listening; the words come as `voice://partial` and, once stopped,
/// one `voice://final`. Errors that are known up front come back at once.
#[tauri::command]
pub fn voice_start<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Shared>,
    language: String,
) -> Result<u64, Problem> {
    let mut slot = state.recording.lock().unwrap();
    if slot.is_some() {
        return Err(Problem::Busy);
    }
    let path = model::path(&app).filter(|p| model::is_ready(p));
    let Some(path) = path else {
        return Err(Problem::ModelMissing);
    };
    let id = state.next.fetch_add(1, Ordering::SeqCst) + 1;
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));
    start(
        app.clone(),
        id,
        path,
        whisper_language(&language),
        stop.clone(),
        cancel.clone(),
    )?;
    *slot = Some(Recording { id, stop, cancel });
    Ok(id)
}

/// Stops listening and transcribes what was heard one last time.
#[tauri::command]
pub fn voice_stop(state: tauri::State<'_, Shared>, id: u64) {
    if let Some(recording) = state.recording.lock().unwrap().as_ref().filter(|r| r.id == id) {
        recording.stop.store(true, Ordering::SeqCst);
    }
}

/// Stops listening and drops what was heard, without a last transcription.
#[tauri::command]
pub fn voice_cancel(state: tauri::State<'_, Shared>, id: u64) {
    if let Some(recording) = state.recording.lock().unwrap().as_ref().filter(|r| r.id == id) {
        recording.cancel.store(true, Ordering::SeqCst);
        recording.stop.store(true, Ordering::SeqCst);
    }
}

/// The worker is done with recording `id`; another may start.
fn finished<R: Runtime>(app: &AppHandle<R>, id: u64) {
    let state = app.state::<Shared>();
    let mut slot = state.recording.lock().unwrap();
    if slot.as_ref().is_some_and(|r| r.id == id) {
        *slot = None;
    }
}

#[cfg(windows)]
fn start<R: Runtime>(
    app: AppHandle<R>,
    id: u64,
    model: std::path::PathBuf,
    language: Option<&'static str>,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
) -> Result<(), Problem> {
    let source = match replay_file() {
        Some(file) => capture::Source::File(file),
        None => capture::Source::Microphone,
    };
    let island = crate::island::LABEL;
    let meter = {
        let app = app.clone();
        move |level: f32| {
            let _ = app.emit_to(island, "voice://level", Level { id, level });
        }
    };
    let audio = capture::start(source, stop.clone(), meter)?;
    std::thread::spawn(move || {
        let outcome = whisper::listen(&model, language, &audio, &stop, &cancel, |text| {
            let _ = app.emit_to(island, "voice://partial", Text { id, text });
        });
        audio.join();
        match outcome {
            Ok(Some(text)) => {
                let _ = app.emit_to(island, "voice://final", Text { id, text });
            }
            Ok(None) => {}
            Err(problem) => {
                let _ = app.emit_to(island, "voice://error", Failure { id, problem });
            }
        }
        finished(&app, id);
    });
    Ok(())
}

#[cfg(not(windows))]
fn start<R: Runtime>(
    app: AppHandle<R>,
    id: u64,
    _model: std::path::PathBuf,
    _language: Option<&'static str>,
    _stop: Arc<AtomicBool>,
    _cancel: Arc<AtomicBool>,
) -> Result<(), Problem> {
    let _ = (app, id);
    Err(Problem::NoMicrophone)
}

/// `--voice-reply-file <wav>` plays a recording in place of the microphone,
/// through the same recognizer, for end-to-end runs in a VM without one.
#[cfg_attr(not(windows), allow(dead_code))]
fn replay_file() -> Option<std::path::PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|a| a == "--voice-reply-file")?;
    args.get(at + 1).map(std::path::PathBuf::from)
}

/// The island's language, or one picked in Settings, as whisper names it;
/// None lets whisper tell which language is spoken.
pub fn whisper_language(choice: &str) -> Option<&'static str> {
    let base = choice.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    Some(match base.as_str() {
        "en" => "en",
        "pt" => "pt",
        "tr" => "tr",
        "ru" => "ru",
        "es" => "es",
        "sk" => "sk",
        "de" => "de",
        "fr" => "fr",
        "it" => "it",
        "ja" => "ja",
        "ko" => "ko",
        "uk" => "uk",
        "zh" => "zh",
        "pl" => "pl",
        "nl" => "nl",
        "cs" => "cs",
        "sv" => "sv",
        _ => return None,
    })
}

/// A buffer's RMS.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// The level the island's meter and silence count read: -60…-10 dBFS on
/// 0…1, the same scale as the Mac's (VoiceReplyMachine.level).
pub fn level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() + 60.0) / 50.0).clamp(0.0, 1.0)
}

/// 16-bit or float mono/stereo PCM from a WAV file, as 16 kHz mono floats.
/// Only what `say -o … --data-format=LEI16@16000` and common tools write.
pub fn read_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut at = 12;
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            format = u16::from_le_bytes([body[0], body[1]]);
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            if channels == 0 {
                return Err("no format before the data".into());
            }
            if rate != SAMPLE_RATE {
                return Err(format!("{rate} Hz; voice replies read {SAMPLE_RATE} Hz"));
            }
            let frames: Vec<f32> = match (format, bits) {
                (1, 16) => body
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                    .collect(),
                (3, 32) => body
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                    .collect(),
                _ => return Err(format!("format {format}, {bits} bits")),
            };
            let channels = channels as usize;
            return Ok(frames
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect());
        }
        at += 8 + size + (size & 1);
    }
    Err("no audio in the file".into())
}

/// Text whisper puts in place of speech, or marks for noises, which the
/// island must not type into a session.
pub fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `Vorssaint.exe --voice-probe <wav> <out> [language]`: the island's
/// recognizer on a file, and how long it took.
pub fn probe_main(wav: &str, out: &str, language: &str) -> ! {
    let report = probe(wav, language);
    let _ = std::fs::write(out, report);
    std::process::exit(0)
}

#[cfg(windows)]
fn probe(wav: &str, language: &str) -> String {
    let Some(path) = model::default_path().filter(|p| model::is_ready(p)) else {
        return "model missing".into();
    };
    let audio = match std::fs::read(wav).map_err(|e| e.to_string()).and_then(|b| read_wav(&b)) {
        Ok(audio) => audio,
        Err(error) => return format!("unreadable: {error}"),
    };
    let started = std::time::Instant::now();
    match whisper::transcribe_once(&path, whisper_language(language), &audio) {
        Ok(text) => format!(
            "{:.2}s for {:.1}s of audio: {}",
            started.elapsed().as_secs_f32(),
            audio.len() as f32 / SAMPLE_RATE as f32,
            text
        ),
        Err(problem) => format!("failed: {problem:?}"),
    }
}

#[cfg(not(windows))]
fn probe(_wav: &str, _language: &str) -> String {
    "voice replies run on Windows".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(format: u16, bits: u16, channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        out.extend(b"fmt \x10\0\0\0");
        out.extend(format.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * channels as u32 * bits as u32 / 8).to_le_bytes());
        out.extend((channels * bits / 8).to_le_bytes());
        out.extend(bits.to_le_bytes());
        // A chunk the reader skips, with an odd size and its pad byte.
        out.extend(b"LIST\x03\0\0\0abc\0");
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn reads_the_wavs_say_writes() {
        let pcm: Vec<u8> = [0i16, 16384, -32768].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(read_wav(&wav(1, 16, 1, 16_000, &pcm)).unwrap(), vec![0.0, 0.5, -1.0]);
        let stereo: Vec<u8> = [16384i16, 0].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(read_wav(&wav(1, 16, 2, 16_000, &stereo)).unwrap(), vec![0.25]);
        let float: Vec<u8> = [0.25f32].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(read_wav(&wav(3, 32, 1, 16_000, &float)).unwrap(), vec![0.25]);
        assert!(read_wav(&wav(1, 16, 1, 44_100, &pcm)).unwrap_err().contains("44100"));
        assert!(read_wav(&wav(1, 8, 1, 16_000, &[1, 2])).is_err());
        assert!(read_wav(b"nope").is_err());
        assert!(read_wav(b"RIFF\0\0\0\0WAVE").is_err());
    }

    #[test]
    fn whisper_languages() {
        assert_eq!(whisper_language("de-DE"), Some("de"));
        assert_eq!(whisper_language("pt-BR"), Some("pt"));
        assert_eq!(whisper_language("zh-Hans"), Some("zh"));
        assert_eq!(whisper_language("zh_TW"), Some("zh"));
        assert_eq!(whisper_language("EN"), Some("en"));
        assert_eq!(whisper_language("auto"), None);
        assert_eq!(whisper_language(""), None);
    }

    #[test]
    fn levels_match_the_mac() {
        assert_eq!(level(0.0), 0.0);
        assert_eq!(level(0.001), 0.0);
        assert_eq!(level(1.0), 1.0);
        assert!((level(0.01) - 0.4).abs() < 1e-5);
        assert!((rms(&[0.5, -0.5]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
        // The speech gate sits just under where the island hears a voice.
        assert!(level(SPEECH_RMS) < 0.35 && level(SPEECH_RMS) > 0.2);
    }

    #[test]
    fn noise_marks_are_not_words() {
        assert_eq!(clean(" [BLANK_AUDIO]"), "");
        assert_eq!(clean("(music) Run the tests. [laughs]"), "Run the tests.");
        assert_eq!(clean("  two   spaces "), "two spaces");
        assert_eq!(clean("keep ) stray"), "keep ) stray");
    }
}
