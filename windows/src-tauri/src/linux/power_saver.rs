// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The energy saver on Linux, where Windows follows Battery Saver and Energy
//! saver: the "power-saver" profile of power-profiles-daemon (GNOME, KDE and
//! XFCE's power settings switch it), which the island's Clawds treat as Low
//! Power Mode. `power_saver.rs` includes this file as `imp`, since it keeps
//! its state private.

use std::collections::HashMap;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use zbus::blocking::{Connection, Proxy, connection::Builder};
use zbus::zvariant::OwnedValue;

use super::{APP, EVENT, Setting, state, update};

/// The daemon's name, path and interface (one name for all): newer versions
/// are UPower's, older ones their own.
const SERVICES: [(&str, &str); 2] = [
    (
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
    ),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
];

/// Whether a profile saves energy: only "power-saver" does.
fn saving(profile: &str) -> bool {
    profile == "power-saver"
}

fn profile(value: &OwnedValue) -> Option<bool> {
    match &**value {
        zbus::zvariant::Value::Str(s) => Some(saving(s.as_str())),
        zbus::zvariant::Value::Value(inner) => match &**inner {
            zbus::zvariant::Value::Str(s) => Some(saving(s.as_str())),
            _ => None,
        },
        _ => None,
    }
}

/// The profile now and then every change of it, as "saving or not", until
/// the daemon goes away; false when the daemon is not on this bus.
fn watch(connection: &Connection, mut on_change: impl FnMut(bool)) -> bool {
    for (name, path) in SERVICES {
        let Ok(properties) = Proxy::new(connection, name, path, "org.freedesktop.DBus.Properties")
        else {
            continue;
        };
        // Listening first, so a change between the read and the wait is not lost.
        let Ok(changes) = properties.receive_signal("PropertiesChanged") else {
            continue;
        };
        let Ok(current) = properties.call::<_, _, OwnedValue>("Get", &(name, "ActiveProfile"))
        else {
            continue;
        };
        if let Some(on) = profile(&current) {
            on_change(on);
        }
        for message in changes {
            let Ok((interface, changed, _)) =
                message
                    .body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                continue;
            };
            if interface == name
                && let Some(on) = changed.get("ActiveProfile").and_then(profile)
            {
                on_change(on);
            }
        }
        return true;
    }
    false
}

/// A value to `update`, which counts either setting.
fn apply(on: bool) {
    let mut saver = state();
    if let Some(on) = update(&mut saver, Setting::BatterySaver, u32::from(on))
        && let Some(app) = APP.get()
    {
        // Sent under the lock, so the windows hear the changes in order.
        let _ = app.emit(EVENT, on);
    }
}

pub fn start(app: AppHandle) {
    if APP.set(app).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("openbar-power-profile".into())
        .spawn(|| {
            loop {
                let connection = Builder::system().and_then(|b| b.build());
                // No daemon (or no bus) now may be one later.
                let served = connection
                    .as_ref()
                    .is_ok_and(|connection| watch(connection, apply));
                std::thread::sleep(Duration::from_secs(if served { 5 } else { 60 }));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    struct Profiles(Arc<Mutex<String>>);

    #[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
    impl Profiles {
        #[zbus(property)]
        fn active_profile(&self) -> String {
            self.0.lock().unwrap().clone()
        }
        #[zbus(property)]
        fn set_active_profile(&mut self, profile: String) {
            *self.0.lock().unwrap() = profile;
        }
    }

    #[test]
    fn only_the_power_saver_profile_saves_energy() {
        assert!(saving("power-saver"));
        for other in ["balanced", "performance", "", "Power-Saver"] {
            assert!(!saving(other));
        }
    }

    #[test]
    fn the_profile_is_read_and_its_changes_followed() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let held = Arc::new(Mutex::new("balanced".to_string()));
        let service = bus.serve("org.freedesktop.UPower.PowerProfiles", |b| {
            b.serve_at(
                "/org/freedesktop/UPower/PowerProfiles",
                Profiles(held.clone()),
            )
            .unwrap()
        });
        let client = bus.connect();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            watch(&client, |on| {
                let _ = tx.send(on);
            })
        });
        let heard = |wait: u64| rx.recv_timeout(Duration::from_secs(wait)).ok();
        assert_eq!(heard(3), Some(false), "the profile as it is now");
        // Changed by a settings panel: the service says so.
        let changer = bus.connect();
        Proxy::new(
            &changer,
            "org.freedesktop.UPower.PowerProfiles",
            "/org/freedesktop/UPower/PowerProfiles",
            "org.freedesktop.DBus.Properties",
        )
        .unwrap()
        .call::<_, _, ()>(
            "Set",
            &(
                "org.freedesktop.UPower.PowerProfiles",
                "ActiveProfile",
                zbus::zvariant::Value::from("power-saver"),
            ),
        )
        .unwrap();
        assert_eq!(heard(3), Some(true));
        assert_eq!(*held.lock().unwrap(), "power-saver");
        drop(service);
    }

    #[test]
    fn no_daemon_is_not_an_error() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let client = bus.connect();
        let mut heard = Vec::new();
        assert!(!watch(&client, |on| heard.push(on)));
        assert!(heard.is_empty());
    }
}
