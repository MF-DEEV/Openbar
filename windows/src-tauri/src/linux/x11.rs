// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Brings a terminal or editor window forward on X11: the window manager lists
//! windows with the pid of the program that owns each, and is asked to
//! activate one the way a taskbar does.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub window: Window,
    pub pid: u32,
    pub title: String,
}

/// The window to raise: of the program nearest the session in `pids`, then
/// one whose title names the folder, then the topmost. `windows` runs from
/// the bottom of the stack to the top.
pub fn pick(windows: &[Candidate], pids: &[u32], cwd: &str) -> Option<Window> {
    let folder = cwd
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|f| !f.is_empty());
    pids.iter().find_map(|pid| {
        let own: Vec<_> = windows.iter().filter(|w| w.pid == *pid).collect();
        own.iter()
            .rev()
            .find(|w| folder.is_some_and(|f| w.title.contains(f)))
            .or_else(|| own.last())
            .map(|w| w.window)
    })
}

fn atom(connection: &RustConnection, name: &str) -> Option<Atom> {
    Some(
        connection
            .intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()?
            .atom,
    )
}

fn cardinals(
    connection: &RustConnection,
    window: Window,
    property: Atom,
    kind: impl Into<Atom>,
) -> Vec<u32> {
    connection
        .get_property(false, window, property, kind, 0, 4096)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| Some(reply.value32()?.collect()))
        .unwrap_or_default()
}

fn title(connection: &RustConnection, window: Window, name: Atom, utf8: Atom) -> String {
    connection
        .get_property(false, window, name, utf8, 0, 1024)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
        .unwrap_or_default()
}

/// Asks the window manager to activate `window`, as a taskbar does.
fn activate(connection: &RustConnection, root: Window, active: Atom, window: Window) -> bool {
    // Source 2 says a pager asked, which window managers honour without a user timestamp.
    let event = ClientMessageEvent::new(32, window, active, [2, 0, 0, 0, 0]);
    let sent = connection.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    // `check` waits for the server to answer, so the request is handled before the connection closes.
    sent.is_ok_and(|cookie| cookie.check().is_ok())
}

/// Raises the window of the nearest program in `pids` that has one.
pub fn raise(pids: &[u32], cwd: &str) -> bool {
    let Ok((connection, screen)) = RustConnection::connect(None) else {
        return false;
    };
    let Some(root) = connection.setup().roots.get(screen).map(|s| s.root) else {
        return false;
    };
    let (Some(stacking), Some(pid), Some(name), Some(utf8), Some(active)) = (
        atom(&connection, "_NET_CLIENT_LIST_STACKING"),
        atom(&connection, "_NET_WM_PID"),
        atom(&connection, "_NET_WM_NAME"),
        atom(&connection, "UTF8_STRING"),
        atom(&connection, "_NET_ACTIVE_WINDOW"),
    ) else {
        return false;
    };
    let windows: Vec<Candidate> = cardinals(&connection, root, stacking, AtomEnum::WINDOW)
        .into_iter()
        .filter_map(|window| {
            let pid = *cardinals(&connection, window, pid, AtomEnum::CARDINAL).first()?;
            Some(Candidate {
                window,
                pid,
                title: title(&connection, window, name, utf8),
            })
        })
        .collect();
    let Some(window) = pick(&windows, pids, cwd) else {
        return false;
    };
    activate(&connection, root, active, window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(window: Window, pid: u32, title: &str) -> Candidate {
        Candidate {
            window,
            pid,
            title: title.into(),
        }
    }

    #[test]
    fn the_nearest_program_wins_and_the_topmost_of_its_windows() {
        let windows = [w(1, 10, "a"), w(2, 20, "b"), w(3, 20, "c")];
        assert_eq!(pick(&windows, &[30, 20, 10], "/x/app"), Some(3));
        assert_eq!(pick(&windows, &[10, 20], "/x/app"), Some(1));
        assert_eq!(pick(&windows, &[99], "/x/app"), None);
    }

    #[test]
    fn a_title_naming_the_folder_beats_the_stack() {
        let windows = [w(1, 20, "~/work/app"), w(2, 20, "~/other")];
        assert_eq!(pick(&windows, &[20], "/home/me/work/app/"), Some(1));
    }

    /// By hand on a live X server: OPENBAR_RAISE_PIDS=12,34 cargo test live_raise -- --ignored
    #[test]
    #[ignore]
    fn live_raise() {
        let pids: Vec<u32> = std::env::var("OPENBAR_RAISE_PIDS")
            .unwrap()
            .split(',')
            .filter_map(|p| p.parse().ok())
            .collect();
        assert!(raise(&pids, "/x/app"));
    }
}
