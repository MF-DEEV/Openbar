// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The screen on X11, for the capture tools where Windows has DXGI and GDI:
//! the picture of an area (`GetImage` of the root window, which has what every
//! window and the compositor drew), the display under the pointer (RandR),
//! and the window under a point (the window manager's stacking list).

use tauri::{Runtime, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _, ImageFormat, MapState, Window};
use x11rb::rust_connection::RustConnection;

use crate::capture::Rect;
use crate::capture::snapshot::Frame;

/// Rows asked for at a time, so no one reply is more than a few megabytes.
const ROWS: u16 = 256;

fn connect() -> Option<(RustConnection, Window)> {
    let (connection, screen) = RustConnection::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    Some((connection, root))
}

/// BGRX pixels in rows of `stride` bytes, as the server's usual 24-bit
/// TrueColor visual has them, as tight opaque RGBA.
fn rgba_from_bgrx(data: &[u8], width: usize, rows: usize, stride: usize, out: &mut Vec<u8>) {
    for y in 0..rows {
        for px in data[y * stride..y * stride + width * 4].as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
}

/// The picture of `rect` (desktop pixels), or None where the server will not
/// give it (the area is off the screen, or its pixels are not 32 bits).
pub fn grab(rect: Rect) -> Option<Frame> {
    let (connection, root) = connect()?;
    let setup = connection.setup();
    let depth_ok = setup
        .pixmap_formats
        .iter()
        .any(|f| f.depth == 24 && f.bits_per_pixel == 32);
    let little = setup.image_byte_order == xproto::ImageOrder::LSB_FIRST;
    if !depth_ok || !little || rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    let (width, height) = (rect.width as usize, rect.height as usize);
    let mut rgba = Vec::with_capacity(width * height * 4);
    let mut y = 0usize;
    while y < height {
        let rows = (height - y).min(usize::from(ROWS));
        let reply = connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                root,
                i16::try_from(rect.x).ok()?,
                i16::try_from(rect.y + y as i32).ok()?,
                u16::try_from(width).ok()?,
                u16::try_from(rows).ok()?,
                !0,
            )
            .ok()?
            .reply()
            .ok()?;
        let stride = reply.data.len() / rows;
        if stride < width * 4 {
            return None;
        }
        rgba_from_bgrx(&reply.data, width, rows, stride, &mut rgba);
        y += rows;
    }
    Some(Frame {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// The display under the pointer: a number for it, and where it is in the desktop.
pub fn monitor_under_cursor() -> Option<(isize, Rect)> {
    let (connection, root) = connect()?;
    let pointer = connection.query_pointer(root).ok()?.reply().ok()?;
    let (x, y) = (i32::from(pointer.root_x), i32::from(pointer.root_y));
    let monitors = connection
        .randr_get_monitors(root, true)
        .ok()?
        .reply()
        .ok()?;
    let rects: Vec<Rect> = monitors
        .monitors
        .iter()
        .map(|m| Rect {
            x: i32::from(m.x),
            y: i32::from(m.y),
            width: i32::from(m.width),
            height: i32::from(m.height),
        })
        .collect();
    let at = containing(&rects, x, y)?;
    Some((at as isize, rects[at]))
}

/// Which rect holds the point; the nearest one when none does.
fn containing(rects: &[Rect], x: i32, y: i32) -> Option<usize> {
    let inside = |r: &Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
    rects.iter().position(inside).or_else(|| {
        let distance = |r: &Rect| {
            let dx = (r.x - x).max(0).max(x - (r.x + r.width - 1)).abs();
            let dy = (r.y - y).max(0).max(y - (r.y + r.height - 1)).abs();
            i64::from(dx).pow(2) + i64::from(dy).pow(2)
        };
        (0..rects.len()).min_by_key(|i| distance(&rects[*i]))
    })
}

/// The topmost window at the point that is not ours or the desktop's own
/// (a panel, the desktop window): 0 when there is none.
pub fn window_at(x: i32, y: i32) -> isize {
    window_at_inner(x, y).unwrap_or(0)
}

fn window_at_inner(x: i32, y: i32) -> Option<isize> {
    let (connection, root) = connect()?;
    let atom = |name: &str| -> Option<xproto::Atom> {
        Some(
            connection
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    };
    let stacking = atom("_NET_CLIENT_LIST_STACKING")?;
    let list: Vec<Window> = connection
        .get_property(false, root, stacking, AtomEnum::WINDOW, 0, 4096)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .collect();
    let (pid_atom, state_atom, hidden, kind_atom) = (
        atom("_NET_WM_PID")?,
        atom("_NET_WM_STATE")?,
        atom("_NET_WM_STATE_HIDDEN")?,
        atom("_NET_WM_WINDOW_TYPE")?,
    );
    let (desktop, dock) = (
        atom("_NET_WM_WINDOW_TYPE_DESKTOP")?,
        atom("_NET_WM_WINDOW_TYPE_DOCK")?,
    );
    let ours = std::process::id();
    // The list runs from the bottom of the stack to the top.
    for window in list.into_iter().rev() {
        let atoms = |property: xproto::Atom| -> Vec<u32> {
            connection
                .get_property(false, window, property, AtomEnum::ATOM, 0, 32)
                .ok()
                .and_then(|c| c.reply().ok())
                .and_then(|r| Some(r.value32()?.collect()))
                .unwrap_or_default()
        };
        if atoms(state_atom).contains(&hidden) {
            continue;
        }
        let kinds = atoms(kind_atom);
        if kinds.contains(&desktop) || kinds.contains(&dock) {
            continue;
        }
        let pid = connection
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32()?.next());
        if pid == Some(ours) {
            continue;
        }
        let attributes = connection
            .get_window_attributes(window)
            .ok()?
            .reply()
            .ok()?;
        if attributes.map_state != MapState::VIEWABLE {
            continue;
        }
        let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
        // The window's own corner on the desktop, whatever its frame is.
        let corner = connection
            .translate_coordinates(window, root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let (left, top) = (i32::from(corner.dst_x), i32::from(corner.dst_y));
        if x >= left
            && x < left + i32::from(geometry.width)
            && y >= top
            && y < top + i32::from(geometry.height)
        {
            return Some(window as isize);
        }
    }
    Some(0)
}

/// The overlay asks for the keyboard when it comes up.
pub fn take_foreground<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.set_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_opaque_rgba_without_the_rows_padding() {
        // Two pixels a row, rows 12 bytes apart (4 of padding).
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, 99, 99, 99, 99, //
            7, 8, 9, 0, 10, 11, 12, 0, 99, 99, 99, 99,
        ];
        let mut out = Vec::new();
        rgba_from_bgrx(&data, 2, 2, 12, &mut out);
        assert_eq!(
            out,
            [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
        );
    }

    #[test]
    fn the_display_under_the_pointer_is_the_one_holding_it_else_the_nearest() {
        let rects = [
            Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                width: 1280,
                height: 1024,
            },
        ];
        assert_eq!(containing(&rects, 10, 10), Some(0));
        assert_eq!(containing(&rects, 1920, 500), Some(1));
        assert_eq!(containing(&rects, 3199, 1023), Some(1));
        // Off the screen: the nearest.
        assert_eq!(containing(&rects, 5000, 5), Some(1));
        assert_eq!(containing(&rects, -50, 2000), Some(0));
        assert_eq!(containing(&[], 0, 0), None);
    }

    /// A window of `color` where this test can see it, on a nested X server.
    #[test]
    #[ignore = "needs an X server to draw on: use a nested one such as Xephyr on :97"]
    fn the_screen_of_a_nested_x_server_is_grabbed_and_its_windows_found() {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(!display.is_empty(), "no DISPLAY");
        let (connection, root) = connect().expect("an X server");
        let screen = &connection.setup().roots[0];
        let (w, h) = (
            usize::from(screen.width_in_pixels),
            usize::from(screen.height_in_pixels),
        );
        // A window of one color, in a corner, as a client would draw it.
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                root,
                20,
                30,
                100,
                60,
                0,
                xproto::WindowClass::INPUT_OUTPUT,
                0,
                &xproto::CreateWindowAux::new().background_pixel(0x00_33_66_99),
            )
            .unwrap();
        connection.map_window(window).unwrap();
        connection.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let frame = grab(Rect {
            x: 0,
            y: 0,
            width: w as i32,
            height: h as i32,
        })
        .expect("a picture");
        assert_eq!((frame.width as usize, frame.height as usize), (w, h));
        // Inside the window: its color (0x336699 as red, green, blue).
        assert_eq!(frame.pixel(60, 50), Some([0x33, 0x66, 0x99]));
        // A crop through the same call agrees with the whole.
        let part = grab(Rect {
            x: 50,
            y: 40,
            width: 20,
            height: 10,
        })
        .unwrap();
        assert_eq!(part.pixel(0, 0), Some([0x33, 0x66, 0x99]));
        assert!(
            grab(Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 5
            })
            .is_none()
        );
        let (index, monitor) = monitor_under_cursor().expect("a display");
        println!("display {index}: {monitor:?}");
        assert!(monitor.width > 0 && monitor.height > 0);
        // Without a window manager the stacking list is not there: no window, not a crash.
        assert_eq!(window_at(60, 50), 0);
    }

    #[test]
    #[ignore = "reads a few pixels and the pointer's display from DISPLAY (draws nothing)"]
    fn a_few_pixels_and_the_display_are_read_from_the_screen_in_use() {
        let frame = grab(Rect {
            x: 0,
            y: 0,
            width: 6,
            height: 4,
        })
        .expect("a picture");
        assert_eq!((frame.width, frame.height), (6, 4));
        assert_eq!(frame.rgba.len(), 6 * 4 * 4);
        assert!(frame.rgba.chunks(4).all(|p| p[3] == 255));
        println!("top left pixel: {:?}", frame.pixel(0, 0));
        println!("display under the pointer: {:?}", monitor_under_cursor());
    }
}
