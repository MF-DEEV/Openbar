// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Amazon's Kiro CLI (`kiro-cli`, #238), as the Mac's AgentKiro.swift. Kiro
//! CLI V3 loads every hooks file in `~/.kiro/hooks` (`KIRO_HOME` moves the
//! folder) for any agent the person starts, so the island writes one file
//! of its own there. Its hooks run in Kiro's agent server, a `node` its
//! terminal UI starts, and get Claude Code's fields on stdin; tools carry
//! Kiro's own names.
//!
//! Its permission hook runs after Kiro's policy and before its dialog, and
//! the dialog only opens once the hook answered nothing, with no time limit
//! of its own: the island holds a tool's request and answers it, and lets it
//! go when the person should answer in Kiro (Open, the island going away).
//! Its dialog then takes Return for "Allow once", the first choice, and Down
//! and Return for "Deny once"; Escape there cancels the whole turn.
//!
//! What a prompt's or a session's hook prints goes into the model's context,
//! so those print nothing. A turn cancelled with Escape brings no `Stop`:
//! the session's own log tells it.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::hook_message::{DENY_MESSAGE, Decision};
use crate::hosts::Key;

/// Every event the island hears, named as Kiro's hook files name them.
pub const EVENTS: [&str; 7] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
];

/// Events Kiro matches against a tool; `*` takes every tool.
pub const TOOL_EVENTS: [&str; 3] = ["PreToolUse", "PermissionRequest", "PostToolUse"];

/// What a tool's result says when the person cancelled its dialog with
/// Escape, which ends the turn without a `Stop`.
const CANCELLED: &str = "The tool call was cancelled before it was approved";

// MARK: Places

/// Kiro CLI's folder: `KIRO_HOME`, or `~/.kiro` (`%USERPROFILE%\.kiro`). The
/// Kiro IDE keeps its files there too, so only its command says it is here.
pub fn folder(home: &Path) -> PathBuf {
    std::env::var_os("KIRO_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".kiro"))
}

// MARK: Hooks

/// What the permission hook prints: allow or deny, Kiro's decision.
pub fn reply(decision: &Decision) -> Option<Vec<u8>> {
    let body = match decision {
        Decision::Allow | Decision::AllowAlways => json!({"behavior": "allow"}),
        Decision::Deny => json!({"behavior": "deny", "message": DENY_MESSAGE}),
        Decision::Answer(_) | Decision::ApprovePlan(_) => return None,
    };
    serde_json::to_vec(
        &json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": body}}),
    )
    .ok()
}

/// The keys that answer Kiro's own dialog once the island let its hook go:
/// Return takes "Allow once", picked first; Down and Return "Deny once".
pub fn keys(decision: &Decision) -> Option<Vec<Key>> {
    match decision {
        Decision::Allow => Some(vec![Key::Enter]),
        Decision::Deny => Some(vec![Key::Down, Key::Enter]),
        _ => None,
    }
}

// MARK: Tools

/// A tool's input in Claude Code's words, so the card, the list and a file's
/// diff read it as they read Claude Code's: Kiro names a file `path`, a
/// written file's text `text`, an edit's `oldStr` and `newStr`.
pub fn claude_input(raw: &str, input: &Map<String, Value>) -> Map<String, Value> {
    let mut out = input.clone();
    if let Some(path) = input.get("path").filter(|v| v.is_string())
        && !input.contains_key("file_path")
    {
        out.insert("file_path".into(), path.clone());
    }
    match raw {
        "fs_write" | "write_file" => {
            if let Some(text) = input.get("text").filter(|v| v.is_string())
                && !input.contains_key("content")
            {
                out.insert("content".into(), text.clone());
            }
        }
        "str_replace" | "replace_text" => {
            if let Some(old) = input.get("oldStr").filter(|v| v.is_string()) {
                out.insert("old_string".into(), old.clone());
            }
            if let Some(new) = input.get("newStr").filter(|v| v.is_string()) {
                out.insert("new_string".into(), new.clone());
            }
        }
        _ => {}
    }
    out
}

/// Whether a tool's result says the person cancelled its dialog with Escape.
pub fn cancelled(response: Option<&Value>) -> bool {
    response
        .and_then(Value::as_str)
        .is_some_and(|text| text.contains(CANCELLED))
}

// MARK: Process

/// The process the session's terminal shows, from the hook's ancestors
/// (nearest first): Kiro's hooks run in its agent server, a `node` without a
/// terminal or a console of its own. Its terminal UI, `bun`, is the one the
/// session's terminal, tmux pane or console tab holds: right above `node` on
/// the Mac and Linux, above a `kiro-cli` with a hidden console of its own on
/// Windows. It lives as long as the session.
pub fn terminal_process(chain: &[(u32, String)], agent: u32) -> u32 {
    let Some(at) = chain.iter().position(|(pid, _)| *pid == agent) else {
        return agent;
    };
    let above = &chain[at + 1..chain.len().min(at + 4)];
    let named = |want: &str| {
        above
            .iter()
            .find(|(_, name)| {
                let name = name.to_ascii_lowercase();
                name.strip_suffix(".exe").unwrap_or(&name) == want
            })
            .map(|(pid, _)| *pid)
    };
    named("bun").or_else(|| named("kiro-cli")).unwrap_or(agent)
}

// MARK: Log

/// The session's folder, `sessions/<workspace>/<id>` in Kiro's folder.
pub fn session_folder(id: &str, kiro: &Path) -> Option<PathBuf> {
    if !id.starts_with("sess_") || id.contains(['/', '\\']) || id.contains("..") {
        return None;
    }
    let sessions = kiro.join("sessions");
    std::fs::read_dir(&sessions)
        .ok()?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path().join(id))
        .find(|candidate| candidate.join("messages.jsonl").is_file())
}

/// What the session's `session.json` says: its title and its model, as
/// Kiro names them ("auto" when the server picks one per turn).
pub fn details(session_folder: &Path) -> (Option<String>, Option<String>) {
    let Some(json) = std::fs::read(session_folder.join("session.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
    else {
        return (None, None);
    };
    let named = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    (named("title"), named("modelId"))
}

/// The end of the session's last turn in its `messages.jsonl`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LastTurn {
    pub ended: bool,
    pub cancelled: bool,
    pub reply: Option<String>,
    /// When it ended, in milliseconds since 1970.
    pub at: Option<i64>,
}

/// Reads the end of the session's log: whether its last turn ended, and
/// the reply it said last, kept only when `text` is true.
pub fn last_turn(log: &Path, text: bool) -> LastTurn {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(log) else {
        return LastTurn::default();
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(size.saturating_sub(262_144)));
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        return LastTurn::default();
    }
    last_turn_in(&String::from_utf8_lossy(&data), text)
}

pub fn last_turn_in(log: &str, text: bool) -> LastTurn {
    let mut turn = LastTurn::default();
    // From the end: the turn's end, then what it said before it, up to its start.
    for line in log.lines().rev() {
        let Ok(json) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(payload) = json.get("payload") else {
            continue;
        };
        match payload.get("type").and_then(Value::as_str) {
            Some("turn_end") => {
                if turn.ended {
                    return turn;
                }
                turn.ended = true;
                turn.cancelled =
                    payload.get("stopReason").and_then(Value::as_str) == Some("cancelled");
                turn.at = json
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.timestamp_millis());
            }
            Some("turn_start" | "user") => return turn,
            Some("assistant")
                if payload.get("operationType").and_then(Value::as_str) == Some("Say") =>
            {
                let said = payload
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if turn.reply.is_none() && !said.is_empty() && said != "..." && text {
                    turn.reply = Some(said.to_owned());
                }
            }
            _ => {}
        }
    }
    turn
}

#[cfg(test)]
mod tests;
