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
                    + 0.35
                        * (2.0 * std::f32::consts::PI * freq * 2.76 * t).sin()
                        * (-t * 9.0).exp()
                    + 0.15
                        * (2.0 * std::f32::consts::PI * freq * 5.4 * t).sin()
                        * (-t * 14.0).exp();
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
    #[cfg(target_os = "linux")]
    crate::linux::sound::play(chime_wav());
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
    alarms()
        .lock()
        .unwrap()
        .insert(key.to_string(), stop.clone());
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

#[cfg(test)]
pub fn alarm_running(key: &str) -> bool {
    alarms().lock().unwrap().contains_key(key)
}

/// A sound of the system's that can stand in for the chime.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SoundChoice {
    /// What settings keep: the file's name, under its theme on Linux.
    pub id: String,
    pub name: String,
}

/// The system's sounds with their files: Windows' Media folder, or the
/// freedesktop sound themes on Linux (the channel and volume test sounds left
/// out). Sorted by name.
fn system_sounds() -> Vec<(SoundChoice, std::path::PathBuf)> {
    #[cfg(windows)]
    let mut sounds = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(|root| sounds_in(&std::path::Path::new(&root).join("Media"), None))
        .unwrap_or_default();
    #[cfg(target_os = "linux")]
    let mut sounds = crate::linux::sound::themes()
        .into_iter()
        .flat_map(|(theme, dir)| sounds_in(&dir, Some(&theme)))
        .collect::<Vec<_>>();
    #[cfg(not(any(windows, target_os = "linux")))]
    let mut sounds: Vec<(SoundChoice, std::path::PathBuf)> = Vec::new();
    // A theme in the person's own folder comes first and hides the system's.
    let mut seen = std::collections::HashSet::new();
    sounds.retain(|(choice, _)| seen.insert(choice.id.clone()));
    sounds.sort_by_cached_key(|(choice, _)| choice.name.to_lowercase());
    sounds
}

/// The sound files directly in `dir`. `theme` names the Linux theme, which
/// goes into the id and, unless it is the freedesktop default, the name.
fn sounds_in(dir: &std::path::Path, theme: Option<&str>) -> Vec<(SoundChoice, std::path::PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sounds = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let (Some(stem), Some(extension)) = (
            path.file_stem().and_then(|s| s.to_str()),
            path.extension().and_then(|s| s.to_str()),
        ) else {
            continue;
        };
        let playable = match theme {
            None => extension.eq_ignore_ascii_case("wav"),
            Some(_) => ["oga", "ogg", "wav"]
                .iter()
                .any(|e| extension.eq_ignore_ascii_case(e)),
        };
        // Speaker tests and the volume tick say nothing about a session.
        if !playable || !path.is_file() || (theme.is_some() && stem.starts_with("audio-")) {
            continue;
        }
        let file = format!("{stem}.{extension}");
        let (id, name) = match theme {
            None => (file, stem.to_string()),
            Some(theme) => {
                let words = stem.replace(['-', '_'], " ");
                let mut name: String = words
                    .chars()
                    .take(1)
                    .flat_map(char::to_uppercase)
                    .chain(words.chars().skip(1))
                    .collect();
                if theme != "freedesktop" {
                    name = format!("{name} ({theme})");
                }
                (format!("{theme}/{file}"), name)
            }
        };
        sounds.push((SoundChoice { id, name }, path));
    }
    sounds
}

/// The file of the system sound `id`; only a listed sound has one.
fn sound_file(id: Option<&str>) -> Option<std::path::PathBuf> {
    let id = id.filter(|id| !id.is_empty())?;
    system_sounds()
        .into_iter()
        .find(|(choice, _)| choice.id == id)
        .map(|(_, path)| path)
}

/// Plays the system sound `id`, or the chime when there is none by that name.
pub fn play(id: Option<&str>) {
    match sound_file(id) {
        Some(path) => play_file(&path),
        None => play_once(),
    }
}

fn play_file(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    crate::linux::sound::play_file(path);
    #[cfg(windows)]
    unsafe {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let _ = PlaySoundW(
            windows::core::PCWSTR(wide.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        );
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = path;
}

#[tauri::command]
pub fn sound_list() -> Vec<SoundChoice> {
    system_sounds()
        .into_iter()
        .map(|(choice, _)| choice)
        .collect()
}

/// Plays `sound` (an id from `sound_list`), or the chime.
#[tauri::command]
pub fn sound_play(sound: Option<String>) {
    play(sound.as_deref());
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
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs())
            .max()
            .unwrap();
        assert!(
            peak > 3000 && peak < i16::MAX as u16,
            "audible without clipping: {peak}"
        );
    }

    fn folder(name: &str, files: &[&str]) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openbar-sounds-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Raga")).unwrap();
        for file in files {
            std::fs::write(dir.join(file), b"RIFF").unwrap();
        }
        dir
    }

    #[test]
    fn windows_lists_the_wave_files_of_its_media_folder() {
        let dir = folder(
            "media",
            &[
                "Windows Ding.wav",
                "chimes.WAV",
                "desktop.ini",
                "Alarm01.wav",
            ],
        );
        let mut found: Vec<SoundChoice> =
            sounds_in(&dir, None).into_iter().map(|(c, _)| c).collect();
        found.sort_by_key(|c| c.name.to_lowercase());
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect();
        // Only the wave files, not the folder of a theme or other files.
        assert_eq!(
            names,
            [
                ("Alarm01.wav", "Alarm01"),
                ("chimes.WAV", "chimes"),
                ("Windows Ding.wav", "Windows Ding")
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_linux_theme_names_its_sounds_and_leaves_the_speaker_tests_out() {
        let dir = folder(
            "theme",
            &[
                "message-new-instant.oga",
                "bell.ogg",
                "complete.wav",
                "audio-channel-front-left.oga",
                "audio-volume-change.oga",
                "notes.txt",
            ],
        );
        let mut found: Vec<SoundChoice> = sounds_in(&dir, Some("freedesktop"))
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        found.sort_by_key(|c| c.name.to_lowercase());
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("freedesktop/bell.ogg", "Bell"),
                ("freedesktop/complete.wav", "Complete"),
                ("freedesktop/message-new-instant.oga", "Message new instant")
            ]
        );
        // Another theme says which it is.
        let other = sounds_in(&dir, Some("Yaru"));
        assert!(
            other
                .iter()
                .any(|(c, _)| c.id == "Yaru/bell.ogg" && c.name == "Bell (Yaru)")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_listed_sound_has_a_file() {
        // Anything else is the chime, so a settings file cannot point at another file.
        assert_eq!(sound_file(Some("../../../etc/passwd")), None);
        assert_eq!(sound_file(Some("/etc/passwd")), None);
        assert_eq!(sound_file(Some("")), None);
        assert_eq!(sound_file(None), None);
        if let Some((choice, path)) = system_sounds().into_iter().next() {
            assert_eq!(sound_file(Some(&choice.id)), Some(path));
        }
    }

    #[test]
    fn an_alarm_stops_when_asked() {
        start_alarm("test");
        assert!(alarm_running("test"));
        stop_alarm("test");
        assert!(!alarm_running("test"));
    }
}
