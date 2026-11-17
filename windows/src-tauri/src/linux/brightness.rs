// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Display brightness on Linux, where Windows uses WMI for a laptop's panel
//! and DDC/CI for an external display:
//!
//! - a panel's own backlight is the kernel's (`/sys/class/backlight`), which
//!   is written through systemd-logind so that no root is needed,
//! - an external display is asked over DDC/CI with `ddcutil` (VCP code 0x10),
//!   which wants access to the `/dev/i2c-*` devices (the `i2c` group).
//!
//! The displays are read when the page asks, never polled, and the slider's
//! writes are coalesced by `brightness.rs` as on Windows.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use zbus::blocking::{Connection, Proxy, connection::Builder};

use crate::brightness::{Display, from_device, to_device};

const BACKLIGHTS: &str = "/sys/class/backlight";

/// Each DDC display's maximum, as last read; the write scales to it.
fn maxima() -> &'static Mutex<HashMap<String, u32>> {
    static M: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ddcutil(arguments: &[&str]) -> Option<String> {
    super::command::run("ddcutil", &[("LC_ALL", "C")], arguments, None)
}

/// The displays `ddcutil detect --terse` lists: the I2C bus each is on, and
/// what it calls itself ("ACR:KG251Q:T8ZEE0018524": maker, model, serial).
fn parse_detect(text: &str) -> Vec<(u32, String)> {
    let mut found = Vec::new();
    let mut bus: Option<u32> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("Display ") {
            bus = None;
        } else if let Some(device) = line.strip_prefix("I2C bus:") {
            bus = device
                .trim()
                .strip_prefix("/dev/i2c-")
                .and_then(|n| n.parse().ok());
        } else if let Some(monitor) = line.strip_prefix("Monitor:")
            && let Some(bus) = bus.take()
        {
            found.push((bus, monitor.trim().to_string()));
        }
    }
    found
}

/// What a display is called: its model, else its maker.
fn display_name(monitor: &str, bus: u32) -> String {
    let mut parts = monitor.split(':');
    let (maker, model) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    [model, maker]
        .into_iter()
        .find(|part| !part.is_empty())
        .map_or_else(|| format!("Display {bus}"), str::to_string)
}

/// `VCP 10 C 35 100`: the brightness now and its range.
fn parse_vcp(text: &str) -> Option<(u32, u32)> {
    let mut words = text.split_whitespace();
    (words.next()? == "VCP" && words.next()? == "10" && words.next()? == "C").then_some(())?;
    Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
}

/// The backlight of the panel: the kernel lists one for each way of driving
/// it, and the firmware's is preferred, then the platform's, then the raw one.
fn panel(root: &Path) -> Option<(String, u32, u32)> {
    let mut found: Vec<(u8, String, u32, u32)> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let read = |name: &str| -> Option<String> {
                Some(
                    std::fs::read_to_string(dir.join(name))
                        .ok()?
                        .trim()
                        .to_string(),
                )
            };
            let max: u32 = read("max_brightness")?.parse().ok().filter(|m| *m > 0)?;
            let now: u32 = read("brightness")?.parse().ok()?;
            let rank = match read("type").as_deref() {
                Some("firmware") => 0,
                Some("platform") => 1,
                _ => 2,
            };
            Some((
                rank,
                entry.file_name().to_string_lossy().into_owned(),
                now,
                max,
            ))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .next()
        .map(|(_, name, now, max)| (name, now, max))
}

/// A backlight name the kernel lists, which is all `set` will write to.
fn backlight_listed(root: &Path, name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && name != "."
        && name != ".."
        && root.join(name).join("max_brightness").is_file()
}

pub fn displays() -> Vec<Display> {
    let mut found = Vec::new();
    if let Some((name, now, max)) = panel(Path::new(BACKLIGHTS)) {
        found.push(Display {
            id: format!("backlight:{name}"),
            name: "Built-in display".into(),
            brightness: from_device(now, max),
            built_in: true,
        });
    }
    let detected = ddcutil(&["detect", "--terse"])
        .map(|text| parse_detect(&text))
        .unwrap_or_default();
    for (bus, monitor) in detected {
        // A display that does not answer for brightness is not one to show.
        let Some((now, max)) = ddcutil(&["--bus", &bus.to_string(), "getvcp", "10", "--terse"])
            .and_then(|text| parse_vcp(&text))
        else {
            continue;
        };
        let id = format!("ddc:bus{bus}");
        maxima().lock().unwrap().insert(id.clone(), max);
        found.push(Display {
            name: display_name(&monitor, bus),
            id,
            brightness: from_device(now, max),
            built_in: false,
        });
    }
    found
}

/// Sets a panel's backlight through logind, which lets the active session
/// do it without root.
fn set_backlight_through(connection: &Connection, name: &str, value: u32) -> zbus::Result<()> {
    Proxy::new(
        connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
    )?
    .call("SetBrightness", &("backlight", name, value))
}

fn set_backlight(root: &Path, name: &str, level: f32) -> Result<(), String> {
    if !backlight_listed(root, name) {
        return Err("That display is not there".into());
    }
    let max: u32 = std::fs::read_to_string(root.join(name).join("max_brightness"))
        .ok()
        .and_then(|m| m.trim().parse().ok())
        .ok_or("The display does not say its range")?;
    // Zero is dark on most panels, and nothing on the screen to bring it back.
    let value = to_device(level, max).max(1);
    let through = Builder::system()
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())
        .and_then(|c| set_backlight_through(&c, name, value).map_err(|e| e.to_string()));
    through.or_else(|e| {
        // Where a udev rule lets the person write the file, that works too.
        std::fs::write(root.join(name).join("brightness"), value.to_string())
            .map_err(|_| format!("The backlight was not set: {e}"))
    })
}

fn set_ddc(id: &str, bus: &str, level: f32) -> Result<(), String> {
    let max = maxima().lock().unwrap().get(id).copied().unwrap_or(100);
    let value = to_device(level, max).to_string();
    ddcutil(&["--bus", bus, "setvcp", "10", &value, "--noverify"])
        .map(|_| ())
        .ok_or_else(|| "The display did not take the brightness (ddcutil failed)".into())
}

pub fn set(id: &str, value: f32) -> Result<(), String> {
    if let Some(name) = id.strip_prefix("backlight:") {
        return set_backlight(Path::new(BACKLIGHTS), name, value);
    }
    match id
        .strip_prefix("ddc:bus")
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && n.len() < 6)
    {
        Some(bus) => set_ddc(id, bus, value),
        None => Err("That display is not one this can set".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::sync::{Arc, Mutex};

    // What `ddcutil detect --terse` prints on a machine with two monitors.
    const DETECT: &str = "Display 1\n   I2C bus:          /dev/i2c-3\n   DRM connector:    card1-HDMI-A-1\n   drm_connector_id: 830\n   Monitor:          ACR:KG251Q:T8ZEE0018524\n\nDisplay 2\n   I2C bus:          /dev/i2c-4\n   DRM connector:    card1-DP-1\n   Monitor:          IVM:PL2770H:\n\nInvalid display\n   I2C bus:          /dev/i2c-9\n   EDID synopsis: none\n";

    #[test]
    fn detected_displays_are_read_with_their_bus_and_name() {
        let found = parse_detect(DETECT);
        assert_eq!(
            found,
            [
                (3, "ACR:KG251Q:T8ZEE0018524".to_string()),
                (4, "IVM:PL2770H:".to_string())
            ]
        );
        assert_eq!(display_name("ACR:KG251Q:T8ZEE0018524", 3), "KG251Q");
        assert_eq!(display_name("IVM::", 4), "IVM");
        assert_eq!(display_name("", 5), "Display 5");
        assert!(parse_detect("").is_empty());
        assert!(parse_detect("No displays found.\n").is_empty());
    }

    #[test]
    fn a_brightness_reading_is_its_value_and_range() {
        assert_eq!(parse_vcp("VCP 10 C 35 100\n"), Some((35, 100)));
        assert_eq!(parse_vcp("VCP 10 C 5 100"), Some((5, 100)));
        // Another feature, a failure, and noise are not a reading.
        assert_eq!(parse_vcp("VCP 12 C 5 100"), None);
        assert_eq!(parse_vcp("VCP 10 ERR"), None);
        assert_eq!(parse_vcp("Display not found"), None);
        assert_eq!(parse_vcp(""), None);
    }

    fn backlights(name: &str, entries: &[(&str, &str, &str, &str)]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("openbar-light-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, kind, now, max) in entries {
            let path = root.join(dir);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("type"), format!("{kind}\n")).unwrap();
            std::fs::write(path.join("brightness"), format!("{now}\n")).unwrap();
            std::fs::write(path.join("max_brightness"), format!("{max}\n")).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn the_panel_is_the_one_the_firmware_drives_before_the_raw_one() {
        let root = backlights(
            "panel",
            &[
                ("intel_backlight", "raw", "400", "1000"),
                ("acpi_video0", "firmware", "5", "10"),
                ("nvidia_0", "platform", "50", "100"),
            ],
        );
        assert_eq!(panel(&root), Some(("acpi_video0".into(), 5, 10)));
        std::fs::remove_dir_all(root.join("acpi_video0")).unwrap();
        assert_eq!(panel(&root), Some(("nvidia_0".into(), 50, 100)));
        std::fs::remove_dir_all(root.join("nvidia_0")).unwrap();
        assert_eq!(panel(&root), Some(("intel_backlight".into(), 400, 1000)));
        // A backlight with no range is not usable; none at all is none.
        std::fs::write(root.join("intel_backlight").join("max_brightness"), "0\n").unwrap();
        assert_eq!(panel(&root), None);
        assert_eq!(panel(&root.join("nowhere")), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_a_backlight_the_kernel_lists_is_written() {
        let root = backlights("listed", &[("intel_backlight", "raw", "400", "1000")]);
        assert!(backlight_listed(&root, "intel_backlight"));
        for bad in ["", ".", "..", "../x", "a/b", "missing"] {
            assert!(!backlight_listed(&root, bad), "{bad:?}");
        }
        assert!(set_backlight(&root, "../../etc", 0.5).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn displays_other_than_the_two_kinds_are_refused() {
        for id in [
            "",
            "wmi:x",
            "ddc:",
            "ddc:bus",
            "ddc:bus3;reboot",
            "ddc:bus-1",
            "ddc:bus1234567",
            "ddc:3",
        ] {
            assert!(set(id, 0.5).is_err(), "{id:?}");
        }
    }

    struct Session(Arc<Mutex<Vec<String>>>);

    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl Session {
        fn set_brightness(&self, subsystem: String, name: String, brightness: u32) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{subsystem} {name} {brightness}"));
        }
    }

    #[test]
    fn a_panel_is_set_through_logind() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let _logind = bus.serve("org.freedesktop.login1", |b| {
            b.serve_at("/org/freedesktop/login1/session/auto", Session(log.clone()))
                .unwrap()
        });
        set_backlight_through(&bus.connect(), "intel_backlight", 250).unwrap();
        assert_eq!(*log.lock().unwrap(), ["backlight intel_backlight 250"]);
    }

    #[test]
    #[ignore = "asks this machine's monitors over DDC/CI; OPENBAR_LIVE_WRITE=1 also writes each one's own value back"]
    fn the_monitors_of_this_machine_are_read_and_taken_at_their_word() {
        let found = displays();
        println!("{found:?}");
        assert!(!found.is_empty(), "no display answered");
        if std::env::var("OPENBAR_LIVE_WRITE").is_ok() {
            for display in found.iter().filter(|d| !d.built_in) {
                // The value it has now: nothing on the screen changes.
                set(&display.id, display.brightness).unwrap();
                let again = displays().into_iter().find(|d| d.id == display.id).unwrap();
                assert!(
                    (again.brightness - display.brightness).abs() < 0.011,
                    "{display:?} -> {again:?}"
                );
                println!("{} took its own {} back", display.name, display.brightness);
            }
        }
    }
}
