// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Process queries on Linux, read from `/proc`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::hosts::process::{Entry, Metadata};

fn proc(pid: u32, leaf: &str) -> PathBuf {
    Path::new("/proc").join(pid.to_string()).join(leaf)
}

/// State, parent and start time (clock ticks since boot) from a `stat` line.
fn parse_stat(text: &str) -> Option<(char, u32, u64)> {
    // The name sits in parentheses and may hold spaces and parentheses itself.
    let rest = &text[text.rfind(')')? + 1..];
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let parent = fields.next()?.parse().ok()?;
    let started = fields.nth(17)?.parse().ok()?;
    Some((state, parent, started))
}

fn stat(pid: u32) -> Option<(char, u32, u64)> {
    parse_stat(&std::fs::read_to_string(proc(pid, "stat")).ok()?)
}

fn comm(pid: u32) -> Option<String> {
    let text = std::fs::read_to_string(proc(pid, "comm")).ok()?;
    Some(text.trim_end().to_owned())
}

/// The program's file name; `comm` (cut at 15 characters) when the link is unreadable.
fn name(pid: u32) -> String {
    image_path(pid)
        .and_then(|exe| exe.file_name().map(|n| n.to_string_lossy().into_owned()))
        .map(|n| n.trim_end_matches(" (deleted)").to_owned())
        .or_else(|| comm(pid))
        .unwrap_or_default()
}

pub fn metadata(pid: u32) -> Option<Metadata> {
    let cwd = std::fs::read_link(proc(pid, "cwd")).ok()?;
    let raw = std::fs::read(proc(pid, "cmdline")).ok()?;
    let arguments: Vec<_> = raw
        .split(|&b| b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    (!arguments.is_empty()).then(|| Metadata {
        cwd: cwd.to_string_lossy().into_owned(),
        command_line: arguments.join(" "),
    })
}

/// The few variables named in `names` from the environment the process started with.
pub fn environment_values(pid: u32, names: &[&str]) -> HashMap<String, String> {
    let Ok(block) = std::fs::read(proc(pid, "environ")) else {
        return HashMap::new();
    };
    let mut found = HashMap::new();
    for entry in block.split(|&b| b == 0) {
        let Some(equals) = entry.iter().position(|&b| b == b'=') else {
            continue;
        };
        let variable = String::from_utf8_lossy(&entry[..equals]);
        if let Some(wanted) = names.iter().find(|wanted| **wanted == variable) {
            found.insert(
                (*wanted).to_owned(),
                String::from_utf8_lossy(&entry[equals + 1..]).into_owned(),
            );
        }
    }
    found
}

pub fn image_path(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(proc(pid, "exe")).ok()
}

/// The process and its parents, nearest first, stopping where a parent
/// started after its child (the pid was reused).
pub fn ancestors(pid: u32) -> Option<Vec<Entry>> {
    let (_, mut parent, mut started) = stat(pid)?;
    let mut chain = vec![Entry {
        pid,
        name: name(pid),
    }];
    while parent > 1 && chain.len() < 32 {
        let Some((_, next, begun)) = stat(parent) else {
            break;
        };
        if begun > started {
            break;
        }
        chain.push(Entry {
            pid: parent,
            name: name(parent),
        });
        (parent, started) = (next, begun);
    }
    Some(chain)
}

pub fn pids_named(image: &str) -> Vec<u32> {
    let wanted = image.trim_end_matches(".exe");
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|&pid| comm(pid).is_some_and(|c| c == wanted) || name(pid) == wanted)
        .collect()
}

/// Seconds since the Unix epoch at `ticks` clock ticks after boot, the unit
/// `stat` gives a process start in (and Claude Code records as `procStart`).
pub fn epoch_of_ticks(ticks: u64) -> Option<f64> {
    let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if per_second <= 0 {
        return None;
    }
    let boot: f64 = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("btime ")?.trim().parse().ok())?;
    Some(boot + ticks as f64 / per_second as f64)
}

/// Seconds since the Unix epoch when the process started.
pub fn started_at(pid: u32) -> Option<f64> {
    let (_, _, ticks) = stat(pid)?;
    epoch_of_ticks(ticks)
}

/// Alive and not a zombie waiting for its parent.
pub fn is_running(pid: u32) -> bool {
    stat(pid).is_some_and(|(state, _, _)| state != 'Z' && state != 'X')
}

/// The pseudo-terminal the process is attached to (`/dev/pts/12`), None without one.
pub fn terminal_of(pid: u32) -> Option<String> {
    device_path(terminal_number(
        &std::fs::read_to_string(proc(pid, "stat")).ok()?,
    )?)
}

/// The terminal's device number, the fifth field after the name in `stat` (0 is none).
fn terminal_number(text: &str) -> Option<u32> {
    let rest = &text[text.rfind(')')? + 1..];
    let number: u32 = rest.split_whitespace().nth(4)?.parse().ok()?;
    (number != 0).then_some(number)
}

/// A pseudo-terminal's path from its device number: majors 136 to 143 are `/dev/pts`.
fn device_path(number: u32) -> Option<String> {
    let major = (number >> 8) & 0xfff;
    let minor = (number & 0xff) | ((number >> 12) & 0xfff00);
    (136..=143)
        .contains(&major)
        .then(|| format!("/dev/pts/{}", (major - 136) * 256 + minor))
}

/// A `sleep` on a pseudo-terminal of its own, with `environment` added, for tests.
#[cfg(test)]
pub(super) struct OnTerminal {
    pub child: std::process::Child,
    pub device: String,
    _master: std::os::fd::OwnedFd,
}

#[cfg(test)]
impl OnTerminal {
    /// None where the system has no pseudo-terminals to give.
    pub(super) fn start(environment: &[(&str, &str)]) -> Option<OnTerminal> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) };
        if master < 0 {
            return None;
        }
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let mut path = [0 as libc::c_char; 128];
        let named = unsafe {
            libc::grantpt(master.as_raw_fd()) == 0
                && libc::unlockpt(master.as_raw_fd()) == 0
                && libc::ptsname_r(master.as_raw_fd(), path.as_mut_ptr(), path.len()) == 0
        };
        if !named {
            return None;
        }
        let device = unsafe { std::ffi::CStr::from_ptr(path.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let slave = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
        if slave < 0 {
            return None;
        }
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let mut command = Command::new("sleep");
        command
            .arg("30")
            .envs(environment.iter().copied())
            .stdin(Stdio::from(slave))
            .stdout(Stdio::null());
        // Its own session, with the pseudo-terminal on stdin as the terminal.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }
        let child = command.spawn().ok()?;
        // Until exec has finished /proc shows the program that forked it, and the
        // environment can read as empty a moment after the name has changed.
        for _ in 0..1000 {
            let ready = comm(child.id()).as_deref() == Some("sleep")
                && std::fs::read(proc(child.id(), "environ")).is_ok_and(|bytes| {
                    environment.iter().all(|(key, value)| {
                        let entry = format!("{key}={value}");
                        bytes.split(|b| *b == 0).any(|e| e == entry.as_bytes())
                    })
                });
            if ready {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Some(OnTerminal {
            child,
            device,
            _master: master,
        })
    }
}

#[cfg(test)]
impl Drop for OnTerminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Sleeper(Child);

    impl Sleeper {
        fn start() -> Sleeper {
            let child = Command::new("sleep")
                .arg("30")
                .env("OPENBAR_PROBE", "marker")
                .stdout(Stdio::null())
                .spawn()
                .expect("start sleep");
            // Until exec has finished /proc shows the program that forked it,
            // then a process whose arguments and environment are not set up yet.
            for _ in 0..1000 {
                let ready = comm(child.id()).as_deref() == Some("sleep")
                    && std::fs::read(proc(child.id(), "cmdline")).is_ok_and(|c| !c.is_empty());
                if ready {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Sleeper(child)
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for Sleeper {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn stat_survives_a_name_with_spaces_and_parentheses() {
        let line = "42 (a (b) c) S 7 42 42 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 9000 1000 100 18446744073709551615";
        assert_eq!(parse_stat(line), Some(('S', 7, 9000)));
        assert_eq!(parse_stat("42 (short) S 7"), None);
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn own_process_runs_and_started_moments_ago() {
        let pid = std::process::id();
        assert!(is_running(pid));
        let started = started_at(pid).expect("start time");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(started <= now + 1.0 && now - started < 3600.0);
    }

    #[test]
    fn a_reaped_process_is_not_running() {
        let mut child = Command::new("true").spawn().expect("start true");
        let pid = child.id();
        child.wait().unwrap();
        assert!(!is_running(pid));
        assert!(ancestors(pid).is_none());
    }

    #[test]
    fn metadata_names_the_folder_and_the_command() {
        let child = Sleeper::start();
        let found = metadata(child.pid()).expect("metadata");
        assert_eq!(PathBuf::from(&found.cwd), std::env::current_dir().unwrap());
        assert_eq!(found.command_line, "sleep 30");
    }

    #[test]
    fn environment_keeps_only_the_names_asked_for() {
        let child = Sleeper::start();
        let values = environment_values(child.pid(), &["OPENBAR_PROBE", "OPENBAR_ABSENT"]);
        assert_eq!(values.len(), 1);
        assert_eq!(values["OPENBAR_PROBE"], "marker");
    }

    #[test]
    fn ancestors_walk_up_from_the_process() {
        let child = Sleeper::start();
        let chain = ancestors(child.pid()).expect("chain");
        assert_eq!(
            chain[0],
            Entry {
                pid: child.pid(),
                name: "sleep".into()
            }
        );
        assert_eq!(chain[1].pid, std::process::id());
    }

    #[test]
    fn a_device_number_names_its_pseudo_terminal() {
        assert_eq!(device_path((136 << 8) | 12).as_deref(), Some("/dev/pts/12"));
        assert_eq!(device_path((137 << 8) | 1).as_deref(), Some("/dev/pts/257"));
        // A virtual console is not a pane.
        assert_eq!(device_path((4 << 8) | 1), None);
    }

    #[test]
    fn the_terminal_number_is_read_from_stat() {
        let line = "42 (a (b) c) S 7 42 42 34828 42 4194560 100 0 0 0 1 2 0 0 20 0 1 0 9000";
        assert_eq!(terminal_number(line), Some(34828));
        assert_eq!(terminal_number("42 (a) S 7 42 42 0 -1 4194560"), None);
        assert_eq!(terminal_number("42 (a) S 7"), None);
    }

    #[test]
    fn a_process_names_the_pseudo_terminal_it_runs_on() {
        let Some(session) = OnTerminal::start(&[]) else {
            eprintln!("no pseudo-terminals here");
            return;
        };
        assert_eq!(
            terminal_of(session.child.id()).as_deref(),
            Some(session.device.as_str())
        );
    }

    #[test]
    fn programs_are_found_by_name_with_or_without_exe() {
        let child = Sleeper::start();
        assert!(pids_named("sleep").contains(&child.pid()));
        assert!(pids_named("sleep.exe").contains(&child.pid()));
        assert_eq!(
            image_path(child.pid()).unwrap().file_name().unwrap(),
            "sleep"
        );
    }
}
