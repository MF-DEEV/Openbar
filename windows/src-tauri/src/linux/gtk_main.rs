// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! GTK may only be used from the thread that started it. Code that runs on
//! another thread (a worker, a command) hands its GTK work over here and
//! waits for the answer.

use std::time::Duration;

use gtk::glib;

/// Runs `job` on the thread GTK may be used from, and waits for it. None when
/// GTK is not running (no display), or the main thread did not get to it.
pub fn on_main<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    glib::MainContext::default().invoke(move || {
        // Without GTK (no display) there is nothing to hand over to.
        if gtk::is_initialized_main_thread() {
            let _ = tx.send(job());
        }
    });
    rx.recv_timeout(Duration::from_secs(5)).ok()
}
