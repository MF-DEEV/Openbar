// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! What the island does differently on Linux (X11 for now). Each file is the
//! Linux side of a module that is Windows code elsewhere, picked with
//! `cfg(target_os = "linux")` where that module hands over to it.
//!
//! Two files are not declared here: `island.rs` and `fullscreen.rs` are
//! included by the modules they belong to (as their `platform` and `imp`), since
//! they use what those modules keep private.

pub mod autostart;
pub mod paths;

/// NVIDIA cannot back WebKitGTK's GBM buffers, so draw into shared memory; a
/// setting the person made wins. Called first thing in `run`.
pub fn before_start() {
    let chosen = [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DMABUF_RENDERER_FORCE_SHM",
        "WEBKIT_DMABUF_RENDERER_DISABLE_GBM",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some());
    if !chosen && std::path::Path::new("/proc/driver/nvidia/version").exists() {
        // SAFETY: nothing else is running yet, the first thing `run` does.
        unsafe { std::env::set_var("WEBKIT_DMABUF_RENDERER_FORCE_SHM", "1") };
    }
}
