// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The island on Wayland (#252). A Wayland app cannot place its own window,
//! keep it above the others or reserve room for it; a compositor that speaks
//! the layer-shell protocol (sway, Hyprland, labwc, river, KDE's KWin) lets a
//! panel do all three, and gtk-layer-shell turns a GTK window into one.
//!
//! The library is looked up when the island is made and not linked, as
//! `sqlite.rs` does with SQLite: an X11 desktop needs nothing new, and a
//! Wayland one without it gets an ordinary window.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::{Cast, GtkWindowExt, MonitorExt, WidgetExt};

/// `GtkLayerShellLayer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Layer {
    Top = 2,
    Overlay = 3,
}

/// `GtkLayerShellEdge`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Edge {
    Left = 0,
    Right = 1,
    Top = 2,
    Bottom = 3,
}

/// `GtkLayerShellKeyboardMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Keyboard {
    None = 0,
    Exclusive = 1,
    OnDemand = 2,
}

type Window = *mut c_void;

struct Library {
    is_supported: Option<unsafe extern "C" fn() -> c_int>,
    init_for_window: unsafe extern "C" fn(Window),
    set_namespace: unsafe extern "C" fn(Window, *const c_char),
    set_layer: unsafe extern "C" fn(Window, Layer),
    set_monitor: unsafe extern "C" fn(Window, *mut c_void),
    set_anchor: unsafe extern "C" fn(Window, Edge, c_int),
    set_exclusive_zone: unsafe extern "C" fn(Window, c_int),
    /// Room from an anchored edge; optional, so an island never depends on it.
    set_margin: Option<unsafe extern "C" fn(Window, Edge, c_int)>,
    /// 0.6 and later; older ones only had keyboard interactivity on or off.
    set_keyboard_mode: Option<unsafe extern "C" fn(Window, Keyboard)>,
    set_keyboard_interactivity: Option<unsafe extern "C" fn(Window, c_int)>,
}

/// # Safety
/// `F` is the `extern "C"` function pointer type gtk-layer-shell documents for `name`.
unsafe fn function<F: Copy>(handle: *mut c_void, name: &CStr) -> Option<F> {
    assert_eq!(size_of::<F>(), size_of::<*mut c_void>());
    let found = unsafe { libc::dlsym(handle, name.as_ptr()) };
    // SAFETY: a function pointer is as large as the data pointer dlsym gives.
    (!found.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, F>(&found) })
}

fn load() -> Option<Library> {
    // SAFETY: the names are C strings, and the library is never closed.
    unsafe {
        let handle = [c"libgtk-layer-shell.so.0", c"libgtk-layer-shell.so"]
            .iter()
            .find_map(|name| {
                let handle = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                (!handle.is_null()).then_some(handle)
            })?;
        Some(Library {
            is_supported: function(handle, c"gtk_layer_is_supported"),
            init_for_window: function(handle, c"gtk_layer_init_for_window")?,
            set_namespace: function(handle, c"gtk_layer_set_namespace")?,
            set_layer: function(handle, c"gtk_layer_set_layer")?,
            set_monitor: function(handle, c"gtk_layer_set_monitor")?,
            set_anchor: function(handle, c"gtk_layer_set_anchor")?,
            set_exclusive_zone: function(handle, c"gtk_layer_set_exclusive_zone")?,
            set_margin: function(handle, c"gtk_layer_set_margin"),
            set_keyboard_mode: function(handle, c"gtk_layer_set_keyboard_mode"),
            set_keyboard_interactivity: function(handle, c"gtk_layer_set_keyboard_interactivity"),
        })
    }
}

fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY.get_or_init(load).as_ref()
}

/// Whether GTK talks to a Wayland compositor (and not to X11, or XWayland).
/// Main thread only.
pub fn wayland() -> bool {
    use gtk::glib::prelude::ObjectExt;
    gtk::gdk::Display::default()
        .is_some_and(|display| display.type_().name() == "GdkWaylandDisplay")
}

/// Whether windows can be made layer surfaces here: a Wayland session whose
/// compositor offers the protocol, with the library installed. Main thread only.
pub fn available() -> bool {
    if !wayland() {
        return false;
    }
    let Some(library) = library() else {
        return false;
    };
    // Before 0.6 there is no asking; the compositor is taken at its word.
    library
        .is_supported
        .is_none_or(|supported| unsafe { supported() } != 0)
}

fn raw(window: &gtk::Window) -> Window {
    let pointer: *mut gtk::ffi::GtkWindow = window.to_glib_none().0;
    pointer.cast()
}

/// Makes `window` a layer surface named `namespace` on `layer`, anchored to
/// `edges`. GTK sets a window up as one only before it is shown, so a window
/// already on screen is taken down and shown again. Main thread only.
pub fn init(window: &gtk::Window, namespace: &CStr, layer: Layer, edges: &[Edge]) -> bool {
    let Some(library) = library() else {
        return false;
    };
    let shown = window.is_visible();
    if window.is_realized() {
        window.hide();
        window.unrealize();
    }
    let target = raw(window);
    // SAFETY: `target` is a live GtkWindow, not yet realized, and the strings are C strings.
    unsafe {
        (library.init_for_window)(target);
        (library.set_namespace)(target, namespace.as_ptr());
        (library.set_layer)(target, layer);
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            (library.set_anchor)(target, edge, c_int::from(edges.contains(&edge)));
        }
        // Not moved aside by other panels' zones: the island is drawn from the very top.
        (library.set_exclusive_zone)(target, -1);
    }
    set_keyboard(window, Keyboard::None);
    if shown {
        window.show();
    }
    true
}

/// Puts a layer surface on `monitor` (the compositor's choice for None).
pub fn set_monitor(window: &gtk::Window, monitor: Option<&gtk::gdk::Monitor>) {
    let Some(library) = library() else { return };
    let monitor: *mut gtk::gdk::ffi::GdkMonitor = monitor.to_glib_none().0;
    // SAFETY: a live window that was made a layer surface; a live monitor or null.
    unsafe { (library.set_monitor)(raw(window), monitor.cast()) };
}

/// Room kept free for the surface along the edge it is anchored to, in
/// logical pixels; -1 for none and not moved by others.
pub fn set_exclusive_zone(window: &gtk::Window, zone: i32) {
    let Some(library) = library() else { return };
    // SAFETY: a live window that was made a layer surface.
    unsafe { (library.set_exclusive_zone)(raw(window), zone) };
}

/// Room kept between the surface and the edge it is anchored to, in logical
/// pixels, as the sessions' tray panel keeps from the corner (#331).
pub fn set_margin(window: &gtk::Window, edge: Edge, margin: i32) {
    let Some(set) = library().and_then(|library| library.set_margin) else {
        return;
    };
    // SAFETY: a live window that was made a layer surface.
    unsafe { set(raw(window), edge, margin) };
}

/// Whether the surface may take the keyboard when clicked.
pub fn set_keyboard(window: &gtk::Window, mode: Keyboard) {
    let Some(library) = library() else { return };
    // SAFETY: a live window that was made a layer surface.
    unsafe {
        if let Some(set) = library.set_keyboard_mode {
            set(raw(window), mode);
        } else if let Some(set) = library.set_keyboard_interactivity {
            set(raw(window), c_int::from(mode != Keyboard::None));
        }
    }
}

/// A display as GTK knows it: its top-left corner and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub scale: i32,
}

/// Which of `monitors` stands at the physical point `at`, as tao reports a
/// display's position (GTK's logical one times its scale).
pub fn index_at(monitors: &[Geometry], at: (i32, i32)) -> Option<usize> {
    monitors
        .iter()
        .position(|m| (m.x * m.scale, m.y * m.scale) == at)
}

/// The GTK display whose physical top-left corner is `at`. Main thread only.
pub fn monitor_at(at: (i32, i32)) -> Option<gtk::gdk::Monitor> {
    let display = gtk::gdk::Display::default()?;
    let monitors: Vec<gtk::gdk::Monitor> = (0..display.n_monitors())
        .filter_map(|i| display.monitor(i))
        .collect();
    let geometry: Vec<Geometry> = monitors
        .iter()
        .map(|m| {
            let area = m.geometry();
            Geometry {
                x: area.x(),
                y: area.y(),
                scale: m.scale_factor(),
            }
        })
        .collect();
    index_at(&geometry, at).map(|i| monitors[i].clone())
}

/// An empty, transparent surface along the top of `monitor` (the compositor's
/// choice for None) that keeps
/// `height` logical pixels free, as `room.rs` does with a strut on X11.
pub fn spacer(monitor: Option<&gtk::gdk::Monitor>, height: i32) -> Option<gtk::Window> {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_app_paintable(true);
    window.set_accept_focus(false);
    if let Some(visual) = WidgetExt::screen(&window).and_then(|s| s.rgba_visual()) {
        window.set_visual(Some(&visual));
    }
    window.set_size_request(1, 1);
    if !init(
        window.upcast_ref(),
        c"openbar-room",
        Layer::Top,
        &[Edge::Left, Edge::Right, Edge::Top],
    ) {
        return None;
    }
    set_monitor(window.upcast_ref(), monitor);
    set_exclusive_zone(window.upcast_ref(), height);
    window.show();
    // Takes no clicks anywhere.
    window.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    Some(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_is_found_by_its_physical_corner() {
        let monitors = [
            Geometry {
                x: 0,
                y: 0,
                scale: 2,
            },
            Geometry {
                x: 1280,
                y: 0,
                scale: 1,
            },
        ];
        assert_eq!(index_at(&monitors, (0, 0)), Some(0));
        assert_eq!(index_at(&monitors, (1280, 0)), Some(1));
        assert_eq!(
            index_at(&monitors, (2560, 0)),
            None,
            "the second display is not scaled"
        );
        assert_eq!(index_at(&[], (0, 0)), None);
    }

    #[test]
    fn the_enums_are_the_c_values() {
        assert_eq!(Layer::Overlay as i32, 3);
        assert_eq!(Edge::Bottom as i32, 3);
        assert_eq!(Keyboard::OnDemand as i32, 2);
    }
}
