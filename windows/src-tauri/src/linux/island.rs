// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The island window on X11: a dock-type GTK window that is cut to the drawn island with XShape.
//! Included by `island.rs` as its `platform`, so it can reach the private state of the window.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use gtk::gdk::{Screen, WindowTypeHint};
use gtk::prelude::{GtkWindowExt, WidgetExt};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{Runtime, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, KeyButMask};
use x11rb::rust_connection::RustConnection;

use super::{HitRect, IslandState};

static DISPLAY_CHANGES: AtomicU32 = AtomicU32::new(0);

struct X11 {
    connection: RustConnection,
    root: u32,
    active_window: u32,
}

/// One shared connection to the X server; None on a session without one.
fn x11() -> Option<&'static X11> {
    static CONNECTION: OnceLock<Option<X11>> = OnceLock::new();
    CONNECTION
        .get_or_init(|| {
            let (connection, screen) = RustConnection::connect(None).ok()?;
            let root = connection.setup().roots.get(screen)?.root;
            let active_window = connection
                .intern_atom(false, b"_NET_ACTIVE_WINDOW")
                .ok()?
                .reply()
                .ok()?
                .atom;
            Some(X11 {
                connection,
                root,
                active_window,
            })
        })
        .as_ref()
}

/// The window the window manager calls active, 0 for none.
fn active_window(x: &X11) -> u32 {
    x.connection
        .get_property(false, x.root, x.active_window, AtomEnum::WINDOW, 0, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32()?.next())
        .unwrap_or(0)
}

fn activate(x: &X11, window: u32) {
    let event = ClientMessageEvent::new(32, window, x.active_window, [2, 0, 0, 0, 0]);
    let _ = x.connection.send_event(
        false,
        x.root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    let _ = x.connection.flush();
}

fn own_window<R: Runtime>(window: &WebviewWindow<R>) -> Option<u32> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
        _ => None,
    }
}

pub fn display_changes() -> u32 {
    DISPLAY_CHANGES.load(Ordering::Relaxed)
}

/// GTK is only safe on the main thread, and the mirrors are styled from the watcher.
fn on_main_thread<R: Runtime>(
    window: &WebviewWindow<R>,
    work: impl FnOnce(gtk::ApplicationWindow) + Send + 'static,
) {
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        if let Ok(native) = target.gtk_window() {
            work(native);
        }
    });
}

/// Monitors plugged in or out, a new resolution: counted for the pointer watcher.
pub fn watch_displays<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.run_on_main_thread(|| {
        let Some(screen) = Screen::default() else {
            return;
        };
        screen.connect_monitors_changed(|_| {
            DISPLAY_CHANGES.fetch_add(1, Ordering::Relaxed);
        });
        screen.connect_size_changed(|_| {
            DISPLAY_CHANGES.fetch_add(1, Ordering::Relaxed);
        });
    });
}

/// Mouse input passes through the window. Tao would do this on the GDK
/// window, which does not exist before the first show, and would leave
/// one pixel in the corner clickable; an empty input shape on the widget
/// waits for the window and covers all of it.
pub fn set_click_through<R: Runtime>(window: &WebviewWindow<R>, ignore: bool) -> tauri::Result<()> {
    on_main_thread(window, move |native| {
        let nothing = gtk::cairo::Region::create();
        native.input_shape_combine_region(ignore.then_some(&nothing));
    });
    Ok(())
}

/// A dock-type window: no frame, on every workspace, not in the taskbar,
/// and clicking the closed island does not take the keyboard. It has no
/// shape until the page says what is drawn, so nothing of it catches the pointer.
pub fn style<R: Runtime>(window: &WebviewWindow<R>) {
    on_main_thread(window, |native| {
        native.set_type_hint(WindowTypeHint::Dock);
        native.set_accept_focus(false);
        native.set_focus_on_map(false);
        native.shape_combine_region(Some(&gtk::cairo::Region::create()));
    });
}

/// Room around a drawn shape for its outline and anti-aliased edge.
const ROOM: f64 = 2.0;

/// How far a corner of radius `r` is cut in, `depth` from its centre line.
fn cut(r: f64, depth: f64) -> f64 {
    r - (r * r - depth * depth).max(0.0).sqrt()
}

/// The rects as `(x, y, width, height)` strips, rounded corners included.
fn shape_rows(rects: &[HitRect]) -> Vec<(i32, i32, i32, i32)> {
    let mut strips = Vec::new();
    for rect in rects {
        if !(rect.width > 0.0 && rect.height > 0.0) {
            continue;
        }
        let (x, y) = (rect.x - ROOM, rect.y - ROOM);
        let (w, h) = (rect.width + 2.0 * ROOM, rect.height + 2.0 * ROOM);
        let limit = (w / 2.0).min(h / 2.0);
        let top = (rect.top_radius + ROOM).clamp(0.0, limit);
        let bottom = (rect.bottom_radius + ROOM).clamp(0.0, limit);
        let (first, last) = (y.floor() as i32, (y + h).ceil() as i32);
        // Rows of the same extent are one strip.
        let mut run: Option<(i32, i32, i32)> = None;
        for row in first..last {
            let py = f64::from(row) + 0.5;
            let inset = if py < y + top {
                cut(top, y + top - py)
            } else if py > y + h - bottom {
                cut(bottom, py - (y + h - bottom))
            } else {
                0.0
            };
            let (left, right) = ((x + inset).floor() as i32, (x + w - inset).ceil() as i32);
            match run {
                Some((a, b, _)) if (a, b) == (left, right) => {}
                Some((a, b, start)) => {
                    strips.push((a, start, b - a, row - start));
                    run = Some((left, right, row));
                }
                None => run = Some((left, right, row)),
            }
        }
        if let Some((a, b, start)) = run {
            strips.push((a, start, b - a, last - start));
        }
    }
    strips
}

/// The window exists only where the island is drawn: the window manager
/// gives its frame the same shape, so a click anywhere else reaches the
/// window below. While the capture overlay is up it is the whole canvas.
pub fn set_shape<R: Runtime>(window: &WebviewWindow<R>, rects: &[HitRect], capture: bool) {
    let strips = shape_rows(rects);
    on_main_thread(window, move |native| {
        if capture {
            native.shape_combine_region(None);
            return;
        }
        let strips: Vec<_> = strips
            .iter()
            .map(|&(x, y, w, h)| gtk::cairo::RectangleInt::new(x, y, w, h))
            .collect();
        native.shape_combine_region(Some(&gtk::cairo::Region::create_rectangles(&strips)));
    });
}

pub fn show_in_captures<R: Runtime>(window: &WebviewWindow<R>, show: bool) {
    let _ = window.set_content_protected(!show);
}

pub fn set_focusable<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &IslandState,
    focusable: bool,
    take: bool,
) {
    on_main_thread(window, move |native| native.set_accept_focus(focusable));
    let (Some(x), Some(own)) = (x11(), own_window(window)) else {
        if focusable && take {
            let _ = window.set_focus();
        }
        return;
    };
    let active = active_window(x);
    if focusable && take {
        if active != own {
            *state.previous_foreground.lock().unwrap() = active as isize;
        }
        let _ = window.set_focus();
    } else if !focusable {
        let previous = std::mem::take(&mut *state.previous_foreground.lock().unwrap()) as u32;
        if active == own && previous != 0 {
            activate(x, previous);
        }
    }
}

pub fn mouse_buttons_down() -> bool {
    let Some(x) = x11() else { return false };
    let buttons = KeyButMask::BUTTON1 | KeyButMask::BUTTON2 | KeyButMask::BUTTON3;
    x.connection
        .query_pointer(x.root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|pointer| u16::from(pointer.mask) & u16::from(buttons) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64, top: f64, bottom: f64) -> HitRect {
        HitRect {
            x,
            y,
            width,
            height,
            top_radius: top,
            bottom_radius: bottom,
        }
    }

    fn covers(strips: &[(i32, i32, i32, i32)], x: i32, y: i32) -> bool {
        strips
            .iter()
            .any(|&(sx, sy, w, h)| x >= sx && x < sx + w && y >= sy && y < sy + h)
    }

    #[test]
    fn a_square_rect_is_one_strip_with_room_around_it() {
        let strips = shape_rows(&[rect(100.0, 0.0, 200.0, 40.0, 0.0, 0.0)]);
        assert_eq!(strips, vec![(98, -2, 204, 44)]);
    }

    #[test]
    fn rounded_corners_cut_the_strips_in() {
        let strips = shape_rows(&[rect(100.0, 0.0, 200.0, 40.0, 0.0, 16.0)]);
        assert!(strips.len() > 2, "the bottom corners need their own rows");
        assert!(
            covers(&strips, 200, 39),
            "the middle of the bottom edge is drawn"
        );
        assert!(!covers(&strips, 101, 39), "the bottom corner is cut away");
        assert!(covers(&strips, 101, 5), "the top corners are square");
        for &(x, _, w, _) in &strips {
            assert!(
                x >= 98 && x + w <= 302,
                "never wider than the rect with its room"
            );
        }
    }

    #[test]
    fn the_corners_are_symmetrical() {
        let strips = shape_rows(&[rect(100.0, 0.0, 200.0, 40.0, 12.0, 12.0)]);
        for y in -2..42 {
            for dx in 0..30 {
                assert_eq!(
                    covers(&strips, 98 + dx, y),
                    covers(&strips, 301 - dx, y),
                    "row {y}, {dx} in from the edge"
                );
            }
        }
    }

    #[test]
    fn nothing_is_drawn_for_an_empty_rect_and_rects_add_up() {
        assert!(shape_rows(&[rect(10.0, 10.0, 0.0, 30.0, 0.0, 0.0)]).is_empty());
        let two = shape_rows(&[
            rect(100.0, 0.0, 50.0, 20.0, 0.0, 0.0),
            rect(400.0, 100.0, 50.0, 20.0, 0.0, 0.0),
        ]);
        assert!(covers(&two, 120, 10) && covers(&two, 420, 110));
        assert!(!covers(&two, 250, 60), "nothing between them");
    }

    #[test]
    fn radii_larger_than_the_rect_are_held_to_its_size() {
        let strips = shape_rows(&[rect(0.0, 0.0, 20.0, 20.0, 500.0, 500.0)]);
        assert!(covers(&strips, 10, 10));
        assert!(!covers(&strips, 0, 0), "a circle leaves the corners out");
    }
}
