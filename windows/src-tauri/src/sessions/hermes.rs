// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Hermes Agent (Nous Research, #244), as AgentHermes.swift. Its shell
//! hooks sit under `hooks:` in the config.yaml of its home, and its gateway,
//! which runs every session whether a terminal, its Ink UI or its app shows
//! it, runs them. So a hook names the gateway, not the session's terminal,
//! and the folder it reports is the gateway's: the session's own folder and
//! title come from Hermes's state.db, read only, and its terminal from the
//! note each terminal client leaves of the session it shows
//! (`terminal-sessions/`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::models::Time;
use crate::hooks::installer as claude;

/// The events the island listens to. Hermes asks the person once for each
/// event and command the first time it runs them, so they are as few as the
/// island can do with.
pub const EVENTS: [&str; 8] = [
    "on_session_open",
    "pre_llm_call",
    "pre_tool_call",
    "post_tool_call",
    "on_session_end",
    "on_human_input_request",
    "on_human_input_resolved",
    "on_session_finalize",
];

/// Where a session runs that the island lists: a terminal, Hermes's own
/// terminal UI or app. Chats its gateway answers on Telegram, Discord, Slack
/// and the rest are left out.
pub const LOCAL_PLATFORMS: [&str; 7] = ["cli", "tui", "desktop", "gui", "local", "oneshot", "acp"];

/// The clients a session can be shown in.
pub const CLIENTS: [&str; 4] = ["cli", "tui", "desktop", "acp"];

/// How long a hook may take: the island answers at once.
pub const TIMEOUT: u64 = 5;

/// Hermes's home: `HERMES_HOME`, or `~/.hermes`.
pub fn home(home: &Path) -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".hermes"))
}

pub fn config(home: &Path) -> PathBuf {
    self::home(home).join("config.yaml")
}

/// The hook for one event: this app, which Hermes starts with no shell
/// between, splitting the line as a shell would (with Windows' own rules
/// there, which keep backslashes).
pub fn command(event: &str, exe: &Path, app: &str) -> String {
    let arguments = format!(
        "--hook --agent hermes --event {event} {}",
        claude::marker(app)
    );
    let path = exe.to_string_lossy();
    if cfg!(windows) {
        format!("\"{path}\" {arguments}")
    } else {
        format!("'{}' {arguments}", path.replace('\'', "'\\''"))
    }
}

pub fn entries(exe: &Path, app: &str) -> Vec<(String, String)> {
    EVENTS
        .iter()
        .map(|e| ((*e).to_owned(), command(e, exe, app)))
        .collect()
}

// MARK: Approval

/// Which of the island's hooks Hermes was told to run: the person answers
/// its prompt once per event in a terminal, or starts it once with
/// `--accept-hooks`. Read from its allowlist, never written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    Approved,
    /// Hooks Hermes hasn't been told to run yet: it skips them.
    Waiting(usize),
}

pub fn approval(home: &Path, exe: &Path, app: &str) -> Approval {
    let file = self::home(home).join("shell-hooks-allowlist.json");
    let approved = approved_pairs(std::fs::read(file).ok().as_deref());
    let missing = entries(exe, app)
        .into_iter()
        .filter(|pair| !approved.contains(pair))
        .count();
    if missing == 0 {
        Approval::Approved
    } else {
        Approval::Waiting(missing)
    }
}

/// The pairs Hermes's allowlist approves (agent/shell_hooks.py: an
/// `approvals` list of `{event, command}`).
pub fn approved_pairs(data: Option<&[u8]>) -> HashSet<(String, String)> {
    let Some(data) = data.filter(|d| d.len() < 4 << 20) else {
        return HashSet::new();
    };
    let Ok(json) = serde_json::from_slice::<Value>(data) else {
        return HashSet::new();
    };
    json.get("approvals")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|entry| {
                    Some((
                        entry.get("event")?.as_str()?.to_owned(),
                        entry.get("command")?.as_str()?.trim().to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

// MARK: config.yaml

/// Hermes's config.yaml, changed as text so the person's comments, order
/// and style stay as they were (AgentHermesConfig). Hermes reads it with a
/// parser that refuses a key twice, so the island's entries join the
/// person's `hooks:` mapping when there is one: each is one line of its own
/// under its event, carrying the island's marker in its command, and a key
/// the island adds carries the marker as a comment.
pub mod yaml {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Form {
        /// Nothing on the key's line: its entries are on the lines under it.
        Block,
        /// `{}`, `[]`, `null` or `~`: nothing in it.
        Empty,
        /// Entries on the key's own line, in a flow mapping or list.
        Unsupported,
    }

    #[derive(Debug)]
    pub struct Block {
        pub line: usize,
        pub end: usize,
        pub content: usize,
        pub form: Form,
    }

    #[derive(Debug)]
    pub struct Event {
        pub name: String,
        pub key: usize,
        pub form: Form,
        pub items: Vec<std::ops::Range<usize>>,
    }

    /// The comment that marks a key the island added. The marker is the
    /// hook's arguments here, not a comment of its own.
    fn comment(marker: &str) -> String {
        if marker.starts_with('#') {
            marker.to_owned()
        } else {
            format!("# {marker}")
        }
    }

    /// One line of the island's under an event: a flow mapping with the
    /// command in a JSON string, which YAML reads.
    pub fn item(command: &str, timeout: u64) -> String {
        let quoted = serde_json::to_string(command).unwrap_or_default();
        format!("- {{command: {quoted}, timeout: {timeout}}}")
    }

    pub fn split(text: &str) -> Vec<String> {
        let text = text.replace("\r\n", "\n");
        let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        if lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    }

    fn indent(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    fn blank_or_comment(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    }

    /// What follows a key's colon, without a comment after it.
    fn form(value: &str) -> Form {
        let value = if value.trim_start().starts_with('#') {
            ""
        } else {
            value.split(" #").next().unwrap_or("")
        };
        match value.trim() {
            "" => Form::Block,
            "{}" | "[]" | "null" | "~" | "Null" | "NULL" => Form::Empty,
            _ => Form::Unsupported,
        }
    }

    pub fn hooks_block(lines: &[String]) -> Option<Block> {
        let line = lines.iter().position(|l| l.starts_with("hooks:"))?;
        let form = form(&lines[line]["hooks:".len()..]);
        let mut end = line + 1;
        while end < lines.len() {
            if !blank_or_comment(&lines[end]) && indent(&lines[end]) == 0 {
                break;
            }
            end += 1;
        }
        let mut content = end;
        while content > line + 1 {
            let text = &lines[content - 1];
            if !(text.trim().is_empty() || (blank_or_comment(text) && indent(text) == 0)) {
                break;
            }
            content -= 1;
        }
        Some(Block {
            line,
            end,
            content,
            form,
        })
    }

    pub fn events(lines: &[String], block: &Block) -> Vec<Event> {
        if block.form != Form::Block {
            return Vec::new();
        }
        let body = block.line + 1..block.end;
        let Some(first) = body.clone().find(|&i| !blank_or_comment(&lines[i])) else {
            return Vec::new();
        };
        let key_indent = indent(&lines[first]);
        let mut keys: Vec<(String, usize, Form)> = Vec::new();
        for index in body {
            let text = &lines[index];
            if blank_or_comment(text) || indent(text) != key_indent {
                continue;
            }
            let rest = &text[key_indent..];
            if rest.starts_with("- ") {
                continue;
            }
            let Some(colon) = rest.find(':') else {
                continue;
            };
            let name = rest[..colon].trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            keys.push((name.to_owned(), index, form(&rest[colon + 1..])));
        }
        let mut events = Vec::new();
        for (position, (name, key, form)) in keys.iter().enumerate() {
            let end = keys
                .get(position + 1)
                .map(|(_, line, _)| *line)
                .unwrap_or(block.end);
            let mut items = Vec::new();
            let mut item_indent: Option<usize> = None;
            let mut start: Option<usize> = None;
            let mut last = *key;
            for (index, text) in lines.iter().enumerate().take(end).skip(key + 1) {
                if blank_or_comment(text) {
                    continue;
                }
                let level = indent(text);
                let rest = &text[level..];
                let is_item = rest.starts_with("- ") || rest == "-";
                if is_item && item_indent.is_none_or(|i| i == level) {
                    if let Some(start) = start {
                        items.push(start..last + 1);
                    }
                    item_indent = Some(level);
                    start = Some(index);
                }
                last = index;
            }
            if let Some(start) = start {
                items.push(start..last + 1);
            }
            events.push(Event {
                name: name.clone(),
                key: *key,
                form: *form,
                items,
            });
        }
        events
    }

    fn joined(lines: &[String], range: &std::ops::Range<usize>) -> String {
        lines[range.clone()].join("\n")
    }

    /// The command of an entry the island wrote (a flow mapping on one
    /// line) or the person wrote in block style.
    pub fn command_in(item: &str) -> Option<String> {
        let start = item.find("command:")? + "command:".len();
        let value = item[start..].trim_start();
        if let Some(rest) = value.strip_prefix('"') {
            let mut escaped = false;
            for (i, c) in rest.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if c == '\\' {
                    escaped = true;
                    continue;
                }
                if c == '"' {
                    return serde_json::from_str::<String>(&format!("\"{}\"", &rest[..i])).ok();
                }
            }
            return None;
        }
        if let Some(rest) = value.strip_prefix('\'') {
            let mut out = String::new();
            let mut chars = rest.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\'' {
                    if chars.peek() == Some(&'\'') {
                        chars.next();
                        out.push('\'');
                        continue;
                    }
                    return Some(out);
                }
                out.push(c);
            }
            return None;
        }
        let plain = value.lines().next().unwrap_or(value);
        let plain = plain.split(" #").next().unwrap_or(plain);
        Some(
            plain
                .trim_matches(|c| c == ' ' || c == ',' || c == '}')
                .to_owned(),
        )
    }

    /// The commands under each event; None when there is no `hooks:` block.
    pub fn commands(text: &str) -> Option<std::collections::HashMap<String, Vec<String>>> {
        let lines = split(text);
        let block = hooks_block(&lines)?;
        Some(
            events(&lines, &block)
                .into_iter()
                .map(|event| {
                    let found = event
                        .items
                        .iter()
                        .filter_map(|range| command_in(&joined(&lines, range)))
                        .collect();
                    (event.name, found)
                })
                .collect(),
        )
    }

    /// The text with the island's lines taken out; None when it has none.
    pub fn removing(text: &str, marker: &str) -> Option<String> {
        let mut lines = split(text);
        let block = hooks_block(&lines)?;
        let mut drop: std::collections::BTreeSet<usize> = Default::default();
        for event in events(&lines, &block) {
            let own: Vec<_> = event
                .items
                .iter()
                .filter(|range| joined(&lines, range).contains(marker))
                .collect();
            for range in &own {
                drop.extend((*range).clone());
            }
            if event.items.len() == own.len()
                && (!own.is_empty() || lines[event.key].contains(marker))
            {
                drop.insert(event.key);
            }
        }
        let hooks_marked = lines[block.line].contains(marker);
        if drop.is_empty() && !hooks_marked {
            return None;
        }
        let children = (block.line + 1..block.end)
            .filter(|i| !drop.contains(i) && !blank_or_comment(&lines[*i]))
            .count();
        if children == 0 && (hooks_marked || !drop.is_empty()) {
            drop.insert(block.line);
            drop.extend(
                (block.line + 1..block.end).filter(|&index| lines[index].trim().is_empty()),
            );
        }
        for index in drop.into_iter().rev() {
            lines.remove(index);
        }
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        Some(if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        })
    }

    /// The text with the island's line under each event, after the
    /// person's own, and any older lines of the island's taken out first.
    /// Err when the hooks are written in a form the island can't add to.
    pub fn installing(
        text: Option<&str>,
        entries: &[(String, String)],
        timeout: u64,
        marker: &str,
    ) -> Result<String, ()> {
        let mut base = text
            .map(|t| removing(t, marker).unwrap_or_else(|| t.to_owned()))
            .unwrap_or_default();
        let mut lines = split(&base);
        let Some(mut block) = hooks_block(&lines) else {
            if !base.is_empty() && !base.ends_with('\n') {
                base.push('\n');
            }
            if !base.is_empty() {
                base.push('\n');
            }
            let mut added = vec![format!("hooks:  {}", comment(marker))];
            for (event, command) in entries {
                added.push(format!("  {event}:  {}", comment(marker)));
                added.push(format!("    {}", item(command, timeout)));
            }
            return Ok(base + &added.join("\n") + "\n");
        };
        if block.form == Form::Unsupported {
            return Err(());
        }
        if block.form == Form::Empty {
            lines[block.line] = format!("hooks:  {}", comment(marker));
            block = hooks_block(&lines).ok_or(())?;
        }
        let parsed = events(&lines, &block);
        let key_indent = parsed.first().map(|e| indent(&lines[e.key])).unwrap_or(2);
        let pad = |n: usize| " ".repeat(n);
        let mut appended: Vec<String> = Vec::new();
        let mut insertions: Vec<(usize, Vec<String>)> = Vec::new();
        for (event, command) in entries {
            let line = item(command, timeout);
            if let Some(found) = parsed.iter().find(|e| &e.name == event) {
                match found.form {
                    Form::Unsupported => return Err(()),
                    Form::Empty => {
                        lines[found.key] =
                            format!("{}{event}:  {}", pad(key_indent), comment(marker));
                        insertions.push((
                            found.key + 1,
                            vec![format!("{}{line}", pad(key_indent + 2))],
                        ));
                    }
                    Form::Block => {
                        let item_indent = found
                            .items
                            .first()
                            .map(|r| indent(&lines[r.start]))
                            .unwrap_or(key_indent + 2);
                        let after = found.items.last().map(|r| r.end).unwrap_or(found.key + 1);
                        insertions.push((after, vec![format!("{}{line}", pad(item_indent))]));
                    }
                }
            } else {
                appended.push(format!("{}{event}:  {}", pad(key_indent), comment(marker)));
                appended.push(format!("{}{line}", pad(key_indent + 2)));
            }
        }
        // New keys go in first where an entry goes at the same line, so the
        // entry ends up under its own key, above them.
        if !appended.is_empty() {
            insertions.insert(0, (block.content, appended));
        }
        let mut ordered: Vec<(usize, usize, Vec<String>)> = insertions
            .into_iter()
            .enumerate()
            .map(|(order, (at, lines))| (at, order, lines))
            .collect();
        ordered.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (at, _, added) in ordered {
            for (offset, line) in added.into_iter().enumerate() {
                lines.insert(at + offset, line);
            }
        }
        let mut result = lines.join("\n");
        if !result.ends_with('\n') {
            result.push('\n');
        }
        Ok(result)
    }
}

/// Installed when every event holds the current hook; outdated when some
/// other line of the island's is there; unreadable when the hooks are
/// written in a form the island can't add to.
pub fn state(text: &str, exe: &Path, app: &str) -> super::hook_agents::installer::State {
    use super::hook_agents::installer::State;
    let marker = claude::marker(app);
    if yaml::installing(Some(text), &entries(exe, app), TIMEOUT, &marker).is_err() {
        return State::Unreadable;
    }
    let commands = yaml::commands(text).unwrap_or_default();
    let current = entries(exe, app)
        .iter()
        .all(|(event, command)| commands.get(event).is_some_and(|c| c.contains(command)));
    if current {
        State::Installed
    } else if text.contains(&marker) {
        State::Outdated
    } else {
        State::Missing
    }
}

// MARK: state.db

/// What Hermes's state.db says of a session, read only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub source: Option<String>,
    pub created_source: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub ended: bool,
}

impl Record {
    /// A session of a terminal, Hermes's terminal UI or its app; a chat its
    /// gateway answers is not.
    pub fn is_local(&self) -> bool {
        [&self.source, &self.created_source]
            .iter()
            .any(|s| s.as_deref().is_some_and(|s| LOCAL_PLATFORMS.contains(&s)))
    }

    /// Where the person sees it: `cli`, `tui` or `desktop`.
    pub fn client(&self) -> Option<&str> {
        match self.created_source.as_deref().or(self.source.as_deref()) {
            Some("gui" | "desktop") => Some("desktop"),
            other => other,
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod db {
    use super::Record;
    #[cfg(target_os = "linux")]
    use crate::linux::sqlite::{
        sqlite3_bind_text, sqlite3_busy_timeout, sqlite3_close, sqlite3_column_text,
        sqlite3_column_type, sqlite3_finalize, sqlite3_open_v2, sqlite3_prepare_v2, sqlite3_step,
    };
    #[cfg_attr(target_os = "linux", allow(unused_imports))]
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    use std::path::Path;
    #[cfg(windows)]
    #[link(name = "winsqlite3")]
    unsafe extern "C" {
        fn sqlite3_open_v2(
            path: *const c_char,
            db: *mut *mut c_void,
            flags: c_int,
            vfs: *const c_char,
        ) -> c_int;
        fn sqlite3_close(db: *mut c_void) -> c_int;
        fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
        fn sqlite3_prepare_v2(
            db: *mut c_void,
            sql: *const c_char,
            len: c_int,
            statement: *mut *mut c_void,
            tail: *mut *const c_char,
        ) -> c_int;
        fn sqlite3_bind_text(
            statement: *mut c_void,
            index: c_int,
            text: *const c_char,
            len: c_int,
            destructor: isize,
        ) -> c_int;
        fn sqlite3_step(statement: *mut c_void) -> c_int;
        fn sqlite3_finalize(statement: *mut c_void) -> c_int;
        fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char;
        fn sqlite3_column_type(statement: *mut c_void, column: c_int) -> c_int;
    }
    /// SQLITE_TRANSIENT: SQLite copies the text.
    const TRANSIENT: isize = -1;
    struct Db(*mut c_void);
    impl Drop for Db {
        fn drop(&mut self) {
            unsafe {
                sqlite3_close(self.0);
            }
        }
    }
    struct Statement(*mut c_void);
    impl Drop for Statement {
        fn drop(&mut self) {
            unsafe {
                sqlite3_finalize(self.0);
            }
        }
    }

    fn text(statement: *mut c_void, column: c_int) -> Option<String> {
        unsafe {
            let value = sqlite3_column_text(statement, column);
            if value.is_null() {
                return None;
            }
            let value = CStr::from_ptr(value).to_string_lossy().into_owned();
            (!value.is_empty()).then_some(value)
        }
    }

    fn query<T>(
        database: &Path,
        sql: &std::ffi::CStr,
        session: &str,
        row: impl Fn(*mut c_void) -> Option<T>,
    ) -> Option<T> {
        if !database.is_file() {
            return None;
        }
        let path = CString::new(database.to_string_lossy().as_bytes()).ok()?;
        let session = CString::new(session).ok()?;
        unsafe {
            let mut pointer = std::ptr::null_mut();
            // READONLY: Hermes's own writes are never in the way.
            let result = sqlite3_open_v2(path.as_ptr(), &mut pointer, 1, std::ptr::null());
            if pointer.is_null() {
                return None;
            }
            let db = Db(pointer);
            if result != 0 {
                return None;
            }
            sqlite3_busy_timeout(db.0, 500);
            let mut pointer = std::ptr::null_mut();
            let result =
                sqlite3_prepare_v2(db.0, sql.as_ptr(), -1, &mut pointer, std::ptr::null_mut());
            if pointer.is_null() {
                return None;
            }
            let statement = Statement(pointer);
            if result != 0 {
                return None;
            }
            sqlite3_bind_text(statement.0, 1, session.as_ptr(), -1, TRANSIENT);
            if sqlite3_step(statement.0) != 100 {
                return None;
            }
            row(statement.0)
        }
    }

    pub fn record(database: &Path, session: &str) -> Option<Record> {
        query(
            database,
            c"SELECT source, created_source, cwd, title, model, ended_at FROM sessions WHERE id = ?",
            session,
            |statement| {
                Some(Record {
                    source: text(statement, 0),
                    created_source: text(statement, 1),
                    cwd: text(statement, 2),
                    title: text(statement, 3),
                    model: text(statement, 4),
                    // SQLITE_NULL is 5.
                    ended: unsafe { sqlite3_column_type(statement, 5) } != 5,
                })
            },
        )
    }

    pub fn last_reply(database: &Path, session: &str) -> Option<String> {
        query(
            database,
            c"SELECT content FROM messages WHERE session_id = ? AND role = 'assistant' AND content IS NOT NULL AND content != '' ORDER BY id DESC LIMIT 1",
            session,
            |statement| text(statement, 0),
        )
    }
}

pub fn database(hermes: &Path) -> PathBuf {
    hermes.join("state.db")
}

pub fn record(session: &str, hermes: &Path) -> Option<Record> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        db::record(&database(hermes), session)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (session, hermes);
        None
    }
}

/// The last thing the agent wrote in the session, for previews.
pub fn last_reply(session: &str, hermes: &Path) -> Option<String> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        db::last_reply(&database(hermes), session)
            .and_then(|t| super::models::preview(Some(&t), super::transcript::REPLY_LIMIT))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (session, hermes);
        None
    }
}

// MARK: Where a session runs

/// The terminals each session was last shown in, newest first, from the
/// notes Hermes's terminal client leaves for `hermes -c`
/// (hermes_cli/terminal_breadcrumbs.py): a file per terminal, named after
/// its device (`tty-dev-pts-3`) or, without one, the multiplexer or
/// terminal variable that names it (`wt_session-<id>` in Windows Terminal).
pub fn notes(hermes: &Path) -> HashMap<String, Vec<String>> {
    let folder = hermes.join("terminal-sessions");
    let Ok(read) = std::fs::read_dir(&folder) else {
        return HashMap::new();
    };
    let mut found: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for entry in read.flatten().take(512) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(data) = std::fs::read(entry.path()) else {
            continue;
        };
        if data.len() > 16 << 10 {
            continue;
        }
        let Ok(json) = serde_json::from_slice::<Value>(&data) else {
            continue;
        };
        let Some(session) = json
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let at = json.get("ts").and_then(Value::as_f64).unwrap_or(0.0);
        found
            .entry(session.to_owned())
            .or_default()
            .push((name, at));
    }
    found
        .into_iter()
        .map(|(session, mut terminals)| {
            terminals.sort_by(|a, b| b.1.total_cmp(&a.1));
            (session, terminals.into_iter().map(|(t, _)| t).collect())
        })
        .collect()
}

/// The device a note's name stands for: `tty-dev-pts-3` is /dev/pts/3.
pub fn device(note: &str) -> Option<String> {
    let rest = note.strip_prefix("tty-dev-")?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(format!("/dev/{}", rest.replace('-', "/")))
}

/// Windows Terminal's tab a note names: `wt_session-<guid>`.
pub fn wt_session(note: &str) -> Option<&str> {
    note.strip_prefix("wt_session-").filter(|s| !s.is_empty())
}

/// Whether a command line is Hermes's terminal client: `hermes` or
/// `hermes_cli.main` run by Python, never its gateway.
pub fn is_client(arguments: &[String]) -> bool {
    if arguments
        .iter()
        .any(|a| a == "gateway" || a == "acp" || a == "mcp-serve")
    {
        return false;
    }
    arguments.iter().take(4).any(|argument| {
        if argument == "hermes_cli.main" {
            return true;
        }
        // A Windows path's last part too, whatever system reads it.
        let name = argument
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let name = name.trim_end_matches(".exe");
        name == "hermes" || (name.starts_with("hermes-") && name != "hermes-acp")
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Client {
    pub pid: u32,
    pub parent: u32,
    /// The terminal device (Linux) or Windows Terminal session it runs in.
    pub terminal: Option<String>,
    pub cwd: Option<String>,
}

/// Hermes's terminal clients running now.
#[cfg(target_os = "linux")]
pub fn running() -> Vec<Client> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            let arguments: Vec<String> = raw
                .split(|&b| b == 0)
                .filter(|a| !a.is_empty())
                .map(|a| String::from_utf8_lossy(a).into_owned())
                .collect();
            if !is_client(&arguments) {
                return None;
            }
            let terminal = crate::linux::process::terminal_of(pid)?;
            let parent = crate::hosts::process::ancestors(pid)
                .and_then(|chain| chain.get(1).map(|e| e.pid))
                .unwrap_or(0);
            Some(Client {
                pid,
                parent,
                terminal: Some(terminal),
                cwd: crate::hosts::process::metadata(pid).map(|m| m.cwd),
            })
        })
        .collect()
}

/// Hermes's terminal clients running now: `hermes.exe` (its launcher) and
/// Python running it, each with the Windows Terminal tab it runs in.
#[cfg(windows)]
pub fn running() -> Vec<Client> {
    let mut pids = crate::hosts::process::pids_named("hermes.exe");
    pids.extend(crate::hosts::process::pids_named("python.exe"));
    pids.extend(crate::hosts::process::pids_named("pythonw.exe"));
    pids.into_iter()
        .filter_map(|pid| {
            let metadata = crate::hosts::process::metadata(pid)?;
            let arguments: Vec<String> = metadata
                .command_line
                .split_whitespace()
                .map(|a| a.trim_matches('"').to_owned())
                .collect();
            if !is_client(&arguments) {
                return None;
            }
            let terminal = crate::hosts::process::environment_values(pid, &["WT_SESSION"])
                .remove("WT_SESSION");
            let parent = crate::hosts::process::ancestors(pid)
                .and_then(|chain| chain.get(1).map(|e| e.pid))
                .unwrap_or(0);
            Some(Client {
                pid,
                parent,
                terminal,
                cwd: Some(metadata.cwd),
            })
        })
        .collect()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn running() -> Vec<Client> {
    Vec::new()
}

/// The client showing `session`: Hermes's own client on the terminal its
/// note names; with no note (its Ink UI writes none, and a new session's
/// note is written only after its first hook), the only client in the
/// session's folder no other listed session claims. A note of a session no
/// longer listed says nothing: terminals are used again. The first of a
/// chain of Hermes processes on a terminal is the one the person started.
pub fn client(
    session: &str,
    cwd: Option<&str>,
    notes: &HashMap<String, Vec<String>>,
    running: &[Client],
    claimed: &HashSet<u32>,
    listed: &HashSet<String>,
) -> Option<u32> {
    let first = |candidates: Vec<&Client>| -> Option<u32> {
        let pids: HashSet<u32> = candidates.iter().map(|c| c.pid).collect();
        candidates
            .iter()
            .find(|c| !pids.contains(&c.parent))
            .or(candidates.first())
            .map(|c| c.pid)
    };
    let terminal_of = |note: &str| -> Option<String> {
        device(note).or_else(|| wt_session(note).map(str::to_owned))
    };
    for note in notes.get(session).into_iter().flatten() {
        let Some(terminal) = terminal_of(note) else {
            continue;
        };
        let here: Vec<&Client> = running
            .iter()
            .filter(|c| c.terminal.as_deref() == Some(terminal.as_str()))
            .collect();
        if let Some(pid) = first(here) {
            return Some(pid);
        }
    }
    let cwd = cwd.filter(|c| !c.is_empty())?;
    let standard = |path: &str| -> String {
        std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.trim_end_matches(['/', '\\']).to_owned())
    };
    let wanted = standard(cwd);
    // Terminals another listed session's note names are that session's.
    let noted: HashSet<String> = notes
        .iter()
        .filter(|(s, _)| s.as_str() != session && listed.contains(s.as_str()))
        .flat_map(|(_, terminals)| terminals.iter().filter_map(|t| terminal_of(t)))
        .collect();
    let here: Vec<&Client> = running
        .iter()
        .filter(|c| {
            !claimed.contains(&c.pid)
                && c.cwd.as_deref().map(standard).as_deref() == Some(wanted.as_str())
                && !c.terminal.as_ref().is_some_and(|t| noted.contains(t))
        })
        .collect();
    let roots: Vec<&Client> = here
        .iter()
        .filter(|c| !here.iter().any(|o| o.pid == c.parent))
        .copied()
        .collect();
    (roots.len() == 1).then(|| roots[0].pid)
}

/// Hermes's app, when it runs: its window shows the session.
pub fn app() -> Option<u32> {
    let names: &[&str] = if cfg!(windows) {
        &["Hermes.exe", "Hermes Agent.exe"]
    } else {
        &["hermes-desktop", "Hermes", "hermes-agent-desktop"]
    };
    names
        .iter()
        .flat_map(|n| crate::hosts::process::pids_named(n))
        .find(|&pid| {
            crate::hosts::process::image_path(pid).is_some_and(|p| {
                let path = p.to_string_lossy().to_ascii_lowercase();
                // Its terminal command shares the name; the app doesn't live in a bin folder.
                !path.contains("/bin/")
                    && !path.contains("\\bin\\")
                    && !path.contains("\\scripts\\")
            })
        })
}

/// Where each Hermes session runs, looked up as its hooks come in and kept
/// while its client lives (AgentHermesPlaces).
#[derive(Default)]
pub struct Places {
    places: HashMap<String, Place>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// The client showing it, or 0 when none was found.
    pub pid: i32,
    pub record: Option<Record>,
    pub looked: Time,
}

/// How often a session whose client wasn't found is looked for again: its
/// note comes a moment after its first hooks.
pub const RETRY_AFTER: Time = 0.5;

pub struct Lookup<'a> {
    pub record: &'a dyn Fn(&str) -> Option<Record>,
    pub notes: &'a dyn Fn() -> HashMap<String, Vec<String>>,
    pub running: &'a dyn Fn() -> Vec<Client>,
    pub app: &'a dyn Fn() -> Option<u32>,
    pub alive: &'a dyn Fn(i32) -> bool,
}

impl Places {
    /// The session's place; None for a chat its gateway answers, which the
    /// island doesn't list. `fresh` reads its record again.
    pub fn place(
        &mut self,
        session: &str,
        now: Time,
        fresh: bool,
        lookup: &Lookup,
    ) -> Option<Place> {
        let mut place = self.places.get(session).cloned();
        if place.is_none() || fresh || place.as_ref().is_some_and(|p| p.record.is_none()) {
            let read = (lookup.record)(session);
            match &mut place {
                None => {
                    place = Some(Place {
                        pid: 0,
                        record: read,
                        looked: f64::NEG_INFINITY,
                    })
                }
                Some(place) => {
                    if read.is_some() {
                        place.record = read;
                    }
                }
            }
        }
        let mut place = place?;
        if place.record.as_ref().is_some_and(|r| !r.is_local()) {
            self.places.insert(session.to_owned(), place);
            return None;
        }
        let lost = place.pid <= 0 || !(lookup.alive)(place.pid);
        if lost && now - place.looked >= RETRY_AFTER {
            let others: Vec<(&String, &Place)> = self
                .places
                .iter()
                .filter(|(s, p)| {
                    s.as_str() != session && p.record.as_ref().is_none_or(Record::is_local)
                })
                .collect();
            let claimed: HashSet<u32> = others
                .iter()
                .filter_map(|(_, p)| (p.pid > 0).then_some(p.pid as u32))
                .collect();
            let listed: HashSet<String> = others.iter().map(|(s, _)| (*s).clone()).collect();
            let cwd = place.record.as_ref().and_then(|r| r.cwd.clone());
            let found = client(
                session,
                cwd.as_deref(),
                &(lookup.notes)(),
                &(lookup.running)(),
                &claimed,
                &listed,
            )
            .or_else(|| {
                (place.record.as_ref().and_then(Record::client) == Some("desktop"))
                    .then(|| (lookup.app)())
                    .flatten()
            });
            place.pid = found.map(|p| p as i32).unwrap_or(0);
            place.looked = now;
        } else if lost {
            place.pid = 0;
        }
        self.places.insert(session.to_owned(), place.clone());
        Some(place)
    }

    pub fn forget(&mut self, session: &str) {
        self.places.remove(session);
    }

    /// Forgets the sessions the island no longer lists.
    pub fn keep(&mut self, sessions: &HashSet<String>) {
        self.places.retain(|s, _| sessions.contains(s));
    }
}

/// Answers a prompt through Hermes's own gateway, the way its Ink UI and its
/// app do (AgentHermes.swift's `AgentHermesGateway`): `hermes gateway attach
/// --json` hands out a one-use ticket for the gateway's local WebSocket, the
/// island joins the session as one more viewer (`session.resume`, which lists
/// what waits), answers (`approval.respond`, `clarify.lock`) and leaves. Its
/// Ink UI and app close their card when the answer comes from elsewhere, and
/// so does its classic terminal UI for a question; it keeps an approval's
/// menu up until a key is pressed, so there the island presses keys instead
/// wherever the terminal takes them.
pub mod gateway {
    use std::collections::BTreeMap;
    use std::net::{TcpStream, ToSocketAddrs};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use tungstenite::Message;
    use tungstenite::client::IntoClientRequest;

    use crate::sessions::hook_message::Decision;
    use crate::sessions::models::Permission;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Answer {
        /// `once` or `deny` for a dangerous command.
        Approval(&'static str),
        /// Each question's picks, or None when skipped.
        Clarify(Vec<Option<Vec<String>>>),
    }

    impl Answer {
        fn kind(&self) -> &'static str {
            match self {
                Answer::Approval(_) => "approval",
                Answer::Clarify(_) => "clarify",
            }
        }
    }

    /// What answers the island's decision, or None for one Hermes can't take.
    pub fn answer(decision: &Decision, asked: &Permission) -> Option<Answer> {
        let asks = !asked.questions.is_empty();
        match decision {
            Decision::Allow if !asks => Some(Answer::Approval("once")),
            Decision::Deny if !asks => Some(Answer::Approval("deny")),
            Decision::Deny => Some(Answer::Clarify(vec![None; asked.questions.len()])),
            Decision::Answer(answers) if asks => {
                let picks: Vec<Option<Vec<String>>> = asked
                    .questions
                    .iter()
                    .map(|q| {
                        let picked = answers.get(&q.question).or_else(|| {
                            (asked.questions.len() == 1 && answers.len() == 1)
                                .then(|| answers.values().next())
                                .flatten()
                        });
                        let picked: Vec<String> = picked
                            .map(|p| p.iter().filter(|a| !a.is_empty()).cloned().collect())
                            .unwrap_or_default();
                        (!picked.is_empty()).then_some(picked)
                    })
                    .collect();
                picks
                    .iter()
                    .any(Option::is_some)
                    .then_some(Answer::Clarify(picks))
            }
            _ => None,
        }
    }

    /// The ticket `hermes gateway attach --json` prints: the socket's URL and
    /// the subprotocols that carry the ticket, for a gateway on this computer.
    pub fn ticket(printed: &Value) -> Option<(String, Vec<String>)> {
        if printed.get("state").is_some_and(|s| s != "ready") {
            return None;
        }
        let url = printed.get("url")?.as_str()?;
        let rest = url.strip_prefix("ws://")?;
        let host = rest.split('/').next()?;
        let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
        if !["127.0.0.1", "localhost", "[::1]"].contains(&name) {
            return None;
        }
        let protocols: Vec<String> = printed
            .get("protocols")?
            .as_array()?
            .iter()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect();
        (!protocols.is_empty()).then(|| (url.to_owned(), protocols))
    }

    /// The prompt of `kind` waiting in a session, from `session.resume`'s
    /// result: its id and the run it belongs to.
    pub fn pending(result: &Value, kind: &str) -> Option<(String, i64)> {
        let prompt = result
            .get("prompts")?
            .as_array()?
            .iter()
            .rev()
            .find(|p| p.get("kind").and_then(Value::as_str) == Some(kind))?;
        let id = prompt
            .get("prompt_id")?
            .as_str()
            .filter(|i| !i.is_empty())?;
        let generation = prompt
            .get("execution_generation")
            .or_else(|| result.get("execution_generation"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        Some((id.to_owned(), generation))
    }

    /// A question the gateway asks as a request of its own
    /// (`open_requests`): its id and each question's id.
    pub fn open_question(result: &Value) -> Option<(String, Vec<String>)> {
        let request = result
            .get("open_requests")?
            .as_array()?
            .iter()
            .rev()
            .find(|r| r.get("method").and_then(Value::as_str) == Some("clarify"))?;
        let id = request.get("id")?.as_str().filter(|i| !i.is_empty())?;
        let ids: Vec<String> = request
            .get("params")?
            .get("questions")?
            .as_array()?
            .iter()
            .filter_map(|q| q.get("qid").and_then(Value::as_str).map(str::to_owned))
            .collect();
        (!ids.is_empty()).then(|| (id.to_owned(), ids))
    }

    /// What `clarify.lock` takes for one question: a single pick as text,
    /// several as a list, a skipped one as null.
    fn locked(picked: Option<&Vec<String>>) -> Value {
        match picked {
            Some(p) if p.len() == 1 => json!(p[0]),
            Some(p) => json!(p),
            None => Value::Null,
        }
    }

    /// Answers the prompt waiting in `session`. Blocks a few seconds at most;
    /// call it off the service's lock.
    pub fn send(answer: &Answer, session: &str, hermes: &Path, timeout: Duration) -> bool {
        let path = crate::agent_usage::cli::child_path(hermes);
        let Some(mut attach) = crate::agent_usage::cli::Conversation::start(
            hermes,
            &["gateway", "attach", "--json"],
            &[("PATH".to_owned(), path)],
            &[],
            timeout,
        ) else {
            return false;
        };
        let printed = attach.next();
        attach.end(Duration::from_millis(500));
        let Some((url, protocols)) = printed.as_ref().and_then(ticket) else {
            return false;
        };
        let Some(mut socket) = Socket::open(&url, &protocols, timeout) else {
            return false;
        };
        let done = socket.answer(answer, session);
        socket.close();
        done
    }

    struct Socket {
        socket: tungstenite::WebSocket<TcpStream>,
        timeout: Duration,
        next: i64,
    }

    impl Socket {
        fn open(url: &str, protocols: &[String], timeout: Duration) -> Option<Socket> {
            let address = url.strip_prefix("ws://")?.split('/').next()?;
            let address = address.to_socket_addrs().ok()?.next()?;
            let stream = TcpStream::connect_timeout(&address, timeout).ok()?;
            stream.set_read_timeout(Some(timeout)).ok()?;
            stream.set_write_timeout(Some(timeout)).ok()?;
            let mut request = url.into_client_request().ok()?;
            request
                .headers_mut()
                .insert("Sec-WebSocket-Protocol", protocols.join(", ").parse().ok()?);
            let (socket, _) = tungstenite::client::client(request, stream).ok()?;
            Some(Socket {
                socket,
                timeout,
                next: 0,
            })
        }

        /// The call's result, past the events between; None on an error, a
        /// refusal or no answer in time.
        fn call(&mut self, method: &str, params: Value) -> Option<Value> {
            self.next += 1;
            let id = self.next;
            let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            self.socket.send(Message::text(request.to_string())).ok()?;
            let deadline = Instant::now() + self.timeout;
            while Instant::now() < deadline {
                let text = match self.socket.read().ok()? {
                    Message::Text(text) => text.to_string(),
                    Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok()?,
                    Message::Close(_) => return None,
                    _ => continue,
                };
                let Ok(reply) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if reply.get("id").and_then(Value::as_i64) == Some(id) {
                    return reply.get("result").cloned();
                }
            }
            None
        }

        fn answer(&mut self, answer: &Answer, session: &str) -> bool {
            let Some(resumed) = self.call("session.resume", json!({"session_id": session})) else {
                return false;
            };
            // A question is the gateway's own request: each answer locked in turn, the last lock
            // settling it.
            if let Answer::Clarify(picks) = answer
                && let Some((request, questions)) = open_question(&resumed)
            {
                for (index, question) in questions.iter().enumerate() {
                    let value = locked(picks.get(index).and_then(Option::as_ref));
                    let params =
                        json!({"request_id": request, "question_id": question, "answer": value});
                    match self.call("clarify.lock", params) {
                        Some(r) if r.get("status").and_then(Value::as_str) != Some("expired") => {}
                        _ => return false,
                    }
                }
                return true;
            }
            let Some((prompt, generation)) = pending(&resumed, answer.kind()) else {
                return false;
            };
            let mut params = BTreeMap::new();
            params.insert("session_id", json!(session));
            params.insert("execution_generation", json!(generation));
            params.insert("prompt_id", json!(prompt));
            let method = match answer {
                Answer::Approval(choice) => {
                    params.insert("choice", json!(choice));
                    "approval.respond"
                }
                Answer::Clarify(picks) => {
                    let first = picks.first().cloned().flatten().unwrap_or_default();
                    params.insert("answer", json!(first.join(", ")));
                    "clarify.respond"
                }
            };
            self.call(method, json!(params)).is_some_and(|r| {
                matches!(
                    r.get("status").and_then(Value::as_str),
                    Some("resolved" | "already_resolved")
                )
            })
        }

        fn close(mut self) {
            let _ = self.socket.close(None);
            let _ = self.socket.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER: &str = "--marker openbar-hook --app com.openbar.dev";

    fn entries() -> Vec<(String, String)> {
        vec![
            (
                "on_session_open".into(),
                format!("'/x/openbar' --hook --agent hermes --event on_session_open {MARKER}"),
            ),
            (
                "pre_llm_call".into(),
                format!("'/x/openbar' --hook --agent hermes --event pre_llm_call {MARKER}"),
            ),
        ]
    }

    #[test]
    fn an_entry_is_one_flow_mapping_that_reads_back() {
        let item = yaml::item("'/a b/openbar' --hook \"x\"", 5);
        assert_eq!(
            item,
            r#"- {command: "'/a b/openbar' --hook \"x\"", timeout: 5}"#
        );
        assert_eq!(
            yaml::command_in(&item).as_deref(),
            Some("'/a b/openbar' --hook \"x\"")
        );
    }

    #[test]
    fn with_no_hooks_the_block_goes_at_the_end_and_comes_out_again() {
        let plain =
            "model:\n  default: gpt\n# hooks:\n#   pre_tool_call: []\nhooks_auto_accept: false\n";
        let added = yaml::installing(Some(plain), &entries(), 5, MARKER).unwrap();
        assert!(added.starts_with(plain));
        assert!(added.contains(&format!(
            "hooks:  # {MARKER}\n  on_session_open:  # {MARKER}\n    - {{command:"
        )));
        assert_eq!(
            yaml::commands(&added).unwrap()["pre_llm_call"],
            vec![entries()[1].1.clone()]
        );
        assert_eq!(yaml::removing(&added, MARKER).unwrap(), plain);
    }

    #[test]
    fn the_islands_hooks_join_the_persons_and_leave_them_as_they_were() {
        let mine = "model: gpt\nhooks:\n  # my formatter\n  post_tool_call:\n    - matcher: \"write_file|patch\"\n      command: \"~/.hermes/agent-hooks/fmt.sh\"\n  on_session_open:\n  - command: ~/.hermes/agent-hooks/hello.sh\n    timeout: 5\n\n# display settings\ndisplay:\n  interface: cli\n";
        let joined = yaml::installing(Some(mine), &entries(), 5, MARKER).unwrap();
        let read = yaml::commands(&joined).unwrap();
        assert_eq!(
            read["post_tool_call"],
            vec!["~/.hermes/agent-hooks/fmt.sh".to_owned()]
        );
        assert_eq!(
            read["on_session_open"],
            vec![
                "~/.hermes/agent-hooks/hello.sh".to_owned(),
                entries()[0].1.clone()
            ]
        );
        assert_eq!(read["pre_llm_call"], vec![entries()[1].1.clone()]);
        assert!(joined.contains(
            "  - command: ~/.hermes/agent-hooks/hello.sh\n    timeout: 5\n  - {command:"
        ));
        assert!(joined.contains("\n\n# display settings\ndisplay:\n  interface: cli"));
        assert_eq!(joined.matches("\nhooks:").count(), 1);
        assert_eq!(yaml::removing(&joined, MARKER).unwrap(), mine);
        assert!(yaml::removing(mine, MARKER).is_none());
        assert_eq!(
            yaml::installing(Some(&joined), &entries(), 5, MARKER).unwrap(),
            joined
        );
    }

    #[test]
    fn empty_hooks_take_the_islands_and_inline_ones_are_left_alone() {
        let filled =
            yaml::installing(Some("hooks: {}\nother: 1\n"), &entries(), 5, MARKER).unwrap();
        assert!(filled.starts_with(&format!(
            "hooks:  # {MARKER}\n  on_session_open:  # {MARKER}\n"
        )));
        assert_eq!(yaml::removing(&filled, MARKER).unwrap(), "other: 1\n");
        let list = yaml::installing(
            Some("hooks:\n  on_session_open: []\n"),
            &entries()[..1],
            5,
            MARKER,
        )
        .unwrap();
        assert_eq!(
            yaml::commands(&list).unwrap()["on_session_open"],
            vec![entries()[0].1.clone()]
        );
        for inline in [
            "hooks: {on_session_open: [{command: x}]}\n",
            "hooks:\n  on_session_open: [{command: x}]\n",
        ] {
            assert!(yaml::installing(Some(inline), &entries(), 5, MARKER).is_err());
        }
        assert_eq!(
            yaml::commands("hooks:\r\n  on_session_open:\r\n    - command: 'it''s'\r\n").unwrap()["on_session_open"],
            vec!["it's".to_owned()]
        );
    }

    #[test]
    fn the_command_starts_this_app_with_no_shell() {
        let line = command(
            "pre_tool_call",
            Path::new("/opt/Open bar/openbar"),
            "com.openbar.dev",
        );
        if cfg!(windows) {
            assert!(line.starts_with(
                "\"/opt/Open bar/openbar\" --hook --agent hermes --event pre_tool_call"
            ));
        } else {
            assert_eq!(
                line,
                "'/opt/Open bar/openbar' --hook --agent hermes --event pre_tool_call --marker openbar-hook --app com.openbar.dev"
            );
        }
    }

    #[test]
    fn the_allowlist_says_which_hooks_hermes_runs() {
        let pairs = approved_pairs(Some(
            br#"{"approvals": [{"event": "pre_llm_call", "command": "x "}, {"event": 1}]}"#,
        ));
        assert!(pairs.contains(&("pre_llm_call".into(), "x".into())) && pairs.len() == 1);
        assert!(approved_pairs(Some(b"nope")).is_empty() && approved_pairs(None).is_empty());
    }

    #[test]
    fn a_session_is_placed_at_its_terminal_client() {
        assert_eq!(device("tty-dev-pts-3").as_deref(), Some("/dev/pts/3"));
        assert_eq!(device("tmux_pane-%3"), None);
        assert_eq!(wt_session("wt_session-ab-12"), Some("ab-12"));
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(is_client(&args(&[
            "/usr/bin/python3",
            "/home/me/.hermes/hermes-agent/venv/bin/hermes"
        ])));
        assert!(is_client(&args(&["C:\\x\\hermes.exe", "chat"])));
        assert!(!is_client(&args(&[
            "python3",
            "-m",
            "hermes_cli.main",
            "gateway",
            "run"
        ])));
        assert!(!is_client(&args(&["/usr/bin/vim", "hermes.md"])));
        let running = vec![
            Client {
                pid: 101,
                parent: 50,
                terminal: Some("/dev/pts/1".into()),
                cwd: Some("/w/a".into()),
            },
            Client {
                pid: 102,
                parent: 101,
                terminal: Some("/dev/pts/1".into()),
                cwd: Some("/w/a".into()),
            },
            Client {
                pid: 201,
                parent: 60,
                terminal: Some("/dev/pts/2".into()),
                cwd: Some("/w/b".into()),
            },
            Client {
                pid: 301,
                parent: 70,
                terminal: Some("/dev/pts/3".into()),
                cwd: Some("/w/c".into()),
            },
            Client {
                pid: 302,
                parent: 71,
                terminal: Some("/dev/pts/4".into()),
                cwd: Some("/w/c".into()),
            },
        ];
        let notes: HashMap<String, Vec<String>> = [
            (
                "s1".to_owned(),
                vec!["tty-dev-pts-9".to_owned(), "tty-dev-pts-1".to_owned()],
            ),
            ("s2".to_owned(), vec!["tty-dev-pts-2".to_owned()]),
        ]
        .into();
        let listed: HashSet<String> = ["s2".to_owned()].into();
        assert_eq!(
            client(
                "s1",
                Some("/w/x"),
                &notes,
                &running,
                &HashSet::new(),
                &listed
            ),
            Some(101)
        );
        // A terminal another listed session's note names is that session's;
        // a note of one no longer listed says nothing.
        assert_eq!(
            client(
                "s3",
                Some("/w/b"),
                &notes,
                &running,
                &HashSet::new(),
                &listed
            ),
            None
        );
        assert_eq!(
            client(
                "s3",
                Some("/w/b"),
                &notes,
                &running,
                &HashSet::new(),
                &HashSet::new()
            ),
            Some(201)
        );
        assert_eq!(
            client(
                "s4",
                Some("/w/c"),
                &notes,
                &running,
                &HashSet::new(),
                &listed
            ),
            None
        );
        assert_eq!(
            client("s4", Some("/w/c"), &notes, &running, &[302].into(), &listed),
            Some(301)
        );
    }

    #[test]
    fn places_keep_a_live_client_and_leave_chats_out() {
        let local = Record {
            source: Some("local".into()),
            created_source: Some("cli".into()),
            cwd: Some("/w/a".into()),
            ..Default::default()
        };
        let mut places = Places::default();
        let looked = std::cell::Cell::new(0);
        let lookup = Lookup {
            record: &|_| Some(local.clone()),
            notes: &|| {
                looked.set(looked.get() + 1);
                [("s1".to_owned(), vec!["tty-dev-pts-1".to_owned()])].into()
            },
            running: &|| {
                vec![Client {
                    pid: 101,
                    parent: 1,
                    terminal: Some("/dev/pts/1".into()),
                    cwd: None,
                }]
            },
            app: &|| Some(999),
            alive: &|pid| pid == 101,
        };
        assert_eq!(
            places.place("s1", 100.0, false, &lookup).map(|p| p.pid),
            Some(101)
        );
        assert_eq!(
            places.place("s1", 101.0, false, &lookup).map(|p| p.pid),
            Some(101)
        );
        assert_eq!(looked.get(), 1);
        let chat = Record {
            source: Some("telegram".into()),
            created_source: Some("telegram".into()),
            ..Default::default()
        };
        let telegram = Lookup {
            record: &|_| Some(chat.clone()),
            ..lookup
        };
        assert!(places.place("t1", 100.0, false, &telegram).is_none());
        let app = Record {
            created_source: Some("gui".into()),
            source: Some("local".into()),
            ..Default::default()
        };
        let desktop = Lookup {
            record: &|_| Some(app.clone()),
            running: &Vec::new,
            notes: &HashMap::new,
            ..telegram
        };
        assert_eq!(
            places.place("d1", 100.0, false, &desktop).map(|p| p.pid),
            Some(999)
        );
    }

    #[test]
    fn gateway_ticket_prompt_and_question() {
        use super::gateway::{open_question, pending, ticket};
        let printed = serde_json::json!({"state": "ready", "url": "ws://127.0.0.1:56033/api/ws",
            "protocols": ["hermes-gateway-v1", "hermes-gateway-ticket.Al0c"], "ticket": "Al0c"});
        assert_eq!(
            ticket(&printed),
            Some((
                "ws://127.0.0.1:56033/api/ws".to_owned(),
                vec![
                    "hermes-gateway-v1".to_owned(),
                    "hermes-gateway-ticket.Al0c".to_owned()
                ]
            ))
        );
        // Only a local gateway that is ready is joined.
        for other in [
            serde_json::json!({"state": "ready", "url": "ws://example.com/api/ws", "protocols": ["x"]}),
            serde_json::json!({"state": "absent"}),
            serde_json::json!({"state": "ready", "url": "ws://127.0.0.1:1/api/ws", "protocols": []}),
        ] {
            assert_eq!(ticket(&other), None, "{other}");
        }
        let resumed = serde_json::json!({"execution_generation": 3, "prompts": [
            {"kind": "clarify", "prompt_id": "c1", "execution_generation": 2},
            {"kind": "approval", "prompt_id": "a1", "execution_generation": 2},
        ]});
        assert_eq!(pending(&resumed, "approval"), Some(("a1".to_owned(), 2)));
        assert_eq!(
            pending(&resumed, "clarify").map(|p| p.0),
            Some("c1".to_owned())
        );
        assert_eq!(
            pending(&serde_json::json!({"prompts": []}), "approval"),
            None
        );
        let asking = serde_json::json!({"open_requests": [
            {"id": "srq-1", "method": "sudo", "params": {}},
            {"id": "srq-3c49", "method": "clarify", "params": {"questions": [
                {"qid": "q0", "question": "Which color?", "choices": ["Red (Recommended)", "Blue"]}]}},
        ]});
        assert_eq!(
            open_question(&asking),
            Some(("srq-3c49".to_owned(), vec!["q0".to_owned()]))
        );
        assert_eq!(
            open_question(
                &serde_json::json!({"open_requests": [{"id": "srq-1", "method": "sudo"}]})
            ),
            None
        );
    }

    /// Answers a live Hermes gateway: with `OB_HERMES` the `hermes` to run,
    /// `HERMES_HOME` its home, `OB_SESSION` the session waiting and
    /// `OB_ANSWER` `once`, `deny`, `skip` or a question's answer (`a|b` for
    /// several picks of one question).
    #[test]
    #[ignore]
    fn gateway_answers_a_live_session() {
        use super::gateway::{Answer, send};
        let hermes = std::env::var("OB_HERMES").expect("OB_HERMES");
        let session = std::env::var("OB_SESSION").expect("OB_SESSION");
        let answer = match std::env::var("OB_ANSWER").expect("OB_ANSWER").as_str() {
            "once" => Answer::Approval("once"),
            "deny" => Answer::Approval("deny"),
            "skip" => Answer::Clarify(vec![None]),
            text => Answer::Clarify(vec![Some(text.split('|').map(str::to_owned).collect())]),
        };
        assert!(send(
            &answer,
            &session,
            Path::new(&hermes),
            std::time::Duration::from_secs(8)
        ));
    }
}
