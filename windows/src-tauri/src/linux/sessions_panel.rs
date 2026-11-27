// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The sessions' tray panel on Linux (#331). On X11 it is a window placed by
//! the tray's corner like any other. A Wayland window cannot place itself,
//! so where the compositor speaks layer-shell the panel becomes a surface in
//! the corner by the bar instead, which takes the keyboard when clicked.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Runtime, WebviewWindow};

use crate::linux::layer;

/// The panel is a layer surface: the compositor places it, and the window's
/// own size request sizes it.
static LAYERED: AtomicBool = AtomicBool::new(false);

/// Makes the panel a layer surface the first time it shows on Wayland,
/// anchored to the bar's corner: the top right, or the bottom right under
/// KDE, whose panel is at the bottom. Nothing to do on X11. Done on GTK's
/// thread and waited for, so the window is a layer surface before it maps:
/// made one afterwards, it would be taken down and lose the keyboard.
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

/// Whether the panel is a layer surface, which keeps one size.
pub fn layered() -> bool {
    LAYERED.load(Ordering::Relaxed)
}

/// The window's size on X11, in logical pixels, set on GTK's thread. GTK
/// keeps a window that can't be resized by hand at least as large as its
/// default size, so that follows the panel's size, letting it shrink to a
/// short list.
pub fn set_size<R: Runtime>(window: &WebviewWindow<R>, width: f64, height: f64) {
    if layered() {
        return;
    }
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(native) = target.gtk_window() else {
            return;
        };
        use gtk::prelude::GtkWindowExt;
        let (width, height) = (width.round() as i32, height.round() as i32);
        native.set_default_size(width, height);
        native.resize(width, height);
    });
}

/// The layer surface's one size, `room`, and the panel's rectangle in it,
/// in logical pixels: the right edge, at the top or the bottom. A surface
/// held by two edges takes the size its window asks for, and a web view on
/// its own asks for next to nothing, so the window asks for the room. Only
/// the panel takes the pointer; the rest of the room lets it through.
pub fn set_frame<R: Runtime>(
    window: &WebviewWindow<R>,
    room: (f64, f64),
    frame: &crate::sessions_panel::Frame,
) {
    let (room_width, room_height) = (room.0.round() as i32, room.1.round() as i32);
    let (x, y, width, height) = frame_rect((room_width, room_height), frame);
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(native) = target.gtk_window() else {
            return;
        };
        use gtk::prelude::{GtkWindowExt, WidgetExt};
        let (now_width, now_height) = native.size_request();
        if (now_width, now_height) != (room_width, room_height) {
            native.set_default_size(room_width, room_height);
            native.set_size_request(room_width, room_height);
            native.resize(room_width, room_height);
        }
        let rect = gtk::cairo::RectangleInt::new(x, y, width, height);
        native.input_shape_combine_region(Some(&gtk::cairo::Region::create_rectangle(&rect)));
    });
}

/// The panel's rectangle (x, y, width, height) in a room of `room`.
pub fn frame_rect(room: (i32, i32), frame: &crate::sessions_panel::Frame) -> (i32, i32, i32, i32) {
    let width = (frame.width.round() as i32).clamp(0, room.0);
    let height = (frame.height.round() as i32).clamp(0, room.1);
    let y = if frame.bottom { room.1 - height } else { 0 };
    (room.0 - width, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::frame_rect;
    use crate::sessions_panel::Frame;

    #[test]
    fn the_panel_hangs_in_the_rooms_corner_by_the_bar() {
        let top = Frame {
            width: 460.0,
            height: 228.0,
            bottom: false,
        };
        assert_eq!(frame_rect((720, 640), &top), (260, 0, 460, 228));
        let bottom = Frame {
            width: 460.0,
            height: 228.0,
            bottom: true,
        };
        assert_eq!(frame_rect((720, 640), &bottom), (260, 412, 460, 228));
        let enlarged = Frame {
            width: 720.0,
            height: 640.0,
            bottom: false,
        };
        assert_eq!(frame_rect((720, 640), &enlarged), (0, 0, 720, 640));
        // Never past the room on a small screen.
        let large = Frame {
            width: 800.0,
            height: 700.0,
            bottom: true,
        };
        assert_eq!(frame_rect((700, 500), &large), (0, 0, 700, 500));
    }
}
