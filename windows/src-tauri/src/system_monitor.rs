// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The System section's readings (SystemMonitor.swift; spec 04 §8). Nothing is
//! sampled until the page asks: `system_monitor_watch(true, …)` wakes the
//! thread, which primes its counters, takes a first reading after a short
//! warm-up and then one every `monitorInterval` seconds (1, 2 or 5) until the
//! page closes. Each reading is emitted as `system://snapshot`.
//!
//! Sources: PDH for CPU (`% Processor Utility`, as Task Manager), GPU engines
//! and disk activity, `GlobalMemoryStatusEx`, `GetSystemPowerStatus` and the
//! battery state's charge rate, `GetIfTable2` on hardware interfaces,
//! `GetDiskFreeSpaceExW` on the system drive, SetupAPI for USB devices and the
//! Bluetooth API for connected classic devices. Fans have no Windows API.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// How long counters run between priming and the first reading. PDH rate
/// counters and byte deltas need two samples; this keeps the "…" brief.
const WARM_UP: Duration = Duration::from_millis(400);

/// A metric detail the page has open; CPU and memory add their top processes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Detail {
    Cpu,
    Gpu,
    Memory,
    Network,
    Disk,
    Battery,
    Power,
    ConnectedDevices,
}

/// Which readings this machine can give at all; a card without one hides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Supported {
    pub cpu: bool,
    pub gpu: bool,
    pub memory: bool,
    pub network: bool,
    pub disk: bool,
    pub devices: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
    pub plugged_in: bool,
    /// Watts into the battery: positive while charging, negative while it
    /// drains. None when the battery doesn't report a rate.
    pub rate_watts: Option<f64>,
    /// Windows' estimate while on battery.
    pub seconds_left: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    /// "C:".
    pub name: String,
    pub free: u64,
    pub total: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub name: String,
    pub bluetooth: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub name: String,
    /// The exe, for its icon; None when Windows won't say (protected processes).
    pub path: Option<String>,
    /// CPU: percent of the whole machine, 0…100. Memory: private bytes.
    pub value: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Bumped by every reading, so the page records each one in its history.
    pub seq: u64,
    pub supported: Supported,
    /// 0…1.
    pub cpu: Option<f64>,
    pub gpu: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    /// None on machines without a battery.
    pub battery: Option<Battery>,
    /// Bytes per second.
    pub net_down: Option<f64>,
    pub net_up: Option<f64>,
    /// Bytes since the app started.
    pub net_total_down: Option<u64>,
    pub net_total_up: Option<u64>,
    pub disk: Option<Disk>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
    pub devices: Option<Vec<Device>>,
    /// The open detail's top processes (CPU or memory); empty otherwise.
    pub processes: Vec<ProcessRow>,
    /// The detail the rows were read for, so a reading still on its way
    /// when the page switched details isn't shown under the other one.
    pub processes_for: Option<Detail>,
    pub uptime_seconds: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Control {
    visible: bool,
    detail: Option<Detail>,
}

#[derive(Default)]
pub struct Monitor {
    control: Mutex<Control>,
    wake: Condvar,
    /// None until the sampler has probed what this PC can read.
    last: Mutex<Option<Snapshot>>,
}

pub type Shared = Arc<Monitor>;

/// The page opens (`visible`) or closes, or opens a metric's detail.
#[tauri::command]
pub fn system_monitor_watch(state: tauri::State<'_, Shared>, visible: bool, detail: Option<Detail>) {
    let next = Control { visible, detail: detail.filter(|_| visible) };
    let mut control = state.control.lock().unwrap();
    if *control != next {
        *control = next;
        state.wake.notify_all();
    }
}

/// The latest reading, so a page opened again starts from it; None before
/// the first probe, so the page never mistakes "not yet" for "nothing".
#[tauri::command]
pub fn system_monitor_snapshot(state: tauri::State<'_, Shared>) -> Option<Snapshot> {
    state.last.lock().unwrap().clone()
}

pub fn start(app: AppHandle, shared: Shared) {
    let _ = std::thread::Builder::new()
        .name("system-monitor".into())
        .spawn(move || run(app, shared));
}

fn run(app: AppHandle, shared: Shared) {
    let mut sampler = imp::Sampler::new();
    *shared.last.lock().unwrap() = Some(Snapshot { supported: sampler.supported(), ..Default::default() });
    let mut seq = 0u64;
    loop {
        let control = {
            let mut control = shared.control.lock().unwrap();
            while !control.visible {
                control = shared.wake.wait(control).unwrap();
            }
            control.clone()
        };
        sampler.prime(control.detail);
        if wait_unless_changed(&shared, &control, WARM_UP) {
            continue;
        }
        loop {
            let interval = monitor_interval(&app);
            seq += 1;
            let mut snapshot = sampler.sample(control.detail, interval);
            snapshot.seq = seq;
            *shared.last.lock().unwrap() = Some(snapshot.clone());
            let _ = app.emit_to(crate::island::LABEL, "system://snapshot", &snapshot);
            if wait_unless_changed(&shared, &control, Duration::from_secs(interval)) {
                break;
            }
        }
    }
}

/// Sleeps for `period`, waking early (and returning true) when the page
/// closes or opens another detail.
fn wait_unless_changed(shared: &Monitor, seen: &Control, period: Duration) -> bool {
    let deadline = Instant::now() + period;
    let mut control = shared.control.lock().unwrap();
    loop {
        if *control != *seen {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        control = shared.wake.wait_timeout(control, deadline - now).unwrap().0;
    }
}

/// `monitorInterval`: 1, 2 or 5 seconds, anything else reads as 2 (Defaults.sanitizedMonitorInterval).
fn monitor_interval(app: &AppHandle) -> u64 {
    let prefs = app.state::<crate::prefs::Prefs>().get();
    sanitized_interval(prefs.pointer("/island/monitorInterval").and_then(|v| v.as_u64()))
}

fn sanitized_interval(value: Option<u64>) -> u64 {
    value.filter(|seconds| matches!(seconds, 1 | 2 | 5)).unwrap_or(2)
}

// MARK: Pure arithmetic (shared by the Windows sampler and its tests)

#[cfg_attr(not(windows), allow(dead_code))]
mod math {
    use super::*;

    /// Rows in a metric detail's process list.
    pub const PROCESS_ROWS: usize = 10;

    /// Ticks between readings of a metric that wants one every `target` seconds
    /// (MonitorSamplingPolicy.sampleStride, foreground).
    pub fn sample_stride(target_seconds: u64, interval_seconds: u64) -> u64 {
        target_seconds.div_ceil(interval_seconds.max(1)).max(1)
    }

    /// Busy share of the machine between two `GetSystemTimes` readings (idle,
    /// kernel, user in 100 ns units; kernel includes idle).
    pub fn cpu_from_times(previous: (u64, u64, u64), current: (u64, u64, u64)) -> Option<f64> {
        let idle = current.0.checked_sub(previous.0)?;
        let total = current.1.checked_sub(previous.1)? + current.2.checked_sub(previous.2)?;
        if total == 0 {
            return None;
        }
        Some((1.0 - idle as f64 / total as f64).clamp(0.0, 1.0))
    }

    /// Bytes moved over `elapsed` seconds as a per-second rate; no time, no rate (MetricFormat.netSpeed).
    pub fn per_second(bytes: u64, elapsed: f64) -> f64 {
        if elapsed <= 0.0 || !elapsed.is_finite() {
            return 0.0;
        }
        bytes as f64 / elapsed
    }

    /// A process's share of the machine from its CPU-time delta (100 ns units)
    /// over the same wall-clock interval (MetricFormat.processCPUPercentage).
    pub fn process_cpu_percent(previous: u64, current: u64, elapsed: f64, processors: u32) -> f64 {
        if current < previous || elapsed <= 0.0 || !elapsed.is_finite() {
            return 0.0;
        }
        let capacity = elapsed * 10_000_000.0 * f64::from(processors.max(1));
        ((current - previous) as f64 / capacity * 100.0).clamp(0.0, 100.0)
    }

    /// The rows never claim more than the CPU meter (MetricFormat.processReconciliationScale).
    pub fn reconciliation_scale(sampled_total: f64, aggregate_percent: Option<f64>) -> f64 {
        match aggregate_percent {
            Some(aggregate) if sampled_total.is_finite() && sampled_total > 0.0 && aggregate.is_finite() => {
                (aggregate.clamp(0.0, 100.0) / sampled_total).min(1.0)
            }
            _ => 1.0,
        }
    }

    /// Task Manager's GPU figure: each engine's utilization summed across the
    /// processes using it, then the busiest engine type of any adapter. Instance
    /// names read "pid_1234_luid_0x0_0x1_phys_0_eng_0_engtype_3D".
    pub fn gpu_usage<'a>(instances: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f64> {
        let mut engines: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        let mut any = false;
        for (name, value) in instances {
            any = true;
            let Some(luid) = name.find("luid_") else { continue };
            let Some(engtype) = name.find("_engtype_") else { continue };
            let Some(eng) = name[luid..].find("_eng_").map(|i| luid + i) else { continue };
            // Adapter (luid + phys) and engine type; the engine index is summed in.
            let key = format!("{}|{}", &name[luid..eng], &name[engtype + 9..]);
            *engines.entry(key).or_default() += value.max(0.0);
        }
        if !any {
            return None;
        }
        Some((engines.into_values().fold(0.0, f64::max) / 100.0).clamp(0.0, 1.0))
    }

    /// Physical adapters only: no NDIS filter copies (they repeat the counters),
    /// loopback or tunnels. Bits are `InterfaceAndOperStatusFlags`.
    pub fn counts_interface(flags: u8, if_type: u32) -> bool {
        const HARDWARE: u8 = 1;
        const FILTER: u8 = 1 << 1;
        const SOFTWARE_LOOPBACK: u32 = 24;
        const TUNNEL: u32 = 131;
        flags & HARDWARE != 0 && flags & FILTER == 0 && if_type != SOFTWARE_LOOPBACK && if_type != TUNNEL
    }

    /// A battery's rate in watts; `BATTERY_UNKNOWN_RATE` reads as none.
    pub fn battery_watts(rate_milliwatts: u32) -> Option<f64> {
        const UNKNOWN: u32 = 0x8000_0000;
        if rate_milliwatts == UNKNOWN {
            return None;
        }
        Some(f64::from(rate_milliwatts as i32) / 1000.0)
    }

    /// `SYSTEM_POWER_STATUS` as the card reads it; None without a battery.
    pub fn battery_from_status(ac_line: u8, flag: u8, percent: u8, seconds: u32) -> Option<Battery> {
        const NO_BATTERY: u8 = 128;
        const UNKNOWN: u8 = 255;
        const CHARGING: u8 = 8;
        if flag == UNKNOWN || flag & NO_BATTERY != 0 {
            return None;
        }
        let plugged_in = ac_line == 1;
        Some(Battery {
            percent: (percent <= 100).then_some(percent),
            charging: flag & CHARGING != 0,
            plugged_in,
            rate_watts: None,
            seconds_left: (!plugged_in && seconds != u32::MAX).then_some(seconds),
        })
    }

    /// External USB devices only (USBDeviceSampler.parseDevice): no root hubs,
    /// no interfaces of a composite device, no hubs (class 09) or billboards
    /// (class 11), nothing built in.
    pub fn counts_usb_device(instance_id: &str, compatible_ids: &[String], removable: bool) -> bool {
        let id = instance_id.to_ascii_uppercase();
        if !id.starts_with("USB\\") || id.starts_with("USB\\ROOT_HUB") || id.contains("&MI_") {
            return false;
        }
        let infrastructure = compatible_ids.iter().any(|c| {
            let c = c.to_ascii_uppercase();
            c.starts_with("USB\\CLASS_09") || c.starts_with("USB\\CLASS_11")
        });
        removable && !infrastructure
    }

    /// Each adapter's (received, sent) byte counters, by interface LUID.
    pub type Interfaces = std::collections::HashMap<u64, (u64, u64)>;

    /// Bytes moved between two readings, counting only adapters present in
    /// both: one plugged in or re-enabled mid-way would otherwise add its
    /// whole lifetime in one tick, and one that went away would subtract.
    pub fn interface_delta(previous: &Interfaces, current: &Interfaces) -> (u64, u64) {
        current.iter().fold((0, 0), |(down, up), (luid, (received, sent))| match previous.get(luid) {
            Some((was_received, was_sent)) => (
                down + received.saturating_sub(*was_received),
                up + sent.saturating_sub(*was_sent),
            ),
            None => (down, up),
        })
    }

    /// Bytes since each adapter was first seen, which records new ones.
    pub fn since_first_seen(baseline: &mut Interfaces, current: &Interfaces) -> (u64, u64) {
        current.iter().fold((0, 0), |(down, up), (luid, (received, sent))| {
            let (base_received, base_sent) = *baseline.entry(*luid).or_insert((*received, *sent));
            (down + received.saturating_sub(base_received), up + sent.saturating_sub(base_sent))
        })
    }

    /// Top rows by value, largest first.
    pub fn top_rows(mut rows: Vec<ProcessRow>, limit: usize) -> Vec<ProcessRow> {
        rows.sort_by(|a, b| b.value.total_cmp(&a.value).then_with(|| a.name.cmp(&b.name)));
        rows.truncate(limit);
        rows
    }
}
#[cfg_attr(not(windows), allow(unused_imports))]
use math::*;

// MARK: Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Performance::*;

    const PDH_FMT_NOCAP100: u32 = 0x8000;

    /// The PDH query: CPU, GPU engines and disk activity. Lives on the monitor thread.
    struct Pdh {
        query: PDH_HQUERY,
        cpu: Option<PDH_HCOUNTER>,
        gpu: Option<PDH_HCOUNTER>,
        disk_read: Option<PDH_HCOUNTER>,
        disk_write: Option<PDH_HCOUNTER>,
    }

    impl Pdh {
        fn open() -> Option<Self> {
            let mut query = PDH_HQUERY::default();
            if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
                return None;
            }
            // English names work on every display language.
            let add = |path: PCWSTR| {
                let mut counter = PDH_HCOUNTER::default();
                (unsafe { PdhAddEnglishCounterW(query, path, 0, &mut counter) } == 0).then_some(counter)
            };
            let cpu = add(w!("\\Processor Information(_Total)\\% Processor Utility"))
                .or_else(|| add(w!("\\Processor Information(_Total)\\% Processor Time")))
                .or_else(|| add(w!("\\Processor(_Total)\\% Processor Time")));
            Some(Pdh {
                query,
                cpu,
                gpu: add(w!("\\GPU Engine(*)\\Utilization Percentage")),
                disk_read: add(w!("\\PhysicalDisk(_Total)\\Disk Read Bytes/sec")),
                disk_write: add(w!("\\PhysicalDisk(_Total)\\Disk Write Bytes/sec")),
            })
        }

        fn collect(&self) -> bool {
            unsafe { PdhCollectQueryData(self.query) == 0 }
        }

        fn value(counter: Option<PDH_HCOUNTER>) -> Option<f64> {
            let mut value = PDH_FMT_COUNTERVALUE::default();
            let format = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100);
            let status = unsafe { PdhGetFormattedCounterValue(counter?, format, None, &mut value) };
            let valid = status == 0 && (value.CStatus == PDH_CSTATUS_VALID_DATA || value.CStatus == PDH_CSTATUS_NEW_DATA);
            let number = unsafe { value.Anonymous.doubleValue };
            (valid && number.is_finite()).then_some(number)
        }

        /// Each instance of a wildcard counter with valid data, by name, and
        /// how many instances exist at all.
        fn instances(counter: PDH_HCOUNTER) -> Option<(Vec<(String, f64)>, u32)> {
            let format = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100);
            let (mut size, mut count) = (0u32, 0u32);
            let status = unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, None) };
            if status == 0 && count == 0 {
                return Some((Vec::new(), 0));
            }
            if status != PDH_MORE_DATA {
                return None;
            }
            let item = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity((size as usize).div_ceil(item) + 1);
            let status = unsafe {
                PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, Some(buffer.as_mut_ptr()))
            };
            if status != 0 {
                return None;
            }
            unsafe { buffer.set_len((count as usize).min(buffer.capacity())) };
            let valid = buffer
                    .iter()
                    .filter(|i| i.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA || i.FmtValue.CStatus == PDH_CSTATUS_NEW_DATA)
                    .map(|i| {
                        let name = unsafe { i.szName.to_string() }.unwrap_or_default();
                        (name, unsafe { i.FmtValue.Anonymous.doubleValue })
                    })
                    .collect();
            Some((valid, count))
        }
    }

    impl Drop for Pdh {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct ProcessKey {
        pid: u32,
        created: i64,
    }

    struct ProcessEntry {
        key: ProcessKey,
        image: String,
        cpu_time: u64,
        private_bytes: u64,
    }

    pub struct Sampler {
        pdh: Option<Pdh>,
        gpu_seen: Option<bool>,
        times: Option<(u64, u64, u64)>,
        net: Option<Interfaces>,
        net_baseline: Interfaces,
        last: Instant,
        processes: HashMap<ProcessKey, u64>,
        process_names: HashMap<String, (Option<String>, String)>,
        devices: Option<Vec<Device>>,
        tick: u64,
        processors: u32,
    }

    impl Sampler {
        pub fn new() -> Self {
            let pdh = Pdh::open();
            let net = network_counters();
            let mut net_baseline = Interfaces::new();
            if let Some(current) = &net {
                since_first_seen(&mut net_baseline, current);
            }
            Sampler {
                pdh,
                gpu_seen: None,
                times: None,
                net,
                net_baseline,
                last: Instant::now(),
                processes: HashMap::new(),
                process_names: HashMap::new(),
                devices: None,
                tick: 0,
                processors: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
            }
        }

        pub fn supported(&self) -> Supported {
            Supported {
                cpu: true,
                gpu: self.pdh.as_ref().is_some_and(|p| p.gpu.is_some()) && self.gpu_seen != Some(false),
                memory: true,
                network: self.net.is_some(),
                disk: system_drive_space().is_some(),
                devices: true,
            }
        }

        /// Starts the deltas over, so the first reading covers only the warm-up.
        pub fn prime(&mut self, detail: Option<Detail>) {
            if let Some(pdh) = &self.pdh {
                pdh.collect();
            }
            self.times = system_times();
            self.net = network_counters().or(self.net.take());
            self.processes.clear();
            if detail == Some(Detail::Cpu) {
                self.processes = read_processes().into_iter().map(|p| (p.key, p.cpu_time)).collect();
            }
            self.last = Instant::now();
            self.tick = 0;
        }

        pub fn sample(&mut self, detail: Option<Detail>, interval: u64) -> Snapshot {
            let elapsed = self.last.elapsed().as_secs_f64();
            self.last = Instant::now();
            let mut snapshot = Snapshot::default();

            let collected = self.pdh.as_ref().is_some_and(Pdh::collect);
            let pdh = self.pdh.as_ref().filter(|_| collected);
            let times = system_times();
            snapshot.cpu = pdh
                .and_then(|p| Pdh::value(p.cpu))
                .map(|v| (v / 100.0).clamp(0.0, 1.0))
                .or_else(|| cpu_from_times(self.times?, times?));
            self.times = times;
            if let Some(counter) = pdh.and_then(|p| p.gpu)
                && let Some((list, count)) = Pdh::instances(counter)
            {
                self.gpu_seen = Some(count > 0);
                snapshot.gpu = gpu_usage(list.iter().map(|(n, v)| (n.as_str(), *v)));
            }
            snapshot.disk_read = pdh.and_then(|p| Pdh::value(p.disk_read)).map(|v| v.max(0.0));
            snapshot.disk_write = pdh.and_then(|p| Pdh::value(p.disk_write)).map(|v| v.max(0.0));

            if let Some((used, total)) = memory() {
                snapshot.memory_used = Some(used);
                snapshot.memory_total = Some(total);
            }

            if let Some(current) = network_counters() {
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

            snapshot.battery = battery();
            snapshot.disk = system_drive_space();

            // Device lists change rarely: every 2 s at most (MonitorSamplingPolicy, foreground).
            if self.devices.is_none() || self.tick.is_multiple_of(sample_stride(2, interval)) {
                self.devices = Some(connected_devices());
            }
            snapshot.devices = self.devices.clone();
            self.tick += 1;

            snapshot.processes = match detail {
                Some(Detail::Cpu) => self.cpu_rows(elapsed, snapshot.cpu.map(|c| c * 100.0)),
                Some(Detail::Memory) => self.memory_rows(),
                _ => Vec::new(),
            };
            snapshot.processes_for = detail.filter(|d| matches!(d, Detail::Cpu | Detail::Memory));
            snapshot.uptime_seconds = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
            snapshot.supported = self.supported();
            snapshot
        }

        fn cpu_rows(&mut self, elapsed: f64, aggregate: Option<f64>) -> Vec<ProcessRow> {
            let entries = read_processes();
            let mut by_image: HashMap<String, (f64, u32)> = HashMap::new();
            for entry in &entries {
                let Some(previous) = self.processes.get(&entry.key) else { continue };
                let percent = process_cpu_percent(*previous, entry.cpu_time, elapsed, self.processors);
                let slot = by_image.entry(entry.image.clone()).or_insert((0.0, entry.key.pid));
                slot.0 += percent;
            }
            self.processes = entries.iter().map(|p| (p.key, p.cpu_time)).collect();
            let total: f64 = by_image.values().map(|v| v.0).sum();
            let scale = reconciliation_scale(total, aggregate);
            let rows = by_image
                .into_iter()
                .filter(|(_, (percent, _))| *percent * scale >= 0.05)
                .map(|(image, (percent, pid))| self.row(&image, pid, percent * scale))
                .collect();
            top_rows(rows, PROCESS_ROWS)
        }

        fn memory_rows(&mut self) -> Vec<ProcessRow> {
            let mut by_image: HashMap<String, (u64, u32, u64)> = HashMap::new();
            for entry in read_processes() {
                let slot = by_image.entry(entry.image.clone()).or_insert((0, entry.key.pid, 0));
                slot.0 += entry.private_bytes;
                // The group's largest process names it.
                if entry.private_bytes > slot.2 {
                    slot.1 = entry.key.pid;
                    slot.2 = entry.private_bytes;
                }
            }
            let mut ranked: Vec<_> = by_image.into_iter().collect();
            ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1 .0));
            ranked.truncate(PROCESS_ROWS);
            let rows = ranked.into_iter().map(|(image, (bytes, pid, _))| self.row(&image, pid, bytes as f64)).collect();
            top_rows(rows, PROCESS_ROWS)
        }

        /// A row named as Windows names the app ("Google Chrome"), cached per image.
        fn row(&mut self, image: &str, pid: u32, value: f64) -> ProcessRow {
            let (path, name) = self
                .process_names
                .entry(image.to_string())
                .or_insert_with(|| match crate::shell::process_path(pid) {
                    Some(path) => {
                        let name = crate::shell::app_name(&path);
                        (Some(path), if name.is_empty() { image.to_string() } else { name })
                    }
                    None => (None, image.trim_end_matches(".exe").to_string()),
                })
                .clone();
            ProcessRow { name, path, value }
        }
    }

    fn filetime(t: windows::Win32::Foundation::FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }

    fn system_times() -> Option<(u64, u64, u64)> {
        use windows::Win32::Foundation::FILETIME;
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { windows::Win32::System::Threading::GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }
            .ok()?;
        Some((filetime(idle), filetime(kernel), filetime(user)))
    }

    /// Used = total − available, as Task Manager's "In use" (spec 04 §8).
    fn memory() -> Option<(u64, u64)> {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
        (status.ullTotalPhys > 0).then(|| (status.ullTotalPhys.saturating_sub(status.ullAvailPhys), status.ullTotalPhys))
    }

    fn network_counters() -> Option<Interfaces> {
        use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_ROW2, MIB_IF_TABLE2};
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if unsafe { GetIfTable2(&mut table) }.is_err() || table.is_null() {
            return None;
        }
        let mut interfaces = Interfaces::new();
        unsafe {
            // The rows run past the declared one-element array.
            let first = std::ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>();
            let rows = std::slice::from_raw_parts(first, (*table).NumEntries as usize);
            for row in rows {
                if counts_interface(row.InterfaceAndOperStatusFlags._bitfield, row.Type) {
                    interfaces.insert(row.InterfaceLuid.Value, (row.InOctets, row.OutOctets));
                }
            }
            FreeMibTable(table as *const _);
        }
        Some(interfaces)
    }

    fn battery() -> Option<Battery> {
        use windows::Win32::System::Power::{
            CallNtPowerInformation, GetSystemPowerStatus, SystemBatteryState, SYSTEM_BATTERY_STATE, SYSTEM_POWER_STATUS,
        };
        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
        let mut battery = battery_from_status(
            status.ACLineStatus,
            status.BatteryFlag,
            status.BatteryLifePercent,
            status.BatteryLifeTime,
        )?;
        let mut state = SYSTEM_BATTERY_STATE::default();
        let read = unsafe {
            CallNtPowerInformation(
                SystemBatteryState,
                None,
                0,
                Some(&mut state as *mut _ as *mut _),
                std::mem::size_of::<SYSTEM_BATTERY_STATE>() as u32,
            )
        };
        if read.is_ok() && state.BatteryPresent {
            battery.rate_watts = battery_watts(state.Rate);
        }
        Some(battery)
    }

    fn system_drive_space() -> Option<Disk> {
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let root = windows::core::HSTRING::from(format!("{drive}\\"));
        let (mut free, mut total) = (0u64, 0u64);
        unsafe { GetDiskFreeSpaceExW(&root, Some(&mut free), Some(&mut total), None) }.ok()?;
        (total > 0).then_some(Disk { name: drive, free, total })
    }

    fn connected_devices() -> Vec<Device> {
        let mut devices = usb_devices();
        devices.extend(bluetooth_devices());
        devices.sort_by_key(|d| d.name.to_lowercase());
        devices
    }

    fn usb_devices() -> Vec<Device> {
        use windows::Win32::Devices::DeviceAndDriverInstallation::*;
        use windows::Win32::Devices::Properties::{DEVPKEY_Device_BusReportedDeviceDesc, DEVPROPTYPE};
        let mut out = Vec::new();
        let Ok(set) = (unsafe { SetupDiGetClassDevsW(None, w!("USB"), None, DIGCF_PRESENT | DIGCF_ALLCLASSES) }) else {
            return out;
        };
        let wide = |bytes: &[u8]| -> Vec<String> {
            let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            units.split(|u| *u == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
        };
        for index in 0.. {
            let mut data = SP_DEVINFO_DATA { cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
                break;
            }
            let mut id = [0u16; 512];
            if unsafe { SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), None) }.is_err() {
                continue;
            }
            let id = String::from_utf16_lossy(&id[..id.iter().position(|u| *u == 0).unwrap_or(id.len())]);
            let registry = |property| {
                let mut bytes = vec![0u8; 2048];
                unsafe { SetupDiGetDeviceRegistryPropertyW(set, &data, property, None, Some(&mut bytes), None) }
                    .ok()
                    .map(|_| bytes)
            };
            let compatible = registry(SPDRP_COMPATIBLEIDS).map(|b| wide(&b)).unwrap_or_default();
            let removable = registry(SPDRP_CAPABILITIES)
                .is_some_and(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) & CM_DEVCAP_REMOVABLE.0 != 0);
            if !counts_usb_device(&id, &compatible, removable) {
                continue;
            }
            let mut kind = DEVPROPTYPE::default();
            let mut bytes = vec![0u8; 1024];
            let reported = unsafe {
                SetupDiGetDevicePropertyW(set, &data, &DEVPKEY_Device_BusReportedDeviceDesc, &mut kind, Some(&mut bytes), None, 0)
            }
            .ok()
            .and_then(|_| wide(&bytes).into_iter().next());
            let name = reported
                .or_else(|| registry(SPDRP_FRIENDLYNAME).and_then(|b| wide(&b).into_iter().next()))
                .or_else(|| registry(SPDRP_DEVICEDESC).and_then(|b| wide(&b).into_iter().next()))
                .unwrap_or_default();
            out.push(Device { name: name.trim().to_string(), bluetooth: false });
        }
        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };
        out
    }

    /// Connected classic Bluetooth devices, from the system's cache (no inquiry).
    fn bluetooth_devices() -> Vec<Device> {
        use windows::Win32::Devices::Bluetooth::*;
        let mut out = Vec::new();
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnConnected: true.into(),
            ..Default::default()
        };
        let mut info = BLUETOOTH_DEVICE_INFO { dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
        let Ok(find) = (unsafe { BluetoothFindFirstDevice(&params, &mut info) }) else { return out };
        loop {
            if info.fConnected.as_bool() {
                let end = info.szName.iter().position(|u| *u == 0).unwrap_or(info.szName.len());
                out.push(Device { name: String::from_utf16_lossy(&info.szName[..end]), bluetooth: true });
            }
            info = BLUETOOTH_DEVICE_INFO { dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
            if unsafe { BluetoothFindNextDevice(find, &mut info) }.is_err() {
                break;
            }
        }
        let _ = unsafe { BluetoothFindDeviceClose(find) };
        out
    }

    /// Every process's CPU time and private working set in one call
    /// (`SystemProcessInformation`; the documented struct keeps the times in
    /// `Reserved1`: private working set at 0, create time at 24, user at 32,
    /// kernel at 40).
    fn read_processes() -> Vec<ProcessEntry> {
        use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
        use windows::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;
        const LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
        let mut buffer: Vec<u64> = vec![0; 512 * 1024 / 8];
        loop {
            let mut needed = 0u32;
            let status = unsafe {
                NtQuerySystemInformation(
                    SystemProcessInformation,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * 8) as u32,
                    &mut needed,
                )
            };
            if status.0 == LENGTH_MISMATCH && buffer.len() < 64 * 1024 * 1024 / 8 {
                let bytes = (needed as usize + 64 * 1024).max(buffer.len() * 16);
                buffer = vec![0; bytes.div_ceil(8)];
                continue;
            }
            if status.is_err() {
                return Vec::new();
            }
            break;
        }
        let base = buffer.as_ptr().cast::<u8>();
        let mut out = Vec::new();
        let mut offset = 0usize;
        loop {
            let info = unsafe { &*base.add(offset).cast::<SYSTEM_PROCESS_INFORMATION>() };
            let pid = info.UniqueProcessId.0 as usize as u32;
            let field = |at: usize| i64::from_le_bytes(info.Reserved1[at..at + 8].try_into().unwrap());
            if pid != 0 {
                let image = if info.ImageName.Buffer.is_null() {
                    String::new()
                } else {
                    let units = unsafe {
                        std::slice::from_raw_parts(info.ImageName.Buffer.0, usize::from(info.ImageName.Length) / 2)
                    };
                    String::from_utf16_lossy(units)
                };
                out.push(ProcessEntry {
                    key: ProcessKey { pid, created: field(24) },
                    image: if image.is_empty() { "System".into() } else { image },
                    cpu_time: (field(32).max(0) + field(40).max(0)) as u64,
                    private_bytes: field(0).max(0) as u64,
                });
            }
            if info.NextEntryOffset == 0 {
                break;
            }
            offset += info.NextEntryOffset as usize;
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    /// Off Windows there is nothing to read; the page shows "monitor unavailable".
    pub struct Sampler;

    impl Sampler {
        pub fn new() -> Self {
            Sampler
        }
        pub fn supported(&self) -> Supported {
            Supported::default()
        }
        pub fn prime(&mut self, _detail: Option<Detail>) {}
        pub fn sample(&mut self, _detail: Option<Detail>, _interval: u64) -> Snapshot {
            Snapshot::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_is_one_two_or_five_seconds() {
        assert_eq!(sanitized_interval(Some(1)), 1);
        assert_eq!(sanitized_interval(Some(5)), 5);
        assert_eq!(sanitized_interval(Some(3)), 2);
        assert_eq!(sanitized_interval(Some(0)), 2);
        assert_eq!(sanitized_interval(None), 2);
    }

    #[test]
    fn device_lists_refresh_every_two_seconds() {
        assert_eq!(sample_stride(2, 1), 2);
        assert_eq!(sample_stride(2, 2), 1);
        assert_eq!(sample_stride(2, 5), 1);
        assert_eq!(sample_stride(2, 0), 2);
    }

    #[test]
    fn cpu_is_the_busy_share_between_two_readings() {
        // Kernel time includes idle: 100 ticks passed, 25 of them idle.
        assert_eq!(cpu_from_times((0, 0, 0), (25, 60, 40)), Some(0.75));
        assert_eq!(cpu_from_times((10, 10, 10), (10, 10, 10)), None);
        // A counter that went back (sleep, wrap) gives no reading.
        assert_eq!(cpu_from_times((10, 50, 50), (5, 60, 60)), None);
        assert_eq!(cpu_from_times((0, 0, 0), (100, 100, 0)), Some(0.0));
    }

    #[test]
    fn rates_are_per_second() {
        assert_eq!(per_second(2_000, 2.0), 1_000.0);
        assert_eq!(per_second(500, 0.5), 1_000.0);
        assert_eq!(per_second(2_000, 0.0), 0.0);
        assert_eq!(per_second(2_000, f64::NAN), 0.0);
    }

    #[test]
    fn process_cpu_is_a_share_of_all_processors() {
        // One second of CPU time over one second on four processors.
        assert_eq!(process_cpu_percent(0, 10_000_000, 1.0, 4), 25.0);
        assert_eq!(process_cpu_percent(10, 5, 1.0, 4), 0.0);
        assert_eq!(process_cpu_percent(0, 10_000_000, 0.0, 4), 0.0);
        assert_eq!(process_cpu_percent(0, 100_000_000, 1.0, 1), 100.0);
    }

    #[test]
    fn process_rows_never_exceed_the_meter() {
        assert_eq!(reconciliation_scale(50.0, Some(25.0)), 0.5);
        assert_eq!(reconciliation_scale(20.0, Some(25.0)), 1.0);
        assert_eq!(reconciliation_scale(0.0, Some(25.0)), 1.0);
        assert_eq!(reconciliation_scale(50.0, None), 1.0);
    }

    #[test]
    fn gpu_takes_the_busiest_engine_type_summed_across_processes() {
        let instances = [
            ("pid_1_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 30.0),
            ("pid_2_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_2_luid_0x0_0xA_phys_0_eng_4_engtype_VideoDecode", 40.0),
            ("pid_3_luid_0x0_0xA_phys_0_eng_1_engtype_Copy", 5.0),
        ];
        assert_eq!(gpu_usage(instances), Some(0.55));
        // Two adapters are not summed together.
        let two = [
            ("pid_1_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 60.0),
            ("pid_1_luid_0x0_0xB_phys_0_eng_0_engtype_3D", 60.0),
        ];
        assert_eq!(gpu_usage(two), Some(0.6));
        assert_eq!(gpu_usage([("pid_1_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 250.0)]), Some(1.0));
        assert_eq!(gpu_usage(std::iter::empty()), None);
    }

    #[test]
    fn only_physical_interfaces_count() {
        assert!(counts_interface(0b0000_0001, 6));
        assert!(counts_interface(0b0000_0101, 71));
        assert!(!counts_interface(0b0000_0011, 6), "an NDIS filter repeats the adapter's counters");
        assert!(!counts_interface(0, 6), "virtual adapters");
        assert!(!counts_interface(1, 24));
        assert!(!counts_interface(1, 131));
    }

    #[test]
    fn network_counts_only_adapters_seen_twice() {
        let interfaces = |rows: &[(u64, u64, u64)]| rows.iter().map(|(l, r, s)| (*l, (*r, *s))).collect::<Interfaces>();
        let before = interfaces(&[(1, 1_000, 500), (2, 10, 10)]);
        // Adapter 3 appears with its lifetime counters, adapter 2 goes away.
        let after = interfaces(&[(1, 3_000, 900), (3, 9_000_000, 9_000_000)]);
        assert_eq!(interface_delta(&before, &after), (2_000, 400));
        // A counter that went back (reset) adds nothing.
        assert_eq!(interface_delta(&after, &interfaces(&[(1, 100, 100)])), (0, 0));
        let mut baseline = Interfaces::new();
        assert_eq!(since_first_seen(&mut baseline, &before), (0, 0));
        assert_eq!(since_first_seen(&mut baseline, &after), (2_000, 400));
        let later = interfaces(&[(1, 3_500, 900), (3, 9_000_100, 9_000_000)]);
        assert_eq!(since_first_seen(&mut baseline, &later), (2_600, 400));
    }

    #[test]
    fn battery_rate_is_signed_milliwatts() {
        assert_eq!(battery_watts(0), Some(0.0));
        assert_eq!(battery_watts(12_500), Some(12.5));
        assert_eq!(battery_watts((-8_000i32) as u32), Some(-8.0));
        assert_eq!(battery_watts(0x8000_0000), None);
    }

    #[test]
    fn battery_reads_only_when_present() {
        assert_eq!(battery_from_status(1, 128, 255, u32::MAX), None);
        assert_eq!(battery_from_status(1, 255, 255, u32::MAX), None);
        let charging = battery_from_status(1, 8, 64, u32::MAX).unwrap();
        assert_eq!(charging.percent, Some(64));
        assert!(charging.charging && charging.plugged_in);
        assert_eq!(charging.seconds_left, None);
        let draining = battery_from_status(0, 2, 15, 3_600).unwrap();
        assert!(!draining.charging && !draining.plugged_in);
        assert_eq!(draining.seconds_left, Some(3_600));
        assert_eq!(battery_from_status(0, 1, 255, u32::MAX).unwrap().percent, None);
    }

    #[test]
    fn usb_counts_external_devices_only() {
        let none: [String; 0] = [];
        assert!(counts_usb_device("USB\\VID_046D&PID_C52B\\5&1A2B", &["USB\\Class_03".into()], true));
        assert!(!counts_usb_device("USB\\ROOT_HUB30\\4&2B", &none, true));
        assert!(!counts_usb_device("USB\\VID_046D&PID_C52B&MI_00\\6&1", &none, true));
        assert!(!counts_usb_device("USB\\VID_05E3&PID_0610\\5&1", &["USB\\Class_09&SubClass_00".into()], true));
        assert!(!counts_usb_device("USB\\VID_2109&PID_0103\\5&1", &["USB\\Class_11".into()], true));
        assert!(!counts_usb_device("USB\\VID_8087&PID_0026\\5&1", &none, false), "built in");
        assert!(!counts_usb_device("HID\\VID_046D", &none, true));
    }

    #[test]
    fn rows_rank_largest_first() {
        let row = |name: &str, value: f64| ProcessRow { name: name.into(), path: None, value };
        let rows = top_rows(vec![row("b", 1.0), row("a", 5.0), row("c", 3.0), row("d", 3.0)], 3);
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a", "c", "d"]);
    }

    #[test]
    fn snapshot_serializes_for_the_page() {
        let snapshot = Snapshot { cpu: Some(0.5), memory_used: Some(1), ..Default::default() };
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["cpu"], 0.5);
        assert_eq!(json["memoryUsed"], 1);
        assert!(json["battery"].is_null());
        assert_eq!(json["supported"]["gpu"], false);
        let detail: Detail = serde_json::from_str("\"connectedDevices\"").unwrap();
        assert_eq!(detail, Detail::ConnectedDevices);
    }

    #[test]
    fn watch_wakes_the_sampler_only_on_change() {
        let monitor = Monitor::default();
        let seen = Control { visible: true, detail: None };
        *monitor.control.lock().unwrap() = seen.clone();
        assert!(!wait_unless_changed(&monitor, &seen, Duration::from_millis(5)));
        monitor.control.lock().unwrap().detail = Some(Detail::Cpu);
        assert!(wait_unless_changed(&monitor, &seen, Duration::from_secs(5)));
    }
}
