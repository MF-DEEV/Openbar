// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The clipboard and typed keys on Wayland (#252), through the command line
//! tools that speak the compositor's protocols. A Wayland app sees the
//! clipboard only while it has the keyboard, so GTK's clipboard cannot keep a
//! history; `wl-paste` and `wl-copy` (wl-clipboard) use the data-control
//! protocol sway, Hyprland, KDE and other compositors offer to clipboard
//! managers, and `wtype` types through the virtual keyboard protocol, or
//! `ydotool` through the kernel where the compositor has none (KDE).

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
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    let on_change: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(on_change);
    std::thread::spawn(move || {
        loop {
            // The protocol is spoken here; wl-paste only where the compositor
            // has a newer version of it than ours (and wl-paste 2.2 misses
            // KWin 5.27's changes, so it is not the first choice).
            match data_control::watch(on_change.clone()) {
                Err(data_control::Missing) => watch_with_wl_paste(&*on_change),
                Ok(()) => {}
            }
            // The compositor went away or restarted; try again calmly.
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

/// `wl-paste --watch`, ended with the app.
fn watch_with_wl_paste(on_change: &dyn Fn()) {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new("wl-paste");
    command
        .args(["--watch", "sh", "-c", "cat >/dev/null; echo"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // SAFETY: prctl is async-signal-safe; it only asks for a signal when we are gone.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let Ok(mut child) = command.spawn() else {
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
    let _ = child.kill();
    let _ = child.wait();
}

/// The data-control protocol clipboard managers use (`zwlr_data_control_v1`):
/// the compositor says each time the clipboard changes hands.
mod data_control {
    use std::sync::Arc;
    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_registry, wl_seat};
    use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
    use wayland_protocols_wlr::data_control::v1::client::{
        zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
        zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
        zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    };

    /// The compositor has no such protocol (or there is no compositor).
    pub struct Missing;

    struct Watcher {
        selections: u64,
        on_change: Arc<dyn Fn() + Send + Sync>,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Watcher {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for Watcher {
        fn event(
            _: &mut Self,
            _: &wl_seat::WlSeat,
            _: wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrDataControlManagerV1, ()> for Watcher {
        fn event(
            _: &mut Self,
            _: &ZwlrDataControlManagerV1,
            _: <ZwlrDataControlManagerV1 as wayland_client::Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrDataControlOfferV1, ()> for Watcher {
        fn event(
            _: &mut Self,
            _: &ZwlrDataControlOfferV1,
            _: <ZwlrDataControlOfferV1 as wayland_client::Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrDataControlDeviceV1, ()> for Watcher {
        fn event(
            state: &mut Self,
            _: &ZwlrDataControlDeviceV1,
            event: zwlr_data_control_device_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwlr_data_control_device_v1::Event::Selection { id } => {
                    // What it holds is read by wl-paste; the offer is not kept.
                    if let Some(offer) = id {
                        offer.destroy();
                    }
                    state.selections += 1;
                    // The first is what was on the clipboard when we asked.
                    if state.selections > 1 {
                        (state.on_change)();
                    }
                }
                zwlr_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                    offer.destroy();
                }
                _ => {}
            }
        }

        event_created_child!(Watcher, ZwlrDataControlDeviceV1, [
            zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
        ]);
    }

    /// Calls `on_change` for each new copy until the compositor goes away.
    pub fn watch(on_change: Arc<dyn Fn() + Send + Sync>) -> Result<(), Missing> {
        let connection = Connection::connect_to_env().map_err(|_| Missing)?;
        let (globals, mut queue) =
            registry_queue_init::<Watcher>(&connection).map_err(|_| Missing)?;
        let handle = queue.handle();
        let seat: wl_seat::WlSeat = globals.bind(&handle, 1..=1, ()).map_err(|_| Missing)?;
        let manager: ZwlrDataControlManagerV1 =
            globals.bind(&handle, 1..=2, ()).map_err(|_| Missing)?;
        let _device = manager.get_data_device(&seat, &handle, ());
        let mut watcher = Watcher {
            selections: 0,
            on_change,
        };
        while queue.blocking_dispatch(&mut watcher).is_ok() {}
        Ok(())
    }
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

/// The copy as `mime` (the type wl-paste picks for None), at most `limit` bytes.
pub fn get(mime: Option<&str>, limit: usize) -> Option<Vec<u8>> {
    let mut command = Command::new("wl-paste");
    command.arg("--no-newline");
    if let Some(mime) = mime {
        command.args(["--type", mime]);
    }
    let output = command
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

/// ydotool's arguments for the paste shortcut: key codes with 1 for down and
/// 0 for up (ydotool 1.0 and later), or key names (the 0.1 series).
pub fn ydotool_keys(terminal: bool, named: bool) -> Vec<&'static str> {
    match (named, terminal) {
        (true, true) => vec!["key", "ctrl+shift+v"],
        (true, false) => vec!["key", "ctrl+v"],
        // KEY_LEFTCTRL 29, KEY_LEFTSHIFT 42, KEY_V 47.
        (false, true) => vec!["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
        (false, false) => vec!["key", "29:1", "47:1", "47:0", "29:0"],
    }
}

fn quietly(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Types the paste shortcut into the window with the keyboard: through the
/// virtual keyboard protocol (`wtype`), else through the kernel's input
/// (`ydotool`, whose daemon must be running), which also works on KDE.
pub fn type_paste(terminal: bool) -> bool {
    if installed("wtype") && quietly("wtype", &paste_keys(terminal)) {
        return true;
    }
    installed("ydotool")
        && (quietly("ydotool", &ydotool_keys(terminal, false))
            || quietly("ydotool", &ydotool_keys(terminal, true)))
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
    fn ydotool_is_told_the_keys_by_code_or_by_name() {
        assert_eq!(
            ydotool_keys(true, false),
            ["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"]
        );
        assert_eq!(ydotool_keys(false, false), ["key", "29:1", "47:1", "47:0", "29:0"]);
        assert_eq!(ydotool_keys(false, true), ["key", "ctrl+v"]);
    }

    #[test]
    fn a_program_is_found_on_the_path() {
        assert!(installed("sh"));
        assert!(!installed("openbar-no-such-program"));
    }
}
