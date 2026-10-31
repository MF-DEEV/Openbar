// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Reads the `sessions/<pid>.json` records Claude Code keeps while a session
//! runs, ported from `ClaudeSessionRegistryReader.swift`. A killed session
//! leaves its record behind and its process number can come back for
//! something else, so a record counts only while a process with that number
//! started when the record says. Listing the folders, reading the files and
//! asking the system when a process started are the service's; everything
//! decided from what it found is here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::models::{
    RegistryStatus, SessionRecord, Time, days_from_civil, int_of, last_path_component, str_of,
};

/// Records may differ from the process start by the second they were
/// rounded to.
pub const START_TOLERANCE: f64 = 2.0;

/// A record is a few hundred bytes; anything this big is not one.
pub const MAX_RECORD_BYTES: usize = 1 << 16;

/// The `pidDomain` a record names the machine with. The Mac accepts only its
/// own, so a container sharing the folder, whose numbers mean other
/// processes, is left out.
pub const MAC_PID_DOMAINS: &[&str] = &["darwin"];

/// [VERIFY] What Claude Code writes as `pidDomain` on Windows has not been
/// seen yet. It looks like Node's `process.platform`, which is "win32"
/// there. A record that names no domain is always accepted, so a wrong guess
/// here hides sessions only from builds that do write one; check a real
/// record and fix this list. A WSL session writes its own, Linux, domain:
/// its pids are not Windows pids, so leaving it out is right.
pub const WINDOWS_PID_DOMAINS: &[&str] = &["win32"];

/// The folders Claude Code keeps its state in: the two standard ones, the
/// same the AI section reads logs from, then any `CLAUDE_CONFIG_DIR` moved it
/// to, as on the Mac. The variable counts only for the person's own home (a
/// test's home is not theirs); a `.claude…` folder beside `~/.claude` holding
/// a session registry (`~/.claude-work`) counts for any, since a session
/// started from a shell that set the variable leaves no trace in the app's
/// own environment.
pub fn homes(home: &Path) -> Vec<PathBuf> {
    let own = crate::hooks::installer::home_dir().is_some_and(|own| own == home);
    let named = own
        .then(|| std::env::var_os(CONFIG_VARIABLE))
        .flatten()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir());
    merge_homes(
        vec![home.join(".claude"), home.join(".config").join("claude")],
        named.into_iter().chain(sibling_homes(home)).collect(),
    )
}

pub const CONFIG_VARIABLE: &str = "CLAUDE_CONFIG_DIR";

/// `.claude…` folders in the home holding a session registry, by name.
pub fn sibling_homes(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(home) else {
        return vec![];
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".claude") && name != ".claude")
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| home.join(name))
        .filter(|folder| folder.join("sessions").is_dir())
        .collect()
}

/// The standard folders, then the extra ones none of them already is.
pub fn merge_homes(standard: Vec<PathBuf>, extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    standard
        .into_iter()
        .chain(extra)
        .filter(|folder| seen.insert(canonical_home(folder)))
        .collect()
}

/// A folder's identity: through links, without case or a trailing separator
/// (Windows paths ignore case).
fn canonical_home(folder: &Path) -> String {
    let resolved = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let text = resolved.to_string_lossy().to_lowercase();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        text
    } else {
        trimmed.to_string()
    }
}

pub fn folders(home: &Path) -> Vec<PathBuf> {
    homes(home)
        .into_iter()
        .map(|folder| folder.join("sessions"))
        .collect()
}

/// The pid a registry file is named after: only `<int>.json` counts, so a
/// `<pid>.<hash>.key` beside it is ignored.
pub fn pid_of_file(name: &str) -> Option<i32> {
    name.strip_suffix(".json")?.parse().ok()
}

/// One record, or nil when it is being written or describes a process this
/// machine cannot see, as one in a container sharing the folder.
pub fn record(data: &[u8], accepted_domains: &[&str]) -> Option<SessionRecord> {
    let value: Value = serde_json::from_slice(data).ok()?;
    let json = value.as_object()?;
    let pid = int_of(json.get("pid")).filter(|pid| *pid > 0 && *pid <= i64::from(i32::MAX))?;
    let session = str_of(json.get("sessionId")).filter(|s| !s.is_empty())?;
    let cwd = str_of(json.get("cwd")).filter(|c| !c.is_empty())?;
    if let Some(domain) = str_of(json.get("pidDomain"))
        && !accepted_domains.contains(&domain)
    {
        return None;
    }
    let status = str_of(json.get("status")).and_then(RegistryStatus::from_raw);
    Some(SessionRecord {
        pid: pid as i32,
        session_id: session.to_owned(),
        cwd: cwd.to_owned(),
        name: str_of(json.get("name")).map_or_else(|| last_path_component(cwd), str::to_owned),
        status,
        waiting_for: if status == Some(RegistryStatus::Waiting) {
            str_of(json.get("waitingFor")).map(str::to_owned)
        } else {
            None
        },
        status_updated: milliseconds(json.get("statusUpdatedAt"))
            .or_else(|| milliseconds(json.get("updatedAt"))),
        started: str_of(json.get("procStart"))
            .and_then(proc_start)
            .or_else(|| milliseconds(json.get("startedAt"))),
        version: str_of(json.get("version")).unwrap_or("").to_owned(),
        host: Default::default(),
        accepts_replies: false,
        name_source: str_of(json.get("nameSource")).map(str::to_owned),
    })
}

/// The record belongs to the process now running under its number: `actual`
/// is when that process started, nil when none runs. A record with no start
/// of its own matches any running process.
pub fn is_current(recorded: Option<Time>, actual: Option<Time>) -> bool {
    let Some(actual) = actual else { return false };
    let Some(recorded) = recorded else {
        return true;
    };
    (recorded - actual).abs() <= START_TOLERANCE
        // Older records give only the moment the session began, a little
        // after its process did.
        || (recorded > actual && recorded - actual < 60.0)
}

/// `procStart` is `ps -o lstart=` read in UTC, like "Wed Oct 21 17:50:25 2026",
/// with runs of spaces collapsed as the Mac does. [VERIFY] what Claude Code
/// writes on Windows; `startedAt` is the fallback when this does not parse.
pub fn proc_start(text: &str) -> Option<Time> {
    let words: Vec<&str> = text.split(' ').filter(|word| !word.is_empty()).collect();
    let [weekday, month, day, time, year] = words.as_slice() else {
        return None;
    };
    const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let named = |word: &str, names: &[&str]| -> Option<usize> {
        let lower = word.to_ascii_lowercase();
        names
            .iter()
            .position(|name| lower == *name || (lower.len() > 3 && lower.starts_with(name)))
    };
    named(weekday, &WEEKDAYS)?;
    let month = named(month, &MONTHS)? as i64 + 1;
    let digits = |word: &str, lengths: std::ops::RangeInclusive<usize>| -> Option<i64> {
        (lengths.contains(&word.len()) && word.bytes().all(|b| b.is_ascii_digit()))
            .then(|| word.parse().ok())?
    };
    let day = digits(day, 1..=2)?;
    let year = digits(year, 4..=4)?;
    let [hour, minute, second] = time.split(':').collect::<Vec<_>>()[..] else {
        return None;
    };
    let (hour, minute, second) = (
        digits(hour, 1..=2)?,
        digits(minute, 1..=2)?,
        digits(second, 1..=2)?,
    );
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ][month as usize - 1];
    if day < 1 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second) as f64)
}

fn milliseconds(value: Option<&Value>) -> Option<Time> {
    let number = value?.as_f64()?;
    (number > 0.0).then(|| number / 1000.0)
}

/// A file the service found in a registry folder: its name, and its
/// contents, or nil when it could not be read.
#[derive(Clone, Debug, PartialEq)]
pub struct RegistryFile {
    pub name: String,
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scan {
    /// The running sessions, by pid.
    pub records: Vec<SessionRecord>,
    /// Running processes whose record could not be read this time, as while
    /// Claude Code rewrites it; the service keeps their last good record.
    pub unreadable: Vec<i32>,
}

/// The running sessions among the files of every registry folder, in the
/// order of `folders`. Across homes the first record per pid wins.
/// `process_start` says when the process with a pid started, or nil when
/// none runs.
pub fn scan(
    files: &[RegistryFile],
    accepted_domains: &[&str],
    process_start: impl Fn(i32) -> Option<Time>,
) -> Scan {
    let mut records: BTreeMap<i32, SessionRecord> = BTreeMap::new();
    let mut unreadable: BTreeSet<i32> = BTreeSet::new();
    for file in files {
        // No process has a pid of zero or below, and Windows' 0 is the idle
        // process; the Mac guarded this inside its own lookup.
        let Some(pid) = pid_of_file(&file.name).filter(|pid| *pid > 0) else {
            continue;
        };
        let parsed = file
            .data
            .as_deref()
            .filter(|data| data.len() < MAX_RECORD_BYTES)
            .and_then(|data| record(data, accepted_domains));
        let Some(parsed) = parsed else {
            if process_start(pid).is_some() {
                unreadable.insert(pid);
            }
            continue;
        };
        if records.contains_key(&parsed.pid)
            || !is_current(parsed.started, process_start(parsed.pid))
        {
            continue;
        }
        records.insert(parsed.pid, parsed);
    }
    for pid in records.keys() {
        unreadable.remove(pid);
    }
    Scan {
        records: records.into_values().collect(),
        unreadable: unreadable.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    // Skipped from ClaudeSessionReaderTests.swift: `hosts` (Terminal tty and
    // Antigravity classification, `KERN_PROCARGS2` environment parsing,
    // `proc_pidinfo` start times), all macOS process APIs that the Windows
    // service replaces.
    use super::*;

    const WAITING: &str = r#"{"pid":85842,"sessionId":"99c35831","cwd":"/tmp/spike/work","startedAt":1792605026467,"procStart":"Wed Oct 21 17:50:25 2026","version":"2.1.306","kind":"interactive","pidDomain":"darwin","messagingSocketPath":"/tmp/cc-socks/85842.sock","name":"work-1f","status":"waiting","waitingFor":"permission prompt","updatedAt":1792605040000,"statusUpdatedAt":1792605039000}"#;

    fn mac(json: &str) -> Option<SessionRecord> {
        record(json.as_bytes(), MAC_PID_DOMAINS)
    }

    // ClaudeSessionReaderTests.swift registry.
    #[test]
    fn registry() {
        let parsed = mac(WAITING).unwrap();
        assert!(
            parsed.pid == 85842
                && parsed.session_id == "99c35831"
                && parsed.cwd == "/tmp/spike/work"
                && parsed.name == "work-1f",
            "a registry record gives the process, session, folder and name"
        );
        assert!(
            parsed.status == Some(RegistryStatus::Waiting)
                && parsed.waiting_for.as_deref() == Some("permission prompt"),
            "and whether the session waits, and why"
        );
        assert_eq!(
            parsed.status_updated,
            Some(1_792_605_039.0),
            "the status change time is read from milliseconds"
        );
        let started = 1_792_605_025.0;
        assert_eq!(parsed.started, Some(started), "procStart is read as UTC");
        assert_eq!(parsed.version, "2.1.306");

        let idle = mac(r#"{"pid":7,"sessionId":"s","cwd":"/a","status":"idle","waitingFor":"x"}"#)
            .unwrap();
        assert!(
            idle.waiting_for.is_none() && idle.name == "a",
            "an idle record keeps no reason and is named after its folder"
        );
        let renamed =
            mac(r#"{"pid":7,"sessionId":"s","cwd":"/a","name":"bombs","nameSource":"user"}"#)
                .unwrap();
        assert!(
            renamed.name == "bombs"
                && renamed.name_source.as_deref() == Some("user")
                && idle.name_source.is_none(),
            "where a session's name came from is read"
        );
        let unknown = mac(r#"{"pid":7,"sessionId":"s","cwd":"/a","status":"thinking"}"#);
        assert!(
            unknown.is_some_and(|r| r.status.is_none()),
            "an unknown status leaves the transcript to decide"
        );
        for (label, json) in [
            ("partial", r#"{"pid":7,"sessionId":"#),
            ("no session", r#"{"pid":7,"cwd":"/a"}"#),
            (
                "other machine",
                r#"{"pid":7,"sessionId":"s","cwd":"/a","pidDomain":"linux"}"#,
            ),
            ("bad pid", r#"{"pid":-3,"sessionId":"s","cwd":"/a"}"#),
            (
                "huge pid",
                r#"{"pid":2147483648,"sessionId":"s","cwd":"/a"}"#,
            ),
            (
                "fractional pid",
                r#"{"pid":7.5,"sessionId":"s","cwd":"/a"}"#,
            ),
            ("no folder", r#"{"pid":7,"sessionId":"s","cwd":""}"#),
        ] {
            assert!(mac(json).is_none(), "a {label} record is skipped");
        }

        assert!(
            is_current(parsed.started, Some(started + 0.6)),
            "a record matches the process started that second"
        );
        assert!(
            !is_current(parsed.started, Some(started - 3600.0)),
            "a record left by a killed session does not match a reused process number"
        );
        assert!(
            !is_current(parsed.started, None),
            "nor a process that is gone"
        );
        assert!(
            is_current(None, Some(started)),
            "a record without a start matches any running process"
        );
        assert!(
            is_current(Some(started + 30.0), Some(started)),
            "an older record's session began a little after its process"
        );
        assert!(
            !is_current(Some(started + 61.0), Some(started))
                && !is_current(Some(started - 2.5), Some(started))
        );

        // Read from a folder, as the service does, with the files in hand.
        let file = |name: &str, data: Option<&str>| RegistryFile {
            name: name.into(),
            data: data.map(|d| d.as_bytes().to_vec()),
        };
        let folder = vec![
            file("85842.json", Some(WAITING)),
            file(
                "90.json",
                Some(
                    r#"{"pid":90,"sessionId":"gone","cwd":"/b","procStart":"Wed Oct 21 10:00:00 2026"}"#,
                ),
            ),
            file("85842.abc.key", Some("{}")),
        ];
        let running = |pid: i32| (pid == 85842).then_some(started);
        let read = scan(&folder, MAC_PID_DOMAINS, running);
        assert_eq!(
            read.records
                .iter()
                .map(|r| r.session_id.as_str())
                .collect::<Vec<_>>(),
            ["99c35831"],
            "only records of running processes are listed"
        );
        let mut torn = folder.clone();
        torn.push(file("85843.json", Some(r#"{"pid":85843,"sessionId":"#)));
        torn.push(file("85844.json", None));
        let both = |pid: i32| matches!(pid, 85842..=85844).then_some(started);
        let caught = scan(&torn, MAC_PID_DOMAINS, both);
        assert!(
            caught.records.iter().map(|r| r.pid).eq([85842]) && caught.unreadable == [85843, 85844],
            "a running session's record caught mid-write is reported, not dropped silently"
        );
        let mut oversized = folder.clone();
        oversized.push(file(
            "5.json",
            Some(&format!(
                r#"{{"pid":5,"sessionId":"s","cwd":"/a","pad":"{}"}}"#,
                "x".repeat(70_000)
            )),
        ));
        assert_eq!(
            scan(&oversized, MAC_PID_DOMAINS, |_| Some(started)).unreadable,
            [5],
            "an oversized file is no record"
        );

        // Across homes the first record per pid wins, and the output is by pid.
        let second_home = vec![
            file(
                "12.json",
                Some(r#"{"pid":12,"sessionId":"first","cwd":"/a"}"#),
            ),
            file("3.json", Some(r#"{"pid":3,"sessionId":"low","cwd":"/a"}"#)),
            file(
                "12.json",
                Some(r#"{"pid":12,"sessionId":"second","cwd":"/a"}"#),
            ),
        ];
        let merged = scan(&second_home, MAC_PID_DOMAINS, |_| Some(started));
        assert_eq!(
            merged
                .records
                .iter()
                .map(|r| r.session_id.as_str())
                .collect::<Vec<_>>(),
            ["low", "first"]
        );
        // A readable record for a pid also listed as torn clears it.
        let healed = vec![
            file("12.json", None),
            file("12.json", Some(r#"{"pid":12,"sessionId":"ok","cwd":"/a"}"#)),
        ];
        assert!(
            scan(&healed, MAC_PID_DOMAINS, |_| Some(started))
                .unreadable
                .is_empty()
        );

        assert_eq!(
            folders(Path::new("/Users/me")),
            [
                PathBuf::from("/Users/me/.claude/sessions"),
                PathBuf::from("/Users/me/.config/claude/sessions")
            ],
            "both Claude Code homes are read"
        );
        assert_eq!(pid_of_file("85842.json"), Some(85842));
        assert_eq!(pid_of_file("85842.abc.key"), None);
        assert_eq!(pid_of_file("x.json"), None);
    }

    #[test]
    fn config_homes() {
        let home = std::env::temp_dir().join(format!("vs-config-homes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for folder in [
            ".claude/sessions",
            ".claude-2/sessions",
            ".claude-old",
            ".config/claude/sessions",
        ] {
            std::fs::create_dir_all(home.join(folder)).unwrap();
        }
        std::fs::write(home.join(".claude.json"), "{}").unwrap();
        let names: Vec<String> = homes(&home)
            .iter()
            .map(|h| h.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [".claude", "claude", ".claude-2"],
            "a .claude… folder holding a session registry is read after the standard two; \
             one without a registry, and files, are not"
        );
        assert_eq!(
            folders(&home).last(),
            Some(&home.join(".claude-2").join("sessions")),
            "a CLAUDE_CONFIG_DIR=~/.claude-2 session's records are read"
        );
        let merged = merge_homes(
            vec![home.join(".claude")],
            vec![
                home.join(".claude/"),
                home.join(".CLAUDE"),
                home.join(".claude-2"),
            ],
        );
        assert_eq!(
            merged,
            [home.join(".claude"), home.join(".claude-2")],
            "a folder named twice is read once"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    // The Windows change: the accepted domain is a parameter.
    #[test]
    fn domains() {
        let windows =
            r#"{"pid":4321,"sessionId":"w","cwd":"C:\\Users\\me\\code\\app","pidDomain":"win32"}"#;
        let parsed = record(windows.as_bytes(), WINDOWS_PID_DOMAINS).unwrap();
        assert_eq!(parsed.name, "app", "a Windows folder names the session too");
        assert!(
            record(windows.as_bytes(), MAC_PID_DOMAINS).is_none(),
            "the Mac refuses a Windows record"
        );
        assert!(
            record(WAITING.as_bytes(), WINDOWS_PID_DOMAINS).is_none(),
            "and Windows a Mac one"
        );
        assert!(
            record(WAITING.as_bytes(), &["darwin", "win32"]).is_some(),
            "several domains can be accepted"
        );
        let unnamed = r#"{"pid":4321,"sessionId":"w","cwd":"/a"}"#;
        assert!(
            record(unnamed.as_bytes(), WINDOWS_PID_DOMAINS).is_some(),
            "a record naming no domain is accepted"
        );
        let odd = r#"{"pid":4321,"sessionId":"w","cwd":"/a","pidDomain":7}"#;
        assert!(
            record(odd.as_bytes(), WINDOWS_PID_DOMAINS).is_some(),
            "a domain that is not text is ignored, as on the Mac"
        );
    }

    #[test]
    fn starts() {
        assert_eq!(
            proc_start("Wed Oct 21 17:50:25 2026"),
            Some(1_792_605_025.0)
        );
        assert_eq!(
            proc_start("Wed Oct  1 07:05:09 2026"),
            Some(1_790_838_309.0),
            "runs of spaces collapse"
        );
        assert_eq!(proc_start("  Thu Jan 1 00:00:00 1970 "), Some(0.0));
        assert_eq!(
            proc_start("Sun Feb 29 12:00:00 2032"),
            Some(1_961_668_800.0),
            "leap days exist"
        );
        assert_eq!(
            proc_start("Wed Feb 30 12:00:00 2026"),
            None,
            "impossible days do not"
        );
        assert_eq!(proc_start("Wed Oct 21 24:00:00 2026"), None);
        assert_eq!(proc_start("Wed Foo 21 17:50:25 2026"), None);
        assert_eq!(proc_start("Wed Oct 21 17:50 2026"), None);
        assert_eq!(proc_start("Wed Oct 21 17:50:25 2026 extra"), None);
        assert_eq!(proc_start("2026-10-21T17:50:25Z"), None);
        let fallback = record(br#"{"pid":7,"sessionId":"s","cwd":"/a","procStart":"garbage","startedAt":1792605026467}"#, MAC_PID_DOMAINS);
        assert_eq!(
            fallback.unwrap().started,
            Some(1_792_605_026.467),
            "an unreadable procStart falls back to startedAt"
        );
        let updated = record(
            br#"{"pid":7,"sessionId":"s","cwd":"/a","updatedAt":1000,"statusUpdatedAt":0}"#,
            MAC_PID_DOMAINS,
        );
        assert_eq!(
            updated.unwrap().status_updated,
            Some(1.0),
            "a zero time is no time"
        );
    }

    #[test]
    fn no_pid_zero() {
        let files = [
            RegistryFile {
                name: "0.json".into(),
                data: None,
            },
            RegistryFile {
                name: "-5.json".into(),
                data: Some(b"{}".to_vec()),
            },
        ];
        let scanned = scan(&files, MAC_PID_DOMAINS, |pid| {
            panic!("asked about pid {pid}")
        });
        assert!(
            scanned.records.is_empty() && scanned.unreadable.is_empty(),
            "pids of zero and below are never looked up"
        );
    }
}
