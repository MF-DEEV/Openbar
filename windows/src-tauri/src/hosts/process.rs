// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A process and its ancestors, guarded against pid reuse: a parent must
//! have started before its child.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub pid: u32,
    pub name: String,
}

/// `pid` first, then its parent, up to 32 levels.
#[cfg(windows)]
pub fn ancestors(pid: u32) -> Option<Vec<Entry>> {
    use std::collections::HashMap;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let mut table: HashMap<u32, (u32, String)> = HashMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                table.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    let (_, name) = table.get(&pid)?.clone();
    let mut chain = vec![Entry { pid, name }];
    let mut current = pid;
    let mut started = creation_time(pid);
    for _ in 0..32 {
        let Some((parent, _)) = table.get(&current) else {
            break;
        };
        let parent = *parent;
        if parent == 0 || parent == current {
            break;
        }
        let Some((_, name)) = table.get(&parent) else {
            break;
        };
        // A parent that started after its child is a stranger reusing the pid.
        let parent_started = creation_time(parent);
        if let (Some(p), Some(c)) = (parent_started, started)
            && p > c
        {
            break;
        }
        chain.push(Entry {
            pid: parent,
            name: name.clone(),
        });
        current = parent;
        started = parent_started;
    }
    Some(chain)
}

/// Every running process with this image name (case-insensitive).
#[cfg(windows)]
pub fn pids_named(image: &str) -> Vec<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    let mut pids = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(image) {
                    pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    pids
}

#[cfg(not(windows))]
pub fn pids_named(image: &str) -> Vec<u32> {
    let out = std::process::Command::new("pgrep")
        .args(["-x", image.trim_end_matches(".exe")])
        .output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect()
    })
    .unwrap_or_default()
}

/// When the process started, in 100 ns ticks since 1601 (FILETIME).
#[cfg(windows)]
pub fn creation_time(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }
}

/// Seconds since the Unix epoch when the process started.
#[cfg(windows)]
pub fn started_at(pid: u32) -> Option<f64> {
    // FILETIME counts from 1601-01-01; the Unix epoch is 11644473600 s later.
    creation_time(pid).map(|t| t as f64 / 1e7 - 11_644_473_600.0)
}

#[cfg(windows)]
pub fn is_running(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        alive
    }
}

/// The Mac build is only for designing; it reads processes with `ps`.
#[cfg(not(windows))]
pub fn ancestors(pid: u32) -> Option<Vec<Entry>> {
    let mut chain = Vec::new();
    let mut current = pid;
    for _ in 0..32 {
        let out = std::process::Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &current.to_string()])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let (ppid, name) = line.split_once(' ')?;
        chain.push(Entry {
            pid: current,
            name: name.trim().rsplit('/').next().unwrap_or("").into(),
        });
        let ppid: u32 = ppid.trim().parse().ok()?;
        if ppid <= 1 {
            break;
        }
        current = ppid;
    }
    (!chain.is_empty()).then_some(chain)
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn started_at(_pid: u32) -> Option<f64> {
    None
}

#[cfg(not(windows))]
pub fn is_running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}
