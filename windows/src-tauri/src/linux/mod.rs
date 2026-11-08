// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! What the island does differently on Linux (X11 for now). Each file is the
//! Linux side of a module that is Windows code elsewhere, picked with
//! `cfg(target_os = "linux")` where that module hands over to it.
//!
//! Some files are not declared here: `island.rs`, `fullscreen.rs`, `ide.rs`,
//! `media.rs` and `audio.rs` are included by the modules they belong to (as
//! their `platform`, `imp`, `linux_ide`, `imp` and `engine`), since they use
//! what those modules keep private.

use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

pub mod autostart;
pub mod battery;
pub mod brightness;
pub mod command;
pub mod desktop;
pub mod downloads;
pub mod files;
pub mod gtk_main;
pub mod hooks;
pub mod hosts;
pub mod ics;
pub mod image;
pub mod kitty;
pub mod locale;
pub mod net;
pub mod ocr;
pub mod paths;
pub mod process;
pub mod pulse;
pub mod recorder;
pub mod screen;
pub mod session_lock;
pub mod sessions;
pub mod shelf;
pub mod sound;
pub mod sqlite;
#[cfg(test)]
pub mod testbus;
pub mod tmux;
pub mod toggles;
pub mod wezterm;
pub mod x11;

/// What WebKit is told before it starts, from what the machine offers, as
/// (name, value). A setting the person made wins over each of these.
fn webkit_settings(
    chosen: impl Fn(&str) -> bool,
    nvidia: bool,
    dri3: Option<bool>,
) -> Vec<(&'static str, &'static str)> {
    let mut settings = Vec::new();
    // NVIDIA cannot back WebKitGTK's GBM buffers, so draw into shared memory.
    let buffers = [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
        "WEBKIT_DMABUF_RENDERER_DISABLE_GBM",
    ];
    if nvidia && !buffers.iter().any(|name| chosen(name)) {
        settings.push(("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1"));
    }
    // Where the X server has no DRI3 (Xvfb, a VM without 3D) the island stays invisible, so compose in software.
    if dri3 == Some(false) && !chosen("WEBKIT_DISABLE_COMPOSITING_MODE") {
        settings.push(("WEBKIT_DISABLE_COMPOSITING_MODE", "1"));
    }
    settings
}

/// Whether the X server offers DRI3, which is how WebKit gets GPU buffers for
/// a window; None when it cannot be asked.
fn has_dri3() -> Option<bool> {
    let (connection, _) = RustConnection::connect(None).ok()?;
    Some(
        connection
            .query_extension(b"DRI3")
            .ok()?
            .reply()
            .ok()?
            .present,
    )
}

/// Called first thing in `run`: the WebKit settings this machine needs.
pub fn before_start() {
    let chosen = |name: &str| std::env::var_os(name).is_some();
    let nvidia = std::path::Path::new("/proc/driver/nvidia/version").exists();
    for (name, value) in webkit_settings(chosen, nvidia, has_dri3()) {
        // SAFETY: nothing else is running yet, the first thing `run` does.
        unsafe { std::env::set_var(name, value) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHM: (&str, &str) = ("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1");
    const SOFTWARE: (&str, &str) = ("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

    fn with(set: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |name| set.contains(&name)
    }

    #[test]
    fn nvidia_draws_into_shared_memory_unless_the_person_chose_otherwise() {
        assert_eq!(webkit_settings(with(&[]), true, Some(true)), [SHM]);
        assert_eq!(webkit_settings(with(&[]), false, Some(true)), []);
        for name in [
            "WEBKIT_DISABLE_DMABUF_RENDERER",
            "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
            "WEBKIT_DMABUF_RENDERER_DISABLE_GBM",
        ] {
            let chosen = move |asked: &str| asked == name;
            assert_eq!(webkit_settings(chosen, true, Some(true)), []);
        }
    }

    #[test]
    fn a_server_without_dri3_composes_in_software_unless_the_person_chose_otherwise() {
        assert_eq!(webkit_settings(with(&[]), false, Some(false)), [SOFTWARE]);
        assert_eq!(webkit_settings(with(&[]), false, Some(true)), []);
        assert_eq!(webkit_settings(with(&[]), false, None), []);
        let chosen = with(&["WEBKIT_DISABLE_COMPOSITING_MODE"]);
        assert_eq!(webkit_settings(chosen, false, Some(false)), []);
    }

    #[test]
    fn both_settings_can_apply_to_one_machine() {
        assert_eq!(
            webkit_settings(with(&[]), true, Some(false)),
            [SHM, SOFTWARE]
        );
    }

    /// By hand on a nested X server (Xephyr has no DRI3): DISPLAY=:97 cargo test live_dri3 -- --ignored
    #[test]
    #[ignore = "asks the X server of DISPLAY, which is only for a nested one"]
    fn live_dri3() {
        assert_eq!(has_dri3(), Some(false));
    }
}
