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
/// KDE, whose panel is at the bottom. Nothing to do on X11.
pub fn prepare<R: Runtime>(window: &WebviewWindow<R>, top: bool) {
    static DONE: AtomicBool = AtomicBool::new(false);
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        if DONE.load(Ordering::Relaxed) || !layer::available() {
            return;
        }
        let Ok(native) = target.gtk_window() else {
            return;
        };
        use gtk::prelude::Cast;
        let gtk_window: &gtk::Window = native.upcast_ref();
        let edge = if top {
            layer::Edge::Top
        } else {
            layer::Edge::Bottom
        };
        if layer::init(
            gtk_window,
            c"openbar-sessions",
            layer::Layer::Top,
            &[edge, layer::Edge::Right],
        ) {
            DONE.store(true, Ordering::Relaxed);
            // Beside the bar, not under it: other panels' room is kept.
            layer::set_exclusive_zone(gtk_window, 0);
            layer::set_margin(gtk_window, edge, 8);
            layer::set_margin(gtk_window, layer::Edge::Right, 8);
            layer::set_keyboard(gtk_window, layer::Keyboard::OnDemand);
        }
    });
}
