// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The island's alert chime on Linux, where Windows plays a WAV from memory:
//! the WAV is written once to a file, and a player the system has (PulseAudio's
//! or PipeWire's, ALSA's, ffmpeg's) is asked to play it, without waiting for it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Players in the order they are tried, and what each needs besides the file.
const PLAYERS: [(&str, &[&str]); 4] = [
    ("paplay", &["--client-name=Openbar"]),
    ("pw-play", &[]),
    ("aplay", &["-q"]),
    ("ffplay", &["-nodisp", "-autoexit", "-loglevel", "quiet"]),
];

/// The WAV as a file the players can open, kept for the next chime. Where
/// the runtime folder is there, the file is the person's alone.
fn chime_file(wav: &[u8]) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("openbar-chime-{}.wav", wav.len()));
    if std::fs::metadata(&path).is_ok_and(|m| m.len() == wav.len() as u64) {
        return Some(path);
    }
    // Written beside it and moved into place, so a player never opens half of it.
    let part = dir.join(format!(
        "openbar-chime-{}.{}.part",
        wav.len(),
        std::process::id()
    ));
    std::fs::write(&part, wav).ok()?;
    std::fs::rename(&part, &path).ok()?;
    Some(path)
}

/// Starts the first of `players` that is on `path` playing `file`; true when
/// one started. The player is left to finish on its own.
fn play_with(
    players: &[(&str, &[&str])],
    path: Option<&str>,
    file: &Path,
    extra: &[(&str, &str)],
) -> bool {
    for (program, arguments) in players {
        let found = path
            .unwrap_or("")
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join(program))
            .find(|candidate| candidate.is_file());
        let Some(found) = found else { continue };
        let started = Command::new(found)
            .args(*arguments)
            .arg(file)
            .envs(extra.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = started {
            // Reaped when it ends, so it does not linger as a zombie.
            std::thread::spawn(move || child.wait());
            return true;
        }
    }
    false
}

/// Plays the chime once, in the background. Nothing happens where there is
/// no player.
pub fn play(wav: &[u8]) {
    if let Some(file) = chime_file(wav) {
        play_with(&PLAYERS, std::env::var("PATH").ok().as_deref(), &file, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::command::fake_program;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-sound-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_first_player_found_is_asked_to_play_the_file() {
        let dir = temp("players");
        let log = dir.join("calls");
        let make = |name: &str| {
            fake_program(
                &dir,
                name,
                &format!("echo \"{name} $*\" >> {}", log.display()),
            )
        };
        make("aplay");
        make("ffplay");
        let path = dir.to_string_lossy().into_owned();
        assert!(play_with(
            &PLAYERS,
            Some(&path),
            Path::new("/x/chime.wav"),
            &[]
        ));
        for _ in 0..100 {
            if log.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // paplay and pw-play are not there: aplay is next, with its own flags.
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "aplay -q /x/chime.wav\n"
        );
        assert!(!play_with(
            &PLAYERS,
            Some("/nonexistent"),
            Path::new("/x/chime.wav"),
            &[]
        ));
        assert!(!play_with(&PLAYERS, None, Path::new("/x/chime.wav"), &[]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_wav_is_written_once_and_read_back_whole() {
        let wav = crate::sound::chime_wav();
        let file = chime_file(wav).expect("somewhere to write it");
        assert_eq!(std::fs::read(&file).unwrap(), wav);
        // Asked again, the same file; a different chime would be another one.
        assert_eq!(chime_file(wav), Some(file));
        let shorter = chime_file(&wav[..wav.len() - 2]);
        assert_ne!(shorter, chime_file(wav));
        if let Some(shorter) = shorter {
            let _ = std::fs::remove_file(shorter);
        }
    }

    #[test]
    #[ignore = "plays the chime into the sound server of this desktop; OPENBAR_LIVE_SINK names a null sink made for it"]
    fn the_chime_is_heard_on_the_test_sink() {
        let sink = std::env::var("OPENBAR_LIVE_SINK").expect("OPENBAR_LIVE_SINK");
        let file = chime_file(crate::sound::chime_wav()).unwrap();
        let path = std::env::var("PATH").ok();
        // Recorded from the sink's monitor while it plays, read as it comes (a pipe holds 64 KiB).
        let mut recording = std::process::Command::new("parec")
            .args(["--raw", "--format=s16le", "--rate=44100", "--channels=1"])
            .arg(format!("--device={sink}.monitor"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("parec");
        let mut pipe = recording.stdout.take().unwrap();
        let (started, running) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut data = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(count) = std::io::Read::read(&mut pipe, &mut chunk) {
                if count == 0 {
                    break;
                }
                if data.is_empty() {
                    let _ = started.send(());
                }
                data.extend_from_slice(&chunk[..count]);
            }
            data
        });
        // Not before the recording is under way, or the start of the chime is lost.
        running
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("parec gave nothing");
        assert!(play_with(
            &PLAYERS[..1],
            path.as_deref(),
            &file,
            &[("PULSE_SINK", &sink)]
        ));
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let _ = recording.kill();
        let _ = recording.wait();
        let data = reader.join().unwrap();
        let peak = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b).unsigned_abs())
            .max()
            .unwrap_or(0);
        println!("recorded {} bytes, peak {peak}", data.len());
        assert!(peak > 2000, "the chime was not heard (peak {peak})");
    }
}
