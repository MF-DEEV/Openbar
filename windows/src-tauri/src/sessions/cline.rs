// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Cline (#241): its VS Code extension (and the editors built on VS Code)
//! and its CLI (`cline`) keep every session in one store,
//! `~/.cline/data/db/sessions.db`, with each conversation beside it in
//! `sessions/<id>/<id>.messages.json`. The store says what hooks would:
//! `running` while a turn works, `pending` while a tool waits for the
//! person's approval, `idle` once the turn ended, and `ended_at` once the
//! session is closed. The conversation is written as it goes, a tool call
//! before its result. So the island reads both and puts nothing into
//! Cline: a hook would show as a step of its own in the extension's chat.
//! As AgentCline.swift.
//!
//! The extension asks in its own panel, and nothing outside it can answer.
//! The CLI's terminal UI takes `y` for Approve and `n` for Deny, and a
//! question's number. A CLI session that runs in Cline's hub (its local
//! daemon) can be answered and sent replies through the hub too.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::hook_agents::{AgentEvent, Kind};
use super::hook_message::Decision;
use super::models::{ContentPolicy, Permission, Provider, Question, QuestionOption, Time, ToolUse};
use crate::hosts::Key;

/// The tool Cline asks a question with: one question, two to five choices.
pub const QUESTION_TOOL: &str = "ask_question";

/// Where a session is answered: the extension's panel, the CLI's terminal,
/// or the CLI's terminal and Cline's hub.
pub const EXTENSION_CLIENT: &str = "vscode";
pub const CLI_CLIENT: &str = "cli";
pub const HUB_CLIENT: &str = "hub";

// MARK: Places

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// Cline's data folder: `CLINE_DATA_DIR`, else `data` in `CLINE_DIR`, else
/// `~/.cline/data`.
pub fn data_folder(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(data) = non_empty(env("CLINE_DATA_DIR")) {
        return PathBuf::from(data);
    }
    if let Some(dir) = non_empty(env("CLINE_DIR")) {
        return PathBuf::from(dir).join("data");
    }
    home.join(".cline").join("data")
}

/// The folder `cline` was used from: `CLINE_DIR`, or `~/.cline`.
pub fn folder(home: &Path) -> PathBuf {
    non_empty(std::env::var("CLINE_DIR").ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cline"))
}

pub fn database(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(db) = non_empty(env("CLINE_DB_DATA_DIR")) {
        return PathBuf::from(db).join("sessions.db");
    }
    data_folder(home, env).join("db").join("sessions.db")
}

pub fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

// MARK: Store

/// A session as Cline's store has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: String,
    /// `vscode` for the extension, `cli` for the CLI.
    pub source: String,
    /// The process that runs it: the editor's extension host, the CLI, or
    /// Cline's hub when the CLI runs its sessions there.
    pub pid: i32,
    pub status: String,
    pub cwd: String,
    pub model: Option<String>,
    pub title: Option<String>,
    pub messages: Option<String>,
}

impl Record {
    /// A turn works, or waits on the person in it.
    pub fn is_busy(&self) -> bool {
        self.status == "running" || self.status == "pending"
    }
    pub fn waits_for_approval(&self) -> bool {
        self.status == "pending"
    }
    pub fn failed(&self) -> bool {
        self.status == "failed"
    }
    pub fn is_extension(&self) -> bool {
        self.source == "vscode"
    }
}

/// The sessions still open: no end written, and not a subagent's.
pub fn records(database: &Path) -> Vec<Record> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        db::records(database)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = database;
        Vec::new()
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod db {
    use super::Record;
    #[cfg(target_os = "linux")]
    use crate::linux::sqlite::{
        sqlite3_busy_timeout, sqlite3_close, sqlite3_column_int64, sqlite3_column_text,
        sqlite3_finalize, sqlite3_open_v2, sqlite3_prepare_v2, sqlite3_step,
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
        fn sqlite3_step(statement: *mut c_void) -> c_int;
        fn sqlite3_finalize(statement: *mut c_void) -> c_int;
        fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char;
        fn sqlite3_column_int64(statement: *mut c_void, column: c_int) -> i64;
    }
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

    pub fn records(database: &Path) -> Vec<Record> {
        if !database.is_file() {
            return Vec::new();
        }
        let Ok(path) = CString::new(database.to_string_lossy().as_bytes()) else {
            return Vec::new();
        };
        let sql = c"SELECT session_id, source, pid, status, cwd, model, metadata_json, messages_path FROM sessions WHERE ended_at IS NULL AND is_subagent = 0 ORDER BY started_at";
        let mut out = Vec::new();
        unsafe {
            let mut pointer = std::ptr::null_mut();
            // READONLY: Cline's own writes are never in the way.
            let result = sqlite3_open_v2(path.as_ptr(), &mut pointer, 1, std::ptr::null());
            if pointer.is_null() {
                return out;
            }
            let db = Db(pointer);
            if result != 0 {
                return out;
            }
            sqlite3_busy_timeout(db.0, 300);
            let mut pointer = std::ptr::null_mut();
            let result =
                sqlite3_prepare_v2(db.0, sql.as_ptr(), -1, &mut pointer, std::ptr::null_mut());
            if pointer.is_null() {
                return out;
            }
            let statement = Statement(pointer);
            if result != 0 {
                return out;
            }
            // SQLITE_ROW is 100.
            while sqlite3_step(statement.0) == 100 {
                let (Some(id), Some(source)) = (text(statement.0, 0), text(statement.0, 1)) else {
                    continue;
                };
                let title = text(statement.0, 6)
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok())
                    .and_then(|m| m.get("title").and_then(|t| t.as_str()).map(str::to_owned))
                    .filter(|t| !t.is_empty());
                out.push(Record {
                    id,
                    source,
                    pid: sqlite3_column_int64(statement.0, 2) as i32,
                    status: text(statement.0, 3).unwrap_or_default(),
                    cwd: text(statement.0, 4).unwrap_or_default(),
                    model: text(statement.0, 5),
                    title,
                    messages: text(statement.0, 7),
                });
            }
        }
        out
    }
}

// MARK: Conversation

/// A tool call in the conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub input: Map<String, Value>,
    pub at: Option<Time>,
}

/// What the conversation says: its calls in order, those that have a
/// result, the last thing the person and the agent wrote, and the model
/// that answered last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub calls: Vec<Call>,
    pub finished: HashSet<String>,
    pub failed: HashSet<String>,
    pub last_user: Option<String>,
    pub last_text: Option<String>,
    pub model: Option<String>,
    /// How many messages the person sent.
    pub prompts: usize,
}

impl Transcript {
    /// The last call still without a result.
    pub fn open(&self) -> Option<&Call> {
        self.calls
            .iter()
            .rev()
            .find(|c| !self.finished.contains(&c.id))
    }
}

/// The conversation file read whole; None when it is too big or not Cline's.
pub fn transcript(path: &Path) -> Option<Transcript> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 16 << 20 {
        return None;
    }
    transcript_of(&std::fs::read(path).ok()?)
}

pub fn transcript_of(data: &[u8]) -> Option<Transcript> {
    let json: Value = serde_json::from_slice(data).ok()?;
    let messages = json.get("messages")?.as_array()?;
    let mut out = Transcript::default();
    for message in messages {
        let role = message.get("role").and_then(Value::as_str);
        let at = super::hook_agents::timestamp(message.get("ts"));
        let parts: Vec<Value> = match message.get("content") {
            Some(Value::Array(list)) => list.clone(),
            Some(Value::String(text)) => vec![json!({"type": "text", "text": text})],
            _ => Vec::new(),
        };
        match role {
            Some("assistant") => {
                if let Some(model) = message
                    .get("modelInfo")
                    .and_then(|m| m.get("id"))
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())
                {
                    out.model = Some(model.to_owned());
                }
                let texts: Vec<String> = parts
                    .iter()
                    .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .map(|t| t.trim().to_owned())
                    .filter(|t| !t.is_empty())
                    .collect();
                if !texts.is_empty() {
                    out.last_text = Some(texts.join("\n"));
                }
                for part in &parts {
                    if part.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let (Some(id), Some(name)) = (
                        part.get("id").and_then(Value::as_str),
                        part.get("name").and_then(Value::as_str),
                    ) else {
                        continue;
                    };
                    out.calls.push(Call {
                        id: id.to_owned(),
                        name: name.to_owned(),
                        input: super::hook_agents::arguments(part.get("input")),
                        at,
                    });
                }
            }
            Some("user") => {
                for part in &parts {
                    match part.get("type").and_then(Value::as_str) {
                        Some("tool_result") => {
                            if let Some(id) = part.get("tool_use_id").and_then(Value::as_str) {
                                out.finished.insert(id.to_owned());
                                if part.get("is_error").and_then(Value::as_bool) == Some(true) {
                                    out.failed.insert(id.to_owned());
                                }
                            }
                        }
                        Some("text") => {
                            if let Some(typed) = typed(part.get("text").and_then(Value::as_str)) {
                                out.last_user = Some(typed);
                                out.prompts += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// What the person typed, from Cline's `<user_input>` wrapper; None for a
/// note Cline adds itself (`<hook_context>`, reminders).
pub fn typed(text: Option<&str>) -> Option<String> {
    let text = text?.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(open) = text.find("<user_input")
        && let Some(close) = text[open..].find("</user_input>").map(|c| c + open)
        && let Some(end) = text[open..close].find('>').map(|e| e + open)
    {
        let inner = text[end + 1..close].trim();
        return (!inner.is_empty()).then(|| inner.to_owned());
    }
    (!text.starts_with('<')).then(|| text.to_owned())
}

// MARK: Tools

/// A call under the name Claude Code gives the same tool, with Claude
/// Code's arguments, so it reads and sums up alike.
pub fn claude_tool(call: &Call) -> (String, Map<String, Value>) {
    let input = &call.input;
    let text = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let map = |pairs: Vec<(&str, Value)>| -> Map<String, Value> {
        pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
    };
    match call.name.as_str() {
        "run_commands" => (
            "Bash".into(),
            map(vec![("command", json!(commands(input).join("\n")))]),
        ),
        "editor" => {
            let path = text("path");
            let new = text("new_text");
            match input.get("old_text").and_then(Value::as_str) {
                Some(old) if !old.is_empty() => (
                    "Edit".into(),
                    map(vec![
                        ("file_path", json!(path)),
                        ("old_string", json!(old)),
                        ("new_string", json!(new)),
                    ]),
                ),
                _ if input.get("insert_line").is_some_and(Value::is_number) => (
                    "Edit".into(),
                    map(vec![("file_path", json!(path)), ("new_string", json!(new))]),
                ),
                _ => (
                    "Write".into(),
                    map(vec![("file_path", json!(path)), ("content", json!(new))]),
                ),
            }
        }
        "apply_patch" => {
            let patch = text("input");
            (
                "Edit".into(),
                map(vec![
                    ("file_path", json!(patch_file(&patch).unwrap_or_default())),
                    ("patch", json!(patch)),
                ]),
            )
        }
        "read_files" => (
            "Read".into(),
            map(vec![(
                "file_path",
                json!(files(input).into_iter().next().unwrap_or_default()),
            )]),
        ),
        "search_codebase" => {
            let queries: Vec<String> = match input.get("queries") {
                Some(Value::Array(list)) => list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                Some(Value::String(one)) => vec![one.clone()],
                _ => Vec::new(),
            };
            (
                "Grep".into(),
                map(vec![("pattern", json!(queries.join(" | ")))]),
            )
        }
        "fetch_web_content" => {
            let first = input
                .get("requests")
                .and_then(Value::as_array)
                .and_then(|r| r.first());
            let field = |key: &str| {
                first
                    .and_then(|f| f.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            (
                "WebFetch".into(),
                map(vec![
                    ("url", json!(field("url"))),
                    ("prompt", json!(field("prompt"))),
                ]),
            )
        }
        "skills" => ("Skill".into(), map(vec![("skill", json!(text("skill")))])),
        QUESTION_TOOL => (Question::TOOL.into(), input.clone()),
        other => (
            super::hook_agents::name(Provider::Cline, other),
            input.clone(),
        ),
    }
}

/// The commands of a `run_commands` call, whichever form it took.
pub fn commands(input: &Map<String, Value>) -> Vec<String> {
    fn line(value: &Value) -> Option<String> {
        if let Some(text) = value.as_str() {
            return Some(text.to_owned());
        }
        let command = value.get("command")?.as_str()?;
        let args: Vec<&str> = value
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        Some(
            std::iter::once(command)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
    for key in ["commands", "command", "cmd"] {
        match input.get(key) {
            Some(Value::Array(list)) => return list.iter().filter_map(line).collect(),
            Some(one) => {
                if let Some(text) = line(one) {
                    return vec![text];
                }
            }
            None => {}
        }
    }
    Vec::new()
}

fn files(input: &Map<String, Value>) -> Vec<String> {
    for key in ["files", "file_paths", "paths"] {
        match input.get(key) {
            Some(Value::String(one)) => return vec![one.clone()],
            Some(Value::Array(list)) => {
                for item in list {
                    if let Some(text) = item.as_str() {
                        return vec![text.to_owned()];
                    }
                    if let Some(path) = item.get("path").and_then(Value::as_str) {
                        return vec![path.to_owned()];
                    }
                }
            }
            Some(object) => {
                if let Some(path) = object.get("path").and_then(Value::as_str) {
                    return vec![path.to_owned()];
                }
            }
            None => {}
        }
    }
    Vec::new()
}

/// The first file an apply_patch payload changes.
pub fn patch_file(patch: &str) -> Option<String> {
    for line in patch.lines() {
        for head in ["*** Update File:", "*** Add File:", "*** Delete File:"] {
            if let Some(path) = line.strip_prefix(head) {
                let path = path.trim();
                if !path.is_empty() {
                    return Some(path.to_owned());
                }
            }
        }
    }
    None
}

/// The call as the timeline shows it.
pub fn tool_use(call: &Call, at: Time, policy: ContentPolicy) -> ToolUse {
    let (name, input) = claude_tool(call);
    let mut tool =
        super::hook_agents::tool_use(&name, &input, call.at.map_or(at, |c| c.min(at)), policy);
    tool.id = call.id.clone();
    tool
}

// MARK: Asks

/// What Cline asks about the call: the command or change it waits to run, or
/// the question it asks. Shown whatever the previews, since the person
/// decides on it. The call's id is the request's.
pub fn ask(call: &Call, cwd: Option<&str>, at: Time) -> Option<Permission> {
    if call.name == QUESTION_TOOL {
        let text = call.input.get("question")?.as_str()?.trim();
        if text.is_empty() {
            return None;
        }
        let options: Vec<QuestionOption> = call
            .input
            .get("options")
            .and_then(Value::as_array)
            .map(|o| {
                o.iter()
                    .filter_map(Value::as_str)
                    .filter(|o| !o.is_empty())
                    .map(QuestionOption::new)
                    .collect()
            })
            .unwrap_or_default();
        let mut permission = Permission::new(
            &call.id,
            Question::TOOL,
            Some(super::models::tool_summary::clip(text)).as_deref(),
            false,
            at,
        );
        permission.questions = vec![Question {
            question: text.to_owned(),
            header: None,
            options,
            multi_select: false,
        }];
        return Some(permission);
    }
    let (tool, input) = claude_tool(call);
    let mut permission = super::hook_agents::grok_ask(&tool, &input, cwd, false, at);
    permission.request_id = call.id.clone();
    Some(permission)
}

// MARK: Keys

/// What answers the CLI's dialog: keys, then the person's own words typed
/// with a Return. Its approval takes `y` for Approve and `n` for Deny; a
/// question takes its choice's number, or arrows down to "Type a
/// response..." for an answer of the person's own.
#[derive(Clone, Debug, PartialEq)]
pub struct Keys {
    pub keys: Vec<Key>,
    pub typed: Option<String>,
}

pub fn keys(decision: &Decision, asked: &Permission) -> Option<Keys> {
    let only = |keys: Vec<Key>| Some(Keys { keys, typed: None });
    if let Some(question) = asked.questions.first() {
        return match decision {
            // Escape leaves the question with an empty answer.
            Decision::Deny => only(vec![Key::Escape]),
            Decision::Answer(answers) => {
                let given: Vec<String> = answers
                    .get(&question.question)
                    .or_else(|| {
                        (answers.len() == 1)
                            .then(|| answers.values().next())
                            .flatten()
                    })
                    .map(|a| {
                        a.iter()
                            .map(|s| s.trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                let [answer] = given.as_slice() else {
                    return None;
                };
                match question.options.iter().position(|o| &o.label == answer) {
                    Some(index) if index < 9 => only(vec![Key::Digit(index as u8 + 1)]),
                    // The person's own words go on the last row, under the choices.
                    _ => Some(Keys {
                        keys: vec![Key::Down; question.options.len()],
                        typed: Some(answer.clone()),
                    }),
                }
            }
            _ => None,
        };
    }
    match decision {
        // `y` rather than Return: a notice Cline shows over its dialog on a
        // first run opens a web page on Return, but closes on any other key.
        Decision::Allow => only(vec![Key::Letter('y')]),
        Decision::Deny => only(vec![Key::Letter('n')]),
        _ => None,
    }
}

// MARK: Watching the store

#[derive(Clone, Debug, Default)]
struct Seen {
    pid: i32,
    turn_open: bool,
    prompts: usize,
    told: HashSet<String>,
    finished: HashSet<String>,
    asked: Option<String>,
    model: Option<String>,
    title: Option<String>,
    client: Option<String>,
}

/// Turns what Cline's store says into the events its hooks would have
/// told, for the island's list of hook agents.
#[derive(Default)]
pub struct Watcher {
    seen: HashMap<String, Seen>,
    read: HashMap<String, (Option<std::time::SystemTime>, u64, Transcript)>,
}

impl Watcher {
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Forgets the titles told, so they are told again once previews are back on.
    pub fn forget_content(&mut self) {
        for seen in self.seen.values_mut() {
            seen.title = None;
        }
    }

    /// The events since the last scan. `pid` gives the process that shows a
    /// session (None while none does: a hub session whose CLI quit),
    /// `client` where it is answered.
    pub fn scan(
        &mut self,
        records: &[Record],
        now: Time,
        policy: ContentPolicy,
        pid: &dyn Fn(&Record) -> Option<i32>,
        client: &dyn Fn(&Record) -> &'static str,
        alive: &dyn Fn(i32) -> bool,
        transcript: &dyn Fn(&Path) -> Option<Transcript>,
    ) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        let mut live = HashSet::new();
        for record in records {
            let Some(shown) = pid(record).filter(|p| *p > 0 && alive(*p)) else {
                continue;
            };
            live.insert(record.id.clone());
            let fresh = !self.seen.contains_key(&record.id);
            let talk = record
                .messages
                .as_deref()
                .and_then(|m| self.reading(m, transcript))
                .unwrap_or_default();
            let mut state = self.seen.remove(&record.id).unwrap_or_default();
            state.pid = shown;
            let cwd = (!record.cwd.is_empty()).then(|| record.cwd.clone());
            let mut add = |kind: Kind| {
                events.push(AgentEvent {
                    provider: Provider::Cline,
                    session_id: record.id.clone(),
                    pid: shown,
                    cwd: cwd.clone(),
                    date: now,
                    kind,
                });
            };
            if fresh {
                add(Kind::Started);
                // What ended before the island looked isn't told again; a call still open is.
                state.told = talk
                    .calls
                    .iter()
                    .map(|c| c.id.clone())
                    .filter(|id| talk.finished.contains(id))
                    .collect();
                state.finished = talk.finished.clone();
                state.prompts = talk.prompts;
            }
            let place = client(record);
            if state.client.as_deref() != Some(place) {
                state.client = Some(place.to_owned());
                add(Kind::Client(place.to_owned()));
            }
            if let Some(model) = talk.model.clone().or_else(|| record.model.clone())
                && state.model.as_ref() != Some(&model)
            {
                state.model = Some(model.clone());
                add(Kind::Model(model));
            }
            if policy.previews
                && let Some(title) = &record.title
                && state.title.as_ref() != Some(title)
            {
                state.title = Some(title.clone());
                add(Kind::Title(title.clone()));
            }
            // A turn opens while the store says it works, or when the person
            // wrote again and the turn already ended between two readings.
            let wrote_again = talk.prompts > state.prompts;
            state.prompts = state.prompts.max(talk.prompts);
            if !state.turn_open && (record.is_busy() || wrote_again) {
                state.turn_open = true;
                add(Kind::Prompt(
                    policy
                        .previews
                        .then(|| {
                            super::models::preview(
                                talk.last_user.as_deref(),
                                super::transcript::REPLY_LIMIT,
                            )
                        })
                        .flatten(),
                ));
            }
            for call in &talk.calls {
                if state.told.insert(call.id.clone()) {
                    add(Kind::ToolStarted(tool_use(call, now, policy)));
                }
            }
            for call in &talk.calls {
                if talk.finished.contains(&call.id) && state.finished.insert(call.id.clone()) {
                    let (name, input) = claude_tool(call);
                    add(Kind::ToolFinished {
                        id: Some(call.id.clone()),
                        input_key: Some(super::models::tool_summary::input_key(&name, &input)),
                        name,
                        failed: talk.failed.contains(&call.id),
                        started: call.at,
                    });
                }
            }
            // A call waits on the person: Cline marks the session pending for
            // an approval; a question is a call that runs until answered.
            let waiting = record
                .is_busy()
                .then(|| talk.open())
                .flatten()
                .filter(|open| open.name == QUESTION_TOOL || record.waits_for_approval());
            match waiting {
                Some(call) if state.asked.as_deref() != Some(call.id.as_str()) => {
                    if let Some(asked) = ask(call, cwd.as_deref(), now) {
                        if let Some(before) = state.asked.take() {
                            add(Kind::PermissionSettled(Some(before)));
                        }
                        state.asked = Some(call.id.clone());
                        add(Kind::Permission(asked));
                    }
                }
                None => {
                    if let Some(before) = state.asked.take() {
                        add(Kind::PermissionSettled(Some(before)));
                    }
                }
                _ => {}
            }
            if !record.is_busy() && state.turn_open {
                state.turn_open = false;
                add(Kind::Stopped {
                    last_text: policy
                        .previews
                        .then(|| {
                            super::models::preview(
                                talk.last_text.as_deref(),
                                super::transcript::REPLY_LIMIT,
                            )
                        })
                        .flatten(),
                    failed: record.failed(),
                });
            }
            self.seen.insert(record.id.clone(), state);
        }
        let gone: Vec<String> = self
            .seen
            .keys()
            .filter(|id| !live.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(state) = self.seen.remove(&id) {
                events.push(AgentEvent {
                    provider: Provider::Cline,
                    session_id: id,
                    pid: state.pid,
                    cwd: None,
                    date: now,
                    kind: Kind::Ended,
                });
            }
        }
        let kept: HashSet<&str> = records
            .iter()
            .filter(|r| live.contains(&r.id))
            .filter_map(|r| r.messages.as_deref())
            .collect();
        self.read.retain(|path, _| kept.contains(path.as_str()));
        events
    }

    fn reading(
        &mut self,
        path: &str,
        transcript: &dyn Fn(&Path) -> Option<Transcript>,
    ) -> Option<Transcript> {
        let meta = std::fs::metadata(path).ok();
        let stamp = (
            meta.as_ref().and_then(|m| m.modified().ok()),
            meta.as_ref().map_or(u64::MAX, |m| m.len()),
        );
        if let Some((modified, size, known)) = self.read.get(path)
            && meta.is_some()
            && (*modified, *size) == stamp
        {
            return Some(known.clone());
        }
        match transcript(Path::new(path)) {
            Some(fresh) => {
                self.read
                    .insert(path.to_owned(), (stamp.0, stamp.1, fresh.clone()));
                Some(fresh)
            }
            None => self.read.get(path).map(|(_, _, t)| t.clone()),
        }
    }
}

/// The editor an extension session runs in, by its short name.
pub fn editor_name(host_name: Option<&str>) -> String {
    match host_name {
        Some(name) if !name.is_empty() => name.replace("Visual Studio Code", "VS Code"),
        _ => "VS Code".into(),
    }
}

// MARK: Cline's hub

/// Cline's hub: the local daemon the CLI runs its sessions in once it is up
/// (`cline --cline-hub-daemon`). It takes a WebSocket on 127.0.0.1 whose
/// token sits in Cline's data folder. Through it the island learns which
/// CLI shows a session, answers an approval (the first answer wins, the
/// CLI's own or the island's) and sends a reply. A question is the CLI's own
/// and can't be answered here.
pub mod hub {
    use std::collections::HashMap;
    use std::net::{TcpStream, ToSocketAddrs};
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use tungstenite::Message;
    use tungstenite::client::IntoClientRequest;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Discovery {
        pub url: String,
        pub token: String,
        pub pid: i32,
    }

    /// The hub that runs now, from the file it writes when it starts.
    pub fn discovery(data: &Path, alive: &dyn Fn(i32) -> bool) -> Option<Discovery> {
        let hub = data.join("locks").join("hub");
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for folder in [hub.clone(), hub.join("owners")] {
            if let Ok(entries) = std::fs::read_dir(&folder) {
                files.extend(
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e == "json")),
                );
            }
        }
        files.sort();
        files.into_iter().find_map(|file| {
            let found = parse(&std::fs::read(&file).ok()?)?;
            alive(found.pid).then_some(found)
        })
    }

    pub fn parse(data: &[u8]) -> Option<Discovery> {
        let json: Value = serde_json::from_slice(data).ok()?;
        let url = json.get("url")?.as_str()?;
        let host = url.strip_prefix("ws://")?.split('/').next()?;
        let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
        if !["127.0.0.1", "localhost", "[::1]"].contains(&name) {
            return None;
        }
        let token = json.get("authToken")?.as_str().filter(|t| !t.is_empty())?;
        let pid = json.get("pid")?.as_i64().filter(|p| *p > 0)? as i32;
        Some(Discovery {
            url: url.to_owned(),
            token: token.to_owned(),
            pid,
        })
    }

    /// Each hub session's CLI: the process of the client that took part in
    /// it last. A session with no client left was quit.
    pub fn clients(sessions: &[Value], clients: &[Value]) -> HashMap<String, i32> {
        let pids: HashMap<&str, i32> = clients
            .iter()
            .filter_map(|c| {
                let id = c.get("clientId")?.as_str()?;
                let pid = c.get("metadata")?.get("pid")?.as_i64().filter(|p| *p > 0)?;
                Some((id, pid as i32))
            })
            .collect();
        sessions
            .iter()
            .filter_map(|s| {
                let id = s.get("sessionId")?.as_str()?;
                let pid = s
                    .get("participants")?
                    .as_array()?
                    .iter()
                    .rev()
                    .filter_map(|p| p.get("clientId").and_then(Value::as_str))
                    .find_map(|c| pids.get(c).copied())?;
                Some((id.to_owned(), pid))
            })
            .collect()
    }

    /// The CLI showing each hub session, asked of the hub. Blocks a moment.
    pub fn shown(hub: &Discovery, timeout: Duration) -> Option<HashMap<String, i32>> {
        let mut socket = Socket::open(hub, timeout)?;
        let listed = socket.command("session.list", json!({}), None)?;
        let known = socket.command("client.list", json!({}), None)?;
        socket.close();
        let empty = Vec::new();
        Some(clients(
            listed
                .get("sessions")
                .and_then(Value::as_array)
                .unwrap_or(&empty),
            known
                .get("clients")
                .and_then(Value::as_array)
                .unwrap_or(&empty),
        ))
    }

    /// Answers the approval the session waits on for `call`: the hub sends a
    /// waiting approval again to a client that subscribes to its session.
    pub fn approve(
        approved: bool,
        session: &str,
        call: &str,
        hub: &Discovery,
        timeout: Duration,
    ) -> bool {
        let Some(mut socket) = Socket::open(hub, timeout) else {
            return false;
        };
        let done = socket.approve(approved, session, call);
        socket.close();
        done
    }

    /// Sends the person's reply to the session, queued after the turn under way.
    pub fn send(text: &str, session: &str, hub: &Discovery, timeout: Duration) -> bool {
        let Some(mut socket) = Socket::open(hub, timeout) else {
            return false;
        };
        let done = socket
            .command(
                "session.send_input",
                json!({"prompt": text, "delivery": "queue"}),
                Some(session),
            )
            .is_some();
        socket.close();
        done
    }

    struct Socket {
        socket: tungstenite::WebSocket<TcpStream>,
        timeout: Duration,
        client: String,
        next: u64,
    }

    impl Socket {
        fn open(hub: &Discovery, timeout: Duration) -> Option<Socket> {
            let address = hub.url.strip_prefix("ws://")?.split('/').next()?;
            let address = address.to_socket_addrs().ok()?.next()?;
            let stream = TcpStream::connect_timeout(&address, timeout).ok()?;
            stream.set_read_timeout(Some(timeout)).ok()?;
            stream.set_write_timeout(Some(timeout)).ok()?;
            let mut request = hub.url.as_str().into_client_request().ok()?;
            request.headers_mut().insert(
                "Sec-WebSocket-Protocol",
                format!("cline-hub-auth.{}", hub.token).parse().ok()?,
            );
            let (socket, _) = tungstenite::client::client(request, stream).ok()?;
            let mut socket = Socket {
                socket,
                timeout,
                client: format!(
                    "openbar-{:x}",
                    super::super::hook_agents::rand_id() & 0xffff_ffff
                ),
                next: 0,
            };
            let registration = json!({
                "clientId": socket.client, "clientType": "openbar", "displayName": "Openbar",
                "transport": "websocket", "capabilities": ["approval.respond"],
            });
            socket.command("client.register", registration, None)?;
            Some(socket)
        }

        fn frame(&mut self, deadline: Instant) -> Option<Value> {
            while Instant::now() < deadline {
                let text = match self.socket.read().ok()? {
                    Message::Text(text) => text.to_string(),
                    Message::Binary(bytes) => String::from_utf8(bytes.to_vec()).ok()?,
                    Message::Close(_) => return None,
                    _ => continue,
                };
                if let Ok(frame) = serde_json::from_str::<Value>(&text) {
                    return Some(frame);
                }
            }
            None
        }

        /// The reply's payload, past the frames between; None on a refusal or
        /// no reply in time.
        fn command(&mut self, name: &str, payload: Value, session: Option<&str>) -> Option<Value> {
            self.next += 1;
            let request = format!("{}-{}", self.client, self.next);
            let mut envelope = json!({
                "version": "v1", "command": name, "requestId": request,
                "clientId": self.client, "payload": payload,
            });
            if let Some(session) = session {
                envelope["sessionId"] = json!(session);
            }
            let frame = json!({"kind": "command", "envelope": envelope});
            self.socket.send(Message::text(frame.to_string())).ok()?;
            let deadline = Instant::now() + self.timeout;
            loop {
                let frame = self.frame(deadline)?;
                if frame.get("kind").and_then(Value::as_str) != Some("reply") {
                    continue;
                }
                let reply = frame.get("envelope")?;
                if reply.get("requestId").and_then(Value::as_str) != Some(request.as_str()) {
                    continue;
                }
                if reply.get("ok").and_then(Value::as_bool) != Some(true) {
                    return None;
                }
                return Some(reply.get("payload").cloned().unwrap_or(json!({})));
            }
        }

        fn approve(&mut self, approved: bool, session: &str, call: &str) -> bool {
            let subscribe =
                json!({"kind": "stream.subscribe", "clientId": self.client, "sessionId": session});
            if self
                .socket
                .send(Message::text(subscribe.to_string()))
                .is_err()
            {
                return false;
            }
            let deadline = Instant::now() + self.timeout;
            while let Some(frame) = self.frame(deadline) {
                let Some(envelope) = frame.get("envelope") else {
                    continue;
                };
                if frame.get("kind").and_then(Value::as_str) != Some("event")
                    || envelope.get("event").and_then(Value::as_str) != Some("approval.requested")
                {
                    continue;
                }
                let Some(payload) = envelope.get("payload") else {
                    continue;
                };
                if payload.get("sessionId").and_then(Value::as_str) != Some(session)
                    || payload.get("toolCallId").and_then(Value::as_str) != Some(call)
                {
                    continue;
                }
                let Some(approval) = payload.get("approvalId").and_then(Value::as_str) else {
                    continue;
                };
                let mut answer = json!({"approvalId": approval, "approved": approved});
                if !approved {
                    answer["reason"] = json!("Denied in Openbar");
                }
                return self.command("approval.respond", answer, None).is_some();
            }
            false
        }

        fn close(&mut self) {
            let _ = self.socket.close(None);
        }
    }
}

/// Cline's hub, when one runs.
pub fn hub(home: &Path) -> Option<hub::Discovery> {
    hub::discovery(&data_folder(home, &process_env), &|pid| {
        crate::hosts::process::is_running(pid as u32)
    })
}

#[cfg(test)]
mod tests;
