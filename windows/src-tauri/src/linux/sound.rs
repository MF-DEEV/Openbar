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

/// The players that can play `file`: ALSA's only takes WAV, and sound themes
/// ship Ogg.
fn players_for(file: &Path) -> Vec<(&'static str, &'static [&'static str])> {
    let wav = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
    PLAYERS
        .iter()
        .copied()
        .filter(|(program, _)| wav || *program != "aplay")
        .collect()
}

/// Plays a sound theme's file once, in the background.
pub fn play_file(file: &Path) {
    let path = std::env::var("PATH").ok();
    play_with(&players_for(file), path.as_deref(), file, &[]);
}

/// The existing desktop players, with gain on this stream only. ALSA has no
/// gain flag, so its WAV is scaled before it is handed to aplay.
fn session_arguments(program: &str, volume: f64) -> Vec<String> {
    match program {
        "paplay" => vec![
            "--client-name=Openbar".into(),
            format!("--volume={}", (volume * 65536.0).round() as u32),
        ],
        "pw-play" => vec![format!("--volume={volume}")],
        "ffplay" => vec![
            "-nodisp".into(),
            "-autoexit".into(),
            "-loglevel".into(),
            "quiet".into(),
            "-volume".into(),
            (volume * 100.0).round().to_string(),
        ],
        _ => vec!["-q".into()],
    }
}

enum SessionPlayback {
    Play {
        file: PathBuf,
        cache: PathBuf,
        volume: f64,
        ticket: u64,
        result: std::sync::mpsc::SyncSender<bool>,
    },
    Stop,
}

/// One worker owns and reaps the session player. It blocks while idle; while
/// audio is playing it waits briefly for stop/replacement commands. Timer and
/// Watch playback keep their existing independent path.
fn session_worker() -> &'static std::sync::mpsc::Sender<SessionPlayback> {
    static WORKER: std::sync::OnceLock<std::sync::mpsc::Sender<SessionPlayback>> =
        std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut playing: Option<std::process::Child> = None;
            loop {
                let command = if playing.is_some() {
                    match receiver.recv_timeout(std::time::Duration::from_millis(30)) {
                        Ok(command) => Some(command),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    }
                } else {
                    match receiver.recv() {
                        Ok(command) => Some(command),
                        Err(_) => break,
                    }
                };
                if playing
                    .as_mut()
                    .is_some_and(|child| child.try_wait().ok().flatten().is_some())
                {
                    playing = None;
                }
                let Some(command) = command else {
                    continue;
                };
                if let Some(mut child) = playing.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let SessionPlayback::Play {
                    file,
                    cache,
                    volume,
                    ticket,
                    result,
                } = command
                {
                    if crate::sound::session_request_current(ticket) {
                        playing = spawn_session(&file, &cache, volume);
                    }
                    let _ = result.send(playing.is_some());
                }
            }
            if let Some(mut child) = playing {
                let _ = child.kill();
                let _ = child.wait();
            }
        });
        sender
    })
}

fn spawn_session(file: &Path, cache: &Path, volume: f64) -> Option<std::process::Child> {
    for (program, _) in players_for(file) {
        let mut source = file.to_path_buf();
        if program == "aplay" {
            let Some(wav) = std::fs::read(file)
                .ok()
                .and_then(|data| crate::sound::pcm::scaled_wav(&data, volume))
            else {
                continue;
            };
            let folder = cache.join("session-sounds");
            if std::fs::create_dir_all(&folder).is_err() {
                continue;
            }
            source = folder.join("alsa-preview.wav");
            if std::fs::write(&source, wav).is_err() {
                continue;
            }
        }
        if let Ok(mut child) = Command::new(program)
            .args(session_arguments(program, volume))
            .arg(source)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            // A server program may exist while its daemon or audio device does
            // not. If it exits immediately with an error, try the next player.
            std::thread::sleep(std::time::Duration::from_millis(35));
            if child
                .try_wait()
                .ok()
                .flatten()
                .is_some_and(|status| !status.success())
            {
                continue;
            }
            return Some(child);
        }
    }
    None
}

pub fn play_session(file: &Path, volume: f64, cache: &Path, ticket: u64) -> bool {
    if !volume.is_finite() || volume <= 0.0 {
        return false;
    }
    let (result, receiver) = std::sync::mpsc::sync_channel(1);
    if session_worker()
        .send(SessionPlayback::Play {
            file: file.to_owned(),
            cache: cache.to_owned(),
            volume: volume.clamp(0.0, 1.0),
            ticket,
            result,
        })
        .is_err()
    {
        return false;
    }
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false)
}

pub fn stop_session() {
    let _ = session_worker().send(SessionPlayback::Stop);
}

/// The sound themes' folders of sounds, by theme, the person's own first
/// (`$XDG_DATA_HOME`, then `$XDG_DATA_DIRS`, as the sound theme spec has it).
pub fn themes() -> Vec<(String, PathBuf)> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    themes_in(home.into_iter().chain(system.split(':').map(PathBuf::from)))
}

fn themes_in(roots: impl Iterator<Item = PathBuf>) -> Vec<(String, PathBuf)> {
    let mut themes = Vec::new();
    for root in roots.filter(|r| r.is_absolute()) {
        let Ok(entries) = std::fs::read_dir(root.join("sounds")) else {
            continue;
        };
        let mut found: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let stereo = entry.path().join("stereo");
                let theme = entry.file_name().to_str()?.to_string();
                stereo.is_dir().then_some((theme, stereo))
            })
            .collect();
        found.sort();
        themes.extend(found);
    }
    themes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::command::fake_program;

    #[test]
    fn session_volume_is_a_player_argument_never_a_mixer_command() {
        assert_eq!(
            session_arguments("paplay", 0.5),
            ["--client-name=Openbar", "--volume=32768"]
        );
        assert_eq!(session_arguments("pw-play", 0.25), ["--volume=0.25"]);
        assert!(session_arguments("ffplay", 0.75).ends_with(&["-volume".into(), "75".into()]));
        assert_eq!(session_arguments("aplay", 0.5), ["-q"]);
        assert!(!play_session(
            Path::new("/missing.wav"),
            0.0,
            Path::new("/tmp"),
            0
        ));
    }

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
    fn an_ogg_sound_skips_the_wav_only_player() {
        let names = |file: &str| -> Vec<&str> {
            players_for(Path::new(file))
                .into_iter()
                .map(|(program, _)| program)
                .collect()
        };
        assert_eq!(names("/x/bell.oga"), ["paplay", "pw-play", "ffplay"]);
        assert_eq!(names("/x/Bell.OGG"), ["paplay", "pw-play", "ffplay"]);
        assert_eq!(
            names("/x/chime.WAV"),
            ["paplay", "pw-play", "aplay", "ffplay"]
        );
    }

    #[test]
    fn themes_are_found_in_every_data_folder_in_order() {
        let dir = temp("themes");
        let own = dir.join("own");
        let system = dir.join("system");
        for (root, theme) in [
            (&own, "freedesktop"),
            (&system, "freedesktop"),
            (&system, "Yaru"),
        ] {
            std::fs::create_dir_all(root.join("sounds").join(theme).join("stereo")).unwrap();
        }
        // A theme without a stereo folder has nothing to play.
        std::fs::create_dir_all(system.join("sounds/empty")).unwrap();
        let found = themes_in([own.clone(), system.clone(), PathBuf::from("relative")].into_iter());
        // The person's own folder first; a relative one is ignored.
        assert_eq!(
            found,
            [
                (
                    "freedesktop".to_string(),
                    own.join("sounds/freedesktop/stereo")
                ),
                ("Yaru".to_string(), system.join("sounds/Yaru/stereo")),
                (
                    "freedesktop".to_string(),
                    system.join("sounds/freedesktop/stereo")
                ),
            ]
        );
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
