// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! whisper.cpp on this PC's CPU. While the microphone is open, what it heard
//! is transcribed again about every second for the words as they come; once
//! it closes, one last time for the words that stay.

use super::capture::Audio;
use super::{PARTIAL_EVERY_MS, Problem, SAMPLE_RATE, clean};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

fn load(model: &Path) -> Result<WhisperContext, Problem> {
    // whisper.cpp prints its progress to stderr; the island has no console.
    whisper_rs::install_logging_hooks();
    WhisperContext::new_with_params(model, WhisperContextParameters::default())
        .map_err(|_| Problem::Failed)
}

fn transcribe(state: &mut WhisperState, language: Option<&str>, audio: &[f32]) -> Result<String, Problem> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(language.unwrap_or("auto")));
    params.set_translate(false);
    params.set_no_context(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    params.set_n_threads(threads as i32);
    // Under a second of audio, whisper pads it; a short word still reads.
    let mut padded;
    let audio = if audio.len() < SAMPLE_RATE as usize {
        padded = audio.to_vec();
        padded.resize(SAMPLE_RATE as usize + SAMPLE_RATE as usize / 4, 0.0);
        &padded[..]
    } else {
        audio
    };
    state.full(params, audio).map_err(|_| Problem::Failed)?;
    let mut text = String::new();
    for segment in state.as_iter() {
        if let Ok(piece) = segment.to_str_lossy() {
            text.push_str(&piece);
        }
    }
    Ok(clean(&text))
}

/// Transcribes as the audio comes, until it stops; None when cancelled.
pub fn listen(
    model: &Path,
    language: Option<&str>,
    audio: &Audio,
    stop: &AtomicBool,
    cancel: &AtomicBool,
    partial: impl Fn(String),
) -> Result<Option<String>, Problem> {
    let context = load(model)?;
    let mut state = context.create_state().map_err(|_| Problem::Failed)?;
    let mut last = Instant::now();
    let mut transcribed = 0usize;
    let mut said = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(50));
        if cancel.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if stop.load(Ordering::SeqCst) || audio.is_done() {
            break;
        }
        let length = audio.len();
        let due = last.elapsed() >= Duration::from_millis(PARTIAL_EVERY_MS);
        // Half a second more since the last time, and a voice somewhere in it.
        if due && length >= transcribed + SAMPLE_RATE as usize / 2 && audio.heard_speech() {
            let text = transcribe(&mut state, language, &audio.snapshot())?;
            transcribed = length;
            last = Instant::now();
            if text != said && !cancel.load(Ordering::SeqCst) {
                said = text.clone();
                partial(text);
            }
        }
    }
    // Wait for the microphone thread, so the last words are all in.
    while !audio.is_done() {
        std::thread::sleep(Duration::from_millis(10));
    }
    if cancel.load(Ordering::SeqCst) {
        return Ok(None);
    }
    if !audio.heard_speech() {
        return Ok(Some(String::new()));
    }
    transcribe(&mut state, language, &audio.snapshot()).map(Some)
}

/// One transcription of a whole recording, for the probe.
pub fn transcribe_once(model: &Path, language: Option<&str>, audio: &[f32]) -> Result<String, Problem> {
    let context = load(model)?;
    let mut state = context.create_state().map_err(|_| Problem::Failed)?;
    transcribe(&mut state, language, audio)
}
