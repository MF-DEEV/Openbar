// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A read-only row for a Codex TUI whose thread the shared daemon did not
//! list. A terminal can keep its conversation open after the daemon unloads
//! it, or use a separate server. A rollout belongs to the TUI in its folder
//! that started last before it, and each TUI shows its newest one; no stale
//! file alone makes a row.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use chrono::{Days, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::{Value, json};

use super::state::State;
use crate::hooks::installer;
use crate::hosts::process;
use crate::sessions::models::{ChatMessage, ChatRole, ContentPolicy, Session, SessionPhase, Time};

/// Rollout names keep whole seconds; process start times do not.
const SLACK: f64 = 3.0;
/// How far back a long-running TUI's day folders are read.
const MAX_DAYS: u64 = 31;
const MAX_META: u64 = 64 * 1024;
const MAX_CHAT: u64 = 2 * 1024 * 1024;

pub struct LocalSession {
    pub pid: u32,
    pub id: String,
    pub cwd: String,
    pub started: Time,
    pub path: PathBuf,
}

/// A live interactive `codex.exe`.
struct Tui {
    pid: u32,
    started: Time,
    home: PathBuf,
    cwd: String,
}

/// A rollout's first line, which never changes once written.
#[derive(Clone, Debug, PartialEq)]
struct Meta {
    id: String,
    cwd: String,
    /// The TUI's own thread, not `codex exec`, a subagent or an editor's.
    tui: bool,
}

static METAS: LazyLock<Mutex<HashMap<PathBuf, Meta>>> = LazyLock::new(Mutex::default);

fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\'])
        .replace('/', "\\")
        .eq_ignore_ascii_case(&b.trim_end_matches(['/', '\\']).replace('/', "\\"))
}

/// The first line only, as bytes: a cap that splits a multi-byte character
/// further down must not lose the whole row.
fn read_meta(path: &Path) -> Option<Meta> {
    let mut line = Vec::new();
    BufReader::new(File::open(path).ok()?.take(MAX_META))
        .read_until(b'\n', &mut line)
        .ok()?;
    let value: Value = serde_json::from_slice(&line).ok()?;
    if value.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = value.get("payload")?;
    Some(Meta {
        id: payload.get("id")?.as_str()?.to_owned(),
        cwd: payload.get("cwd")?.as_str()?.to_owned(),
        tui: payload
            .get("source")
            .is_none_or(|source| source.as_str() == Some("cli")),
    })
}

fn cached_meta(path: &Path) -> Option<Meta> {
    if let Some(meta) = METAS.lock().unwrap().get(path) {
        return Some(meta.clone());
    }
    // Not cached until a whole first line parses.
    let meta = read_meta(path)?;
    METAS
        .lock()
        .unwrap()
        .insert(path.to_path_buf(), meta.clone());
    Some(meta)
}

/// When the thread began, from `rollout-YYYY-MM-DDTHH-MM-SS-<id>.jsonl` (local
/// time). Codex names the file when the thread starts but writes it only with
/// the first message, so the file's own creation time can be minutes late,
/// and the thread itself can start long after the process (an update prompt
/// waits first).
fn named_start(name: &str) -> Option<Time> {
    let stamp = name.strip_prefix("rollout-")?.get(..19)?;
    let naive = NaiveDateTime::parse_from_str(stamp, "%Y-%m-%dT%H-%M-%S").ok()?;
    Some(Local.from_local_datetime(&naive).earliest()?.timestamp() as Time)
}

/// Day folders from the earliest TUI's start to today.
fn days_since(started: Time) -> Vec<NaiveDate> {
    let today = Local::now().date_naive();
    let first = Local
        .timestamp_opt(started as i64 - 60, 0)
        .single()
        .map_or(today, |d| d.date_naive())
        .max(today - Days::new(MAX_DAYS));
    first.iter_days().take_while(|d| *d <= today).collect()
}

/// Each TUI's newest thread. A rollout goes to the TUI in the same home and
/// folder that started last before the thread did, so two terminals in one
/// folder keep their own and a `/new` moves the row to the new thread.
fn assign(tuis: &[Tui]) -> Vec<LocalSession> {
    let mut best: Vec<Option<(Time, Meta, PathBuf)>> = tuis.iter().map(|_| None).collect();
    let mut seen = HashSet::new();
    let mut homes: Vec<&Path> = Vec::new();
    for tui in tuis {
        if !homes.contains(&tui.home.as_path()) {
            homes.push(&tui.home);
        }
    }
    for home in homes {
        let earliest = tuis
            .iter()
            .filter(|t| t.home == home)
            .map(|t| t.started)
            .fold(f64::INFINITY, f64::min);
        for day in days_since(earliest) {
            let folder = home
                .join("sessions")
                .join(day.format("%Y/%m/%d").to_string());
            let Ok(entries) = fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "jsonl") {
                    continue;
                }
                let Some(begun) = path
                    .file_name()
                    .and_then(|n| named_start(&n.to_string_lossy()))
                else {
                    continue;
                };
                if begun < earliest - SLACK {
                    continue;
                }
                seen.insert(path.clone());
                let Some(meta) = cached_meta(&path).filter(|m| m.tui) else {
                    continue;
                };
                let owner = tuis
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        t.home == home
                            && t.started - SLACK <= begun
                            && same_folder(&t.cwd, &meta.cwd)
                    })
                    .max_by(|a, b| a.1.started.total_cmp(&b.1.started))
                    .map(|(i, _)| i);
                if let Some(i) = owner
                    && best[i].as_ref().is_none_or(|(old, ..)| begun > *old)
                {
                    best[i] = Some((begun, meta, path));
                }
            }
        }
    }
    METAS.lock().unwrap().retain(|path, _| seen.contains(path));
    tuis.iter()
        .zip(best)
        .filter_map(|(tui, found)| {
            let (_, meta, path) = found?;
            Some(LocalSession {
                pid: tui.pid,
                id: meta.id,
                cwd: meta.cwd,
                started: tui.started,
                path,
            })
        })
        .collect()
}

/// Only a live interactive `codex.exe` with a matching rollout gets a row.
pub fn discover() -> Vec<LocalSession> {
    let mut tuis = Vec::new();
    for pid in process::pids_named("codex.exe") {
        let Some(info) = process::metadata(pid) else {
            continue;
        };
        let command = info.command_line.to_ascii_lowercase();
        if command.contains("app-server")
            || command.contains(" exec")
            || command.contains(" mcp-server")
        {
            continue;
        }
        let Some(started) = process::started_at(pid) else {
            continue;
        };
        let home = process::environment_values(pid, &["CODEX_HOME"])
            .remove("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| installer::home_dir().map(|h| h.join(".codex")));
        let Some(home) = home else { continue };
        tuis.push(Tui {
            pid,
            started,
            home,
            cwd: info.cwd,
        });
    }
    if tuis.is_empty() {
        METAS.lock().unwrap().clear();
        return Vec::new();
    }
    assign(&tuis)
}

struct Transcript {
    chat: Vec<ChatMessage>,
    working_since: Option<Time>,
    last_active: Option<Time>,
}

fn transcript(path: &Path, previews: bool) -> Transcript {
    let empty = || Transcript {
        chat: Vec::new(),
        working_since: None,
        last_active: None,
    };
    let Ok(mut file) = File::open(path) else {
        return empty();
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else {
        return empty();
    };
    let skip = size.saturating_sub(MAX_CHAT);
    if file.seek(SeekFrom::Start(skip)).is_err() {
        return empty();
    }
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        return empty();
    }
    let text = String::from_utf8_lossy(&data);
    let text = if skip > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    let mut messages = Vec::new();
    let mut next_id = 0;
    let mut working_since = None;
    let mut last_active = None;
    for line in text.lines() {
        let Ok(item) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let at = item
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map_or(0.0, |d| d.timestamp_millis() as f64 / 1000.0);
        if at > 0.0 {
            last_active = Some(at);
        }
        if item.get("type").and_then(Value::as_str) == Some("event_msg") {
            match item.pointer("/payload/type").and_then(Value::as_str) {
                Some("task_started") => working_since = Some(at),
                Some("task_complete" | "turn_aborted") => working_since = None,
                _ => {}
            }
        }
        if !previews || item.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let Some(payload) = item.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let role = match payload.get("role").and_then(Value::as_str) {
            Some("user") => ChatRole::User,
            Some("assistant") => ChatRole::Assistant,
            _ => continue,
        };
        let body = payload
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| {
                matches!(
                    c.get("type").and_then(Value::as_str),
                    Some("input_text" | "output_text")
                )
            })
            .filter_map(|c| c.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        if body.trim().is_empty() {
            continue;
        }
        next_id += 1;
        messages.push(ChatMessage {
            id: next_id,
            role,
            text: body.chars().take(16_000).collect(),
            images: Vec::new(),
            at,
            ends_turn: payload.get("phase").and_then(Value::as_str) == Some("final_answer"),
        });
        if messages.len() > 100 {
            messages.remove(0);
        }
    }
    Transcript {
        chat: messages,
        working_since,
        last_active,
    }
}

/// The daemon's session takes precedence. This row cannot answer approvals
/// or send replies through a connection it does not own.
pub fn as_session(local: &LocalSession, policy: ContentPolicy) -> Session {
    let mut state = State::default();
    state.upsert_thread(&json!({"id":local.id,"cwd":local.cwd,"status":{"type":"idle"}}));
    let mut session = state.snapshot(local.started, policy).0.remove(0);
    session.pid = local.pid as i32;
    // The service enables terminal delivery only after locating a supported
    // host. A local turn in progress may be waiting at an approval prompt,
    // which rollout files cannot distinguish from generation.
    session.accepts_replies = false;
    session.hooked = false;
    let transcript = transcript(&local.path, policy.previews);
    session.last_active = transcript.last_active.or(Some(local.started));
    if let Some(since) = transcript.working_since {
        session.phase = SessionPhase::Generating;
        session.phase_since = since;
        session.turn_started = Some(since);
    }
    if policy.previews {
        session.chat = transcript.chat;
        session.title = session
            .chat
            .iter()
            .find(|m| m.role == ChatRole::User)
            .map(|m| {
                m.text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect()
            });
        session.last_text = session
            .chat
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::Assistant)
            .map(|m| m.text.clone());
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-codex-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn meta_line(id: &str, cwd: &str, source: &str) -> String {
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":id,"cwd":cwd,"source":source}})
        )
    }

    /// Writes a rollout whose name says the thread began at `begun`.
    fn rollout(home: &Path, begun: Time, id: &str, cwd: &str, source: &str) -> PathBuf {
        let at = Local.timestamp_opt(begun as i64, 0).single().unwrap();
        let folder = home
            .join("sessions")
            .join(at.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!(
            "rollout-{}-{id}.jsonl",
            at.format("%Y-%m-%dT%H-%M-%S")
        ));
        fs::write(&path, meta_line(id, cwd, source)).unwrap();
        path
    }

    fn tui(pid: u32, started: Time, home: &Path, cwd: &str) -> Tui {
        Tui {
            pid,
            started,
            home: home.to_path_buf(),
            cwd: cwd.to_owned(),
        }
    }

    fn ids(found: &[LocalSession]) -> Vec<(u32, &str)> {
        found.iter().map(|s| (s.pid, s.id.as_str())).collect()
    }

    #[test]
    fn local_cli_turn_changes_phase_and_keeps_chat() {
        let dir = temp("turn");
        let path = dir.join("rollout.jsonl");
        let started = json!({"timestamp":"2026-11-09T19:30:00Z","type":"event_msg","payload":{"type":"task_started"}});
        let message = json!({"timestamp":"2026-11-09T19:30:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Working on it"}]}});
        fs::write(&path, format!("{started}\n{message}\n")).unwrap();
        let local = LocalSession {
            pid: 42,
            id: "t1".into(),
            cwd: "C:\\code".into(),
            started: 1.0,
            path: path.clone(),
        };
        let active = as_session(&local, ContentPolicy { previews: true });
        assert_eq!(active.phase, SessionPhase::Generating);
        assert_eq!(active.chat[0].text, "Working on it");
        assert!(!active.accepts_replies);
        fs::write(&path, format!("{started}\n{message}\n{}\n", json!({"timestamp":"2026-11-09T19:31:00Z","type":"event_msg","payload":{"type":"task_complete"}}))).unwrap();
        let done = as_session(&local, ContentPolicy { previews: false });
        assert_eq!(done.phase, SessionPhase::Idle);
        assert!(done.chat.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn metadata_requires_a_codex_session_and_cwd() {
        let dir = temp("meta");
        let path = dir.join("a.jsonl");
        fs::write(&path, meta_line("t1", "C:\\code", "cli")).unwrap();
        let meta = read_meta(&path).unwrap();
        assert_eq!(
            (meta.id.as_str(), meta.cwd.as_str(), meta.tui),
            ("t1", "C:\\code", true)
        );
        fs::write(&path, meta_line("t1", "C:\\code", "exec")).unwrap();
        assert!(!read_meta(&path).unwrap().tui);
        fs::write(&path, b"{\"type\":\"response_item\"}\n").unwrap();
        assert!(read_meta(&path).is_none());
        assert!(same_folder("C:\\code\\", "c:/code"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn metadata_survives_a_cap_inside_a_multibyte_character() {
        let dir = temp("cut");
        let path = dir.join("a.jsonl");
        let mut data = meta_line("t2", "C:\\kod", "cli").into_bytes();
        data.resize(MAX_META as usize - 1, b'a');
        // "ł" takes two bytes, so the cap falls between them.
        data.extend("ł\n".as_bytes());
        fs::write(&path, &data).unwrap();
        assert_eq!(read_meta(&path).map(|m| m.id), Some("t2".to_owned()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn start_comes_from_the_rollout_name() {
        let name = "rollout-2026-10-25T10-07-07-01a17250-4d0c-76d1-89cc-a1800a3b8476.jsonl";
        let naive =
            NaiveDateTime::parse_from_str("2026-10-25 10:07:07", "%Y-%m-%d %H:%M:%S").unwrap();
        let expected = Local
            .from_local_datetime(&naive)
            .earliest()
            .unwrap()
            .timestamp() as Time;
        assert_eq!(named_start(name), Some(expected));
        assert_eq!(named_start("rollout-garbage.jsonl"), None);
        assert_eq!(named_start("notes-2026-10-25T10-07-07.jsonl"), None);
    }

    #[test]
    fn a_thread_begun_long_after_the_process_still_matches() {
        let home = temp("late");
        let started = Local::now().timestamp() as Time - 600.0;
        // An update prompt, then a first message minutes later.
        rollout(&home, started + 300.0, "late", "C:\\x", "cli");
        let found = assign(&[tui(1, started, &home, "c:/x/")]);
        assert_eq!(ids(&found), [(1, "late")]);
        assert!(assign(&[tui(1, started, &home, "C:\\y")]).is_empty());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn older_rollouts_and_other_sources_are_not_the_tui() {
        let home = temp("old");
        let started = Local::now().timestamp() as Time - 600.0;
        rollout(&home, started - 120.0, "before", "C:\\x", "cli");
        rollout(&home, started + 60.0, "exec", "C:\\x", "exec");
        rollout(&home, started + 90.0, "sub", "C:\\x", "{\"subagent\":{}}");
        assert!(assign(&[tui(1, started, &home, "C:\\x")]).is_empty());
        rollout(&home, started + 30.0, "mine", "C:\\x", "cli");
        assert_eq!(
            ids(&assign(&[tui(1, started, &home, "C:\\x")])),
            [(1, "mine")]
        );
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn two_terminals_in_one_folder_keep_their_own_newest_thread() {
        let home = temp("two");
        let a = Local::now().timestamp() as Time - 900.0;
        let b = a + 300.0;
        rollout(&home, a + 10.0, "a1", "C:\\x", "cli");
        rollout(&home, a + 100.0, "a2", "C:\\x", "cli");
        rollout(&home, b + 10.0, "b1", "C:\\x", "cli");
        let found = assign(&[tui(1, a, &home, "C:\\x"), tui(2, b, &home, "C:\\x")]);
        assert_eq!(ids(&found), [(1, "a2"), (2, "b1")]);
        let _ = fs::remove_dir_all(home);
    }
}
