// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

//! The asks Allow all answered (#281). The queue allows every ask that looks
//! safe with one click, so nobody read each row; this keeps what went through,
//! newest first, so it can be looked at afterwards in Settings › Sessions.
//! Only asks the agent actually took are kept, and only from Allow all: an
//! ask allowed on its own row was read on its row.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::models::{Permission, Session, Time};

/// Told to the settings window when the list changes.
pub const CHANGED: &str = "sessions://batch-approvals";
/// Entries kept; the oldest go first.
pub const CAP: usize = 500;
/// Characters of a command kept, so one huge heredoc can't bloat the file.
pub const DETAIL_LIMIT: usize = 4000;
/// Paths kept for one ask.
const PATH_LIMIT: usize = 50;
/// The file, read whole, is ignored past this size.
const FILE_LIMIT: u64 = 16 * 1024 * 1024;

/// One ask Allow all answered. The Mac writes the same fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The click it came with; the asks of one click share it.
    pub batch: String,
    /// Seconds since 1970.
    pub at: Time,
    pub session_id: String,
    /// The session's name as the island showed it.
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub project: String,
    /// The agent, as `Provider` serializes it (claude, codex, opencode…).
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub tool: String,
    /// The whole command, path or arguments, as the row showed them.
    #[serde(default)]
    pub detail: String,
    /// The files a change touches, when it names them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

impl Entry {
    pub fn of(batch: &str, session: &Session, ask: &Permission, at: Time) -> Self {
        let agent = serde_json::to_value(session.provider)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let detail = if ask.detail.trim().is_empty() {
            ask.summary.clone().unwrap_or_default()
        } else {
            ask.detail.clone()
        };
        let mut paths: Vec<String> = Vec::new();
        // A move names where the file goes too.
        for path in ask
            .diffs
            .iter()
            .flat_map(|d| std::iter::once(&d.path).chain(&d.moved_to))
        {
            if !path.is_empty() && !paths.contains(path) && paths.len() < PATH_LIMIT {
                paths.push(path.clone());
            }
        }
        // A path the detail already is says nothing more.
        if paths.len() == 1 && paths[0] == detail.trim() {
            paths.clear();
        }
        Entry {
            batch: batch.to_owned(),
            at,
            session_id: session.id.clone(),
            session: session.name.clone(),
            project: session.project.clone(),
            agent,
            tool: ask.tool.clone(),
            detail: clip(&detail, DETAIL_LIMIT),
            paths,
        }
    }
}

/// At most `limit` characters, with an ellipsis where it was cut.
fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

struct State {
    path: Option<PathBuf>,
    /// Newest first; read from disk on first use.
    entries: Option<Vec<Entry>>,
}

pub struct Log {
    state: Mutex<State>,
}

impl Log {
    pub const fn new() -> Self {
        Log {
            state: Mutex::new(State {
                path: None,
                entries: None,
            }),
        }
    }

    /// Where the list is kept. Until set, it lives only in memory.
    pub fn set_path(&self, path: PathBuf) {
        let mut state = self.state.lock().unwrap();
        state.path = Some(path);
        state.entries = None;
    }

    fn loaded(state: &mut State) -> &mut Vec<Entry> {
        if state.entries.is_none() {
            state.entries = Some(state.path.as_deref().map(load).unwrap_or_default());
        }
        state.entries.as_mut().unwrap()
    }

    /// Keeps the ask, newest first, and writes the list.
    pub fn record(&self, entry: Entry) {
        let mut state = self.state.lock().unwrap();
        let entries = Self::loaded(&mut state);
        entries.insert(0, entry);
        entries.truncate(CAP);
        let snapshot = entries.clone();
        if let Some(path) = state.path.clone() {
            save(&path, &snapshot);
        }
    }

    pub fn entries(&self) -> Vec<Entry> {
        let mut state = self.state.lock().unwrap();
        Self::loaded(&mut state).clone()
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.entries = Some(Vec::new());
        if let Some(path) = state.path.clone() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub static LOG: Log = Log::new();

/// A missing, unreadable or broken file reads as an empty list.
fn load(path: &Path) -> Vec<Entry> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Vec::new();
    };
    if meta.len() > FILE_LIMIT {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = serde_json::from_slice(&bytes).unwrap_or_default();
    entries.sort_by(|a, b| b.at.total_cmp(&a.at));
    entries.truncate(CAP);
    entries
}

/// Written whole through a temporary file, so a crash never leaves half a list.
fn save(path: &Path, entries: &[Entry]) {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(entries)?)?;
        // Commands can hold secrets: owner-only, like the Mac's container.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)
    };
    let _ = write();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::file_diff::{Change, FileDiff};

    fn session() -> Session {
        let mut s = crate::sessions::sample::waiting(1, "/home/me", 0.0);
        s.id = "s1".into();
        s.name = "fix the tests".into();
        s.project = "openbar".into();
        s
    }

    fn ask(detail: &str) -> Permission {
        let mut p = Permission::new("r1", "Bash", None, true, 0.0);
        p.detail = detail.into();
        p
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-batch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("batch-approvals.json")
    }

    #[test]
    fn keeps_the_whole_command_and_the_session() {
        let e = Entry::of("b1", &session(), &ask("git status\ngit diff"), 10.0);
        assert_eq!(e.detail, "git status\ngit diff");
        assert_eq!(
            (e.session.as_str(), e.project.as_str(), e.agent.as_str()),
            ("fix the tests", "openbar", "claude")
        );
        assert_eq!(e.tool, "Bash");
        assert!(e.paths.is_empty());
    }

    #[test]
    fn a_change_lists_its_files_and_a_long_command_is_cut() {
        let mut p = ask("/repo/a.rs");
        p.tool = "Edit".into();
        p.diffs = vec![FileDiff {
            path: "/repo/a.rs".into(),
            change: Change::Update,
            ..FileDiff::default()
        }];
        assert!(Entry::of("b", &session(), &p, 1.0).paths.is_empty());
        p.detail = String::new();
        p.summary = Some("2 files".into());
        p.diffs.push(FileDiff {
            path: "/repo/b.rs".into(),
            moved_to: Some("/repo/c.rs".into()),
            ..FileDiff::default()
        });
        let e = Entry::of("b", &session(), &p, 1.0);
        assert_eq!(e.detail, "2 files");
        assert_eq!(e.paths, vec!["/repo/a.rs", "/repo/b.rs", "/repo/c.rs"]);
        let long = "é".repeat(DETAIL_LIMIT + 10);
        let e = Entry::of("b", &session(), &ask(&long), 1.0);
        assert_eq!(e.detail.chars().count(), DETAIL_LIMIT + 1);
        assert!(e.detail.ends_with('…'));
    }

    #[test]
    fn newest_first_capped_and_kept_on_disk() {
        let path = temp("cap");
        let log = Log::new();
        log.set_path(path.clone());
        for i in 0..CAP + 5 {
            log.record(Entry::of(
                "b",
                &session(),
                &ask(&format!("echo {i}")),
                i as f64,
            ));
        }
        let entries = log.entries();
        assert_eq!(entries.len(), CAP);
        assert_eq!(entries[0].detail, format!("echo {}", CAP + 4));
        let again = Log::new();
        again.set_path(path.clone());
        assert_eq!(again.entries(), entries);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        again.clear();
        assert!(again.entries().is_empty());
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_broken_file_reads_as_empty() {
        let path = temp("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        let log = Log::new();
        log.set_path(path.clone());
        assert!(log.entries().is_empty());
        log.record(Entry::of("b", &session(), &ask("ls"), 1.0));
        assert_eq!(load(&path).len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
