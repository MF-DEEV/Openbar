// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The battery on Linux, where Windows asks `GetSystemPowerStatus`: the kernel
//! lists every power supply under `/sys/class/power_supply`, the laptop's
//! battery and its charger among them. The files are read when asked and on a
//! slow timer, since sysfs has nothing to wait on.
//!
//! Only the machine's own batteries count: a mouse, a keyboard or a phone
//! lists one too (its `scope` is `Device`), and so does a desktop's UPS.

use std::path::Path;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::system_monitor::Battery;
use crate::system_monitor::math::{BatteryPack, battery_wear};

const SUPPLIES: &str = "/sys/class/power_supply";

/// How often the files are read while there is a battery, and while there is
/// none (one may be plugged in).
const WITH_BATTERY: Duration = Duration::from_secs(5);
const WITHOUT: Duration = Duration::from_secs(30);

fn text(dir: &Path, name: &str) -> Option<String> {
    let value = std::fs::read_to_string(dir.join(name)).ok()?;
    Some(value.trim().to_string()).filter(|v| !v.is_empty())
}

fn number(dir: &Path, name: &str) -> Option<i64> {
    text(dir, name)?.parse().ok()
}

/// One battery, as its files say.
#[derive(Clone, Debug, Default, PartialEq)]
struct Pack {
    percent: Option<f64>,
    status: String,
    /// Energy in µWh, or charge in µAh when that is what the battery counts.
    now: Option<f64>,
    full: Option<f64>,
    design: Option<f64>,
    /// µW, or µA when that is what it counts.
    rate: Option<f64>,
    /// µV, to turn a current into a power.
    volts: Option<f64>,
    seconds_to_empty: Option<f64>,
    cycles: Option<u32>,
    /// Whether `now`, `full` and `design` are charge (µAh), not energy (µWh).
    charge: bool,
}

fn pack(dir: &Path) -> Pack {
    let energy = number(dir, "energy_full").is_some() || number(dir, "energy_now").is_some();
    let (now, full, design, rate) = if energy {
        (
            "energy_now",
            "energy_full",
            "energy_full_design",
            "power_now",
        )
    } else {
        (
            "charge_now",
            "charge_full",
            "charge_full_design",
            "current_now",
        )
    };
    let read = |name: &str| number(dir, name).map(|v| v as f64);
    Pack {
        percent: read("capacity"),
        status: text(dir, "status").unwrap_or_default(),
        now: read(now),
        full: read(full),
        design: read(design),
        rate: read(rate),
        volts: read("voltage_now"),
        seconds_to_empty: read("time_to_empty_now").filter(|s| *s > 0.0),
        cycles: number(dir, "cycle_count").and_then(|c| u32::try_from(c).ok()),
        charge: !energy,
    }
}

/// The machine's batteries and whether a charger is connected.
fn supplies(root: &Path) -> (Vec<Pack>, bool) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return (Vec::new(), false);
    };
    let mut batteries = Vec::new();
    let mut mains = false;
    let mut names: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    names.sort();
    for dir in names {
        match text(&dir, "type").as_deref() {
            Some("Battery") => {
                // A peripheral's battery, or a socket with none in it.
                if text(&dir, "scope").as_deref() == Some("Device")
                    || number(&dir, "present") == Some(0)
                {
                    continue;
                }
                batteries.push(pack(&dir));
            }
            // A charger of any kind (mains, USB-C, wireless); not the device
            // supplies of a peripheral.
            Some(_)
                if number(&dir, "online") == Some(1)
                    && text(&dir, "scope").as_deref() != Some("Device") =>
            {
                mains = true;
            }
            Some(_) | None => {}
        }
    }
    (batteries, mains)
}

fn watts(pack: &Pack) -> Option<f64> {
    let rate = pack.rate?;
    if pack.charge {
        // A current and a voltage make a power.
        Some(rate * pack.volts? / 1e12)
    } else {
        Some(rate / 1e6)
    }
}

/// What the island shows for these supplies; None without a battery.
fn summary(packs: &[Pack], mains: bool) -> Option<Battery> {
    if packs.is_empty() {
        return None;
    }
    let charging = packs.iter().any(|p| p.status == "Charging");
    let discharging = packs.iter().any(|p| p.status == "Discharging");
    let full = packs.iter().any(|p| p.status == "Full");
    let plugged_in = mains || charging || full || packs.iter().any(|p| p.status == "Not charging");

    // The charge as a share of everything the batteries hold, else the average
    // of what each says.
    let (now, hold) = packs
        .iter()
        .fold((0.0, 0.0), |(n, f), p| match (p.now, p.full) {
            (Some(now), Some(full)) if full > 0.0 => (n + now, f + full),
            _ => (n, f),
        });
    let percent = if hold > 0.0 {
        Some((now / hold * 100.0).round())
    } else {
        let says: Vec<f64> = packs.iter().filter_map(|p| p.percent).collect();
        (!says.is_empty()).then(|| (says.iter().sum::<f64>() / says.len() as f64).round())
    }
    .map(|p| p.clamp(0.0, 100.0) as u8);

    // Watts into the battery while it charges, out of it while it drains.
    let rate_watts = packs
        .iter()
        .filter(|p| p.status == "Charging" || p.status == "Discharging")
        .map(|p| watts(p).map(|w| w.abs() * if p.status == "Charging" { 1.0 } else { -1.0 }))
        .try_fold(0.0, |sum, w| w.map(|w| sum + w))
        .filter(|_| charging || discharging);

    // Left on battery: the kernel's own estimate, else the energy over the draw.
    let seconds_left = (discharging && !plugged_in)
        .then(|| {
            let own: Vec<f64> = packs.iter().filter_map(|p| p.seconds_to_empty).collect();
            if own.len() == packs.len() {
                return Some(own.iter().sum::<f64>() / own.len() as f64);
            }
            let draw = -rate_watts?;
            let energy: f64 = packs
                .iter()
                .filter(|p| !p.charge)
                .filter_map(|p| p.now)
                .sum::<f64>()
                / 1e6;
            (draw > 0.0 && energy > 0.0).then(|| energy / draw * 3600.0)
        })
        .flatten()
        .map(|s| s.round().clamp(0.0, f64::from(u32::MAX - 1)) as u32);

    let wear: Vec<BatteryPack> = packs
        .iter()
        .map(|p| BatteryPack {
            relative: false,
            designed: p.design.map_or(0, |d| d as u32),
            full_charged: p.full.map_or(0, |f| f as u32),
            cycles: p.cycles.unwrap_or(0),
        })
        .collect();
    let (health_percent, cycle_count) = battery_wear(&wear);
    Some(Battery {
        percent,
        charging,
        plugged_in,
        rate_watts,
        seconds_left,
        health_percent,
        cycle_count,
    })
}

pub fn read_from(root: &Path) -> Option<Battery> {
    let (packs, mains) = supplies(root);
    summary(&packs, mains)
}

pub fn read() -> Option<Battery> {
    read_from(Path::new(SUPPLIES))
}

/// Tells every window when what the battery says changes.
pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("island-battery".into())
        .spawn(move || {
            let mut last = read();
            loop {
                std::thread::sleep(if last.is_some() {
                    WITH_BATTERY
                } else {
                    WITHOUT
                });
                let next = read();
                if next != last {
                    let _ = app.emit("power://battery", &next);
                    last = next;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A power supply folder with these files.
    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, value) in files {
            std::fs::write(dir.join(file), format!("{value}\n")).unwrap();
        }
    }

    fn sysfs(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("openbar-power-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A ThinkPad-like battery that counts energy, draining at 10 W.
    fn draining(root: &Path) {
        supply(
            root,
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("capacity", "50"),
                ("energy_now", "30000000"),
                ("energy_full", "60000000"),
                ("energy_full_design", "70000000"),
                ("power_now", "10000000"),
                ("cycle_count", "212"),
            ],
        );
        supply(root, "AC", &[("type", "Mains"), ("online", "0")]);
    }

    #[test]
    fn a_draining_battery_says_its_charge_rate_time_and_wear() {
        let root = sysfs("draining");
        draining(&root);
        let battery = read_from(&root).unwrap();
        assert_eq!(battery.percent, Some(50));
        assert!(!battery.charging && !battery.plugged_in);
        // 10 W out of it; 30 Wh at 10 W is three hours.
        assert_eq!(battery.rate_watts, Some(-10.0));
        assert_eq!(battery.seconds_left, Some(3 * 3600));
        // 60 of 70 Wh is 85.7% of what it was.
        assert!((battery.health_percent.unwrap() - 85.714).abs() < 0.01);
        assert_eq!(battery.cycle_count, Some(212));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_charging_battery_on_the_charger_has_no_time_left_to_show() {
        let root = sysfs("charging");
        draining(&root);
        supply(
            &root,
            "BAT0",
            &[("status", "Charging"), ("power_now", "25000000")],
        );
        supply(&root, "AC", &[("online", "1")]);
        let battery = read_from(&root).unwrap();
        assert!(battery.charging && battery.plugged_in);
        assert_eq!(battery.rate_watts, Some(25.0));
        assert_eq!(battery.seconds_left, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_full_battery_on_the_charger_is_plugged_in_and_not_charging() {
        let root = sysfs("full");
        draining(&root);
        supply(
            &root,
            "BAT0",
            &[
                ("status", "Full"),
                ("capacity", "100"),
                ("energy_now", "60000000"),
            ],
        );
        supply(&root, "AC", &[("online", "1")]);
        let battery = read_from(&root).unwrap();
        assert_eq!(battery.percent, Some(100));
        assert!(!battery.charging && battery.plugged_in);
        assert_eq!(battery.rate_watts, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_battery_that_counts_charge_gives_its_power_from_current_and_voltage() {
        let root = sysfs("charge");
        supply(
            &root,
            "BAT1",
            &[
                ("type", "Battery"),
                ("status", "Discharging"),
                ("charge_now", "2000000"),
                ("charge_full", "4000000"),
                ("charge_full_design", "5000000"),
                ("current_now", "1000000"),
                ("voltage_now", "12000000"),
            ],
        );
        let battery = read_from(&root).unwrap();
        // 1 A at 12 V is 12 W out; half of 4 Ah is 50%.
        assert_eq!(battery.rate_watts, Some(-12.0));
        assert_eq!(battery.percent, Some(50));
        assert!((battery.health_percent.unwrap() - 80.0).abs() < 0.001);
        // Without the kernel's estimate or an energy to divide, no time is guessed.
        assert_eq!(battery.seconds_left, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_kernels_own_estimate_wins_and_two_batteries_are_one() {
        let root = sysfs("two");
        for (name, now) in [("BAT0", "20000000"), ("BAT1", "40000000")] {
            supply(
                &root,
                name,
                &[
                    ("type", "Battery"),
                    ("status", "Discharging"),
                    ("energy_now", now),
                    ("energy_full", "50000000"),
                    ("energy_full_design", "50000000"),
                    ("power_now", "6000000"),
                    ("time_to_empty_now", "5400"),
                    ("cycle_count", if name == "BAT0" { "30" } else { "90" }),
                ],
            );
        }
        let battery = read_from(&root).unwrap();
        // 60 of 100 Wh; 12 W out of the two; the most worn one's cycles.
        assert_eq!(battery.percent, Some(60));
        assert_eq!(battery.rate_watts, Some(-12.0));
        assert_eq!(battery.seconds_left, Some(5400));
        assert_eq!(battery.cycle_count, Some(90));
        assert_eq!(battery.health_percent, Some(100.0));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_peripherals_battery_and_a_desktop_are_not_a_battery() {
        let root = sysfs("none");
        // A mouse's battery, a UPS-less desktop's charger-less supplies.
        supply(
            &root,
            "hidpp_battery_0",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
                ("capacity", "40"),
            ],
        );
        supply(
            &root,
            "ucsi-source",
            &[("type", "USB"), ("scope", "Device"), ("online", "1")],
        );
        assert_eq!(read_from(&root), None);
        // A battery bay with no battery in it.
        supply(&root, "BAT0", &[("type", "Battery"), ("present", "0")]);
        assert_eq!(read_from(&root), None);
        assert_eq!(read_from(&root.join("not-there")), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_battery_that_only_says_its_capacity_is_still_read() {
        let root = sysfs("capacity");
        supply(
            &root,
            "BAT0",
            &[
                ("type", "Battery"),
                ("status", "Unknown"),
                ("capacity", "73"),
            ],
        );
        let battery = read_from(&root).unwrap();
        assert_eq!(battery.percent, Some(73));
        assert!(!battery.charging && !battery.plugged_in);
        assert_eq!(
            (
                battery.rate_watts,
                battery.seconds_left,
                battery.health_percent,
                battery.cycle_count
            ),
            (None, None, None, None)
        );
        // A USB-C charger counts as a charger.
        supply(
            &root,
            "ucsi-source-psy-1",
            &[("type", "USB"), ("online", "1")],
        );
        assert!(read_from(&root).unwrap().plugged_in);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "reads this machine's power supplies"]
    fn what_this_machine_has() {
        println!("battery: {:?}", read());
    }
}
