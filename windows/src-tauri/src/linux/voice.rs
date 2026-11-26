// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! What voice replies need from Linux: the microphone as 16 kHz mono floats
//! (`parec`, which PulseAudio and PipeWire's pulse server both answer, as the
//! rest of the sound code does) and the speech model's download through
//! libsoup, where Windows has WinHTTP. The audio stays in memory.

use crate::voice::{Problem, SAMPLE_RATE};
use gtk::gio::{self, prelude::*};
use soup::prelude::*;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// The default input, as `parec` reads it.
pub struct Microphone {
    child: Child,
    out: ChildStdout,
    /// Bytes of a sample that came split across two reads.
    carry: Vec<u8>,
}

impl Microphone {
    pub fn open() -> Result<Microphone, Problem> {
        let mut child = Command::new("parec")
            .args([
                "--raw",
                "--format=float32le",
                "--channels=1",
                &format!("--rate={SAMPLE_RATE}"),
                "--latency-msec=50",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Problem::NoMicrophone)?;
        let Some(out) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Problem::Failed);
        };
        Ok(Microphone {
            child,
            out,
            carry: Vec::new(),
        })
    }

    /// Appends what came in since the last call, blocking until some did.
    /// False when the input is gone (the server stopped, or `parec` found none).
    pub fn read(&mut self, into: &mut Vec<f32>) -> bool {
        let mut buffer = [0u8; 8192];
        let Ok(count) = self.out.read(&mut buffer) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        self.carry.extend_from_slice(&buffer[..count]);
        let whole = self.carry.len() / 4 * 4;
        into.extend(samples(&self.carry[..whole]));
        self.carry.drain(..whole);
        true
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Little-endian 32-bit floats; a trailing partial sample is left out.
fn samples(bytes: &[u8]) -> impl Iterator<Item = f32> + '_ {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
}

/// No answer for this long, and the download has failed.
const STEP: u32 = 30;
/// 60 MB on a slow line still finishes; a server that trickles does not hold it for ever.
const WHOLE: Duration = Duration::from_secs(30 * 60);

/// Saves `url` to `part`, reporting the bytes so far.
pub fn fetch(url: &str, part: &Path, progress: &mut dyn FnMut(u64)) -> Result<(), String> {
    use std::io::Write;
    let session = soup::Session::builder()
        .timeout(STEP)
        .idle_timeout(STEP)
        .user_agent(concat!("Openbar/", env!("CARGO_PKG_VERSION")))
        .build();
    let message = soup::Message::new("GET", url).map_err(|e| e.to_string())?;
    let cancel = gio::Cancellable::new();
    let _whole = super::net::Deadline::new(&cancel, Instant::now() + WHOLE);
    let stream = session
        .send(&message, Some(&cancel))
        .map_err(|e| e.to_string())?;
    if message.status() != soup::Status::Ok {
        return Err(format!("the server answered {:?}", message.status()));
    }
    let mut out = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    let mut reported = Instant::now();
    loop {
        let chunk = stream
            .read_bytes(64 << 10, Some(&cancel))
            .map_err(|e| e.to_string())?;
        if chunk.is_empty() {
            break;
        }
        out.write_all(&chunk).map_err(|e| e.to_string())?;
        total += chunk.len() as u64;
        if reported.elapsed() >= Duration::from_millis(250) {
            reported = Instant::now();
            progress(total);
        }
    }
    progress(total);
    out.flush().map_err(|e| e.to_string())
}

/// The desktop's sound settings, where a microphone is picked, as the
/// voice reply's "Open Settings" opens Windows' sound page: the desktop's
/// own panel first, then PulseAudio's volume control (input devices tab).
/// False when none of them is installed.
pub fn open_sound_settings() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    sound_settings(&desktop)
        .iter()
        .any(|(program, args)| crate::linux::command::spawn(program, args))
}

fn sound_settings(desktop: &str) -> Vec<(&'static str, &'static [&'static str])> {
    let desktop = desktop.to_ascii_lowercase();
    let has = |name: &str| desktop.split(':').any(|part| part == name);
    let mut found: Vec<(&'static str, &'static [&'static str])> = Vec::new();
    if has("gnome") || has("ubuntu") || has("unity") || has("pop") || has("budgie") {
        found.push(("gnome-control-center", &["sound"]));
    }
    if has("kde") {
        found.push(("systemsettings", &["kcm_pulseaudio"]));
        found.push(("systemsettings5", &["kcm_pulseaudio"]));
    }
    if has("x-cinnamon") || has("cinnamon") {
        found.push(("cinnamon-settings", &["sound"]));
    }
    if has("mate") {
        found.push(("mate-volume-control", &["--page=input"]));
    }
    found.push(("pavucontrol", &["--tab=4"]));
    found.push(("pavucontrol-qt", &[]));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_own_sound_panel_comes_before_the_volume_control() {
        let first = |desktop: &str| sound_settings(desktop)[0].0;
        assert_eq!(first("ubuntu:GNOME"), "gnome-control-center");
        assert_eq!(first("KDE"), "systemsettings");
        assert_eq!(first("X-Cinnamon"), "cinnamon-settings");
        assert_eq!(first("MATE"), "mate-volume-control");
        assert_eq!(first("XFCE"), "pavucontrol");
        assert_eq!(first(""), "pavucontrol");
        // The volume control stays the last resort everywhere.
        assert!(sound_settings("GNOME").iter().any(|(p, _)| *p == "pavucontrol"));
    }

    #[test]
    fn floats_are_read_little_endian_and_a_split_sample_waits() {
        let mut bytes = Vec::new();
        for value in [0.5f32, -0.25] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.push(0x12);
        assert_eq!(samples(&bytes).collect::<Vec<_>>(), vec![0.5, -0.25]);
    }

    /// By hand with a microphone: cargo test live_microphone -- --ignored
    #[test]
    #[ignore]
    fn live_microphone_gives_a_second_of_audio() {
        let mut input = Microphone::open().expect("parec");
        let mut heard = Vec::new();
        while heard.len() < SAMPLE_RATE as usize {
            assert!(input.read(&mut heard), "the input ended");
        }
    }

    /// By hand, 60 MB from the network: cargo test live_model -- --ignored
    #[test]
    #[ignore]
    fn live_model_download_is_the_model() {
        let part = std::env::temp_dir().join("openbar-model-test.part");
        let mut last = 0;
        fetch(
            &format!(
                "https://{}{}",
                crate::voice::model::HOST,
                crate::voice::model::PATH
            ),
            &part,
            &mut |n| last = n,
        )
        .unwrap();
        assert_eq!(last, crate::voice::model::SIZE);
        crate::voice::model::verify(&part).unwrap();
        let _ = std::fs::remove_file(&part);
    }
}
