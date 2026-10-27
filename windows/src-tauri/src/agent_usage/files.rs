// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Where agents keep their logs (spec 03 §12) and the incremental reader
//! that hands over only newly appended lines (AgentLogRoot, AgentLogCursor
//! and AgentLogReader in AgentUsageStore.swift).
//!
//! Logs are opened with FILE_SHARE_READ|WRITE|DELETE so Claude, Codex and
//! Copilot can still append, rotate or delete them while they are read
//! (§13.6). Identity is the NTFS file index (§7).

use super::model::{Provider, Time};
use super::parse::LogState;
use super::store::Registry;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const CHUNK: usize = 4 << 20;
/// A longer line is a pasted file or a tool's output, never a usage record.
pub const MAXIMUM_LINE: usize = 32 << 20;
pub const OPENCODE_DATABASE: &str = "opencode.db";

#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    pub provider: Provider,
    pub path: PathBuf,
}

pub fn home() -> Option<PathBuf> {
    crate::hooks::installer::home_dir()
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// `%CODEX_HOME%` or `%USERPROFILE%\.codex`.
pub fn codex_home() -> Option<PathBuf> {
    env_dir("CODEX_HOME").or_else(|| home().map(|h| h.join(".codex")))
}

/// `%CLAUDE_CONFIG_DIR%` or `%USERPROFILE%\.claude`.
pub fn claude_dir() -> Option<PathBuf> {
    env_dir("CLAUDE_CONFIG_DIR").or_else(|| home().map(|h| h.join(".claude")))
}

/// Every log root, whether or not it exists yet. Unlike the Mac, the
/// relocation variables are honoured (§13.8).
pub fn roots() -> Vec<Root> {
    let Some(home) = home() else { return vec![] };
    let mut out = vec![Root { provider: Provider::Claude, path: home.join(".claude").join("projects") }];
    if let Some(dir) = env_dir("CLAUDE_CONFIG_DIR") {
        out.push(Root { provider: Provider::Claude, path: dir.join("projects") });
    }
    out.push(Root { provider: Provider::Claude, path: home.join(".config").join("claude").join("projects") });
    if let Some(codex) = codex_home() {
        out.push(Root { provider: Provider::Codex, path: codex.join("sessions") });
        out.push(Root { provider: Provider::Codex, path: codex.join("archived_sessions") });
    }
    let data = env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local").join("share"));
    out.push(Root { provider: Provider::Opencode, path: data.join("opencode") });
    let copilot = env_dir("COPILOT_HOME").unwrap_or_else(|| home.join(".copilot"));
    out.push(Root { provider: Provider::Copilot, path: copilot.join("session-state") });
    let mut seen = Vec::<String>::new();
    out.retain(|r| {
        let key = r.path.to_string_lossy().to_lowercase();
        !seen.contains(&key) && {
            seen.push(key);
            true
        }
    });
    out
}

/// Whether a provider's logs exist on this PC (Settings: "Found on this PC").
pub fn found(provider: Provider) -> bool {
    roots().iter().any(|r| r.provider == provider && r.path.is_dir())
}

/// Claude Code keeps a session's subagents in `<session>\subagents\`,
/// beside the session's own `<session>.jsonl`.
pub fn parent_of(path: &str) -> Option<String> {
    let at = path.rfind("\\subagents\\").or_else(|| path.rfind("/subagents/"))?;
    Some(format!("{}.jsonl", &path[..at]))
}

pub fn modified(meta: &std::fs::Metadata) -> Time {
    meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The newer of the database and its write-ahead log.
pub fn opencode_modified(path: &Path) -> Option<Time> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let wal = std::fs::metadata(format!("{}-wal", path.display())).map(|m| modified(&m)).unwrap_or(0.0);
    Some(modified(&meta).max(wal))
}

/// Log files changed since `horizon`, oldest first, Claude subagents after
/// every session so the turn each one works for already stands.
pub fn discover(roots: &[Root], horizon: Time) -> Vec<(String, Provider)> {
    let mut found: Vec<(String, Provider, Time, bool)> = Vec::new();
    for root in roots.iter().filter(|r| r.path.is_dir()) {
        match root.provider {
            // The database sits beside snapshots and clones, never walked.
            Provider::Opencode => {
                let path = root.path.join(OPENCODE_DATABASE);
                if let Some(m) = opencode_modified(&path).filter(|m| *m >= horizon) {
                    found.push((path.to_string_lossy().into_owned(), root.provider, m, false));
                }
            }
            // A session folder can hold a whole workspace checkout: only
            // `<session>\events.jsonl` is looked at.
            Provider::Copilot => {
                for dir in std::fs::read_dir(&root.path).into_iter().flatten().flatten() {
                    if dir.file_name().to_string_lossy().starts_with('.') {
                        continue;
                    }
                    let path = dir.path().join("events.jsonl");
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if meta.is_file() && modified(&meta) >= horizon {
                            found.push((path.to_string_lossy().into_owned(), root.provider, modified(&meta), false));
                        }
                    }
                }
            }
            _ => walk(&root.path, &mut |path, meta| {
                let text = path.to_string_lossy().into_owned();
                if text.ends_with(".jsonl") && modified(meta) >= horizon {
                    let subagent = root.provider == Provider::Claude && parent_of(&text).is_some();
                    found.push((text, root.provider, modified(meta), subagent));
                }
            }),
        }
    }
    found.sort_by(|a, b| a.3.cmp(&b.3).then(a.2.total_cmp(&b.2)));
    found.into_iter().map(|(p, provider, _, _)| (p, provider)).collect()
}

/// Every regular file under `dir`; links and junctions are not followed.
fn walk(dir: &Path, visit: &mut dyn FnMut(&Path, &std::fs::Metadata)) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(&entry.path(), visit);
        } else if let Ok(meta) = entry.metadata() {
            visit(&entry.path(), &meta);
        }
    }
}

/// A log opened so its writer can still append, rename or delete it.
pub fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
        options.share_mode(0x1 | 0x2 | 0x4);
    }
    options.open(path)
}

/// The file's identity: the NTFS file index, or the inode elsewhere.
#[cfg(windows)]
pub fn identity(file: &File) -> u64 {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    match unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) } {
        Ok(()) => ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        Err(_) => 0,
    }
}
#[cfg(not(windows))]
pub fn identity(file: &File) -> u64 {
    use std::os::unix::fs::MetadataExt;
    file.metadata().map(|m| m.ino()).unwrap_or(0)
}

/// FNV-1a 64 over the offset and the 4 KiB at each end before it: a log
/// rewritten with a different start, or different lines just before where
/// reading stopped, no longer matches.
pub fn fingerprint(file: &mut File, offset: u64) -> Option<u64> {
    const SPAN: u64 = 4096;
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for b in bytes {
            hash = (hash ^ *b as u64).wrapping_mul(0x100_0000_01B3);
        }
    };
    mix(&offset.to_le_bytes());
    let mut buffer = vec![0u8; SPAN as usize];
    for start in [0, offset - SPAN.min(offset)] {
        let length = SPAN.min(offset - start) as usize;
        file.seek(SeekFrom::Start(start)).ok()?;
        file.read_exact(&mut buffer[..length]).ok()?;
        mix(&buffer[..length]);
    }
    Some(hash)
}

/// OpenCode's progress through its database.
#[derive(Clone, Debug, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct OpenCodeProgress {
    /// Replies still being written: (time_updated, length, first seen unchanged).
    pub open: std::collections::HashMap<String, (i64, i64, Time)>,
    /// The last rows read, to notice a revert.
    pub tail: Vec<(i64, String)>,
}

/// How far into one log reading has got.
#[derive(Debug, Default)]
pub struct Cursor {
    pub path: String,
    pub provider: Provider,
    /// The session log a Claude subagent works for.
    pub parent: Option<String>,
    pub offset: u64,
    pub identity: u64,
    pub pending: Vec<u8>,
    pub discarding: bool,
    pub state: LogState,
    pub modified: Time,
    /// The log turned out replaced or cut short after part of it was read.
    pub restarted: bool,
    fingerprinted: Option<(u64, u64, u64)>,
    /// For OpenCode: rows, not bytes.
    pub opencode: OpenCodeProgress,
    codex_parent: Option<String>,
    codex_parent_search: usize,
}

/// Where reading stopped, at a line boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCursor {
    pub path: String,
    pub provider: Provider,
    pub offset: u64,
    pub identity: u64,
    pub discarding: bool,
    pub modified: Time,
    pub state: LogState,
    pub fingerprint: u64,
}

impl Cursor {
    pub fn new(path: &str, provider: Provider) -> Cursor {
        let parent = if provider == Provider::Claude { parent_of(path) } else { None };
        Cursor { path: path.into(), provider, parent, codex_parent_search: usize::MAX, ..Default::default() }
    }

    /// Subagents report to a turn another file tracks.
    pub fn tracks_turns(&self) -> bool {
        match self.provider {
            Provider::Claude => self.parent.is_none(),
            Provider::Codex => !self.state.codex_subagent,
            _ => true,
        }
    }

    /// The log of the thread a Codex subagent works for: the one written last.
    pub fn codex_parent<'a>(&mut self, logs: impl Iterator<Item = (&'a str, &'a LogState, Time)>, count: usize) -> Option<String> {
        if self.provider != Provider::Codex || self.state.codex_root.is_empty() || self.tracks_turns() {
            return None;
        }
        if self.codex_parent_search != count {
            self.codex_parent_search = count;
            let root = self.state.codex_root.clone();
            self.codex_parent = logs.filter(|(_, s, _)| !s.codex_subagent && s.session == root).max_by(|a, b| a.2.total_cmp(&b.2)).map(|(p, _, _)| p.to_string());
        }
        self.codex_parent.clone()
    }

    fn boundary(&self) -> u64 {
        if self.discarding { self.offset } else { self.offset - self.pending.len() as u64 }
    }

    pub fn start_over(&mut self, identity: u64) {
        if self.offset > 0 {
            self.restarted = true;
        }
        self.identity = identity;
        self.offset = 0;
        self.pending.clear();
        self.discarding = false;
        self.state = LogState::default();
        self.fingerprinted = None;
        self.codex_parent_search = usize::MAX;
    }

    pub fn saved(&mut self) -> Option<SavedCursor> {
        let end = self.boundary();
        let fingerprint = match self.fingerprinted {
            Some((offset, identity, value)) if offset == end && identity == self.identity => value,
            _ => {
                let value = fingerprint(&mut open_shared(Path::new(&self.path)).ok()?, end)?;
                self.fingerprinted = Some((end, self.identity, value));
                value
            }
        };
        Some(SavedCursor { path: self.path.clone(), provider: self.provider, offset: end, identity: self.identity, discarding: self.discarding, modified: self.modified, state: self.state.clone(), fingerprint })
    }

    /// None when the path no longer holds the log read up to the saved offset.
    pub fn restore(saved: SavedCursor) -> Option<Cursor> {
        let mut file = open_shared(Path::new(&saved.path)).ok()?;
        if identity(&file) != saved.identity || fingerprint(&mut file, saved.offset)? != saved.fingerprint {
            return None;
        }
        let mut cursor = Cursor::new(&saved.path, saved.provider);
        cursor.offset = saved.offset;
        cursor.identity = saved.identity;
        cursor.discarding = saved.discarding;
        cursor.modified = saved.modified;
        cursor.state = saved.state;
        cursor.fingerprinted = Some((saved.offset, saved.identity, saved.fingerprint));
        Some(cursor)
    }

    /// Reads what was appended since the last call and hands over each
    /// complete line; a line still being written waits for its end. A
    /// replaced, truncated or rewritten file starts over.
    pub fn read_appended(&mut self, keep: &dyn Fn(&[u8]) -> bool, line: &mut dyn FnMut(&mut LogState, &[u8])) {
        let Ok(mut file) = open_shared(Path::new(&self.path)) else { return };
        let Ok(meta) = file.metadata() else { return };
        if !meta.is_file() {
            return;
        }
        let size = meta.len();
        let id = identity(&file);
        self.modified = modified(&meta);
        let rewritten = size > self.offset && self.fingerprinted.is_some_and(|(offset, identity, value)| identity == id && fingerprint(&mut file, offset) != Some(value));
        if id != self.identity || size < self.offset || rewritten {
            self.start_over(id);
        }
        if size > self.offset && file.seek(SeekFrom::Start(self.offset)).is_ok() {
            let mut chunk = vec![0u8; CHUNK.min((size - self.offset) as usize)];
            while self.offset < size {
                let wanted = CHUNK.min((size - self.offset) as usize);
                let Ok(read) = file.read(&mut chunk[..wanted]) else { break };
                if read == 0 {
                    break;
                }
                self.offset += read as u64;
                self.split(&chunk[..read], keep, line);
            }
        }
        let end = self.boundary();
        if self.fingerprinted.is_none_or(|(o, i, _)| o != end || i != self.identity) {
            self.fingerprinted = fingerprint(&mut file, end).map(|v| (end, self.identity, v));
        }
    }

    fn split(&mut self, chunk: &[u8], keep: &dyn Fn(&[u8]) -> bool, line: &mut dyn FnMut(&mut LogState, &[u8])) {
        let carried = self.pending.len();
        let mut buffer = std::mem::take(&mut self.pending);
        buffer.extend_from_slice(chunk);
        let mut start = 0;
        // What was carried over holds no line break.
        for end in memchr::memchr_iter(b'\n', &buffer[carried..]).map(|i| i + carried) {
            let range = start..end;
            start = end + 1;
            if std::mem::take(&mut self.discarding) {
                continue;
            }
            let text = &buffer[range];
            let text = text.strip_suffix(b"\r").unwrap_or(text);
            if !text.is_empty() && text.len() <= MAXIMUM_LINE && keep(text) {
                line(&mut self.state, text);
            }
        }
        // Once a line is oversized only its terminator is looked for.
        if self.discarding || buffer.len() - start > MAXIMUM_LINE {
            self.discarding = true;
        } else {
            self.pending = buffer[start..].to_vec();
        }
    }
}

// MARK: Claude's session registry (§2.1.4)

/// The value Node writes for `process.platform`: a record from a container
/// or VM sharing the folder names a process this PC cannot see.
#[cfg(windows)]
const PID_DOMAIN: &str = "win32";
#[cfg(not(windows))]
const PID_DOMAIN: &str = "darwin";

#[cfg(windows)]
fn is_running(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    if pid == 0 {
        return false;
    }
    unsafe {
        let process = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(process) => process,
            // Access denied still means a process with that id exists.
            Err(error) => return error.code() == windows::Win32::Foundation::E_ACCESSDENIED,
        };
        let mut code = 0u32;
        let alive = GetExitCodeProcess(process, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
        let _ = CloseHandle(process);
        alive
    }
}
#[cfg(not(windows))]
fn is_running(pid: u32) -> bool {
    pid > 0 && std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Reads `<pid>.json` in each `sessions` folder beside a Claude log root.
pub fn registry(roots: &[Root]) -> Registry {
    registry_with(roots, &is_running)
}

pub fn registry_with(roots: &[Root], running: &dyn Fn(u32) -> bool) -> Registry {
    let mut out = Registry { complete: true, ..Default::default() };
    for root in roots.iter().filter(|r| r.provider == Provider::Claude) {
        let Some(folder) = root.path.parent().map(|p| p.join("sessions")) else { continue };
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        out.listed = true;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".json").filter(|s| s.parse::<u32>().is_ok()) else { continue };
            let _ = stem;
            let record = std::fs::read(entry.path()).ok().filter(|b| b.len() < 1 << 16).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            let Some((pid, session)) = record.as_ref().and_then(|r| Some((r.get("pid")?.as_u64()?, r.get("sessionId")?.as_str().filter(|s| !s.is_empty())?.to_string()))) else {
                out.complete = false;
                continue;
            };
            if record.as_ref().and_then(|r| r.get("pidDomain")).and_then(|d| d.as_str()).is_some_and(|d| d != PID_DOMAIN) {
                continue;
            }
            if running(pid as u32) {
                out.running.insert(session);
            } else {
                out.ended.insert(session);
            }
        }
    }
    // A session resumed by a new process after an old one was killed.
    let running = out.running.clone();
    out.ended.retain(|s| !running.contains(s));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vorssaint-agents-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn collect(cursor: &mut Cursor) -> Vec<String> {
        let mut lines = Vec::new();
        cursor.read_appended(&|_| true, &mut |_, l| lines.push(String::from_utf8_lossy(l).into_owned()));
        lines
    }

    #[test]
    fn reads_only_appended_lines_and_waits_for_a_partial_last_line() {
        let dir = temp("append");
        let path = dir.join("log.jsonl");
        std::fs::write(&path, "one\r\ntwo\nthr").unwrap();
        let mut cursor = Cursor::new(&path.to_string_lossy(), Provider::Codex);
        assert_eq!(collect(&mut cursor), ["one", "two"]);
        assert!(collect(&mut cursor).is_empty(), "nothing new");
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"ee\nfour\n").unwrap();
        assert_eq!(collect(&mut cursor), ["three", "four"]);
        // Saved progress resumes at the line boundary.
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"fi").unwrap();
        assert!(collect(&mut cursor).is_empty());
        let saved = cursor.saved().unwrap();
        assert_eq!(saved.offset, std::fs::metadata(&path).unwrap().len() - 2);
        let mut resumed = Cursor::restore(saved.clone()).expect("unchanged log resumes");
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"ve\n").unwrap();
        assert_eq!(collect(&mut resumed), ["five"]);
        // A log rewritten in place is read again from its start.
        std::fs::write(&path, "ONE\r\nTWO\nTHREE\nFOUR\nFIVE\nsix\n").unwrap();
        assert!(Cursor::restore(saved).is_none());
        assert_eq!(collect(&mut resumed).len(), 6);
        assert!(resumed.restarted);
        // A truncated log starts over too.
        std::fs::write(&path, "x\n").unwrap();
        assert_eq!(collect(&mut resumed), ["x"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_oversized_line_is_skipped_without_being_kept() {
        let mut cursor = Cursor::new("none", Provider::Claude);
        let mut lines = Vec::new();
        let big = vec![b'a'; MAXIMUM_LINE + 10];
        cursor.split(&big, &|_| true, &mut |_, l| lines.push(l.len()));
        assert!(cursor.discarding && cursor.pending.is_empty());
        cursor.split(b"tail\nnext\n", &|_| true, &mut |_, l| lines.push(l.len()));
        assert_eq!(lines, [4]);
    }

    #[test]
    fn discovery_orders_subagents_last_and_maps_their_parent() {
        let dir = temp("discover");
        let projects = dir.join("projects").join("C--repo");
        std::fs::create_dir_all(projects.join("abc").join("subagents")).unwrap();
        std::fs::write(projects.join("abc").join("subagents").join("agent-1.jsonl"), "{}\n").unwrap();
        std::fs::write(projects.join("abc.jsonl"), "{}\n").unwrap();
        std::fs::write(projects.join("notes.txt"), "x").unwrap();
        let roots = [Root { provider: Provider::Claude, path: dir.join("projects") }];
        let found = discover(&roots, 0.0);
        assert_eq!(found.len(), 2);
        assert!(found[0].0.ends_with("abc.jsonl"));
        assert_eq!(parent_of(&found[1].0).as_deref(), Some(found[0].0.as_str()));
        assert_eq!(parent_of(r"C:\u\.claude\projects\p\abc\subagents\a.jsonl").as_deref(), Some(r"C:\u\.claude\projects\p\abc.jsonl"));
        // Registry beside the root.
        let sessions = dir.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("10.json"), format!(r#"{{"pid":10,"sessionId":"abc","pidDomain":"{PID_DOMAIN}"}}"#)).unwrap();
        std::fs::write(sessions.join("11.json"), r#"{"pid":11,"sessionId":"vm","pidDomain":"linux"}"#).unwrap();
        std::fs::write(sessions.join("12.json"), r#"{"pid":12,"sessionId":"dead"}"#).unwrap();
        let registry = registry_with(&roots, &|pid| pid == 10);
        assert!(registry.listed && registry.complete);
        assert!(registry.running.contains("abc") && registry.ended.contains("dead") && !registry.ended.contains("vm"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
