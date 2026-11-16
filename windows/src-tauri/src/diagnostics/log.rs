// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Openbar's own log on disk: one file a day in the app's log folder, kept
//! for a week, so a diagnostics file (#230) can carry the last few days.
//! The same files, names and line format as the Mac app's
//! (`DiagnosticsLog.swift`). Every `eprintln!` in the app lands here too
//! (see the macro in lib.rs), masked before it is written.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

/// Files older than this are deleted.
pub const KEEP_DAYS: i64 = 7;
/// How many days a diagnostics file takes.
pub const EXPORT_DAYS: i64 = 3;
/// A day's file stops growing here, with a line that says so.
pub const DAILY_LIMIT: u64 = 2 * 1024 * 1024;
/// Lines written before the folder is known, kept to write once it is.
const EARLY_LINES: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

struct Writer {
    folder: Option<PathBuf>,
    home: String,
    early: Vec<(String, String)>,
    day: Option<String>,
    file: Option<File>,
    written: u64,
    full: bool,
}

static WRITER: Mutex<Writer> = Mutex::new(Writer {
    folder: None,
    home: String::new(),
    early: Vec::new(),
    day: None,
    file: None,
    written: 0,
    full: false,
});

/// Starts writing to `folder`, with what was said before it was known.
pub fn init(folder: PathBuf) {
    let home = crate::hooks::installer::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    writer.folder = Some(folder);
    writer.home = home;
    let early = std::mem::take(&mut writer.early);
    for (day, line) in early {
        writer.append(&day, &line);
    }
}

/// The folder the log is written to, once known.
pub fn folder() -> Option<PathBuf> {
    WRITER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .folder
        .clone()
}

pub fn info(category: &str, message: &str) {
    write(Level::Info, category, message);
}

pub fn error(category: &str, message: &str) {
    write(Level::Error, category, message);
}

pub fn write(level: Level, category: &str, message: &str) {
    let now = Utc::now();
    let mut writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    let home = writer.home.clone();
    let line = line(level, category, message, now, &home);
    let day = day(now);
    if writer.folder.is_none() {
        if writer.early.len() < EARLY_LINES {
            writer.early.push((day, line));
        }
        return;
    }
    writer.append(&day, &line);
}

/// What `eprintln!` prints, on stderr as before and in the log.
pub fn stderr_line(args: std::fmt::Arguments<'_>) {
    let text = args.to_string();
    let _ = writeln!(std::io::stderr(), "{text}");
    write(Level::Info, "stderr", &text);
}

/// Waits for nothing: lines are written as they come. Kept for the
/// export, which reads the files right after.
pub fn flush() {
    let mut writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = writer.file.as_mut() {
        let _ = file.flush();
    }
}

/// The files of the last `days` days, oldest first.
pub fn recent_files(days: i64, now: DateTime<Utc>) -> Vec<PathBuf> {
    flush();
    let Some(folder) = folder() else {
        return Vec::new();
    };
    let names: Vec<String> = fs::read_dir(&folder)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    recent(&names, days, now)
        .into_iter()
        .map(|name| folder.join(name))
        .collect()
}

// MARK: Format (kept in step with DiagnosticsLog.swift)

/// `2026-11-16T15:30:02.123Z INFO  [sessions] hooks installed for gemini`
pub fn line(level: Level, category: &str, message: &str, at: DateTime<Utc>, home: &str) -> String {
    let flat = message.replace("\r\n", " ").replace(['\n', '\r'], " ");
    format!(
        "{} {:<5} [{category}] {}\n",
        timestamp(at),
        level.name(),
        super::redact::redact(&flat, home)
    )
}

pub fn timestamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// The UTC day a line belongs to, as its file names it.
pub fn day(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d").to_string()
}

pub fn file_name(day: &str) -> String {
    format!("openbar-{day}.log")
}

/// The day a log file is named after; None for any other file.
pub fn day_of(name: &str) -> Option<&str> {
    let day = name.strip_prefix("openbar-")?.strip_suffix(".log")?;
    (day.len() == 10 && day.chars().all(|c| c.is_ascii_digit() || c == '-')).then_some(day)
}

/// The log files among `names` from the last `days` days (today counts),
/// oldest first.
pub fn recent(names: &[String], days: i64, now: DateTime<Utc>) -> Vec<String> {
    let first = day(now - Duration::days(days.max(1) - 1));
    let mut found: Vec<(&str, &String)> = names
        .iter()
        .filter_map(|name| {
            day_of(name)
                .filter(|d| *d >= first.as_str())
                .map(|d| (d, name))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, name)| name.clone()).collect()
}

impl Writer {
    fn append(&mut self, day: &str, line: &str) {
        if self.day.as_deref() != Some(day) {
            self.open(day);
        }
        if self.full {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let mut bytes = line.as_bytes().to_vec();
        if self.written + bytes.len() as u64 > DAILY_LIMIT {
            self.full = true;
            bytes = super::log::line(
                Level::Warn,
                "log",
                "today's log reached its size limit; later lines are dropped",
                Utc::now(),
                &self.home,
            )
            .into_bytes();
        }
        if file.write_all(&bytes).is_ok() {
            self.written += bytes.len() as u64;
        } else {
            self.file = None;
        }
    }

    fn open(&mut self, day: &str) {
        self.file = None;
        self.day = Some(day.to_string());
        self.full = false;
        let Some(folder) = self.folder.clone() else {
            return;
        };
        let _ = fs::create_dir_all(&folder);
        let path = folder.join(file_name(day));
        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        self.written = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.full = self.written >= DAILY_LIMIT;
        self.file = Some(file);
        prune(&folder, day);
    }
}

/// Deletes the files older than the week kept.
fn prune(folder: &Path, today: &str) {
    let Ok(now) = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return;
    };
    let now = now.and_hms_opt(12, 0, 0).unwrap_or_default().and_utc();
    let names: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let kept = recent(&names, KEEP_DAYS, now);
    for name in names {
        if day_of(&name).is_some() && !kept.contains(&name) {
            let _ = fs::remove_file(folder.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.timestamp_millis_opt(1_763_307_002_123).unwrap()
    }

    #[test]
    fn lines_match_the_mac_format() {
        let line = line(
            Level::Info,
            "sessions",
            "two\nlines in /Users/jane/x with sk-ant-api03-abcdefghij",
            at(),
            "/Users/jane",
        );
        assert_eq!(
            line,
            "2025-11-16T15:30:02.123Z INFO  [sessions] two lines in ~/x with <secret>\n"
        );
        assert!(super::line(Level::Error, "a", "b", at(), "/x").contains(" ERROR [a] b"));
        assert_eq!(day(at()), "2025-11-16");
        assert_eq!(file_name("2025-11-16"), "openbar-2025-11-16.log");
    }

    #[test]
    fn recent_files_are_the_last_days_oldest_first() {
        let names: Vec<String> = [
            "openbar-2025-11-13.log",
            "openbar-2025-11-16.log",
            "openbar-2025-11-14.log",
            "openbar-2025-11-15.log",
            "notes.txt",
            "openbar-2025-11-16.log.partial",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            recent(&names, 3, at()),
            [
                "openbar-2025-11-14.log",
                "openbar-2025-11-15.log",
                "openbar-2025-11-16.log"
            ]
        );
        assert_eq!(day_of("openbar-2025-11-16.log"), Some("2025-11-16"));
        assert_eq!(day_of("other.log"), None);
    }

    #[test]
    fn old_files_are_pruned() {
        let folder = std::env::temp_dir().join(format!("openbar-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("openbar-2025-11-01.log"), "old\n").unwrap();
        fs::write(folder.join("openbar-2025-11-12.log"), "kept\n").unwrap();
        prune(&folder, "2025-11-16");
        assert!(!folder.join("openbar-2025-11-01.log").exists());
        assert!(folder.join("openbar-2025-11-12.log").exists());
        let _ = fs::remove_dir_all(&folder);
    }
}
