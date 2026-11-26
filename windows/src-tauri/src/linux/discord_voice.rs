// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Discord's own mute and deafen on Linux, for the microphone indicator,
//! where Windows reads Discord's window through UI Automation: the Mute and
//! Deafen buttons of its user panel are read through AT-SPI while its window
//! is shown, and until then the state Discord saved at its last start
//! (`discord_storage`) stands in. Read only: nothing is clicked or typed.
//!
//! Chromium, and so Discord, is on the accessibility bus only while
//! assistive technologies are on (GNOME's `toolkit-accessibility`, or Discord
//! started with `--force-renderer-accessibility`), as for answering it from
//! a notification. Windows also follows Discord's own mute shortcuts while
//! its window is hidden; the key codes Discord saves on Linux are not known,
//! so here a shortcut pressed while Discord is hidden shows once it is seen.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, MapState, Window};
use x11rb::rust_connection::RustConnection;

use crate::discord_storage::{Reader as Storage, VoiceState};
use crate::linux::atspi::{
    Accessibility, Node, STATE_CHECKABLE, STATE_CHECKED, STATE_PRESSED, STATE_SHOWING,
};
use crate::linux::x11;

/// Most accessible objects walked looking for the two buttons.
const WALK_BUDGET: usize = 6000;

/// Reads a button's label and checked state: (is it Deafen, is it on), as
/// `microphone::control` does for Windows.
pub type Parse = fn(&str, Option<bool>) -> Option<(bool, bool)>;

/// Discord's window classes and the folder each keeps its settings in.
const CLIENTS: [&str; 3] = ["discord", "discordcanary", "discordptb"];

/// The class a stand-in's window has in tests; the real client is never driven.
fn class_override() -> Option<String> {
    std::env::var("OPENBAR_DISCORD_WINDOW_CLASS").ok()
}

/// Which client a window class is, by its folder name.
fn client_of(classes: &str, stand_in: Option<&str>) -> Option<&'static str> {
    classes.split('\0').find_map(|class| {
        if stand_in.is_some_and(|s| class.eq_ignore_ascii_case(s)) {
            return Some(CLIENTS[0]);
        }
        CLIENTS
            .iter()
            .find(|c| class.eq_ignore_ascii_case(c))
            .copied()
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Found {
    pid: u32,
    client: &'static str,
    /// Mapped and not minimized: Chromium keeps its page tree current then.
    shown: bool,
}

fn x11_window() -> Option<Found> {
    let (c, screen) = RustConnection::connect(None).ok()?;
    let root = c.setup().roots.get(screen)?.root;
    let list = x11::atom(&c, "_NET_CLIENT_LIST")?;
    let pid_atom = x11::atom(&c, "_NET_WM_PID")?;
    let state = x11::atom(&c, "_NET_WM_STATE")?;
    let hidden = x11::atom(&c, "_NET_WM_STATE_HIDDEN")?;
    let windows: Vec<Window> = c
        .get_property(false, root, list, AtomEnum::WINDOW, 0, 4096)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .collect();
    let stand_in = class_override();
    let mut found: Option<Found> = None;
    for window in windows {
        let classes = c
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
            .unwrap_or_default();
        let Some(client) = client_of(&classes, stand_in.as_deref()) else {
            continue;
        };
        let Some(pid) = x11::pid_of(&c, window, pid_atom) else {
            continue;
        };
        let mapped = c
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|a| a.map_state == MapState::VIEWABLE);
        let minimized = c
            .get_property(false, window, state, AtomEnum::ATOM, 0, 32)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| Some(reply.value32()?.any(|a| a == hidden)))
            .unwrap_or(false);
        let this = Found {
            pid,
            client,
            shown: mapped && !minimized,
        };
        // A shown window of the client beats a hidden one (its tray popups).
        if found.is_none_or(|f| !f.shown && this.shown) {
            found = Some(this);
        }
    }
    found
}

fn wayland_window() -> Option<Found> {
    let stand_in = class_override();
    crate::linux::wm::toplevels()?
        .into_iter()
        .find_map(|t| {
            let id = t.app_id.to_ascii_lowercase();
            let id = id.strip_prefix("com.discordapp.").unwrap_or(&id).to_string();
            Some(Found {
                pid: t.pid,
                client: client_of(&id, stand_in.as_deref())?,
                shown: true,
            })
        })
}

fn discord_window() -> Option<Found> {
    if crate::linux::wm::native_wayland() {
        wayland_window()
    } else {
        x11_window()
    }
}

/// The folders a client may keep its local storage in: its own, Flatpak's
/// and Snap's.
fn storage_folders(home: &Path, client: &str) -> Vec<PathBuf> {
    let leveldb = |base: PathBuf| base.join(client).join("Local Storage").join("leveldb");
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    vec![
        leveldb(config),
        leveldb(home.join(".var/app/com.discordapp.Discord/config")),
        leveldb(home.join("snap/discord/current/.config")),
    ]
}

/// A button's label and its checked state, as AT-SPI states tell it: a
/// checkable button that is neither checked nor pressed is off; anything
/// else says nothing, and the label decides.
fn checked(states: u64) -> Option<bool> {
    let on = |bit: u32| states & (1 << bit) != 0;
    if on(STATE_CHECKED) || on(STATE_PRESSED) {
        Some(true)
    } else if on(STATE_CHECKABLE) {
        Some(false)
    } else {
        None
    }
}

/// Mute and Deafen out of the two buttons' readings.
fn state_of(mic: (bool, bool), headphones: (bool, bool)) -> Option<VoiceState> {
    let (false, muted) = mic else { return None };
    let (true, deafened) = headphones else {
        return None;
    };
    Some(VoiceState {
        muted: muted || deafened,
        deafened,
    })
}

#[derive(Default)]
pub struct Reader {
    storage: Storage,
    a11y: Option<Accessibility>,
    /// The two buttons as last found, with the process they belong to.
    controls: Option<(u32, Node, Node)>,
    client: Option<&'static str>,
    state: Option<VoiceState>,
}

impl Reader {
    /// Discord's mute and deafen now, None while it is not running.
    pub fn read(&mut self, parse: Parse) -> Option<VoiceState> {
        let Some(window) = discord_window() else {
            *self = Reader {
                a11y: self.a11y.take(),
                ..Default::default()
            };
            return None;
        };
        if self.client != Some(window.client) {
            self.client = Some(window.client);
            self.state = None;
            self.controls = None;
        }
        if window.shown
            && let Some(state) = self.read_controls(window.pid, parse)
        {
            self.state = Some(state);
        }
        if self.state.is_none()
            && let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        {
            // Saved at Discord's start: a seed, never over what the buttons say.
            self.state = storage_folders(&home, window.client)
                .iter()
                .filter(|dir| dir.is_dir())
                .find_map(|dir| self.storage.read(dir));
        }
        self.state
    }

    fn read_controls(&mut self, pid: u32, parse: Parse) -> Option<VoiceState> {
        if self.a11y.is_none() {
            self.a11y = Accessibility::open();
        }
        let a11y = self.a11y.as_ref()?;
        let reading = |node: &Node| {
            let states = a11y.states(node);
            (states & (1 << STATE_SHOWING) != 0)
                .then(|| parse(&a11y.name(node), checked(states)))
                .flatten()
        };
        if let Some((known, mic, headphones)) = &self.controls
            && *known == pid
            && let (Some(m), Some(h)) = (reading(mic), reading(headphones))
            && let Some(state) = state_of(m, h)
        {
            return Some(state);
        }
        self.controls = None;
        let app = a11y.application(pid)?;
        let (mut mic, mut headphones) = (None, None);
        let mut queue = VecDeque::from([app]);
        let mut walked = 0;
        while let Some(node) = queue.pop_front() {
            walked += 1;
            if walked > WALK_BUDGET {
                break;
            }
            let states = a11y.states(&node);
            if walked > 1 && states & (1 << STATE_SHOWING) == 0 {
                continue;
            }
            if let Some(control) = parse(&a11y.name(&node), checked(states)) {
                if control.0 {
                    headphones = Some((control, node));
                } else {
                    mic = Some((control, node));
                }
                if mic.is_some() && headphones.is_some() {
                    break;
                }
                continue;
            }
            queue.extend(a11y.children(&node));
        }
        let ((m, mic), (h, headphones)) = (mic?, headphones?);
        let state = state_of(m, h)?;
        self.controls = Some((pid, mic, headphones));
        Some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_client_is_known_by_its_window_class() {
        assert_eq!(client_of("discord\0discord\0", None), Some("discord"));
        assert_eq!(client_of("discordcanary\0DiscordCanary\0", None), Some("discordcanary"));
        assert_eq!(client_of("kitty\0kitty\0", None), None);
        assert_eq!(client_of("standin\0Standin\0", Some("standin")), Some("discord"));
    }

    #[test]
    fn its_settings_are_looked_for_in_each_place_discord_is_installed() {
        let folders = storage_folders(Path::new("/home/me"), "discord");
        assert!(folders.contains(&PathBuf::from(
            "/home/me/.var/app/com.discordapp.Discord/config/discord/Local Storage/leveldb"
        )));
        assert!(folders.contains(&PathBuf::from(
            "/home/me/snap/discord/current/.config/discord/Local Storage/leveldb"
        )));
    }

    #[test]
    fn a_checkable_button_that_is_not_checked_is_off() {
        assert_eq!(checked(1 << STATE_CHECKED), Some(true));
        assert_eq!(checked(1 << STATE_PRESSED), Some(true));
        assert_eq!(checked(1u64 << STATE_CHECKABLE), Some(false));
        assert_eq!(checked(1 << STATE_SHOWING), None);
    }

    #[test]
    fn deafened_is_muted_too_and_both_buttons_are_needed() {
        let off = |deafen: bool| (deafen, false);
        assert_eq!(
            state_of(off(false), off(true)),
            Some(VoiceState {
                muted: false,
                deafened: false
            })
        );
        assert_eq!(
            state_of(off(false), (true, true)),
            Some(VoiceState {
                muted: true,
                deafened: true
            })
        );
        // Two mute buttons are not a mute and a deafen.
        assert_eq!(state_of(off(false), off(false)), None);
    }
}
