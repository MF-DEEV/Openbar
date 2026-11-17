// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! What the parts that talk to the network through libsoup share: the speed test
//! (`speed_test.rs`) and the daily price list (`agent_usage/prices.rs`) on Linux,
//! where Windows has WinHTTP.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Instant;

use gtk::gio::{self, prelude::*};

/// Cancels `cancellable` at `until`, unless this is dropped first.
pub struct Deadline(#[allow(dead_code)] mpsc::Sender<()>);

impl Deadline {
    pub fn new(cancellable: &gio::Cancellable, until: Instant) -> Deadline {
        let (stop, wait) = mpsc::channel::<()>();
        let cancellable = cancellable.clone();
        std::thread::spawn(move || {
            let left = until.saturating_duration_since(Instant::now());
            if wait.recv_timeout(left) == Err(RecvTimeoutError::Timeout) {
                cancellable.cancel();
            }
        });
        Deadline(stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_deadline_cancels_when_it_comes_and_not_when_it_is_dropped_first() {
        let late = gio::Cancellable::new();
        let guard = Deadline::new(&late, Instant::now() + Duration::from_millis(150));
        assert!(!late.is_cancelled());
        std::thread::sleep(Duration::from_millis(400));
        assert!(late.is_cancelled());
        drop(guard);

        let early = gio::Cancellable::new();
        drop(Deadline::new(
            &early,
            Instant::now() + Duration::from_millis(150),
        ));
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            !early.is_cancelled(),
            "a request that finished is not cancelled afterwards"
        );
    }
}
