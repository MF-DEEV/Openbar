// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Linux (X11): Discord's window is found by its class and brought forward
//! through the window manager, its message box is read and focused through
//! AT-SPI, the desktop's accessibility bus, and the reply is typed through
//! XTEST. The checks themselves live in `discord_reply.rs`.
//!
//! Chromium, and so Discord, puts its page on the accessibility bus only while
//! assistive technologies are on (GNOME's `toolkit-accessibility`, or Discord
//! started with `--force-renderer-accessibility`). Without that the message box
//! cannot be read, so nothing is typed and the island says so.

use std::collections::{HashMap, VecDeque};
use std::thread;
use std::time::{Duration, Instant};

use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, Keysym, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::{LIMIT, is_blank, matches_target, target};
use crate::linux::x11;

/// The window class Discord's stable client gives its windows. A stand-in can
/// be named for tests, since the real client is never driven by them.
fn discord_class() -> String {
    std::env::var("OPENBAR_DISCORD_WINDOW_CLASS").unwrap_or_else(|_| "discord".into())
}

/// Most accessible objects walked looking for the message box.
const WALK_BUDGET: usize = 6000;
/// Characters typed between two checks that Discord still has the keyboard.
const CHUNK: usize = 32;
const KEY_GAP: Duration = Duration::from_millis(4);
const XK_RETURN: Keysym = 0xff0d;
const XK_SHIFT_L: Keysym = 0xffe1;
// AT-SPI state bits (AtspiStateType).
const STATE_EDITABLE: u32 = 7;
const STATE_FOCUSED: u32 = 12;
const STATE_SHOWING: u32 = 25;

struct Display {
    connection: RustConnection,
    root: Window,
    active: u32,
}

impl Display {
    fn open() -> Option<Self> {
        let (connection, screen) = RustConnection::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let active = x11::atom(&connection, "_NET_ACTIVE_WINDOW")?;
        Some(Self {
            connection,
            root,
            active,
        })
    }

    fn in_front(&self, window: Window) -> bool {
        x11::active_window(&self.connection, self.root, self.active) == Some(window)
    }

    fn text(&self, window: Window, property: u32, kind: u32) -> String {
        self.connection
            .get_property(false, window, property, kind, 0, 1024)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
            .unwrap_or_default()
    }

    /// Discord's main window, topmost first, with its title and pid.
    fn discord(&self) -> Option<(Window, String, Option<u32>)> {
        let c = &self.connection;
        let (name, utf8, pid) = (
            x11::atom(c, "_NET_WM_NAME")?,
            x11::atom(c, "UTF8_STRING")?,
            x11::atom(c, "_NET_WM_PID")?,
        );
        // Bottom to top where the window manager keeps a stacking list; some
        // (xfwm4 among them) only keep the list in mapping order.
        let windows: Vec<Window> = ["_NET_CLIENT_LIST_STACKING", "_NET_CLIENT_LIST"]
            .into_iter()
            .find_map(|list| {
                let list = x11::atom(c, list)?;
                let reply = c
                    .get_property(false, self.root, list, AtomEnum::WINDOW, 0, 4096)
                    .ok()?
                    .reply()
                    .ok()?;
                let windows: Vec<Window> = reply.value32()?.collect();
                (!windows.is_empty()).then_some(windows)
            })?;
        let class = discord_class();
        windows.into_iter().rev().find_map(|window| {
            let classes = self.text(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into());
            classes
                .split('\0')
                .any(|part| part.eq_ignore_ascii_case(&class))
                .then(|| {
                    (
                        window,
                        self.text(window, name, utf8),
                        x11::pid_of(c, window, pid),
                    )
                })
        })
    }

    /// Asks the window manager for Discord and waits until it is in front.
    fn bring_forward(&self, window: Window) -> bool {
        if !self.in_front(window)
            && !x11::activate(&self.connection, self.root, self.active, window)
        {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        while !self.in_front(window) {
            if Instant::now() > deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
        // The keyboard follows the window a moment after the manager says so.
        thread::sleep(Duration::from_millis(60));
        true
    }
}

/// One object on the accessibility bus.
type Node = (String, OwnedObjectPath);

struct Accessibility {
    bus: Connection,
}

impl Accessibility {
    fn open() -> Option<Self> {
        let session = Connection::session().ok()?;
        let address: String = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            )
            .ok()?
            .body()
            .deserialize()
            .ok()?;
        let bus = zbus::blocking::connection::Builder::address(address.as_str())
            .ok()?
            .build()
            .ok()?;
        Some(Self { bus })
    }

    fn call<B>(&self, node: &Node, interface: &str, method: &str, body: &B) -> Option<zbus::Message>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.bus
            .call_method(
                Some(node.0.as_str()),
                node.1.as_str(),
                Some(interface),
                method,
                body,
            )
            .ok()
    }

    fn children(&self, node: &Node) -> Vec<Node> {
        self.call(node, "org.a11y.atspi.Accessible", "GetChildren", &())
            .and_then(|reply| reply.body().deserialize().ok())
            .unwrap_or_default()
    }

    fn name(&self, node: &Node) -> String {
        self.call(
            node,
            "org.freedesktop.DBus.Properties",
            "Get",
            &("org.a11y.atspi.Accessible", "Name"),
        )
        .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
    }

    fn states(&self, node: &Node) -> u64 {
        let words: Vec<u32> = self
            .call(node, "org.a11y.atspi.Accessible", "GetState", &())
            .and_then(|reply| reply.body().deserialize().ok())
            .unwrap_or_default();
        words
            .iter()
            .take(2)
            .enumerate()
            .fold(0, |bits, (i, word)| bits | (u64::from(*word) << (32 * i)))
    }

    fn text(&self, node: &Node) -> Option<String> {
        self.call(node, "org.a11y.atspi.Text", "GetText", &(0i32, -1i32))?
            .body()
            .deserialize()
            .ok()
    }

    fn grab_focus(&self, node: &Node) -> bool {
        self.call(node, "org.a11y.atspi.Component", "GrabFocus", &())
            .and_then(|reply| reply.body().deserialize::<bool>().ok())
            .unwrap_or(false)
    }

    /// The application object of the process that owns Discord's window.
    fn application(&self, pid: u32) -> Option<Node> {
        let root: Node = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").ok()?,
        );
        self.children(&root).into_iter().find(|app| {
            self.bus
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetConnectionUnixProcessID",
                    &(app.0.as_str(),),
                )
                .ok()
                .and_then(|reply| reply.body().deserialize::<u32>().ok())
                == Some(pid)
        })
    }

    /// The showing, editable object whose label `fits`: Discord's message box.
    fn composer(&self, app: &Node, fits: impl Fn(&str) -> bool) -> Option<(Node, String)> {
        let mut queue = VecDeque::from([app.clone()]);
        let mut walked = 0;
        while let Some(node) = queue.pop_front() {
            walked += 1;
            if walked > WALK_BUDGET {
                return None;
            }
            let states = self.states(&node);
            // A hidden branch holds no box the person is looking at.
            if walked > 1 && states & (1 << STATE_SHOWING) == 0 {
                continue;
            }
            if states & (1 << STATE_EDITABLE) != 0 {
                let name = self.name(&node);
                if fits(&name) {
                    return Some((node, name));
                }
                continue;
            }
            queue.extend(self.children(&node));
        }
        None
    }
}

/// Types `text` into the window in front through XTEST. A character missing
/// from the keyboard map borrows a spare keycode for its key press, as xdotool
/// does, and the spare is cleared again afterwards. Discord must stay in front:
/// it is checked before each chunk, so a switch mid-reply stops the rest.
fn type_text(display: &Display, window: Window, text: &str) -> Result<(), String> {
    let c = &display.connection;
    let setup = c.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = c
        .get_keyboard_mapping(min, max - min + 1)
        .map_err(|_| "inputRefused")?
        .reply()
        .map_err(|_| "inputRefused")?;
    let per = usize::from(map.keysyms_per_keycode);
    let mut known: HashMap<Keysym, (u8, bool)> = HashMap::new();
    let mut spare = None;
    for (i, syms) in map.keysyms.chunks(per).enumerate() {
        let code = min + i as u8;
        if syms.iter().all(|s| *s == 0) {
            spare = spare.or(Some(code));
        }
        for (column, sym) in syms
            .iter()
            .take(2)
            .enumerate()
            .filter(|(_, sym)| **sym != 0)
        {
            known.entry(*sym).or_insert((code, column == 1));
        }
    }
    let (Some(&(shift, _)), Some(&(enter, _))) = (known.get(&XK_SHIFT_L), known.get(&XK_RETURN))
    else {
        return Err("inputRefused".into());
    };
    let key = |kind: u8, code: u8| {
        thread::sleep(KEY_GAP);
        c.xtest_fake_input(kind, code, 0, display.root, 0, 0, 0)
            .is_ok_and(|cookie| cookie.check().is_ok())
    };
    let press = |code: u8, shifted: bool| {
        (!shifted || key(KEY_PRESS_EVENT, shift))
            && key(KEY_PRESS_EVENT, code)
            && key(KEY_RELEASE_EVENT, code)
            && (!shifted || key(KEY_RELEASE_EVENT, shift))
    };
    let remap = |code: u8, sym: Keysym| {
        c.change_keyboard_mapping(1, code, per as u8, &vec![sym; per])
            .is_ok_and(|cookie| cookie.check().is_ok())
    };
    let mut borrowed = false;
    let mut result = Ok(());
    for (i, ch) in text.chars().enumerate() {
        if i % CHUNK == 0 && !display.in_front(window) {
            result = Err(if i == 0 {
                "focusLost"
            } else {
                "typingInterrupted"
            }
            .into());
            break;
        }
        let ok = if ch == '\n' {
            press(enter, true)
        } else {
            let sym = keysym(ch);
            match known.get(&sym) {
                Some(&(code, shifted)) => press(code, shifted),
                None => match spare {
                    Some(code) => {
                        borrowed = true;
                        // Clients reload the map on MappingNotify before the key arrives.
                        let ok = remap(code, sym);
                        thread::sleep(Duration::from_millis(12));
                        ok && press(code, false)
                    }
                    None => false,
                },
            }
        };
        if !ok {
            result = Err(if i == 0 {
                "inputRefused"
            } else {
                "typingInterrupted"
            }
            .into());
            break;
        }
    }
    if let (true, Some(code)) = (borrowed, spare) {
        thread::sleep(Duration::from_millis(12));
        let _ = remap(code, 0);
    }
    result
}

/// The X keysym for a character: Latin-1 as itself, the rest in the Unicode range.
fn keysym(ch: char) -> Keysym {
    let point = u32::from(ch);
    if (0x20..=0x7e).contains(&point) || (0xa0..=0xff).contains(&point) {
        point
    } else {
        0x0100_0000 | point
    }
}

pub fn send(app_id: &str, title: &str, text: &str) -> Result<(), String> {
    let target = target(title).ok_or("noChat")?;
    let text = text.trim();
    if text.is_empty() || text.chars().count() > LIMIT {
        return Err("tooLong".into());
    }
    let display = Display::open().ok_or("openFailed")?;
    if display.discord().is_none() && !crate::notifications::notifications_open(app_id.into()) {
        return Err("openFailed".into());
    }
    // A Discord that was just started needs a moment for its window.
    let mut found = None;
    for _ in 0..25 {
        found = display.discord();
        if found.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    let (window, _, pid) = found.ok_or("openFailed")?;
    if !display.bring_forward(window) {
        return Err("focusLost".into());
    }
    let window_title = display
        .discord()
        .map(|(_, title, _)| title)
        .unwrap_or_default();
    let accessibility = Accessibility::open().ok_or("inspectFailed")?;
    // Discord joins the accessibility bus only when it starts with assistive
    // technologies on; turning them on later does not bring it.
    let app = pid
        .and_then(|pid| accessibility.application(pid))
        .ok_or("accessibilityOff")?;
    let (editor, editor_name) = accessibility
        .composer(&app, |name| matches_target(&target, name, &window_title))
        .ok_or("wrongChat")?;
    // Enter would send a half-written message there together with the reply.
    let existing = accessibility.text(&editor).ok_or("inspectFailed")?;
    if !is_blank(&existing, &editor_name) {
        return Err("draftInDiscord".into());
    }
    if !accessibility.grab_focus(&editor) {
        return Err("focusLost".into());
    }
    thread::sleep(Duration::from_millis(80));
    if accessibility.states(&editor) & (1 << STATE_FOCUSED) == 0 || !display.in_front(window) {
        return Err("focusLost".into());
    }
    type_text(&display, window, text)?;
    thread::sleep(Duration::from_millis(120));
    if !display.in_front(window) || accessibility.states(&editor) & (1 << STATE_FOCUSED) == 0 {
        return Err("typingInterrupted".into());
    }
    let c = &display.connection;
    let enter = {
        let setup = c.setup();
        let map = c
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
            .map_err(|_| "inputRefused")?
            .reply()
            .map_err(|_| "inputRefused")?;
        map.keysyms
            .chunks(usize::from(map.keysyms_per_keycode))
            .position(|syms| syms.first() == Some(&XK_RETURN))
            .map(|i| setup.min_keycode + i as u8)
            .ok_or("inputRefused")?
    };
    for kind in [KEY_PRESS_EVENT, KEY_RELEASE_EVENT] {
        c.xtest_fake_input(kind, enter, 0, display.root, 0, 0, 0)
            .map_err(|_| "inputRefused")?
            .check()
            .map_err(|_| "inputRefused")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms_follow_x11_for_latin1_and_unicode() {
        assert_eq!(keysym('a'), 0x61);
        assert_eq!(keysym('é'), 0xe9);
        assert_eq!(keysym('ł'), 0x0100_0142);
        assert_eq!(keysym('😀'), 0x0101_f600);
    }
}
