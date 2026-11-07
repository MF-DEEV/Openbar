// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The System section's readings on Linux, where Windows asks PDH and the
//! Win32 APIs: the kernel's own files, `/proc` and `/sys`.
//!
//! - CPU from `/proc/stat`, memory and swap from `/proc/meminfo` (and zram's
//!   compressed size), network counters from `/proc/net/dev` on physical
//!   interfaces, disk activity from `/proc/diskstats` on physical disks, the
//!   root filesystem's space from `statvfs`,
//! - the GPU from `nvidia-smi` or an AMD card's `gpu_busy_percent`,
//! - connected devices from the USB bus in `/sys` and BlueZ's list,
//! - the open detail's processes from `/proc/<pid>`, grouped by program.
//!
//! The processes' network traffic would need privileges (or eBPF) to
//! attribute, so that list says it is unavailable, as Windows does before its
//! helper is approved. `system_monitor.rs` includes this file as `imp`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::*;

// MARK: Readings, from the text the kernel gives

/// `cpu  user nice system idle iowait irq softirq steal ...` as the (idle,
/// kernel with idle, user) `cpu_from_times` takes.
fn parse_cpu(stat: &str) -> Option<(u64, u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let n: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .map(|v| v.parse().unwrap_or(0))
        .collect();
    let at = |i: usize| n.get(i).copied().unwrap_or(0);
    let (user, nice, system, idle, iowait) = (at(0), at(1), at(2), at(3), at(4));
    let (irq, softirq, steal) = (at(5), at(6), at(7));
    if n.len() < 4 {
        return None;
    }
    let idle_all = idle + iowait;
    Some((
        idle_all,
        system + irq + softirq + steal + idle_all,
        user + nice,
    ))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Memory {
    total: u64,
    available: u64,
    cached: u64,
    swap_used: Option<u64>,
}

/// `/proc/meminfo`, whose values are kibibytes.
fn parse_meminfo(text: &str) -> Option<Memory> {
    let values: HashMap<&str, u64> = text
        .lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            Some((
                name,
                rest.split_whitespace().next()?.parse::<u64>().ok()? * 1024,
            ))
        })
        .collect();
    let get = |name: &str| values.get(name).copied();
    let total = get("MemTotal").filter(|t| *t > 0)?;
    let swap = get("SwapTotal").filter(|t| *t > 0);
    Some(Memory {
        total,
        available: get("MemAvailable").or_else(|| get("MemFree"))?.min(total),
        cached: get("Cached").unwrap_or(0) + get("Buffers").unwrap_or(0),
        swap_used: swap.map(|total| total.saturating_sub(get("SwapFree").unwrap_or(total))),
    })
}

/// `/proc/net/dev`: each interface's bytes received and sent.
fn parse_net_dev(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let columns: Vec<u64> = rest
                .split_whitespace()
                .map(|v| v.parse().unwrap_or(0))
                .collect();
            Some((
                name.trim().to_string(),
                (*columns.first()?, *columns.get(8)?),
            ))
        })
        .collect()
}

/// `/proc/diskstats`: each device's sectors (of 512 bytes) read and written.
fn parse_diskstats(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            let name = columns.get(2)?.to_string();
            Some((
                name,
                (columns.get(5)?.parse().ok()?, columns.get(9)?.parse().ok()?),
            ))
        })
        .collect()
}

/// `pid (comm) state ppid ...`: the name, the CPU ticks used (user and
/// system) and when the process started, in ticks since boot. The name may
/// hold spaces and brackets, so it is what lies between the first `(` and the
/// last `)`.
fn parse_stat(stat: &str) -> Option<(String, u64, u64)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
    // After the name: state is field 3, so utime is field 14 and starttime 22.
    let at = |field: usize| -> Option<u64> { rest.get(field - 3)?.parse().ok() };
    Some((comm, at(14)? + at(15)?, at(22)?))
}

/// `RssAnon:` in `/proc/<pid>/status`, the memory only this process holds
/// (what Windows calls private bytes), in bytes.
fn parse_rss_anon(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("RssAnon:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
}

/// `nvidia-smi --query-gpu=utilization.gpu`: the busiest card's percent.
fn parse_nvidia_utilization(output: &str) -> Option<f64> {
    output
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .reduce(f64::max)
}

/// `nvidia-smi pmon` rows: (pid, percent of the busiest engine).
fn parse_pmon(output: &str) -> Vec<(u32, f64)> {
    output
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            let pid: u32 = columns.get(1)?.parse().ok().filter(|p| *p != 0)?;
            // sm, mem, enc, dec: a dash where an engine is idle.
            let busiest = columns
                .get(3..7)?
                .iter()
                .filter_map(|v| v.parse::<f64>().ok())
                .fold(0.0, f64::max);
            Some((pid, busiest))
        })
        .collect()
}

// MARK: Readings, from the machine

fn read(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Physical interfaces: ones with a device behind them, not a loopback,
/// bridge, tunnel or the virtual end of a container.
fn physical_interfaces(sys: &Path) -> HashMap<String, u64> {
    let Ok(entries) = std::fs::read_dir(sys.join("class/net")) else {
        return HashMap::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().join("device").exists())
        .filter_map(|e| {
            let index: u64 = read(e.path().join("ifindex"))?.trim().parse().ok()?;
            Some((e.file_name().to_string_lossy().into_owned(), index))
        })
        .collect()
}

/// Each physical interface's (received, sent) by its index.
fn network_counters(proc: &Path, sys: &Path) -> Option<Interfaces> {
    let counters = parse_net_dev(&read(proc.join("net/dev"))?);
    let physical = physical_interfaces(sys);
    if physical.is_empty() {
        return None;
    }
    Some(
        physical
            .into_iter()
            .filter_map(|(name, index)| Some((index, *counters.get(&name)?)))
            .collect(),
    )
}

/// Bytes read and written by every physical disk (not a partition of one, a
/// loop device, or a mapper over one, which would count the same bytes twice).
fn disk_counters(proc: &Path, sys: &Path) -> Option<(u64, u64)> {
    let stats = parse_diskstats(&read(proc.join("diskstats"))?);
    let (read_bytes, written) = stats
        .iter()
        .filter(|(name, _)| sys.join("block").join(name).join("device").exists())
        .fold((0u64, 0u64), |(r, w), (_, (read, wrote))| {
            (r.saturating_add(*read), w.saturating_add(*wrote))
        });
    Some((read_bytes.saturating_mul(512), written.saturating_mul(512)))
}

/// Bytes the compressed part of memory takes (zram's), when there is any.
fn zram_compressed(sys: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(sys.join("block")).ok()?;
    let sizes: Vec<u64> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("zram"))
        .filter_map(|e| read(e.path().join("mm_stat")))
        // orig_data_size, compr_data_size, mem_used_total, ...
        .filter_map(|stat| stat.split_whitespace().nth(1)?.parse().ok())
        .collect();
    (!sizes.is_empty()).then(|| sizes.iter().sum())
}

fn disk_space() -> Option<Disk> {
    let path = std::ffi::CString::new("/").ok()?;
    // SAFETY: `statvfs` writes into a zeroed struct of its own type.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: the path is a valid C string and `stats` is a valid out pointer.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let unit = stats.f_frsize as u64;
    Some(Disk {
        name: "/".into(),
        free: (stats.f_bavail as u64).saturating_mul(unit),
        total: (stats.f_blocks as u64).saturating_mul(unit),
    })
    .filter(|d| d.total > 0)
}

/// What the USB bus lists that is a device the person plugged in: not a root
/// hub, a hub, a billboard, an interface, or something soldered inside.
fn usb_devices(sys: &Path) -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir(sys.join("bus/usb/devices")) else {
        return Vec::new();
    };
    let mut found: Vec<Device> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let dir = entry.path();
            // `usb1` is a root hub; `1-2:1.0` is an interface of a device.
            if name.contains(':') || name.starts_with("usb") {
                return None;
            }
            let text = |file: &str| read(dir.join(file)).map(|v| v.trim().to_string());
            if matches!(text("bDeviceClass").as_deref(), Some("09" | "11")) {
                return None;
            }
            if text("removable").as_deref() == Some("fixed") {
                return None;
            }
            let product = text("product").filter(|p| !p.is_empty())?;
            Some(Device {
                name: product,
                bluetooth: false,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found.dedup_by(|a, b| a.name == b.name);
    found
}

/// The Bluetooth devices BlueZ says are connected now.
fn bluetooth_devices() -> Vec<Device> {
    use zbus::blocking::{Proxy, connection::Builder};
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};
    type Interfaces = HashMap<String, HashMap<String, OwnedValue>>;
    let Ok(connection) = Builder::system().and_then(|b| b.build()) else {
        return Vec::new();
    };
    let Ok(manager) = Proxy::new(
        &connection,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    ) else {
        return Vec::new();
    };
    let Ok(objects) =
        manager.call::<_, _, HashMap<OwnedObjectPath, Interfaces>>("GetManagedObjects", &())
    else {
        return Vec::new();
    };
    let text = |value: &OwnedValue| match &**value {
        zbus::zvariant::Value::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let mut found: Vec<Device> = objects
        .values()
        .filter_map(|interfaces| interfaces.get("org.bluez.Device1"))
        .filter(|device| {
            device
                .get("Connected")
                .is_some_and(|c| matches!(&**c, zbus::zvariant::Value::Bool(true)))
        })
        .filter_map(|device| {
            device
                .get("Alias")
                .and_then(text)
                .or_else(|| device.get("Name").and_then(text))
        })
        .map(|name| Device {
            name,
            bluetooth: true,
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

fn command_output(program: &str, arguments: &[&str]) -> Option<String> {
    crate::linux::command::run(program, &[("LC_ALL", "C")], arguments, None)
}

fn uptime_seconds(proc: &Path) -> u64 {
    read(proc.join("uptime"))
        .and_then(|u| u.split_whitespace().next()?.parse::<f64>().ok())
        .map_or(0, |s| s as u64)
}

/// A running program, as `/proc` shows it.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    pid: u32,
    /// When it started, to tell a reused pid from the program it had.
    started: u64,
    /// The program's file name, or the kernel's short name for it.
    image: String,
    cpu_ticks: u64,
    private_bytes: u64,
}

/// Every process of a program (no kernel threads), by its pid and start.
fn read_processes(proc: &Path, with_memory: bool) -> Vec<Entry> {
    let Ok(entries) = std::fs::read_dir(proc) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_string_lossy().parse().ok()?;
            let dir = entry.path();
            // Kernel threads have no command line.
            if read(dir.join("cmdline")).is_none_or(|c| c.is_empty()) {
                return None;
            }
            let (comm, cpu_ticks, started) = parse_stat(&read(dir.join("stat"))?)?;
            let image = std::fs::read_link(dir.join("exe"))
                .ok()
                .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
                .map(|name| name.trim_end_matches(" (deleted)").to_string())
                .unwrap_or(comm);
            let private_bytes = if with_memory {
                read(dir.join("status"))
                    .and_then(|s| parse_rss_anon(&s))
                    .unwrap_or(0)
            } else {
                0
            };
            Some(Entry {
                pid,
                started,
                image,
                cpu_ticks,
                private_bytes,
            })
        })
        .collect()
}

/// The kernel's clock ticks a second, to turn a process's CPU time into the
/// 100 ns units the shared arithmetic takes.
fn ticks_to_100ns() -> u64 {
    // SAFETY: a plain query of a constant.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    10_000_000 / u64::try_from(hz).ok().filter(|h| *h > 0).unwrap_or(100)
}

type Key = (u32, u64);

pub fn approve_network() -> bool {
    false
}

/// What the System section reads, from where it reads it.
pub struct Sampler {
    proc: PathBuf,
    sys: PathBuf,
    nvidia: bool,
    amd: Option<PathBuf>,
    times: Option<(u64, u64, u64)>,
    net: Option<Interfaces>,
    net_baseline: Interfaces,
    disk_io: Option<(u64, u64)>,
    last: Instant,
    processes: HashMap<Key, u64>,
    names: HashMap<String, (Option<String>, String)>,
    devices: Option<Vec<Device>>,
    tick: u64,
    processors: u32,
}

/// An AMD card's busy file, if the machine has one.
fn amd_busy(sys: &Path) -> Option<PathBuf> {
    std::fs::read_dir(sys.join("class/drm"))
        .ok()?
        .flatten()
        .map(|e| e.path().join("device/gpu_busy_percent"))
        .find(|p| p.is_file())
}

impl Sampler {
    pub fn new() -> Self {
        Self::at("/proc".into(), "/sys".into())
    }

    fn at(proc: PathBuf, sys: PathBuf) -> Self {
        let net = network_counters(&proc, &sys);
        let mut net_baseline = Interfaces::new();
        if let Some(current) = &net {
            since_first_seen(&mut net_baseline, current);
        }
        Sampler {
            nvidia: command_output(
                "nvidia-smi",
                &[
                    "--query-gpu=utilization.gpu",
                    "--format=csv,noheader,nounits",
                ],
            )
            .and_then(|o| parse_nvidia_utilization(&o))
            .is_some(),
            amd: amd_busy(&sys),
            times: None,
            net,
            net_baseline,
            disk_io: None,
            last: Instant::now(),
            processes: HashMap::new(),
            names: HashMap::new(),
            devices: None,
            tick: 0,
            processors: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(1),
            proc,
            sys,
        }
    }

    pub fn supported(&self) -> Supported {
        Supported {
            cpu: true,
            gpu: self.nvidia || self.amd.is_some(),
            memory: true,
            network: self.net.is_some(),
            disk: disk_space().is_some(),
            devices: true,
        }
    }

    /// Starts the deltas over, so the first reading covers only the warm-up.
    pub fn prime(&mut self, detail: Option<Detail>) {
        self.times = read(self.proc.join("stat")).and_then(|s| parse_cpu(&s));
        self.net = network_counters(&self.proc, &self.sys).or(self.net.take());
        self.disk_io = disk_counters(&self.proc, &self.sys);
        self.processes.clear();
        if matches!(detail, Some(Detail::Cpu)) {
            self.processes = read_processes(&self.proc, false)
                .into_iter()
                .map(|p| ((p.pid, p.started), p.cpu_ticks))
                .collect();
        }
        self.last = Instant::now();
        self.tick = 0;
    }

    pub fn pause(&mut self) {}

    fn gpu_load(&self) -> Option<f64> {
        let nvidia = || {
            command_output(
                "nvidia-smi",
                &[
                    "--query-gpu=utilization.gpu",
                    "--format=csv,noheader,nounits",
                ],
            )
            .and_then(|o| parse_nvidia_utilization(&o))
        };
        let amd = || read(self.amd.as_ref()?)?.trim().parse::<f64>().ok();
        (if self.nvidia { nvidia() } else { None })
            .or_else(amd)
            .map(|percent| (percent / 100.0).clamp(0.0, 1.0))
    }

    pub fn sample(&mut self, detail: Option<Detail>, interval: u64) -> Snapshot {
        let elapsed = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();
        let mut snapshot = Snapshot::default();

        let times = read(self.proc.join("stat")).and_then(|s| parse_cpu(&s));
        snapshot.cpu = self
            .times
            .zip(times)
            .and_then(|(previous, current)| cpu_from_times(previous, current));
        self.times = times;
        snapshot.gpu = self.gpu_load();

        if let Some((read_total, write_total)) = disk_counters(&self.proc, &self.sys) {
            if let Some((was_read, was_write)) = self.disk_io {
                snapshot.disk_read = Some(per_second(read_total.saturating_sub(was_read), elapsed));
                snapshot.disk_write =
                    Some(per_second(write_total.saturating_sub(was_write), elapsed));
            }
            self.disk_io = Some((read_total, write_total));
        }

        let memory = read(self.proc.join("meminfo")).and_then(|m| parse_meminfo(&m));
        if let Some(memory) = memory {
            snapshot.memory_used = Some(memory.total - memory.available);
            snapshot.memory_total = Some(memory.total);
        }
        if detail == Some(Detail::Memory)
            && let Some(memory) = memory
        {
            snapshot.memory_cached = Some(memory.cached);
            snapshot.memory_swap_used = memory.swap_used;
            snapshot.memory_compressed = zram_compressed(&self.sys);
            snapshot.memory_pressure =
                Some((1.0 - memory.available as f64 / memory.total as f64).clamp(0.0, 1.0));
        }

        if let Some(current) = network_counters(&self.proc, &self.sys) {
            if let Some(previous) = &self.net {
                let (down, up) = interface_delta(previous, &current);
                snapshot.net_down = Some(per_second(down, elapsed));
                snapshot.net_up = Some(per_second(up, elapsed));
            }
            let (down, up) = since_first_seen(&mut self.net_baseline, &current);
            snapshot.net_total_down = Some(down);
            snapshot.net_total_up = Some(up);
            self.net = Some(current);
        }

        snapshot.battery = crate::linux::battery::read();
        snapshot.disk = disk_space();

        // Device lists change rarely: every 2 s at most.
        if self.devices.is_none() || self.tick.is_multiple_of(sample_stride(2, interval)) {
            let mut devices = usb_devices(&self.sys);
            devices.extend(bluetooth_devices());
            self.devices = Some(devices);
        }
        snapshot.devices = self.devices.clone();
        self.tick += 1;

        snapshot.process_status = ProcessStatus::Ready;
        snapshot.processes = match detail {
            Some(Detail::Cpu) => self.cpu_rows(elapsed, snapshot.cpu.map(|c| c * 100.0)),
            Some(Detail::Memory) => self.memory_rows(read_processes(&self.proc, true)),
            Some(Detail::Gpu) => {
                let (status, rows) = self.gpu_rows();
                snapshot.process_status = status;
                rows
            }
            Some(Detail::Network | Detail::Power) => {
                // Traffic and power by process need privileges, or a trace.
                snapshot.process_status = ProcessStatus::Unavailable;
                Vec::new()
            }
            _ => Vec::new(),
        };
        snapshot.processes_for = detail.filter(|d| {
            matches!(
                d,
                Detail::Cpu | Detail::Memory | Detail::Gpu | Detail::Network | Detail::Power
            )
        });
        snapshot.uptime_seconds = uptime_seconds(&self.proc);
        snapshot.supported = self.supported();
        snapshot
    }

    fn cpu_rows(&mut self, elapsed: f64, aggregate: Option<f64>) -> Vec<ProcessRow> {
        let entries = read_processes(&self.proc, false);
        let unit = ticks_to_100ns();
        let mut by_image: HashMap<String, (f64, u32)> = HashMap::new();
        for entry in &entries {
            let Some(previous) = self.processes.get(&(entry.pid, entry.started)) else {
                continue;
            };
            let percent = process_cpu_percent(
                previous * unit,
                entry.cpu_ticks * unit,
                elapsed,
                self.processors,
            );
            by_image
                .entry(entry.image.clone())
                .or_insert((0.0, entry.pid))
                .0 += percent;
        }
        self.processes = entries
            .iter()
            .map(|p| ((p.pid, p.started), p.cpu_ticks))
            .collect();
        let total: f64 = by_image.values().map(|v| v.0).sum();
        let scale = reconciliation_scale(total, aggregate);
        let rows = by_image
            .into_iter()
            .filter(|(_, (percent, _))| *percent * scale >= 0.05)
            .map(|(image, (percent, pid))| self.row(&image, pid, percent * scale))
            .collect();
        top_rows(rows, PROCESS_ROWS)
    }

    fn memory_rows(&mut self, entries: Vec<Entry>) -> Vec<ProcessRow> {
        let mut by_image: HashMap<String, (u64, u32, u64)> = HashMap::new();
        for entry in entries {
            let slot = by_image
                .entry(entry.image.clone())
                .or_insert((0, entry.pid, 0));
            slot.0 += entry.private_bytes;
            // The group's largest process names it.
            if entry.private_bytes > slot.2 {
                slot.1 = entry.pid;
                slot.2 = entry.private_bytes;
            }
        }
        let mut ranked: Vec<_> = by_image.into_iter().collect();
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1.0));
        ranked.truncate(PROCESS_ROWS);
        let rows = ranked
            .into_iter()
            .map(|(image, (bytes, pid, _))| self.row(&image, pid, bytes as f64))
            .collect();
        top_rows(rows, PROCESS_ROWS)
    }

    fn gpu_rows(&mut self) -> (ProcessStatus, Vec<ProcessRow>) {
        if !self.nvidia {
            return (ProcessStatus::Unavailable, Vec::new());
        }
        let Some(output) = command_output("nvidia-smi", &["pmon", "-c", "1", "-s", "u"]) else {
            return (ProcessStatus::Unavailable, Vec::new());
        };
        let entries: HashMap<u32, String> = read_processes(&self.proc, false)
            .into_iter()
            .map(|e| (e.pid, e.image))
            .collect();
        let mut by_image: HashMap<String, (f64, u32)> = HashMap::new();
        for (pid, percent) in parse_pmon(&output) {
            if let Some(image) = entries.get(&pid) {
                by_image.entry(image.clone()).or_insert((0.0, pid)).0 += percent;
            }
        }
        let rows = by_image
            .into_iter()
            .filter(|(_, (percent, _))| *percent >= 0.05)
            .map(|(image, (percent, pid))| self.row(&image, pid, percent.min(100.0)))
            .collect();
        (ProcessStatus::Ready, top_rows(rows, PROCESS_ROWS))
    }

    /// A row named as the desktop names the app ("Firefox"), cached per program.
    fn row(&mut self, image: &str, pid: u32, value: f64) -> ProcessRow {
        let (path, name) = self
            .names
            .entry(image.to_string())
            .or_insert_with(|| match crate::shell::process_path(pid) {
                Some(path) => {
                    let name = crate::shell::app_name(&path);
                    (
                        Some(path),
                        if name.is_empty() {
                            image.to_string()
                        } else {
                            name
                        },
                    )
                }
                None => (None, image.to_string()),
            })
            .clone();
        ProcessRow {
            name,
            path,
            value,
            net_down: None,
            net_up: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_busy_share_comes_from_two_readings_of_proc_stat() {
        let first =
            parse_cpu("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 1 1 1 1 0 0 0 0 0\n").unwrap();
        // idle + iowait, the kernel's share with it, the user's.
        assert_eq!(first, (850, 900, 100));
        let second = parse_cpu("cpu  200 10 100 850 50 0 0 0 0 0\n").unwrap();
        // 160 of the 210 ticks since were busy: 50 idle.
        let busy = cpu_from_times(first, second).unwrap();
        assert!((busy - (1.0 - 50.0 / 210.0)).abs() < 1e-9, "{busy}");
        assert!(parse_cpu("intr 1 2 3").is_none());
        assert!(parse_cpu("cpu  1 2").is_none());
    }

    const MEMINFO: &str = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    6000000 kB\nBuffers:          200000 kB\nCached:          3000000 kB\nSwapTotal:       4000000 kB\nSwapFree:        3500000 kB\n";

    #[test]
    fn memory_is_what_is_not_available_and_swap_what_is_not_free() {
        let memory = parse_meminfo(MEMINFO).unwrap();
        assert_eq!(memory.total, 16_000_000 * 1024);
        assert_eq!(memory.available, 6_000_000 * 1024);
        assert_eq!(memory.cached, 3_200_000 * 1024);
        assert_eq!(memory.swap_used, Some(500_000 * 1024));
        // No swap: nothing to say. No MemAvailable (old kernels): free stands in.
        let bare = parse_meminfo("MemTotal: 100 kB\nMemFree: 40 kB\n").unwrap();
        assert_eq!((bare.available, bare.swap_used), (40 * 1024, None));
        assert!(parse_meminfo("nothing").is_none());
        assert!(parse_meminfo("MemTotal: 0 kB\nMemFree: 0 kB\n").is_none());
    }

    #[test]
    fn network_counters_are_bytes_received_and_sent_per_interface() {
        let text = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n  eth0: 5000 50 0 0 0 0 0 0 2500 25 0 0 0 0 0 0\n";
        let counters = parse_net_dev(text);
        assert_eq!(counters["lo"], (1000, 1000));
        assert_eq!(counters["eth0"], (5000, 2500));
        assert!(parse_net_dev("").is_empty());
    }

    #[test]
    fn disk_counters_are_sectors_read_and_written() {
        let text = "   8       0 sda 100 0 2048 50 200 0 4096 80 0 0 0\n   8       1 sda1 90 0 1024 40 190 0 2048 70 0 0 0\n 259       0 nvme0n1 5 0 16 1 6 0 32 2 0 0 0\n";
        let stats = parse_diskstats(text);
        assert_eq!(stats["sda"], (2048, 4096));
        assert_eq!(stats["nvme0n1"], (16, 32));
        assert!(parse_diskstats("garbage").is_empty());
    }

    #[test]
    fn a_process_line_gives_its_name_cpu_and_start_even_with_brackets_in_the_name() {
        let line = |name: &str| {
            format!(
                "4242 ({name}) S 1 4242 4242 0 -1 4194560 100 0 0 0 150 50 0 0 20 0 3 0 99999 1000 200 18446744073709551615"
            )
        };
        assert_eq!(
            parse_stat(&line("firefox")),
            Some(("firefox".into(), 200, 99999))
        );
        assert_eq!(
            parse_stat(&line("Web Content (1)")),
            Some(("Web Content (1)".into(), 200, 99999))
        );
        assert_eq!(parse_stat("4242 (short) S 1"), None);
        assert_eq!(parse_stat(""), None);
    }

    #[test]
    fn private_memory_is_the_anonymous_resident_set() {
        let status =
            "Name:\tfirefox\nVmRSS:\t  900000 kB\nRssAnon:\t  400000 kB\nRssFile:\t  500000 kB\n";
        assert_eq!(parse_rss_anon(status), Some(400_000 * 1024));
        assert_eq!(parse_rss_anon("Name:\tx\n"), None);
    }

    #[test]
    fn the_gpu_figures_of_nvidia_smi_are_read() {
        assert_eq!(parse_nvidia_utilization("37\n"), Some(37.0));
        assert_eq!(parse_nvidia_utilization("10\n55\n"), Some(55.0));
        assert_eq!(parse_nvidia_utilization("N/A\n"), None);
        assert_eq!(parse_nvidia_utilization(""), None);
        let pmon = "# gpu         pid   type     sm    mem    enc    dec    jpg    ofa    command \n# Idx           #    C/G      %      %      %      %      %      %    name \n    0       1234     G     12      3      -      -      -      -    firefox\n    0       5678     C      -      -      -      -      -      -    idle\n    0       0     C      9      -      -      -      -      -    skipped\n";
        assert_eq!(parse_pmon(pmon), [(1234, 12.0), (5678, 0.0)]);
        assert!(parse_pmon("").is_empty());
    }

    /// A machine in a folder: its /proc and /sys as these tests need them.
    fn machine(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("openbar-monitor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (root.join("proc"), root.join("sys"), root)
    }

    fn file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn only_physical_interfaces_and_disks_count() {
        let (proc, sys, root) = machine("physical");
        file(&sys.join("class/net/eth0/ifindex"), "2\n");
        file(&sys.join("class/net/eth0/device/vendor"), "x");
        file(&sys.join("class/net/docker0/ifindex"), "3\n");
        file(&sys.join("class/net/lo/ifindex"), "1\n");
        file(
            &proc.join("net/dev"),
            "h\nh\n    lo: 1 0 0 0 0 0 0 0 1 0\n  eth0: 100 0 0 0 0 0 0 0 40 0\ndocker0: 999 0 0 0 0 0 0 0 999 0\n",
        );
        assert_eq!(
            network_counters(&proc, &sys).unwrap(),
            Interfaces::from([(2, (100, 40))])
        );
        // A machine with only virtual interfaces has no network to show.
        std::fs::remove_dir_all(sys.join("class/net/eth0")).unwrap();
        assert!(network_counters(&proc, &sys).is_none());

        file(&sys.join("block/sda/device/model"), "x");
        file(&sys.join("block/loop0/size"), "1");
        file(
            &proc.join("diskstats"),
            "8 0 sda 1 0 10 0 2 0 20 0 0 0 0\n8 1 sda1 1 0 10 0 2 0 20 0 0 0 0\n7 0 loop0 1 0 99 0 1 0 99 0 0 0 0\n",
        );
        assert_eq!(disk_counters(&proc, &sys), Some((10 * 512, 20 * 512)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plugged_in_usb_devices_are_listed_and_hubs_and_internals_are_not() {
        let (_, sys, root) = machine("usb");
        let device = |name: &str, product: &str, class: &str, removable: &str| {
            let dir = sys.join("bus/usb/devices").join(name);
            file(&dir.join("product"), &format!("{product}\n"));
            file(&dir.join("bDeviceClass"), &format!("{class}\n"));
            file(&dir.join("removable"), &format!("{removable}\n"));
        };
        device("1-2", "USB Optical Mouse", "00", "removable");
        device("1-3", "USB2.0 Hub", "09", "removable");
        device("1-4", "Integrated Camera", "ef", "fixed");
        device("1-5", "Billboard", "11", "removable");
        device("usb1", "xHCI Host Controller", "09", "unknown");
        device("1-2:1.0", "interface of the mouse", "00", "removable");
        device("2-1", "Keyboard", "00", "unknown");
        let found = usb_devices(&sys);
        let names: Vec<_> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Keyboard", "USB Optical Mouse"]);
        assert!(found.iter().all(|d| !d.bluetooth));
        assert!(usb_devices(&root.join("nowhere")).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zram_gives_the_compressed_size_of_memory() {
        let (_, sys, root) = machine("zram");
        assert_eq!(zram_compressed(&sys), None);
        file(
            &sys.join("block/zram0/mm_stat"),
            "4096000 1024000 1200000 0 0 0 0 0\n",
        );
        file(&sys.join("block/zram1/mm_stat"), "100 50 60 0 0 0 0 0\n");
        file(&sys.join("block/sda/device/x"), "x");
        assert_eq!(zram_compressed(&sys), Some(1_024_050));
        std::fs::remove_dir_all(root).unwrap();
    }

    fn process(proc: &Path, pid: u32, comm: &str, ticks: u64, rss_anon_kb: u64, kernel: bool) {
        let dir = proc.join(pid.to_string());
        file(
            &dir.join("cmdline"),
            if kernel {
                ""
            } else {
                "/usr/bin/thing\0--flag\0"
            },
        );
        file(
            &dir.join("stat"),
            &format!(
                "{pid} ({comm}) S 1 1 1 0 -1 0 0 0 0 0 {} 0 0 0 20 0 1 0 {} 0 0 0",
                ticks,
                1000 + pid
            ),
        );
        file(
            &dir.join("status"),
            &format!("Name:\t{comm}\nRssAnon:\t{rss_anon_kb} kB\n"),
        );
    }

    #[test]
    fn processes_are_read_without_kernel_threads_and_grouped_by_program_for_memory() {
        let (proc, _, root) = machine("processes");
        process(&proc, 10, "thing", 5, 100, false);
        process(&proc, 11, "thing", 5, 300, false);
        process(&proc, 12, "other", 5, 50, false);
        process(&proc, 2, "kthreadd", 0, 0, true);
        file(&proc.join("not-a-pid/cmdline"), "x");
        let entries = read_processes(&proc, true);
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|e| e.pid != 2));
        let mut sampler = Sampler::at(proc.clone(), root.join("sys"));
        let rows = sampler.memory_rows(entries);
        // Two processes of one program are one row, the biggest first.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].value, 400.0 * 1024.0);
        assert_eq!(rows[1].value, 50.0 * 1024.0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_process_busy_between_two_readings_shows_its_share_and_a_new_one_does_not() {
        let (proc, _, root) = machine("cpu");
        process(&proc, 10, "busy", 100, 0, false);
        process(&proc, 11, "quiet", 100, 0, false);
        let mut sampler = Sampler::at(proc.clone(), root.join("sys"));
        sampler.processors = 4;
        sampler.prime(Some(Detail::Cpu));
        // One second later: `busy` used 100 ticks (one whole CPU of four), `quiet` none,
        // and a program that started meanwhile has nothing to compare with.
        process(&proc, 10, "busy", 200, 0, false);
        process(&proc, 13, "newcomer", 500, 0, false);
        let rows = sampler.cpu_rows(1.0, None);
        assert_eq!(rows.len(), 1, "{rows:?}");
        let share = rows[0].value;
        assert!((share - 25.0).abs() < 0.01, "{share}");
        // The rows never claim more than the meter says.
        process(&proc, 10, "busy", 300, 0, false);
        let scaled = sampler.cpu_rows(1.0, Some(10.0));
        assert!((scaled[0].value - 10.0).abs() < 0.01, "{scaled:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_sampler_reads_a_whole_machine() {
        let (proc, sys, root) = machine("whole");
        file(&proc.join("stat"), "cpu  100 0 50 800 50 0 0 0 0 0\n");
        file(&proc.join("meminfo"), MEMINFO);
        file(&proc.join("uptime"), "12345.67 99999.0\n");
        file(&sys.join("class/net/eth0/ifindex"), "2\n");
        file(&sys.join("class/net/eth0/device/x"), "x");
        file(
            &proc.join("net/dev"),
            "h\nh\n  eth0: 1000 0 0 0 0 0 0 0 500 0\n",
        );
        file(&sys.join("block/sda/device/x"), "x");
        file(
            &proc.join("diskstats"),
            "8 0 sda 1 0 100 0 2 0 200 0 0 0 0\n",
        );
        let mut sampler = Sampler::at(proc.clone(), sys.clone());
        assert!(
            sampler.supported().network && sampler.supported().cpu && sampler.supported().memory
        );
        sampler.prime(None);
        file(&proc.join("stat"), "cpu  200 0 100 850 50 0 0 0 0 0\n");
        file(
            &proc.join("net/dev"),
            "h\nh\n  eth0: 3000 0 0 0 0 0 0 0 1500 0\n",
        );
        file(
            &proc.join("diskstats"),
            "8 0 sda 1 0 300 0 2 0 600 0 0 0 0\n",
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        let snapshot = sampler.sample(Some(Detail::Memory), 2);
        assert!(
            snapshot
                .cpu
                .is_some_and(|c| (c - (1.0 - 50.0 / 200.0)).abs() < 1e-9),
            "{:?}",
            snapshot.cpu
        );
        assert_eq!(snapshot.memory_total, Some(16_000_000 * 1024));
        assert_eq!(snapshot.memory_used, Some(10_000_000 * 1024));
        assert_eq!(snapshot.memory_swap_used, Some(500_000 * 1024));
        assert!(
            snapshot
                .memory_pressure
                .is_some_and(|p| (p - 0.625).abs() < 1e-9)
        );
        assert!(
            snapshot.net_down.is_some_and(|d| d > 0.0) && snapshot.net_up.is_some_and(|u| u > 0.0)
        );
        assert_eq!(
            (snapshot.net_total_down, snapshot.net_total_up),
            (Some(2000), Some(1000))
        );
        assert!(
            snapshot.disk_read.is_some_and(|r| r > 0.0)
                && snapshot.disk_write.is_some_and(|w| w > 0.0)
        );
        assert_eq!(snapshot.uptime_seconds, 12345);
        assert_eq!(snapshot.processes_for, Some(Detail::Memory));
        assert_eq!(snapshot.process_status, ProcessStatus::Ready);
        // Traffic by process is not available, and says so.
        let network = sampler.sample(Some(Detail::Network), 2);
        assert_eq!(network.process_status, ProcessStatus::Unavailable);
        assert!(network.processes.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "reads this machine"]
    fn what_this_machine_reads() {
        let mut sampler = Sampler::new();
        println!("supported: {:?}", sampler.supported());
        sampler.prime(Some(Detail::Cpu));
        std::thread::sleep(std::time::Duration::from_millis(900));
        let snapshot = sampler.sample(Some(Detail::Cpu), 1);
        println!("cpu {:?} gpu {:?}", snapshot.cpu, snapshot.gpu);
        println!(
            "memory {:?}/{:?}",
            snapshot.memory_used, snapshot.memory_total
        );
        println!(
            "net {:?} {:?} totals {:?} {:?}",
            snapshot.net_down, snapshot.net_up, snapshot.net_total_down, snapshot.net_total_up
        );
        println!(
            "disk {:?} io {:?} {:?}",
            snapshot.disk, snapshot.disk_read, snapshot.disk_write
        );
        println!("devices {:?}", snapshot.devices);
        println!("uptime {}", snapshot.uptime_seconds);
        for row in &snapshot.processes {
            println!("  {:>6.1}%  {}  {:?}", row.value, row.name, row.path);
        }
        let memory = sampler.sample(Some(Detail::Memory), 1);
        println!(
            "swap {:?} cached {:?} compressed {:?} pressure {:?}",
            memory.memory_swap_used,
            memory.memory_cached,
            memory.memory_compressed,
            memory.memory_pressure
        );
        for row in &memory.processes {
            println!("  {:>10.0} B  {}", row.value, row.name);
        }
    }
}
