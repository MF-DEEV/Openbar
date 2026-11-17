// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Room for the closed island at the top of a display, so the window manager
//! maximizes windows below it, the way they start below the Mac's menu bar.
//! Each display keeps its strip through a small dock window of its own that
//! carries `_NET_WM_STRUT_PARTIAL`, draws nothing and takes no clicks. The
//! island's window is not used: it moves between displays with the pointer,
//! and every move would resize the maximized windows on both. The X server
//! drops the strips with the connection, so a crash leaves no space behind.

use std::sync::{Mutex, OnceLock, PoisonError};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClipOrdering, ConnectionExt as _, CreateWindowAux, PropMode, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// A display on the root window, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// One display's strip: the root window's rows above `bottom`, from column
/// `left` to `right` inclusive, as `_NET_WM_STRUT_PARTIAL` counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
}

/// The strips `height` CSS pixels of island need on `displays`. A strut always
/// reaches up to the top of the root window, so a display with another one
/// above it gets none: its strip would take that display's columns too.
pub fn bands(displays: &[Area], all: &[Area], height: f64) -> Vec<Band> {
    if height.is_nan() || height <= 0.0 {
        return Vec::new();
    }
    displays
        .iter()
        .filter(|d| d.width > 0 && d.x >= 0 && d.y >= 0)
        .filter(|d| {
            !all.iter().any(|other| {
                other != *d
                    && other.y < d.y
                    && other.x < d.x + d.width as i32
                    && other.x + other.width as i32 > d.x
            })
        })
        .map(|d| Band {
            left: d.x as u32,
            right: (d.x + d.width as i32 - 1) as u32,
            bottom: d.y as u32 + (height * d.scale.max(1.0)).ceil() as u32,
        })
        .collect()
}

struct Atoms {
    strut: Atom,
    strut_partial: Atom,
    window_type: Atom,
    dock: Atom,
    desktop: Atom,
    state: Atom,
    sticky: Atom,
    skip_taskbar: Atom,
    skip_pager: Atom,
}

struct Spacers {
    connection: RustConnection,
    root: Window,
    atoms: Atoms,
    shape: bool,
    windows: Vec<(Band, Window)>,
}

fn atom(connection: &RustConnection, name: &str) -> Option<Atom> {
    crate::linux::x11::atom(connection, name)
}

impl Spacers {
    fn connect() -> Option<Mutex<Spacers>> {
        let (connection, screen) = RustConnection::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let atoms = Atoms {
            strut: atom(&connection, "_NET_WM_STRUT")?,
            strut_partial: atom(&connection, "_NET_WM_STRUT_PARTIAL")?,
            window_type: atom(&connection, "_NET_WM_WINDOW_TYPE")?,
            dock: atom(&connection, "_NET_WM_WINDOW_TYPE_DOCK")?,
            desktop: atom(&connection, "_NET_WM_DESKTOP")?,
            state: atom(&connection, "_NET_WM_STATE")?,
            sticky: atom(&connection, "_NET_WM_STATE_STICKY")?,
            skip_taskbar: atom(&connection, "_NET_WM_STATE_SKIP_TASKBAR")?,
            skip_pager: atom(&connection, "_NET_WM_STATE_SKIP_PAGER")?,
        };
        let shape = connection
            .extension_information(shape::X11_EXTENSION_NAME)
            .ok()
            .flatten()
            .is_some();
        Some(Mutex::new(Spacers {
            connection,
            root,
            atoms,
            shape,
            windows: Vec::new(),
        }))
    }

    /// A dock window on every workspace that holds `band` and nothing else.
    fn open(&self, band: Band) -> Option<Window> {
        let c = &self.connection;
        let window = c.generate_id().ok()?;
        c.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            self.root,
            i16::try_from(band.left).unwrap_or(0),
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )
        .ok()?
        .check()
        .ok()?;
        let a = &self.atoms;
        let _ = c.change_property32(
            PropMode::REPLACE,
            window,
            a.window_type,
            AtomEnum::ATOM,
            &[a.dock],
        );
        let _ = c.change_property32(
            PropMode::REPLACE,
            window,
            a.desktop,
            AtomEnum::CARDINAL,
            &[u32::MAX],
        );
        let _ = c.change_property32(
            PropMode::REPLACE,
            window,
            a.state,
            AtomEnum::ATOM,
            &[a.sticky, a.skip_taskbar, a.skip_pager],
        );
        let _ = c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"Openbar room",
        );
        let _ = c.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"openbar-room\0Openbar\0",
        );
        if self.shape {
            for kind in [SK::BOUNDING, SK::INPUT] {
                let _ =
                    c.shape_rectangles(SO::SET, kind, ClipOrdering::UNSORTED, window, 0, 0, &[]);
            }
        }
        self.hold(window, band);
        c.map_window(window).ok()?.check().ok()?;
        Some(window)
    }

    fn hold(&self, window: Window, band: Band) {
        let a = &self.atoms;
        let partial = [
            0,
            0,
            band.bottom,
            0,
            0,
            0,
            0,
            0,
            band.left,
            band.right,
            0,
            0,
        ];
        let _ = self.connection.change_property32(
            PropMode::REPLACE,
            window,
            a.strut_partial,
            AtomEnum::CARDINAL,
            &partial,
        );
        // Window managers without the partial form read the whole top edge from this one.
        let _ = self.connection.change_property32(
            PropMode::REPLACE,
            window,
            a.strut,
            AtomEnum::CARDINAL,
            &[0, 0, band.bottom, 0],
        );
    }

    /// Keeps a window per wanted strip and closes the rest.
    fn sync(&mut self, bands: &[Band]) {
        let mut kept = Vec::with_capacity(bands.len());
        let mut spare: Vec<(Band, Window)> = std::mem::take(&mut self.windows);
        for band in bands {
            if let Some(i) = spare.iter().position(|(held, _)| held == band) {
                kept.push(spare.swap_remove(i));
            } else if let Some((_, window)) = spare.pop() {
                self.hold(window, *band);
                kept.push((*band, window));
            } else if let Some(window) = self.open(*band) {
                kept.push((*band, window));
            }
        }
        for (_, window) in spare {
            let _ = self.connection.destroy_window(window);
        }
        self.windows = kept;
        // A round trip, so the window manager has the strips before anyone asks it for the work area.
        let _ = self
            .connection
            .get_input_focus()
            .ok()
            .map(|cookie| cookie.reply());
    }
}

static SPACERS: OnceLock<Option<Mutex<Spacers>>> = OnceLock::new();

/// Holds exactly `bands`; an empty list gives every strip back.
pub fn reserve(bands: &[Band]) {
    // Nothing to give back before the first strip, so no connection for it.
    let spacers = if bands.is_empty() {
        SPACERS.get().and_then(Option::as_ref)
    } else {
        SPACERS.get_or_init(Spacers::connect).as_ref()
    };
    if let Some(spacers) = spacers {
        spacers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sync(bands);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: i32, y: i32, width: u32, height: u32, scale: f64) -> Area {
        Area {
            x,
            y,
            width,
            height,
            scale,
        }
    }

    #[test]
    fn each_display_at_the_top_keeps_a_strip_the_islands_height() {
        let left = area(0, 0, 1920, 1080, 1.0);
        let right = area(1920, 0, 2560, 1440, 1.5);
        let all = [left, right];
        assert_eq!(
            bands(&all, &all, 24.0),
            [
                Band {
                    left: 0,
                    right: 1919,
                    bottom: 24
                },
                Band {
                    left: 1920,
                    right: 4479,
                    bottom: 36
                }
            ]
        );
        assert_eq!(
            bands(&[right], &all, 24.0),
            [Band {
                left: 1920,
                right: 4479,
                bottom: 36
            }],
            "only the displays asked for"
        );
    }

    #[test]
    fn a_fraction_of_a_pixel_still_gets_its_row() {
        let one = [area(0, 0, 1366, 768, 1.25)];
        assert_eq!(bands(&one, &one, 25.0)[0].bottom, 32);
    }

    #[test]
    fn a_display_lower_down_counts_from_the_top_of_the_screen() {
        let tall = area(0, 0, 1080, 1920, 1.0);
        let low = area(1080, 840, 1920, 1080, 1.0);
        let all = [tall, low];
        assert_eq!(bands(&[low], &all, 24.0)[0].bottom, 864);
    }

    #[test]
    fn a_display_below_another_gets_no_strip() {
        let top = area(0, 0, 1920, 1080, 1.0);
        let below = area(0, 1080, 1920, 1080, 1.0);
        let all = [top, below];
        assert_eq!(bands(&all, &all, 24.0).len(), 1);
        assert_eq!(bands(&[below], &all, 24.0), []);
        // Only partly below still shares columns with the one above.
        let offset = area(1000, 1080, 1920, 1080, 1.0);
        assert_eq!(bands(&[offset], &[top, offset], 24.0), []);
        // Beside it is fine.
        let beside = area(1920, 300, 1920, 1080, 1.0);
        assert_eq!(bands(&[beside], &[top, beside], 24.0).len(), 1);
    }

    #[test]
    fn no_island_no_strip() {
        let one = [area(0, 0, 1920, 1080, 1.0)];
        assert_eq!(bands(&one, &one, 0.0), []);
        assert_eq!(bands(&one, &one, -3.0), []);
        assert_eq!(bands(&one, &one, f64::NAN), []);
        assert_eq!(bands(&[], &one, 24.0), []);
    }

    /// Reserves a strip on the X server in `$DISPLAY` and reads the work area the
    /// window manager answers with. Needs a window manager that honours struts
    /// (xfwm4 in a nested Xephyr); never run on the screen someone is using.
    #[test]
    #[ignore = "needs a nested X server with a window manager: OPENBAR_LIVE_ROOM=1"]
    fn live_room() {
        if std::env::var("OPENBAR_LIVE_ROOM").as_deref() != Ok("1") {
            return;
        }
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(
            !display.starts_with(":0"),
            "refusing to change the work area of {display}"
        );
        let (connection, screen) = RustConnection::connect(None).unwrap();
        let root = connection.setup().roots[screen].root;
        let (width, height) = (
            connection.setup().roots[screen].width_in_pixels,
            connection.setup().roots[screen].height_in_pixels,
        );
        let workarea = atom(&connection, "_NET_WORKAREA").unwrap();
        let read = || -> Vec<u32> {
            std::thread::sleep(std::time::Duration::from_millis(300));
            connection
                .get_property(false, root, workarea, AtomEnum::CARDINAL, 0, 4)
                .unwrap()
                .reply()
                .unwrap()
                .value32()
                .unwrap()
                .collect()
        };
        let before = read();
        let screen_area = area(0, 0, u32::from(width), u32::from(height), 1.0);
        reserve(&bands(&[screen_area], &[screen_area], 24.0));
        let during = read();
        reserve(&[]);
        let after = read();
        eprintln!("work area before {before:?}, with room {during:?}, after {after:?}");
        assert_eq!(
            during[1],
            before[1].max(24),
            "the work area starts below the strip"
        );
        assert_eq!(after, before, "the strip is given back");
    }
}
