// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A bus of its own for the tests that talk D-Bus: a private `dbus-daemon`,
//! so the services written in a test and the code under test meet over the
//! real protocol without touching the desktop's bus.

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::time::Duration;

use zbus::blocking::{Connection, connection::Builder};

/// A bus, gone with the value. `None` where `dbus-daemon` is not installed.
pub struct PrivateBus {
    child: Child,
    pub address: String,
}

impl PrivateBus {
    pub fn start() -> Option<Self> {
        // Tests run side by side in one process: a folder for each bus.
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "openbar-bus-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).ok()?;
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
            <policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#,
        )
        .ok()?;
        let mut child = std::process::Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1", "--config-file"])
            .arg(&config)
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(child.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        std::fs::remove_dir_all(dir).ok();
        Some(Self {
            child,
            address: address.trim().to_string(),
        })
    }

    /// A client of the bus, with the reply limit the code under test sets.
    pub fn connect(&self) -> Connection {
        Builder::address(self.address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(2))
            .build()
            .unwrap()
    }

    /// A connection that owns `name` and serves what `serve` adds to it.
    pub fn serve(
        &self,
        name: &str,
        serve: impl FnOnce(Builder<'static>) -> Builder<'static>,
    ) -> Connection {
        serve(
            Builder::address(self.address.as_str())
                .unwrap()
                .name(name.to_string())
                .unwrap(),
        )
        .build()
        .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        // Asked to end, the daemon removes its socket; killed, it leaves it.
        // SAFETY: a signal to a child this value started and has not reaped.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let _ = self.child.wait();
    }
}
