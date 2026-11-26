// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! X11: the window manager marks a fullscreen window in `_NET_WM_STATE`, and
//! the active window is asked about it a few times a second. On Wayland the
//! compositor is asked the same (`wm.rs`).
//!
//! The same look at the active window tells whether the person is in an app
//! (busy) or on the desktop, which Windows reads from its foreground window:
//! the island presents itself only after the desktop, or one of Openbar's own
//! windows, has been in front for a moment (#190).
//! Included by `fullscreen.rs` as its `imp`, so it can reach what the module keeps private.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use crate::island;

/// Settings › Behavior › Hide in fullscreen apps.
static HIDES: AtomicBool = AtomicBool::new(true);
static COMPACT_SESSIONS: AtomicBool = AtomicBool::new(false);
static STOPPED: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<super::AttentionContext>> = Mutex::new(None);
static CHANGED: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

struct Atoms {
    active: Atom,
    state: Atom,
    fullscreen: Atom,
    pid: Atom,
    kind: Atom,
    desktop: Atom,
    dock: Atom,
}

struct Probe {
    connection: RustConnection,
    root: Window,
    atoms: Atoms,
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

fn probe() -> Option<&'static Probe> {
    static PROBE: OnceLock<Option<Probe>> = OnceLock::new();
    PROBE
        .get_or_init(|| {
            let (connection, screen) = RustConnection::connect(None).ok()?;
            let root = connection.setup().roots.get(screen)?.root;
            let atoms = Atoms {
                active: atom(&connection, "_NET_ACTIVE_WINDOW")?,
                state: atom(&connection, "_NET_WM_STATE")?,
                fullscreen: atom(&connection, "_NET_WM_STATE_FULLSCREEN")?,
                pid: atom(&connection, "_NET_WM_PID")?,
                kind: atom(&connection, "_NET_WM_WINDOW_TYPE")?,
                desktop: atom(&connection, "_NET_WM_WINDOW_TYPE_DESKTOP")?,
                dock: atom(&connection, "_NET_WM_WINDOW_TYPE_DOCK")?,
            };
            Some(Probe {
                connection,
                root,
                atoms,
            })
        })
        .as_ref()
}

fn values(probe: &Probe, window: Window, property: Atom, kind: AtomEnum) -> Vec<u32> {
    probe
        .connection
        .get_property(false, window, property, kind, 0, 64)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| Some(reply.value32()?.collect()))
        .unwrap_or_default()
}

/// The active window's rectangle on the screen, when it is fullscreen and not ours.
fn fullscreen_bounds(probe: &Probe) -> Option<[i32; 4]> {
    let window = *values(probe, probe.root, probe.atoms.active, AtomEnum::WINDOW).first()?;
    if window == 0 {
        return None;
    }
    let own = values(probe, window, probe.atoms.pid, AtomEnum::CARDINAL)
        .first()
        .is_some_and(|pid| *pid == std::process::id());
    let marked =
        values(probe, window, probe.atoms.state, AtomEnum::ATOM).contains(&probe.atoms.fullscreen);
    if own || !marked {
        return None;
    }
    let size = probe.connection.get_geometry(window).ok()?.reply().ok()?;
    let origin = probe
        .connection
        .translate_coordinates(window, probe.root, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    let (x, y) = (i32::from(origin.dst_x), i32::from(origin.dst_y));
    Some([x, y, x + i32::from(size.width), y + i32::from(size.height)])
}

/// What is in front, as far as the window manager tells.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Front {
    /// The desktop, a panel, one of Openbar's own windows, or no window.
    Desktop,
    /// Another app's window.
    App,
    /// Nobody to ask (a Wayland compositor without an IPC Openbar speaks).
    Unknown,
}

fn x11_front(probe: &Probe) -> Front {
    let Some(&window) = values(probe, probe.root, probe.atoms.active, AtomEnum::WINDOW).first()
    else {
        return Front::Desktop;
    };
    if window == 0 {
        return Front::Desktop;
    }
    let own = values(probe, window, probe.atoms.pid, AtomEnum::CARDINAL)
        .first()
        .is_some_and(|pid| *pid == std::process::id());
    let kinds = values(probe, window, probe.atoms.kind, AtomEnum::ATOM);
    if own || kinds.contains(&probe.atoms.desktop) || kinds.contains(&probe.atoms.dock) {
        Front::Desktop
    } else {
        Front::App
    }
}

fn wayland_front() -> Front {
    let Some(windows) = crate::linux::wm::toplevels() else {
        return Front::Unknown;
    };
    match windows.iter().find(|t| t.focused) {
        Some(t) if t.pid != std::process::id() => Front::App,
        _ => Front::Desktop,
    }
}

/// Since when the desktop has been in front, for the moment it must stay.
static SAFE_SINCE: Mutex<Option<Instant>> = Mutex::new(None);
const SAFE_AFTER: Duration = Duration::from_millis(750);

/// (may the island present itself, is the person busy in an app), as
/// Windows decides it: presenting waits until the desktop has been in front
/// for `SAFE_AFTER`; where nothing tells, the island presents as before.
fn attention(front: Front, since: &mut Option<Instant>, now: Instant) -> (bool, bool) {
    match front {
        Front::Desktop => {
            let since = since.get_or_insert(now);
            (now.saturating_duration_since(*since) >= SAFE_AFTER, false)
        }
        Front::App => {
            *since = None;
            (false, true)
        }
        Front::Unknown => {
            *since = None;
            (true, false)
        }
    }
}

/// The focused window's rectangle in physical pixels, when it is fullscreen,
/// as the compositor tells it on Wayland.
fn wayland_fullscreen_bounds(scale: f64) -> Option<[i32; 4]> {
    let active = crate::linux::wm::active()?;
    if !active.fullscreen || active.pid == std::process::id() {
        return None;
    }
    Some(active.rect.map(|v| (f64::from(v) * scale).round() as i32))
}

pub fn context<R: Runtime>(window: &WebviewWindow<R>) -> super::AttentionContext {
    let mut context = super::AttentionContext {
        can_auto_present: true,
        fullscreen: false,
        busy: false,
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return context;
    };
    let (position, size) = (monitor.position(), monitor.size());
    let display = [
        position.x,
        position.y,
        position.x + size.width as i32,
        position.y + size.height as i32,
    ];
    let mut apply_front = |front: Front| {
        let (allowed, busy) = attention(front, &mut SAFE_SINCE.lock().unwrap(), Instant::now());
        context.can_auto_present = allowed;
        context.busy = busy;
    };
    if crate::linux::wm::available() {
        apply_front(wayland_front());
        if let Some(bounds) = wayland_fullscreen_bounds(monitor.scale_factor()) {
            context.fullscreen = super::fullscreen_candidate(true, bounds, display);
        }
        return context;
    }
    let Some(probe) = probe() else {
        return context;
    };
    apply_front(x11_front(probe));
    if let Some(bounds) = fullscreen_bounds(probe) {
        context.fullscreen = super::fullscreen_candidate(true, bounds, display);
    }
    context
}

pub fn hides() -> bool {
    HIDES.load(Ordering::Relaxed)
}

pub fn watch<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = CHANGED.set(Box::new(move || {
        let Some(window) = handle.get_webview_window(island::LABEL) else {
            return;
        };
        let context = context(&window);
        let mut last = LAST.lock().unwrap();
        if *last != Some(context) {
            *last = Some(context);
            let _ = handle.emit("island://attention-context", context);
        }
        drop(last);
        // A person who keeps sessions compact wants the waiting indicator even over a game.
        let hide = HIDES.load(Ordering::Relaxed)
            && context.fullscreen
            && !COMPACT_SESSIONS.load(Ordering::Relaxed);
        if window.is_visible().unwrap_or(!hide) == hide {
            let _ = if hide { window.hide() } else { window.show() };
            let _ = handle.emit("island://fullscreen", hide);
        }
    }));
    let handle = app.clone();
    std::thread::spawn(move || {
        while !STOPPED.load(Ordering::Relaxed) {
            // The window manager is asked off the main thread; only the show/hide is not.
            let _ = handle.run_on_main_thread(|| {
                if let Some(changed) = CHANGED.get() {
                    changed();
                }
            });
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

pub fn set_compact(on: bool) {
    if COMPACT_SESSIONS.swap(on, Ordering::Relaxed) != on
        && let Some(changed) = CHANGED.get()
    {
        changed();
    }
}

pub fn set_hides(hides: bool) {
    if HIDES.swap(hides, Ordering::Relaxed) != hides
        && let Some(changed) = CHANGED.get()
    {
        changed();
    }
}

pub fn stop<R: Runtime>(_app: &AppHandle<R>) {
    STOPPED.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_island_presents_once_the_desktop_has_been_in_front_a_moment() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut since = None;
        assert_eq!(attention(Front::Desktop, &mut since, at(0)), (false, false));
        assert_eq!(attention(Front::Desktop, &mut since, at(500)), (false, false));
        assert_eq!(attention(Front::Desktop, &mut since, at(750)), (true, false));
        // An app in front: busy, and the wait starts over.
        assert_eq!(attention(Front::App, &mut since, at(800)), (false, true));
        assert_eq!(attention(Front::Desktop, &mut since, at(900)), (false, false));
        assert_eq!(attention(Front::Desktop, &mut since, at(1_700)), (true, false));
        // Nobody to ask: as before, never busy.
        assert_eq!(attention(Front::Unknown, &mut since, at(1_800)), (true, false));
        assert_eq!(since, None);
    }
}
