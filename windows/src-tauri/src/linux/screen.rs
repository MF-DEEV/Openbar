// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The screen on X11, for the capture tools where Windows has DXGI and GDI:
//! the picture of an area (`GetImage` of the root window, which has what every
//! window and the compositor drew), the display under the pointer (RandR),
//! and the window under a point (the window manager's stacking list).
//!
//! On Wayland no app may read the screen by itself: the picture comes from
//! `grim` (the screencopy protocol of sway, Hyprland and other wlroots
//! compositors) or, on KDE, Spectacle, the display is the one the compositor has focused, and the
//! selection overlay is a layer surface over it.

use tauri::{Runtime, WebviewWindow};
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _, ImageFormat, MapState, Window};
use x11rb::rust_connection::RustConnection;

use crate::capture::Rect;
use crate::capture::snapshot::Frame;

/// Rows asked for at a time, so no one reply is more than a few megabytes.
const ROWS: u16 = 256;

pub fn connect() -> Option<(RustConnection, Window)> {
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
    if crate::linux::wm::native_wayland() {
        return grab_wayland(rect);
    }
    let (connection, root) = connect()?;
    grab_from(&connection, root, rect)
}

/// The picture of `rect` of `drawable` (a window or a pixmap, `rect` from its
/// top-left corner), as `grab` has it.
pub fn grab_from(connection: &RustConnection, drawable: u32, rect: Rect) -> Option<Frame> {
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
                drawable,
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

/// A binary PPM (`P6`, 8 bits) as tight opaque RGBA.
fn frame_from_ppm(ppm: &[u8]) -> Option<Frame> {
    // The header is four fields split by whitespace, the last one followed by one byte.
    let mut fields = Vec::with_capacity(4);
    let mut at = 0;
    while fields.len() < 4 {
        while ppm.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        if *ppm.get(at)? == b'#' {
            while *ppm.get(at)? != b'\n' {
                at += 1;
            }
            continue;
        }
        let start = at;
        while !ppm.get(at)?.is_ascii_whitespace() {
            at += 1;
        }
        fields.push(std::str::from_utf8(&ppm[start..at]).ok()?);
    }
    let pixels = &ppm[at + 1..];
    let (width, height): (u32, u32) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    if fields[0] != "P6" || fields[3] != "255" {
        return None;
    }
    let count = width as usize * height as usize;
    let rgb = pixels.get(..count * 3)?;
    let mut rgba = Vec::with_capacity(count * 4);
    for px in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Some(Frame {
        width,
        height,
        rgba,
    })
}

/// `grim`'s picture of `rect`, which is in the compositor's layout pixels.
fn grab_wayland(rect: Rect) -> Option<Frame> {
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    let area = format!("{},{} {}x{}", rect.x, rect.y, rect.width, rect.height);
    let output = std::process::Command::new("grim")
        .args(["-g", &area, "-t", "ppm", "-"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => frame_from_ppm(&output.stdout),
        // KWin has no screencopy for grim; its own screenshot tool may take the picture.
        _ => grab_with_spectacle(rect),
    }
}

/// The picture of `rect` cut from Spectacle's picture of the whole desktop (KDE).
fn grab_with_spectacle(rect: Rect) -> Option<Frame> {
    let file = std::env::temp_dir().join(format!("openbar-grab-{}.png", std::process::id()));
    let taken = std::process::Command::new("spectacle")
        .args(["--background", "--nonotify", "--fullscreen", "--output"])
        .arg(&file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let picture = taken
        .then(|| gtk::gdk_pixbuf::Pixbuf::from_file(&file).ok())
        .flatten();
    let _ = std::fs::remove_file(&file);
    crop_pixbuf(&picture?, rect)
}

/// `rect` of a picture as tight opaque RGBA; None when it is not all inside.
fn crop_pixbuf(picture: &gtk::gdk_pixbuf::Pixbuf, rect: Rect) -> Option<Frame> {
    let (width, height) = (picture.width(), picture.height());
    if rect.x < 0 || rect.y < 0 || rect.x + rect.width > width || rect.y + rect.height > height {
        return None;
    }
    let channels = picture.n_channels() as usize;
    let stride = picture.rowstride() as usize;
    let bytes = picture.read_pixel_bytes();
    let mut rgba = Vec::with_capacity(rect.width as usize * rect.height as usize * 4);
    for y in rect.y..rect.y + rect.height {
        let row = y as usize * stride;
        for x in rect.x..rect.x + rect.width {
            let at = row + x as usize * channels;
            let px = bytes.get(at..at + 3)?;
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Some(Frame {
        width: rect.width as u32,
        height: rect.height as u32,
        rgba,
    })
}

/// The display under the pointer: a number for it, and where it is in the desktop.
pub fn monitor_under_cursor() -> Option<(isize, Rect)> {
    if crate::linux::wm::native_wayland() {
        let output = crate::linux::wm::focused_output()?;
        return Some((
            output.index as isize,
            Rect {
                x: output.x,
                y: output.y,
                width: output.width,
                height: output.height,
            },
        ));
    }
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
    // An X window id means nothing to the Wayland side; the area is the desktop's.
    if crate::linux::wm::native_wayland() {
        return 0;
    }
    window_at_inner(x, y).unwrap_or(0)
}

/// On Wayland the overlay is a layer surface over its display that takes the
/// keyboard, as an ordinary window would be tiled or placed by the compositor.
pub fn overlay<R: Runtime>(window: &WebviewWindow<R>, at: (i32, i32)) {
    use crate::linux::layer;
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(native) = target.gtk_window() else {
            return;
        };
        if !layer::available() {
            return;
        }
        use gtk::prelude::Cast;
        use layer::Edge::{Bottom, Left, Right, Top};
        let gtk_window: &gtk::Window = native.upcast_ref();
        if layer::init(
            gtk_window,
            c"openbar-capture",
            layer::Layer::Overlay,
            &[Left, Right, Top, Bottom],
        ) {
            layer::set_monitor(gtk_window, layer::monitor_at(at).as_ref());
            layer::set_keyboard(gtk_window, layer::Keyboard::Exclusive);
        }
    });
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

    /// By hand in a nested compositor, nothing over (900, 900) but its background:
    /// OPENBAR_LIVE_GRAB_WAYLAND=1 WAYLAND_DISPLAY=wayland-1 SWAYSOCK=... cargo test live_grab_wayland -- --ignored
    #[test]
    #[ignore = "reads the screen of a nested compositor"]
    fn live_grab_wayland() {
        std::env::var("OPENBAR_LIVE_GRAB_WAYLAND").expect("OPENBAR_LIVE_GRAB_WAYLAND");
        let frame = grab(Rect {
            x: 900,
            y: 900,
            width: 40,
            height: 30,
        })
        .expect("a picture");
        assert_eq!((frame.width, frame.height), (40, 30));
        // The background's colour, #335577 unless OPENBAR_LIVE_GRAB_RGB says otherwise.
        let rgb = std::env::var("OPENBAR_LIVE_GRAB_RGB").unwrap_or_else(|_| "335577".into());
        let rgb = u32::from_str_radix(&rgb, 16).unwrap().to_be_bytes();
        assert_eq!(&frame.rgba[..4], &[rgb[1], rgb[2], rgb[3], 255]);
        let (_, display) = monitor_under_cursor().expect("a display");
        assert_eq!((display.width, display.height), (1920, 1080));
    }

    #[test]
    fn a_part_of_a_picture_is_cut_out_and_one_outside_it_is_not() {
        let picture =
            gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, false, 8, 4, 3).unwrap();
        picture.fill(0x11223300);
        picture.put_pixel(2, 1, 0xaa, 0xbb, 0xcc, 0);
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        let part = crop_pixbuf(&picture, rect(2, 1, 2, 2)).unwrap();
        assert_eq!((part.width, part.height), (2, 2));
        assert_eq!(
            &part.rgba[..8],
            &[0xaa, 0xbb, 0xcc, 255, 0x11, 0x22, 0x33, 255]
        );
        assert!(crop_pixbuf(&picture, rect(3, 0, 2, 1)).is_none());
        assert!(crop_pixbuf(&picture, rect(-1, 0, 1, 1)).is_none());
    }

    #[test]
    fn a_ppm_becomes_opaque_rgba_and_a_short_or_odd_one_nothing() {
        let mut ppm = b"P6\n# grim\n2 1\n255\n".to_vec();
        ppm.extend([1, 2, 3, 4, 5, 6]);
        let frame = frame_from_ppm(&ppm).unwrap();
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(frame.rgba, [1, 2, 3, 255, 4, 5, 6, 255]);
        assert!(
            frame_from_ppm(&ppm[..ppm.len() - 1]).is_none(),
            "a pixel short"
        );
        assert!(
            frame_from_ppm(b"P5\n1 1\n255\n\x00").is_none(),
            "grey is not asked for"
        );
        assert!(frame_from_ppm(b"P6\n1").is_none());
    }

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
