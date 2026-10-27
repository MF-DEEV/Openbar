// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's alert chime (the Mac plays Glass.aiff, which can't ship): an
//! original two-note bell synthesized once into a WAV in memory. Timer and
//! Watch alarms repeat it every 2 s for up to 5 minutes, until stopped
//! (NotchTimerAlert.swift; spec 04 §4.4).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const RATE: u32 = 44_100;
pub const REPEAT: Duration = Duration::from_secs(2);
pub const LIMIT: Duration = Duration::from_secs(5 * 60);

/// 16-bit mono WAV: E6 then B6, each a soft sine with a few overtones and an
/// exponential decay, about 0.9 s in all.
pub fn chime_wav() -> &'static [u8] {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    WAV.get_or_init(|| {
        let notes = [(1318.5_f32, 0.0_f32), (1975.5, 0.14)];
        let len = (RATE as f32 * 0.9) as usize;
        let mut samples = vec![0f32; len];
        for (freq, start) in notes {
            let offset = (start * RATE as f32) as usize;
            for (i, sample) in samples.iter_mut().enumerate().skip(offset) {
                let t = (i - offset) as f32 / RATE as f32;
                let attack = (t / 0.004).min(1.0);
                let decay = (-t * 5.5).exp();
                let tone = (2.0 * std::f32::consts::PI * freq * t).sin()
                    + 0.35 * (2.0 * std::f32::consts::PI * freq * 2.76 * t).sin() * (-t * 9.0).exp()
                    + 0.15 * (2.0 * std::f32::consts::PI * freq * 5.4 * t).sin() * (-t * 14.0).exp();
                *sample += 0.28 * attack * decay * tone;
            }
        }
        let mut wav = Vec::with_capacity(44 + len * 2);
        let data = (len * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav
    })
}

pub fn play_once() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Media::Audio::{PlaySoundA, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        let _ = PlaySoundA(
            windows::core::PCSTR(chime_wav().as_ptr()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

fn alarms() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static A: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    A.get_or_init(Default::default)
}

/// Starts (or restarts) the alarm named `key`, e.g. "timer" or "watch".
pub fn start_alarm(key: &str) {
    stop_alarm(key);
    let stop = Arc::new(AtomicBool::new(false));
    alarms().lock().unwrap().insert(key.to_string(), stop.clone());
    let key = key.to_string();
    std::thread::spawn(move || {
        let began = Instant::now();
        while !stop.load(Ordering::Relaxed) && began.elapsed() < LIMIT {
            play_once();
            let next = Instant::now() + REPEAT;
            while Instant::now() < next {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let mut map = alarms().lock().unwrap();
        if map.get(&key).is_some_and(|s| Arc::ptr_eq(s, &stop)) {
            map.remove(&key);
        }
    });
}

pub fn stop_alarm(key: &str) {
    if let Some(stop) = alarms().lock().unwrap().remove(key) {
        stop.store(true, Ordering::Relaxed);
    }
}

pub fn alarm_running(key: &str) -> bool {
    alarms().lock().unwrap().contains_key(key)
}

#[tauri::command]
pub fn sound_play() {
    play_once();
}

#[tauri::command]
pub fn sound_alarm(key: String, on: bool) {
    if on {
        start_alarm(&key);
    } else {
        stop_alarm(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chime_is_a_valid_pcm_wav() {
        let wav = chime_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data);
        assert!(data > RATE as usize); // longer than half a second
        let peak = wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 3000 && peak < i16::MAX as u16, "audible without clipping: {peak}");
    }

    #[test]
    fn an_alarm_stops_when_asked() {
        start_alarm("test");
        assert!(alarm_running("test"));
        stop_alarm("test");
        assert!(!alarm_running("test"));
    }
}
