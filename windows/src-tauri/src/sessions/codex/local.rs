// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A read-only row for a Codex TUI whose thread the shared daemon did not
//! list. A terminal can keep its conversation open after the daemon unloads
//! it, or use a separate server. The rollout's creation time and working
//! folder tie it to the still-running TUI; no stale file alone makes a row.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{Local, TimeZone};
use serde_json::{Value, json};

use super::state::State;
use crate::hooks::installer;
use crate::hosts::process;
use crate::sessions::models::{ChatMessage, ChatRole, ContentPolicy, Session, Time};

const START_WINDOW: f64 = 45.0;
const MAX_META: u64 = 64 * 1024;
const MAX_CHAT: u64 = 2 * 1024 * 1024;

pub struct LocalSession {
    pub pid: u32,
    pub id: String,
    pub cwd: String,
    pub started: Time,
    pub path: PathBuf,
}

fn same_folder(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\'])
        .replace('/', "\\")
        .eq_ignore_ascii_case(&b.trim_end_matches(['/', '\\']).replace('/', "\\"))
}

fn read_meta(path: &Path) -> Option<(String, String)> {
    let mut line = String::new();
    File::open(path)
        .ok()?
        .take(MAX_META)
        .read_to_string(&mut line)
        .ok()?;
    let line = line.lines().next()?;
    let value: Value = serde_json::from_str(&line).ok()?;
    if value.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let payload = value.get("payload")?;
    Some((
        payload.get("id")?.as_str()?.to_owned(),
        payload.get("cwd")?.as_str()?.to_owned(),
    ))
}

fn rollout_for(home: &Path, started: Time, cwd: &str) -> Option<(String, String, PathBuf)> {
    let mut best: Option<(f64, String, String, PathBuf)> = None;
    for offset in [-60, 0, 60] {
        let Some(date) = Local.timestamp_opt(started as i64 + offset, 0).single() else {
            continue;
        };
        let folder = home
            .join("sessions")
            .join(date.format("%Y/%m/%d").to_string());
        let Ok(entries) = fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("rollout-"))
                || path.extension().is_none_or(|e| e != "jsonl")
            {
                continue;
            }
            let Some(created) = entry
                .metadata()
                .ok()
                .and_then(|m| m.created().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64())
            else {
                continue;
            };
            let distance = created - started;
            if !(-3.0..=START_WINDOW).contains(&distance)
                || best
                    .as_ref()
                    .is_some_and(|(old, ..)| distance.abs() >= *old)
            {
                continue;
            }
            let Some((id, recorded_cwd)) = read_meta(&path) else {
                continue;
            };
            if !same_folder(cwd, &recorded_cwd) {
                continue;
            }
            best = Some((distance.abs(), id, recorded_cwd, path));
        }
    }
    best.map(|(_, id, cwd, path)| (id, cwd, path))
}

/// Only a live interactive `codex.exe` with a matching rollout gets a row.
pub fn discover() -> Vec<LocalSession> {
    let mut out = Vec::new();
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
        let Some((id, cwd, path)) = rollout_for(&home, started, &info.cwd) else {
            continue;
        };
        out.push(LocalSession {
            pid,
            id,
            cwd,
            started,
            path,
        });
    }
    out
}

fn chat(path: &Path) -> Vec<ChatMessage> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(size) = file.metadata().map(|m| m.len()) else {
        return Vec::new();
    };
    let skip = size.saturating_sub(MAX_CHAT);
    if file.seek(SeekFrom::Start(skip)).is_err() {
        return Vec::new();
    }
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&data);
    let text = if skip > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    let mut messages = Vec::new();
    let mut next_id = 0;
    for line in text.lines() {
        let Ok(item) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if item.get("type").and_then(Value::as_str) != Some("response_item") {
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
        let at = item
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map_or(0.0, |d| d.timestamp_millis() as f64 / 1000.0);
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
    messages
}

/// The daemon's session takes precedence. This row cannot answer approvals
/// or send replies through a connection it does not own.
pub fn as_session(local: &LocalSession, policy: ContentPolicy) -> Session {
    let mut state = State::default();
    state.upsert_thread(&json!({"id":local.id,"cwd":local.cwd,"status":{"type":"idle"}}));
    let mut session = state.snapshot(local.started, policy).0.remove(0);
    session.pid = local.pid as i32;
    session.accepts_replies = false;
    session.hooked = false;
    session.last_active = Some(local.started);
    if policy.previews {
        session.chat = chat(&local.path);
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

    #[test]
    fn metadata_requires_a_codex_session_and_cwd() {
        let path =
            std::env::temp_dir().join(format!("openbar-codex-meta-{}.jsonl", std::process::id()));
        fs::write(
            &path,
            b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"t1\",\"cwd\":\"C:\\\\code\"}}\n",
        )
        .unwrap();
        assert_eq!(read_meta(&path), Some(("t1".into(), "C:\\code".into())));
        assert!(same_folder("C:\\code\\", "c:/code"));
        let _ = fs::remove_file(path);
    }
}
