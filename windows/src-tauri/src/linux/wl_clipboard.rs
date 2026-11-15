// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The clipboard and typed keys on Wayland (#252), through the command line
//! tools that speak the compositor's protocols. A Wayland app sees the
//! clipboard only while it has the keyboard, so GTK's clipboard cannot keep a
//! history; `wl-paste` and `wl-copy` (wl-clipboard) use the data-control
//! protocol sway, Hyprland, KDE and other compositors offer to clipboard
//! managers, and `wtype` types through the virtual keyboard protocol.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Whether `program` is on the PATH.
pub fn installed(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            let file = dir.join(program);
            std::fs::metadata(&file).is_ok_and(|m| m.is_file())
        })
    })
}

/// Whether the clipboard goes through wl-clipboard: GTK on Wayland itself, with the tools there.
pub fn available() -> bool {
    super::wm::native_wayland() && installed("wl-paste") && installed("wl-copy")
}

/// Calls `on_change` for each new copy, from a thread of its own, for as long as the app runs.
pub fn watch(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        loop {
            // The copy is handed to the command; it is read again by type later.
            let child = Command::new("wl-paste")
                .args(["--watch", "sh", "-c", "cat >/dev/null; echo"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                // wl-paste runs the command once for what is there when it starts.
                let mut first = true;
                for line in BufReader::new(out).lines() {
                    if line.is_err() {
                        break;
                    }
                    if !std::mem::take(&mut first) {
                        on_change();
                    }
                }
            }
            let _ = child.wait();
            // The compositor went away or restarted the protocol; try again calmly.
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

/// The types the copy offers.
pub fn types() -> Vec<String> {
    Command::new("wl-paste")
        .arg("--list-types")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The copy as `mime`, at most `limit` bytes.
pub fn get(mime: &str, limit: usize) -> Option<Vec<u8>> {
    let output = Command::new("wl-paste")
        .args(["--no-newline", "--type", mime])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    (output.status.success() && output.stdout.len() <= limit).then_some(output.stdout)
}

/// Puts `bytes` on the clipboard as `mime`; wl-copy stays behind to serve them.
pub fn set(mime: &str, bytes: &[u8]) -> bool {
    let Ok(mut child) = Command::new("wl-copy")
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(bytes).is_ok());
    // wl-copy forks to serve the copy, so the first process ends once it has it.
    child.wait().is_ok_and(|status| status.success()) && written
}

/// wtype's arguments for the paste shortcut, Shift added for a terminal.
pub fn paste_keys(terminal: bool) -> Vec<&'static str> {
    let mut keys = vec!["-M", "ctrl"];
    if terminal {
        keys.extend(["-M", "shift"]);
    }
    keys.extend(["-k", "v"]);
    if terminal {
        keys.extend(["-m", "shift"]);
    }
    keys.extend(["-m", "ctrl"]);
    keys
}

/// Types the paste shortcut into the window with the keyboard.
pub fn type_paste(terminal: bool) -> bool {
    installed("wtype")
        && Command::new("wtype")
            .args(paste_keys(terminal))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paste_shortcut_takes_shift_in_a_terminal_and_lets_go_in_reverse() {
        assert_eq!(paste_keys(false), ["-M", "ctrl", "-k", "v", "-m", "ctrl"]);
        assert_eq!(
            paste_keys(true),
            [
                "-M", "ctrl", "-M", "shift", "-k", "v", "-m", "shift", "-m", "ctrl"
            ]
        );
    }

    #[test]
    fn a_program_is_found_on_the_path() {
        assert!(installed("sh"));
        assert!(!installed("openbar-no-such-program"));
    }
}
