// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Watch on X11, where Windows has Windows.Graphics.Capture, DWM and the
//! toast notifier: the watched window's frame, title and owner, whether it is
//! hidden, the picture of the area, the person's number format, and the
//! notification for a watch that ends while the island is hidden.
//!
//! `watch/platform.rs` includes this file as `imp`. A window is read through
//! the Composite extension: with an automatic redirection the server keeps
//! the window's own pixels, so a window another one covers is read as it is
//! drawn, as Windows.Graphics.Capture does. Without the extension the window
//! is read where it is on the screen, so what covers it is read instead.
//! A part of the display is read from the root window. X11 has no way to
//! leave a window out of a picture, so the island is in a part of the display
//! when it covers it.

use std::collections::HashMap;

use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, MapState, Pixmap, Window};
use x11rb::rust_connection::RustConnection;

use super::super::Kind;
use super::super::support::{Area, pixel_crop};
use super::{Grab, WindowInfo};
use crate::capture::Rect;
use crate::linux::screen;

/// The atom named `name`, interned.
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

fn window_id(handle: isize) -> Option<Window> {
    Window::try_from(handle).ok().filter(|id| *id != 0)
}

/// The window's title: the window manager's UTF-8 name, else the old one.
fn title_of(connection: &RustConnection, window: Window) -> Option<String> {
    let text = |property: Atom, kind: Atom| -> Option<String> {
        let reply = connection
            .get_property(false, window, property, kind, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        let text = String::from_utf8_lossy(&reply.value).into_owned();
        (!text.trim().is_empty()).then_some(text)
    };
    let utf8 = atom(connection, "UTF8_STRING")?;
    text(atom(connection, "_NET_WM_NAME")?, utf8)
        .or_else(|| text(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
}

/// The watched window as the server has it now; None once it is gone.
pub fn window(handle: isize) -> Option<WindowInfo> {
    let window = window_id(handle)?;
    let (connection, root) = screen::connect()?;
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    if geometry.width == 0 || geometry.height == 0 {
        return None;
    }
    // The window's own corner on the desktop, whatever its frame is.
    let corner = connection
        .translate_coordinates(window, root, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    let pid = crate::linux::x11::pid_of(&connection, window, atom(&connection, "_NET_WM_PID")?);
    Some(WindowInfo {
        bounds: Area {
            x: f64::from(corner.dst_x),
            y: f64::from(corner.dst_y),
            width: f64::from(geometry.width),
            height: f64::from(geometry.height),
        },
        scale: 1.0,
        title: title_of(&connection, window),
        pid: pid.unwrap_or(0),
    })
}

/// Whether the window is minimized or hidden: not on the screen, so there is
/// nothing to read.
pub fn minimized(handle: isize) -> bool {
    let Some(window) = window_id(handle) else {
        return false;
    };
    let Some((connection, _)) = screen::connect() else {
        return false;
    };
    let Some(attributes) = connection
        .get_window_attributes(window)
        .ok()
        .and_then(|c| c.reply().ok())
    else {
        return false;
    };
    if attributes.map_state != MapState::VIEWABLE {
        return true;
    }
    let (Some(state), Some(hidden)) = (
        atom(&connection, "_NET_WM_STATE"),
        atom(&connection, "_NET_WM_STATE_HIDDEN"),
    ) else {
        return false;
    };
    connection
        .get_property(false, window, state, AtomEnum::ATOM, 0, 32)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| Some(r.value32()?.any(|a| a == hidden)))
        .unwrap_or(false)
}

/// The decimal mark and whether thousands are grouped with a space for a
/// language, as a region that has no other mark for them writes numbers.
fn marks_of(language: &str) -> (&'static str, bool) {
    const COMMA: &[&str] = &[
        "bg", "cs", "da", "de", "el", "es", "et", "eu", "fi", "fr", "hr", "hu", "id", "is", "it",
        "lt", "lv", "nb", "nl", "nn", "no", "pl", "pt", "ro", "ru", "sk", "sl", "sr", "sv", "tr",
        "uk", "vi",
    ];
    const SPACED: &[&str] = &[
        "bg", "cs", "et", "fi", "fr", "hu", "lt", "lv", "nb", "nn", "no", "pl", "ru", "sk", "sv",
        "uk",
    ];
    (
        if COMMA.contains(&language) { "," } else { "." },
        SPACED.contains(&language),
    )
}

/// The language the person's numbers follow: `LC_ALL`, `LC_NUMERIC` or `LANG`,
/// the first that is set to something other than the C locale.
fn number_language(env: impl Fn(&str) -> Option<String>) -> String {
    ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .filter_map(|name| env(name))
        .find(|value| !value.is_empty())
        .map(|value| {
            value
                .split(['_', '.', '@'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

/// The decimal mark and whether thousands are grouped with a space, as the
/// person's region writes numbers.
pub fn number_marks() -> (String, bool) {
    let language = number_language(|name| std::env::var(name).ok());
    let (decimal, spaced) = marks_of(&language);
    (decimal.into(), spaced)
}

/// Posts a notification through the notification service.
pub fn toast(title: &str, body: &str) -> bool {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return false;
    };
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Openbar",
                0u32,
                "",
                title,
                body,
                Vec::<&str>::new(),
                HashMap::<&str, zbus::zvariant::Value>::new(),
                -1i32,
            ),
        )
        .is_ok()
}

/// What a watch reads from: a window, kept redirected while it is watched,
/// or a part of the display.
pub struct Source {
    kind: Kind,
    connection: Option<(RustConnection, Window)>,
    /// The window the server keeps the pixels of.
    redirected: Option<Window>,
}

impl Source {
    pub fn new(kind: &Kind) -> Self {
        let connection = screen::connect();
        let mut redirected = None;
        if let (Kind::Window { hwnd, .. }, Some((connection, _))) = (kind, &connection)
            && let Some(window) = window_id(*hwnd)
            && connection
                .composite_query_version(0, 4)
                .ok()
                .is_some_and(|cookie| cookie.reply().is_ok())
            && connection
                .composite_redirect_window(window, Redirect::AUTOMATIC)
                .ok()
                .is_some_and(|cookie| cookie.check().is_ok())
        {
            redirected = Some(window);
        }
        Source {
            kind: kind.clone(),
            connection,
            redirected,
        }
    }

    /// The area now: `crop` in the units of `space`, the size of what it's
    /// measured on (the window in points, which are pixels here, or the
    /// display in pixels).
    pub fn grab(&mut self, crop: Area, space: (f64, f64)) -> Grab {
        let frame = match self.kind {
            // `screen::grab` asks the X server or, on Wayland, grim.
            Kind::Display { monitor, .. } => screen::grab(display_rect(monitor, crop)),
            Kind::Window { hwnd, .. } => {
                let Some((connection, _)) = self.connection.as_ref() else {
                    return Grab::Missing;
                };
                window_frame(connection, hwnd, self.redirected, crop, space)
            }
        };
        frame.map_or(Grab::Missing, Grab::Frame)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if let (Some(window), Some((connection, _))) = (self.redirected, &self.connection) {
            let _ = connection.composite_unredirect_window(window, Redirect::AUTOMATIC);
            let _ = connection.flush();
        }
    }
}

/// The part of a display `crop` (pixels from the display's corner) covers, in
/// desktop pixels, the smallest whole-pixel rect around it kept in the display.
fn display_rect(monitor: Rect, crop: Area) -> Rect {
    let left = crop.x.floor().max(0.0) as i32;
    let top = crop.y.floor().max(0.0) as i32;
    let right = ((crop.x + crop.width).ceil() as i32).min(monitor.width);
    let bottom = ((crop.y + crop.height).ceil() as i32).min(monitor.height);
    Rect {
        x: monitor.x + left,
        y: monitor.y + top,
        width: (right - left).max(0),
        height: (bottom - top).max(0),
    }
}

/// The picture of the part of the window `crop` covers.
fn window_frame(
    connection: &RustConnection,
    handle: isize,
    redirected: Option<Window>,
    crop: Area,
    space: (f64, f64),
) -> Option<crate::capture::snapshot::Frame> {
    let window = window_id(handle)?;
    let geometry = connection.get_geometry(window).ok()?.reply().ok()?;
    let part = pixel_crop(
        crop,
        space,
        (u32::from(geometry.width), u32::from(geometry.height)),
    )?;
    let rect = Rect {
        x: part.x as i32,
        y: part.y as i32,
        width: part.width as i32,
        height: part.height as i32,
    };
    // The pixmap the server keeps for a redirected window is read in place of
    // the window, which holds what covers it where it is covered.
    if redirected == Some(window)
        && let Some(pixmap) = named_pixmap(connection, window)
    {
        let frame = screen::grab_from(connection, pixmap, rect);
        let _ = connection.free_pixmap(pixmap);
        return frame;
    }
    screen::grab_from(connection, window, rect)
}

/// A pixmap with the redirected window's current pixels, to be freed by the caller.
fn named_pixmap(connection: &RustConnection, window: Window) -> Option<Pixmap> {
    let pixmap = connection.generate_id().ok()?;
    connection
        .composite_name_window_pixmap(window, pixmap)
        .ok()?
        .check()
        .ok()?;
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto;

    #[test]
    fn numbers_follow_the_language_of_the_environment() {
        let env = |pairs: &'static [(&str, &str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };
        assert_eq!(
            marks_of(&number_language(env(&[("LANG", "pl_PL.UTF-8")]))),
            (",", true)
        );
        assert_eq!(
            marks_of(&number_language(env(&[("LANG", "de_DE.UTF-8")]))),
            (",", false)
        );
        assert_eq!(
            marks_of(&number_language(env(&[("LANG", "en_US.UTF-8")]))),
            (".", false)
        );
        // LC_ALL leads, and an empty one does not count.
        assert_eq!(
            marks_of(&number_language(env(&[
                ("LC_ALL", "fr_FR.UTF-8"),
                ("LANG", "en_US.UTF-8")
            ]))),
            (",", true)
        );
        assert_eq!(
            marks_of(&number_language(env(&[
                ("LC_ALL", ""),
                ("LANG", "pl_PL.UTF-8")
            ]))),
            (",", true)
        );
        // The C locale and nothing set write numbers the plain way.
        assert_eq!(
            marks_of(&number_language(env(&[("LANG", "C")]))),
            (".", false)
        );
        assert_eq!(marks_of(&number_language(env(&[]))), (".", false));
    }

    #[test]
    fn a_part_of_a_display_is_kept_inside_it() {
        let monitor = Rect {
            x: 1920,
            y: 100,
            width: 1000,
            height: 800,
        };
        let crop = |x: f64, y: f64, width: f64, height: f64| Area {
            x,
            y,
            width,
            height,
        };
        let r = display_rect(monitor, crop(10.4, 20.2, 100.2, 50.0));
        assert_eq!((r.x, r.y, r.width, r.height), (1930, 120, 101, 51));
        let r = display_rect(monitor, crop(950.0, 780.0, 100.0, 100.0));
        assert_eq!((r.x, r.y, r.width, r.height), (2870, 880, 50, 20));
        let r = display_rect(monitor, crop(-20.0, 0.0, 100.0, 10.0));
        assert_eq!((r.x, r.width), (1920, 80));
    }

    /// A window of one color at `(x, y)`, mapped, on the nested server.
    fn painted(
        connection: &RustConnection,
        root: Window,
        at: (i16, i16),
        size: (u16, u16),
        color: u32,
    ) -> Window {
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                root,
                at.0,
                at.1,
                size.0,
                size.1,
                0,
                xproto::WindowClass::INPUT_OUTPUT,
                0,
                &xproto::CreateWindowAux::new().background_pixel(color),
            )
            .unwrap();
        connection.map_window(window).unwrap();
        window
    }

    /// By hand on a nested X server such as Xvfb on :98, never the desktop's own:
    /// `DISPLAY=:98 cargo test live_watch -- --ignored --nocapture`.
    #[test]
    #[ignore = "draws windows on DISPLAY: only a nested X server, never the desktop's own"]
    fn live_a_covered_window_and_a_part_of_the_display_are_read_as_drawn() {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(
            !display.is_empty() && !display.starts_with(":0") && !display.starts_with(":1"),
            "refusing to draw on {display:?}: use a nested server such as Xvfb on :98"
        );
        let (connection, root) = screen::connect().expect("an X server");
        let wait = || std::thread::sleep(std::time::Duration::from_millis(300));
        let below = painted(&connection, root, (100, 100), (200, 100), 0x33_66_99);
        connection.flush().unwrap();
        wait();

        let area = Area {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 20.0,
        };
        let kind = Kind::Window {
            hwnd: below as isize,
            crop: area,
        };
        let info = window(below as isize).expect("the window");
        assert_eq!(
            (
                info.bounds.x,
                info.bounds.y,
                info.bounds.width,
                info.bounds.height
            ),
            (100.0, 100.0, 200.0, 100.0)
        );
        assert!(!minimized(below as isize));
        assert!(window(0).is_none());

        // The watch starts, the window draws, and then another one covers it.
        let mut source = Source::new(&kind);
        assert!(
            source.redirected.is_some(),
            "the server has no Composite extension"
        );
        connection.clear_area(true, below, 0, 0, 0, 0).unwrap();
        connection.flush().unwrap();
        wait();
        let above = painted(&connection, root, (80, 80), (300, 200), 0xcc_33_00);
        connection.flush().unwrap();
        wait();

        let Grab::Frame(frame) = source.grab(area, (200.0, 100.0)) else {
            panic!("no picture of the covered window");
        };
        assert_eq!((frame.width, frame.height), (50, 20));
        assert_eq!(
            frame.pixel(5, 5),
            Some([0x33, 0x66, 0x99]),
            "a covered window is read as it is drawn, not as what covers it"
        );
        println!("covered window read as {:?}", frame.pixel(5, 5));

        // A part of the display is what is on the screen: the covering window.
        let monitor = Rect {
            x: 0,
            y: 0,
            width: 1024,
            height: 768,
        };
        let mut display = Source::new(&Kind::Display {
            monitor,
            crop: area,
        });
        let part = Area {
            x: 150.0,
            y: 150.0,
            width: 30.0,
            height: 10.0,
        };
        let Grab::Frame(frame) = display.grab(part, (1024.0, 768.0)) else {
            panic!("no picture of the display");
        };
        assert_eq!((frame.width, frame.height), (30, 10));
        assert_eq!(frame.pixel(2, 2), Some([0xcc, 0x33, 0x00]));

        // Unmapped, a window is minimized, and gone it is not a window.
        connection.unmap_window(below).unwrap();
        connection.flush().unwrap();
        wait();
        assert!(minimized(below as isize));
        drop(source);
        connection.destroy_window(above).unwrap();
        connection.destroy_window(below).unwrap();
        connection.flush().unwrap();
        wait();
        assert!(window(below as isize).is_none());
    }
}
