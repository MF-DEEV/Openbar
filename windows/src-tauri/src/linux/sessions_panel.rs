// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The sessions' tray panel on Linux (#331). On X11 it is a window placed by
//! the tray's corner like any other. A Wayland window cannot place itself,
//! so where the compositor speaks layer-shell the panel becomes a surface in
//! the corner by the bar instead, which takes the keyboard when clicked.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Runtime, WebviewWindow};

use crate::linux::layer;

/// Makes the panel a layer surface the first time it shows on Wayland,
/// anchored to the bar's corner: the top right, or the bottom right under
/// KDE, whose panel is at the bottom. Nothing to do on X11. Done on GTK's
/// thread and waited for, so the window is a layer surface before it maps:
/// made one afterwards, it would be taken down and lose the keyboard.
/// The panel is a layer surface, so the compositor places it and the
/// window's own size request sizes it.
static LAYERED: AtomicBool = AtomicBool::new(false);

pub fn prepare<R: Runtime>(window: &WebviewWindow<R>, top: bool) {
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.load(Ordering::Relaxed) {
        return;
    }
    let target = window.clone();
    let (done, wait) = std::sync::mpsc::channel();
    let queued = window.run_on_main_thread(move || {
        let made = make_layer(&target, top);
        if made {
            DONE.store(true, Ordering::Relaxed);
            LAYERED.store(true, Ordering::Relaxed);
        }
        let _ = done.send(made);
    });
    if queued.is_ok() {
        let made = wait.recv_timeout(std::time::Duration::from_secs(2));
        crate::diagnostics::log::info("sessions-panel", &format!("layer surface: {made:?}"));
    }
}

fn make_layer<R: Runtime>(window: &WebviewWindow<R>, top: bool) -> bool {
    if !layer::available() {
        return false;
    }
    let Ok(native) = window.gtk_window() else {
        return false;
    };
    use gtk::prelude::Cast;
    let gtk_window: &gtk::Window = native.upcast_ref();
    let edge = if top {
        layer::Edge::Top
    } else {
        layer::Edge::Bottom
    };
    if !layer::init(
        gtk_window,
        c"openbar-sessions",
        layer::Layer::Top,
        &[edge, layer::Edge::Right],
    ) {
        return false;
    }
    // Beside the bar, not under it: other panels' room is kept.
    layer::set_exclusive_zone(gtk_window, 0);
    layer::set_margin(gtk_window, edge, 8);
    layer::set_margin(gtk_window, layer::Edge::Right, 8);
    layer::set_keyboard(gtk_window, layer::Keyboard::OnDemand);
    true
}

/// The panel's size as a layer surface, in logical pixels. A surface held by
/// two edges takes the size its window asks for, and a web view on its own
/// asks for next to nothing, so the window asks for the panel's size.
pub fn set_size<R: Runtime>(window: &WebviewWindow<R>, width: f64, height: f64) {
    if !LAYERED.load(Ordering::Relaxed) {
        return;
    }
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(native) = target.gtk_window() else {
            return;
        };
        use gtk::prelude::{GtkWindowExt, WidgetExt};
        let (width, height) = (width.round() as i32, height.round() as i32);
        // The request is a floor only; the resize lets it shrink to a short list.
        native.set_size_request(width, height);
        native.resize(width, height);
    });
}
