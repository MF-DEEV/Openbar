// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! X11: the window manager marks a fullscreen window in `_NET_WM_STATE`, and
//! the active window is asked about it a few times a second.
//! Included by `fullscreen.rs` as its `imp`, so it can reach what the module keeps private.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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

pub fn context<R: Runtime>(window: &WebviewWindow<R>) -> super::AttentionContext {
    let mut context = super::AttentionContext {
        can_auto_present: true,
        fullscreen: false,
        busy: false,
    };
    let (Ok(Some(monitor)), Some(probe)) = (window.current_monitor(), probe()) else {
        return context;
    };
    let (position, size) = (monitor.position(), monitor.size());
    let display = [
        position.x,
        position.y,
        position.x + size.width as i32,
        position.y + size.height as i32,
    ];
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
