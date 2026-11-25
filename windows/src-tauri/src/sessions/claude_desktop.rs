// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Claude desktop app in Sessions (#246), as on the Mac
//! (`ClaudeDesktop.swift`). Its Code tab runs Claude Code with
//! `CLAUDE_CODE_ENTRYPOINT=claude-desktop` (`claude-desktop-3p` with
//! third-party inference), which the session's registry record carries as
//! `entrypoint`; those sessions come in through the island's hooks like a
//! terminal's. Cowork runs Claude Code inside the app's machine, where no hook
//! of the island's runs: with its own switch, the files the app keeps for
//! each session are read, never written: `local_<id>.json` and the session
//! folder's `audit.jsonl`, the session's messages as the Agent SDK streams
//! them and the app's `permission_request`/`permission_response` lines.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use super::codex::state::ANSWER_IN_TERMINAL;
use super::engine::{FINISHED_FOR, TIMELINE_LENGTH};
use super::hook_agents::QUIET_FOR;
use super::models::{
    ChatMessage, ChatRole, ContentPolicy, HostKind, Provider, Session, SessionEvent, SessionPhase,
    Time, Tokens, ToolUse, project_name, tool_summary,
};
use super::transcript::parse_iso;

pub const NAME: &str = "Claude Desktop";

/// Where a Claude session runs when not in a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// The Code tab: heard through the same hooks and registry as a terminal's.
    Desktop,
    /// Cowork, read from the app's files.
    Cowork,
}

/// The Code tab's entrypoints, signed in and third-party.
pub fn is_code_tab(entrypoint: Option<&str>) -> bool {
    entrypoint.is_some_and(|e| e == "claude-desktop" || e.starts_with("claude-desktop-"))
}

/// The app's own id for a session: Cowork's is its file's name; a Code tab
/// session's is found by its Claude Code id in the record the app keeps of
/// it, `claude-code-sessions/<account>/<org>/local_<id>.json`.
pub fn app_session_id(session: &Session, data: &[PathBuf]) -> Option<String> {
    match session.origin? {
        Origin::Cowork => session.id.strip_prefix(PREFIX).map(str::to_owned),
        Origin::Desktop => {
            let folders: Vec<PathBuf> = data
                .iter()
                .map(|f| f.join("claude-code-sessions"))
                .collect();
            session_files(&folders).into_iter().find_map(|file| {
                let bytes = fs::read(&file).ok().filter(|b| b.len() < 4 << 20)?;
                let Value::Object(json) = serde_json::from_slice::<Value>(&bytes).ok()? else {
                    return None;
                };
                (json.get("cliSessionId").and_then(Value::as_str) == Some(session.id.as_str()))
                    .then(|| {
                        json.get("sessionId")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))
                    })?
            })
        }
    }
}

/// The link that shows one session in the app (Claude Desktop 6):
/// `claude://code/needs-input?session=local_<id>` finds a Code tab or a
/// Cowork session by the app's id for it. (`claude://resume` would import a
/// Code tab session a second time.)
pub fn link(app_id: &str) -> Option<String> {
    let rest = app_id.strip_prefix("local_")?;
    ((1..=64).contains(&rest.len()) && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then(|| format!("claude://code/needs-input?session={app_id}"))
}

pub fn data_folders_here() -> Vec<PathBuf> {
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    data_folders(app_data.as_deref(), local.as_deref())
}

/// The app's data folders: `%APPDATA%\Claude` when signed in,
/// `%LOCALAPPDATA%\Claude-3p` for third-party inference, and the same two
/// inside the Store package's private folders.
pub fn data_folders(app_data: Option<&Path>, local_app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(roaming) = app_data {
        folders.push(roaming.join("Claude"));
        folders.push(roaming.join("Claude-3p"));
    }
    if let Some(local) = local_app_data {
        folders.push(local.join("Claude-3p"));
        let package = local
            .join("Packages")
            .join("Claude_pzs8sxrjxfjjc")
            .join("LocalCache");
        folders.push(package.join("Roaming").join("Claude"));
        folders.push(package.join("Local").join("Claude-3p"));
    }
    folders
}

pub fn cowork_folders() -> Vec<PathBuf> {
    data_folders_here()
        .into_iter()
        .map(|folder| folder.join("local-agent-mode-sessions"))
        .collect()
}

/// Whether a program path is the Claude app's own, not Claude Code's
/// `claude.exe`: the Store package, or the per-user install.
pub fn is_app_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase().replace('/', "\\");
    lower.ends_with("\\claude.exe")
        && (lower.contains("\\windowsapps\\claude_") || lower.contains("\\anthropicclaude\\"))
}

/// The Claude app's processes.
pub fn app_pids() -> Vec<u32> {
    crate::hosts::process::pids_named("claude.exe")
        .into_iter()
        .filter(|pid| {
            crate::hosts::process::image_path(*pid)
                .is_some_and(|path| is_app_path(&path.to_string_lossy()))
        })
        .collect()
}

pub fn is_running() -> bool {
    !app_pids().is_empty()
}

/// Brings the app forward, at the session when it takes a link to it.
pub fn open(link: Option<&str>) -> bool {
    if let Some(link) = link
        && tauri_plugin_opener::open_url(link, None::<&str>).is_ok()
    {
        return true;
    }
    raise()
}

#[cfg(windows)]
fn raise() -> bool {
    app_pids()
        .into_iter()
        .any(crate::hosts::console::raise_process_window)
}

#[cfg(not(windows))]
fn raise() -> bool {
    false
}

pub fn is_installed() -> bool {
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    data_folders(app_data.as_deref(), local.as_deref())
        .iter()
        .any(|folder| folder.is_dir())
}

// MARK: Cowork

pub const PREFIX: &str = "cowork:";
const FIRST_READ: u64 = 1 << 20;
const CHAT_LENGTH: usize = 40;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub id: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub model: Option<String>,
    pub created: Option<Time>,
    pub last_activity: Option<Time>,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub id: String,
    pub tool: String,
    pub since: Time,
}

/// What a session's log told so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogState {
    pub turn_open: bool,
    pub turn_started: Option<Time>,
    pub turn_ended: Option<Time>,
    pub completed: bool,
    pub heard: Option<Time>,
    pub model: Option<String>,
    pub tools: Vec<ToolUse>,
    pub pending: Vec<Ask>,
    pub last_text: Option<String>,
    pub chat: Vec<ChatMessage>,
    next_chat: i64,
    /// The prompt that opened the turn, until the model answers: the app
    /// logs it again as the session replays it.
    opening_prompt: Option<String>,
}

impl LogState {
    pub fn open_tool(&self) -> Option<&ToolUse> {
        self.tools.iter().rev().find(|t| t.finished.is_none())
    }

    fn purge_content(&mut self) {
        self.last_text = None;
        self.opening_prompt = None;
        self.chat.clear();
        for tool in &mut self.tools {
            tool.summary = None;
        }
    }

    fn say(&mut self, text: &str, role: ChatRole, at: Time) {
        self.chat.push(ChatMessage {
            id: self.next_chat,
            role,
            text: text.to_owned(),
            images: Vec::new(),
            at,
            ends_turn: false,
        });
        self.next_chat += 1;
        if self.chat.len() > CHAT_LENGTH {
            let extra = self.chat.len() - CHAT_LENGTH;
            self.chat.drain(..extra);
        }
    }

    fn begin(&mut self, at: Time) {
        self.turn_open = true;
        self.turn_started = Some(at);
        self.completed = false;
    }

    fn prompt(&mut self, text: &str, at: Time, previews: bool) {
        let text = text.trim();
        // Reminders and command output the app adds come wrapped in tags.
        if text.is_empty() || text.starts_with('<') {
            return;
        }
        if self.turn_open && self.opening_prompt.as_deref() == Some(text) {
            return;
        }
        self.begin(at);
        self.opening_prompt = Some(text.to_owned());
        if previews {
            self.say(text, ChatRole::User, at);
        }
    }

    /// One line of `audit.jsonl`.
    pub fn apply(&mut self, line: &[u8], previews: bool, fallback: Time) {
        let Ok(Value::Object(json)) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let at = json
            .get("_audit_timestamp")
            .and_then(Value::as_str)
            .and_then(parse_iso)
            .unwrap_or(fallback);
        self.heard = Some(self.heard.map_or(at, |h| h.max(at)));
        let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
        // A subagent's messages carry the tool call that runs it; its tools
        // are not the session's own.
        let subagent = json
            .get("parent_tool_use_id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty());
        match json.get("type").and_then(Value::as_str) {
            Some("system") => match json.get("subtype").and_then(Value::as_str) {
                Some("init") => {
                    if let Some(model) = text(json.get("model")).filter(|m| !m.is_empty()) {
                        self.model = Some(model);
                    }
                }
                Some("permission_request") => {
                    let id = text(json.get("uuid")).unwrap_or_else(|| format!("ask-{at}"));
                    if !self.pending.iter().any(|a| a.id == id) {
                        self.pending.push(Ask {
                            id,
                            tool: text(json.get("tool_name")).unwrap_or_default(),
                            since: at,
                        });
                    }
                }
                Some("permission_response") => {
                    if let Some(id) = text(json.get("uuid")) {
                        self.pending.retain(|a| a.id != id);
                    }
                }
                _ => {}
            },
            Some("user") => {
                let Some(message) = json.get("message").and_then(Value::as_object) else {
                    return;
                };
                if let Some(prompt) = message.get("content").and_then(Value::as_str) {
                    if !subagent {
                        self.prompt(prompt, at, previews);
                    }
                    return;
                }
                let blocks: Vec<&Map<String, Value>> = message
                    .get("content")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_object).collect())
                    .unwrap_or_default();
                let mut finished = false;
                for block in &blocks {
                    if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                        continue;
                    }
                    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(tool) = self.tools.iter_mut().rev().find(|t| t.id == id) {
                        tool.finished = Some(at);
                        tool.failed = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        finished = true;
                    }
                }
                if !finished && !subagent {
                    let said: Vec<&str> = blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .collect();
                    if !said.is_empty() {
                        self.prompt(&said.join("\n"), at, previews);
                    }
                }
            }
            Some("assistant") => {
                let Some(message) = json.get("message").and_then(Value::as_object) else {
                    return;
                };
                if subagent {
                    return;
                }
                if !self.turn_open {
                    self.begin(at);
                }
                self.opening_prompt = None;
                if let Some(model) =
                    text(message.get("model")).filter(|m| !m.is_empty() && m != "<synthetic>")
                {
                    self.model = Some(model);
                }
                let blocks = message
                    .get("content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for block in blocks.iter().filter_map(Value::as_object) {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") if previews => {
                            let said = block
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .trim();
                            if !said.is_empty() {
                                self.last_text = Some(said.to_owned());
                                self.say(said, ChatRole::Assistant, at);
                            }
                        }
                        Some("tool_use") => {
                            let (Some(id), Some(name)) = (
                                block.get("id").and_then(Value::as_str),
                                block.get("name").and_then(Value::as_str),
                            ) else {
                                continue;
                            };
                            if self.tools.iter().any(|t| t.id == id) {
                                continue;
                            }
                            let empty = Map::new();
                            let input = block
                                .get("input")
                                .and_then(Value::as_object)
                                .unwrap_or(&empty);
                            let summary = if previews {
                                tool_summary::summary(name, input)
                            } else {
                                None
                            };
                            self.tools.push(ToolUse::new(id, name, summary, at));
                            if self.tools.len() > 200 {
                                let extra = self.tools.len() - 200;
                                self.tools.drain(..extra);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("result") if !subagent => {
                self.turn_open = false;
                self.turn_ended = Some(at);
                self.completed = json.get("subtype").and_then(Value::as_str) == Some("success")
                    && json.get("is_error").and_then(Value::as_bool) != Some(true);
                self.pending.clear();
                for tool in &mut self.tools {
                    if tool.finished.is_none() {
                        tool.finished = Some(at);
                    }
                }
                if let Some(last) = self.chat.last_mut()
                    && last.role == ChatRole::Assistant
                {
                    last.ends_turn = true;
                }
            }
            _ => {}
        }
    }
}

pub fn phase(state: &LogState, acknowledged: Option<Time>, t: Time) -> SessionPhase {
    if let Some(ask) = state.pending.first() {
        return if ask.tool == "AskUserQuestion" {
            SessionPhase::WaitingInput
        } else {
            SessionPhase::WaitingPermission
        };
    }
    if state.turn_open && state.heard.is_some_and(|h| t - h < QUIET_FOR) {
        return if state.open_tool().is_some() {
            SessionPhase::RunningTool
        } else {
            SessionPhase::Generating
        };
    }
    if let Some(ended) = state.turn_ended
        && state.completed
        && t - ended < FINISHED_FOR
        && acknowledged.unwrap_or(f64::MIN) < ended
    {
        return SessionPhase::Finished;
    }
    SessionPhase::Idle
}

/// Milliseconds since 1970, or an ISO 8601 date.
fn date(value: Option<&Value>) -> Option<Time> {
    match value? {
        Value::Number(n) => n
            .as_f64()
            .filter(|v| *v > 0.0)
            .map(|v| if v > 1e11 { v / 1000.0 } else { v }),
        Value::String(s) => parse_iso(s),
        _ => None,
    }
}

pub fn meta(data: &[u8], id: &str) -> Option<Meta> {
    let Value::Object(json) = serde_json::from_slice::<Value>(data).ok()? else {
        return None;
    };
    let text = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let folder = json
        .get("userSelectedFolders")
        .and_then(Value::as_array)
        .and_then(|folders| {
            folders.iter().find_map(|item| {
                let path = item
                    .as_str()
                    .or_else(|| item.get("path").and_then(Value::as_str))?;
                is_absolute(path).then(|| path.to_owned())
            })
        });
    Some(Meta {
        id: id.to_owned(),
        title: text("title"),
        folder,
        model: text("model"),
        created: date(json.get("createdAt")),
        last_activity: date(json.get("lastActivityAt")),
        archived: json
            .get("isArchived")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (path.len() > 2 && path.as_bytes()[1] == b':')
}

/// Folders beside the sessions that hold none: the app's plugins and skills,
/// memory and usage.
const SKIPPED: [&str; 7] = [
    "skills-plugin",
    "plugin-cache",
    "memory",
    "usage-ledger",
    "outputs",
    "uploads",
    "uploads-tmp",
];

/// What the folder of the session `local_<uuid>.json` may be called: the
/// first part of its id (Claude Desktop 6), its whole id, or the file's name.
pub fn folder_names(file: &str) -> Vec<String> {
    let name = file.strip_suffix(".json").unwrap_or(file);
    let id = name.strip_prefix("local_").unwrap_or(name);
    let mut names = Vec::new();
    if let Some(first) = id.split('-').next() {
        names.push(first.to_owned());
    }
    names.push(id.to_owned());
    names.push(name.to_owned());
    names.retain(|n| !n.is_empty());
    names
}

/// The session's `audit.jsonl`, in whichever of its folders exists.
pub fn log_of(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or(Path::new(""));
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let candidates: Vec<PathBuf> = folder_names(&name)
        .into_iter()
        .map(|n| parent.join(n).join("audit.jsonl"))
        .collect();
    candidates
        .iter()
        .find(|c| c.is_file())
        .cloned()
        .unwrap_or_else(|| candidates.first().cloned().unwrap_or_default())
}

/// The `local_<id>.json` files under the folders. A session's own folder,
/// which holds the person's files, is never gone into.
pub fn session_files(folders: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = fs::read_dir(folder) else {
            return;
        };
        let mut names: Vec<(String, PathBuf, bool)> = entries
            .flatten()
            .map(|e| {
                let dir = e.file_type().is_ok_and(|t| t.is_dir());
                (e.file_name().to_string_lossy().into_owned(), e.path(), dir)
            })
            .collect();
        names.sort();
        let sessions: HashSet<String> = names
            .iter()
            .filter(|(name, _, dir)| !dir && name.starts_with("local_") && name.ends_with(".json"))
            .flat_map(|(name, _, _)| folder_names(name))
            .collect();
        for (name, path, dir) in names {
            if name.starts_with("local_") {
                if !dir && name.ends_with(".json") {
                    found.push(path);
                }
                continue;
            }
            if dir
                && !name.starts_with('.')
                && !SKIPPED.contains(&name.as_str())
                && !sessions.contains(&name)
                && !path.join("audit.jsonl").is_file()
            {
                walk(&path, depth + 1, found);
            }
        }
    }
    let mut found = Vec::new();
    for folder in folders {
        walk(folder, 0, &mut found);
    }
    found
}

fn modified(path: &Path) -> Option<Time> {
    let time = fs::metadata(path).ok()?.modified().ok()?;
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

struct Entry {
    meta: Meta,
    meta_stamp: Option<Time>,
    log: PathBuf,
    offset: u64,
    state: LogState,
    acknowledged: Option<Time>,
    previews: bool,
}

/// Cowork's sessions, read from the app's files.
#[derive(Default)]
pub struct Cowork {
    entries: HashMap<String, Entry>,
    phases: HashMap<String, (SessionPhase, Time)>,
}

impl Cowork {
    /// Reads what changed and lists the sessions active lately; with the
    /// app not running there are none.
    pub fn scan(
        &mut self,
        folders: &[PathBuf],
        t: Time,
        policy: ContentPolicy,
        running: bool,
    ) -> (Vec<Session>, Vec<SessionEvent>) {
        if !running {
            self.entries.clear();
            self.phases.clear();
            return (Vec::new(), Vec::new());
        }
        let previews = policy.previews;
        let mut seen = HashSet::new();
        for file in session_files(folders) {
            let Some(id) = file.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let stamp = modified(&file);
            let entry = self.entries.entry(id.clone()).or_insert_with(|| Entry {
                meta: Meta {
                    id: id.clone(),
                    ..Meta::default()
                },
                meta_stamp: None,
                log: log_of(&file),
                offset: 0,
                state: LogState::default(),
                acknowledged: None,
                previews: false,
            });
            if !entry.log.is_file() {
                entry.log = log_of(&file);
            }
            if entry.meta_stamp != stamp
                && let Ok(data) = fs::read(&file)
                && data.len() < 4 << 20
                && let Some(found) = meta(&data, &id)
            {
                entry.meta = found;
                entry.meta_stamp = stamp;
            }
        }
        self.entries.retain(|id, _| seen.contains(id));
        let mut ids: Vec<String> = self.entries.keys().cloned().collect();
        ids.sort();
        let mut sessions = Vec::new();
        let mut events = Vec::new();
        for id in ids {
            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            let log_stamp = modified(&entry.log);
            // The app rewrites a session's file now and then; the activity it
            // records, and its log, tell when it worked.
            let heard = [entry.meta.last_activity.or(entry.meta_stamp), log_stamp]
                .into_iter()
                .flatten()
                .fold(None, |a: Option<Time>, b| Some(a.map_or(b, |a| a.max(b))));
            let Some(heard) = heard.filter(|h| !entry.meta.archived && t - h < QUIET_FOR) else {
                self.phases.remove(&id);
                continue;
            };
            read(entry, previews, t);
            let phase = phase(&entry.state, entry.acknowledged, t);
            if !(phase.is_active() || phase.is_waiting() || t - heard < FINISHED_FOR) {
                self.phases.remove(&id);
                continue;
            }
            let row = format!("{PREFIX}{id}");
            let previous = self.phases.get(&id).copied();
            let since = match previous {
                Some((p, since)) if p == phase => since,
                _ => t,
            };
            self.phases.insert(id.clone(), (phase, since));
            if let Some((before, _)) = previous
                && before != phase
            {
                match phase {
                    SessionPhase::WaitingPermission => events.push(SessionEvent::NeedsPermission {
                        id: row.clone(),
                        tool: entry
                            .state
                            .pending
                            .first()
                            .map(|a| a.tool.clone())
                            .unwrap_or_default(),
                    }),
                    SessionPhase::WaitingInput => {
                        events.push(SessionEvent::NeedsInput { id: row.clone() })
                    }
                    SessionPhase::Finished if before.is_active() || before.is_waiting() => events
                        .push(SessionEvent::Finished {
                            id: row.clone(),
                            duration: entry
                                .state
                                .turn_started
                                .zip(entry.state.turn_ended)
                                .map(|(s, e)| e - s),
                        }),
                    _ => {}
                }
            }
            sessions.push(session(entry, row, phase, since, heard, previews));
        }
        (sessions, events)
    }

    pub fn acknowledge(&mut self, row: &str, t: Time) {
        if let Some(id) = row.strip_prefix(PREFIX)
            && let Some(entry) = self.entries.get_mut(id)
        {
            entry.acknowledged = Some(t);
        }
    }

    /// Drops what previews let it keep, when they are turned off.
    pub fn purge_content(&mut self) {
        for entry in self.entries.values_mut() {
            entry.state.purge_content();
            entry.meta.title = None;
            entry.meta_stamp = None;
            entry.previews = false;
        }
    }
}

fn read(entry: &mut Entry, previews: bool, t: Time) {
    if previews && !entry.previews {
        entry.offset = 0;
        entry.state = LogState::default();
    }
    entry.previews = previews;
    let Ok(mut file) = fs::File::open(&entry.log) else {
        return;
    };
    let Ok(size) = file.seek(SeekFrom::End(0)) else {
        return;
    };
    if size < entry.offset {
        entry.offset = 0;
        entry.state = LogState::default();
    }
    if size <= entry.offset {
        return;
    }
    let mut start = entry.offset;
    let mut skip_first = false;
    if start == 0 && size > FIRST_READ {
        start = size - FIRST_READ;
        skip_first = true;
    }
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }
    let mut data = Vec::new();
    if file.read_to_end(&mut data).is_err() {
        return;
    }
    // Only whole lines: one being written is read once it ends.
    let Some(last_break) = data.iter().rposition(|b| *b == b'\n') else {
        return;
    };
    let mut lines = data[..last_break]
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty());
    if skip_first {
        lines.next();
    }
    for line in lines {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        entry.state.apply(line, previews, t);
    }
    entry.offset = start + last_break as u64 + 1;
}

fn session(
    entry: &Entry,
    row: String,
    phase: SessionPhase,
    since: Time,
    heard: Time,
    previews: bool,
) -> Session {
    let state = &entry.state;
    let folder = entry.meta.folder.clone();
    let project = folder
        .as_deref()
        .map(project_name)
        .unwrap_or_else(|| NAME.to_owned());
    let mut timeline: Vec<ToolUse> = state
        .tools
        .iter()
        .rev()
        .take(TIMELINE_LENGTH)
        .rev()
        .cloned()
        .collect();
    let mut open = state.open_tool().cloned();
    if !previews {
        for tool in &mut timeline {
            tool.summary = None;
        }
        if let Some(tool) = &mut open {
            tool.summary = None;
        }
    }
    let model = state.model.clone().or_else(|| entry.meta.model.clone());
    let title = entry.meta.title.clone().filter(|_| previews);
    Session {
        id: row,
        provider: Provider::Claude,
        pid: 0,
        cwd: folder.unwrap_or_default(),
        project: project.clone(),
        name: title.clone().unwrap_or(project),
        name_source: None,
        branch: None,
        model_name: model.clone(),
        model,
        effort: None,
        edits: None,
        failed_at: None,
        started_at: entry.meta.created,
        turn_ended_at: state.turn_ended,
        turn_completed: phase == SessionPhase::Finished,
        host: HostKind::App,
        accepts_replies: false,
        host_name: Some(NAME.to_owned()),
        remote: None,
        phase,
        phase_since: since,
        turn_started: state.turn_started,
        last_active: Some(heard),
        looked_at: entry.acknowledged,
        // Cowork asks inside the app: the island shows the wait, the answer stays there.
        waiting_for: (!state.pending.is_empty()).then(|| ANSWER_IN_TERMINAL.to_owned()),
        current_tool: matches!(
            phase,
            SessionPhase::RunningTool | SessionPhase::WaitingPermission
        )
        .then_some(open)
        .flatten(),
        permission: None,
        later_asks: Vec::new(),
        waiting_diffs: Vec::new(),
        timeline,
        subagents: Vec::new(),
        tasks: None,
        tokens: Tokens::default(),
        cost: 0.0,
        title,
        last_text: state.last_text.clone().filter(|_| previews),
        hooked: false,
        chat: if previews {
            state.chat.clone()
        } else {
            Vec::new()
        },
        background_label: None,
        services: 0,
        background: None,
        limit_stop: None,
        account: None,
        account_label: None,
        account_id: None,
        auto_mode: false,
        let_through: Vec::new(),
        via: None,
        origin: Some(Origin::Cowork),
        editor: None,
        served: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const T0: Time = 1_795_090_000.0;

    fn line(mut value: Value, at: Time) -> Vec<u8> {
        let stamp = chrono_like(at);
        value["_audit_timestamp"] = json!(stamp);
        serde_json::to_vec(&value).unwrap()
    }

    /// An ISO 8601 stamp for epoch seconds, as the app writes them.
    fn chrono_like(at: Time) -> String {
        let secs = at.floor() as i64;
        let millis = ((at - at.floor()) * 1000.0).round() as i64;
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        // Civil from days (Howard Hinnant).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z",
            rem / 3600,
            (rem % 3600) / 60,
            rem % 60
        )
    }

    fn rows_for_lookup(id: &str, origin: Origin) -> Session {
        let entry = Entry {
            meta: Meta::default(),
            meta_stamp: None,
            log: PathBuf::new(),
            offset: 0,
            state: LogState::default(),
            acknowledged: None,
            previews: false,
        };
        let mut row = session(&entry, id.to_owned(), SessionPhase::Idle, 0.0, 0.0, false);
        row.origin = Some(origin);
        row
    }

    fn prompt() -> Value {
        json!({"type": "user", "message": {"role": "user", "content": "tidy the downloads folder"}, "parent_tool_use_id": null})
    }
    fn tool_call() -> Value {
        json!({"type": "assistant", "parent_tool_use_id": null, "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [
            {"type": "text", "text": "Moving the files."},
            {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {"command": "move *.pdf Documents", "description": "Move PDFs"}}]}})
    }
    fn ask() -> Value {
        json!({"type": "system", "subtype": "permission_request", "uuid": "req-1", "tool_name": "Bash"})
    }
    fn granted() -> Value {
        json!({"type": "system", "subtype": "permission_response", "uuid": "req-1", "tool_name": "Bash", "granted": true})
    }
    fn tool_result() -> Value {
        json!({"type": "user", "parent_tool_use_id": null, "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "content": "", "is_error": false}]}})
    }
    fn reply() -> Value {
        json!({"type": "assistant", "parent_tool_use_id": null, "message": {"role": "assistant", "content": [
            {"type": "text", "text": "Done: 12 PDFs moved."}]}})
    }
    fn result() -> Value {
        json!({"type": "result", "subtype": "success", "is_error": false})
    }

    #[test]
    fn code_tab_entrypoints_and_links() {
        assert!(is_code_tab(Some("claude-desktop")));
        assert!(is_code_tab(Some("claude-desktop-3p")));
        for other in [
            Some("cli"),
            Some("local-agent"),
            Some("sdk-ts"),
            Some("claude-desktopx"),
            None,
        ] {
            assert!(!is_code_tab(other), "{other:?}");
        }
        assert!(is_app_path(
            r"C:\Program Files\WindowsApps\Claude_1.2.3.0_x64__pzs8sxrjxfjjc\app\Claude.exe"
        ));
        assert!(is_app_path(
            r"C:\Users\me\AppData\Local\AnthropicClaude\app-1.2.3\claude.exe"
        ));
        assert!(!is_app_path(r"C:\Users\me\.local\bin\claude.exe"));
        assert!(!is_app_path(
            r"C:\Users\me\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\claude.exe"
        ));
        assert_eq!(
            link("local_6e026cc1-16ac-406b-b936-9a5d3079ceb0").as_deref(),
            Some("claude://code/needs-input?session=local_6e026cc1-16ac-406b-b936-9a5d3079ceb0")
        );
        assert!(link("x&y=1").is_none() && link("local_a b").is_none() && link("local_").is_none());
        assert_eq!(
            folder_names("local_2e916da8-c9eb.json"),
            ["2e916da8", "2e916da8-c9eb", "local_2e916da8-c9eb"]
        );
        let folders = data_folders(Some(Path::new("R")), Some(Path::new("L")));
        assert!(folders.contains(&Path::new("R").join("Claude")));
        assert!(folders.contains(&Path::new("L").join("Claude-3p")));
    }

    #[test]
    fn cowork_turn_from_its_log() {
        let mut state = LogState::default();
        state.apply(
            &line(
                json!({"type": "system", "subtype": "init", "model": "claude-opus-5-5"}),
                T0,
            ),
            true,
            T0,
        );
        state.apply(&line(prompt(), T0 + 1.0), true, T0);
        // The app logs the prompt again as the session replays it.
        state.apply(&line(prompt(), T0 + 1.5), true, T0);
        assert!(state.turn_open);
        assert_eq!(state.chat.len(), 1);
        assert_eq!(state.turn_started, Some(T0 + 1.0));
        assert_eq!(state.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(phase(&state, None, T0 + 2.0), SessionPhase::Generating);
        state.apply(&line(tool_call(), T0 + 2.0), true, T0);
        assert_eq!(
            state.open_tool().unwrap().summary.as_deref(),
            Some("move *.pdf Documents")
        );
        assert_eq!(phase(&state, None, T0 + 3.0), SessionPhase::RunningTool);
        state.apply(&line(ask(), T0 + 3.0), true, T0);
        state.apply(&line(ask(), T0 + 3.0), true, T0);
        assert_eq!(
            state.pending.len(),
            1,
            "the same request read twice waits once"
        );
        assert_eq!(
            phase(&state, None, T0 + 4.0),
            SessionPhase::WaitingPermission
        );
        state.apply(&line(granted(), T0 + 5.0), true, T0);
        assert_eq!(phase(&state, None, T0 + 6.0), SessionPhase::RunningTool);
        state.apply(&line(tool_result(), T0 + 7.0), true, T0);
        state.apply(&line(reply(), T0 + 8.0), true, T0);
        assert_eq!(phase(&state, None, T0 + 8.0), SessionPhase::Generating);
        state.apply(&line(result(), T0 + 9.0), true, T0);
        assert!(!state.turn_open && state.completed);
        assert_eq!(state.last_text.as_deref(), Some("Done: 12 PDFs moved."));
        assert!(state.chat.last().unwrap().ends_turn);
        let ended = T0 + 9.0;
        assert_eq!(phase(&state, None, ended + 60.0), SessionPhase::Finished);
        assert_eq!(
            phase(&state, Some(ended + 30.0), ended + 60.0),
            SessionPhase::Idle
        );
        assert_eq!(
            phase(&state, None, ended + FINISHED_FOR + 1.0),
            SessionPhase::Idle
        );

        let mut question = LogState::default();
        question.apply(&line(prompt(), T0), true, T0);
        question.apply(
            &line(json!({"type": "system", "subtype": "permission_request", "uuid": "q", "tool_name": "AskUserQuestion"}), T0 + 1.0),
            true,
            T0,
        );
        assert_eq!(phase(&question, None, T0 + 2.0), SessionPhase::WaitingInput);

        let mut hidden = LogState::default();
        for (value, at) in [
            (prompt(), 1.0),
            (tool_call(), 2.0),
            (tool_result(), 3.0),
            (reply(), 4.0),
            (result(), 5.0),
        ] {
            hidden.apply(&line(value, T0 + at), false, T0);
        }
        assert!(hidden.chat.is_empty() && hidden.last_text.is_none());
        assert!(hidden.tools.iter().all(|t| t.summary.is_none()) && hidden.completed);

        let mut stopped = LogState::default();
        stopped.apply(&line(prompt(), T0), true, T0);
        assert_eq!(
            phase(&stopped, None, T0 + QUIET_FOR + 1.0),
            SessionPhase::Idle
        );

        let mut broken = LogState::default();
        broken.apply(b"{not json", true, T0);
        broken.apply(&line(json!({"type": "user", "message": {"content": "<system-reminder>x</system-reminder>"}}), T0), true, T0);
        assert!(!broken.turn_open && broken.chat.is_empty());

        let mut sub = LogState::default();
        sub.apply(&line(prompt(), T0), true, T0);
        sub.apply(&line(result(), T0 + 1.0), true, T0);
        let mut sub_prompt = prompt();
        sub_prompt["parent_tool_use_id"] = json!("toolu_agent");
        sub.apply(&line(sub_prompt, T0 + 2.0), true, T0);
        assert!(!sub.turn_open, "a subagent's prompt opens no turn");
    }

    #[test]
    fn cowork_files_are_read_and_listed() {
        let root = std::env::temp_dir().join(format!("ob-cowork-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sessions_root = root.join("local-agent-mode-sessions");
        let org = sessions_root.join("acct").join("org");
        // Claude Desktop 6 names a session's folder after the first part of its id.
        fs::create_dir_all(org.join("aaa").join(".claude")).unwrap();
        // The person's files in a session's folder are never looked through.
        fs::create_dir_all(org.join("aaa").join("outputs")).unwrap();
        fs::write(
            org.join("aaa").join("outputs").join("local_trap.json"),
            b"{}",
        )
        .unwrap();
        fs::create_dir_all(sessions_root.join("skills-plugin").join("x")).unwrap();
        let live = super::super::service::now();
        let write_meta = |id: &str, title: &str, archived: bool, activity: Time| {
            let value = json!({"sessionId": id, "cwd": "/sessions/brave-owl",
                "userSelectedFolders": [r"C:\Users\me\Downloads"], "createdAt": (activity - 60.0) * 1000.0,
                "lastActivityAt": activity * 1000.0, "model": "claude-opus-5-5", "isArchived": archived,
                "title": title, "initialMessage": "secret"});
            fs::write(
                org.join(format!("{id}.json")),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        };
        write_meta("local_aaa", "Tidy downloads", false, live);
        write_meta("local_old", "Old one", false, live - 3.0 * 3600.0);
        write_meta("local_arch", "Archived", true, live);
        let audit = org.join("aaa").join("audit.jsonl");
        let mut body = Vec::new();
        for (index, value) in [prompt(), tool_call(), ask()].into_iter().enumerate() {
            body.extend(line(value, live + index as f64));
            body.push(b'\n');
        }
        fs::write(&audit, &body).unwrap();

        let mut found: Vec<String> = session_files(std::slice::from_ref(&sessions_root))
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        assert_eq!(
            found,
            ["local_aaa.json", "local_arch.json", "local_old.json"]
        );

        let folders = vec![sessions_root.clone()];
        let off = ContentPolicy { previews: false };
        let on = ContentPolicy { previews: true };
        let mut cowork = Cowork::default();
        let (rows, _) = cowork.scan(&folders, live + 3.0, off, true);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["cowork:local_aaa"]
        );
        let row = &rows[0];
        assert_eq!(row.origin, Some(Origin::Cowork));
        assert_eq!(row.provider, Provider::Claude);
        assert_eq!(row.phase, SessionPhase::WaitingPermission);
        assert_eq!(row.project, "Downloads");
        assert_eq!(row.name, "Downloads");
        assert!(row.title.is_none() && row.permission.is_none() && !row.accepts_replies);
        assert_eq!(row.waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        let (rows, _) = cowork.scan(&folders, live + 3.0, on, true);
        assert_eq!(rows[0].name, "Tidy downloads");
        assert!(!rows[0].chat.is_empty());

        let mut more = Vec::new();
        for (index, value) in [granted(), tool_result(), reply(), result()]
            .into_iter()
            .enumerate()
        {
            more.extend(line(value, live + 4.0 + index as f64));
            more.push(b'\n');
        }
        more.extend(br#"{"type":"assistant","partial"#);
        let mut all = fs::read(&audit).unwrap();
        all.extend(more);
        fs::write(&audit, all).unwrap();
        let (rows, events) = cowork.scan(&folders, live + 9.0, on, true);
        assert_eq!(rows[0].phase, SessionPhase::Finished);
        assert_eq!(rows[0].last_text.as_deref(), Some("Done: 12 PDFs moved."));
        assert!(
            events.iter().any(
                |e| matches!(e, SessionEvent::Finished { id, .. } if id == "cowork:local_aaa")
            )
        );
        cowork.acknowledge("cowork:local_aaa", live + 10.0);
        let (rows, _) = cowork.scan(&folders, live + 11.0, on, true);
        assert_eq!(rows[0].phase, SessionPhase::Idle);
        assert!(
            cowork.scan(&folders, live + 11.0, on, false).0.is_empty(),
            "with the app gone nothing is listed"
        );
        assert!(
            cowork
                .scan(&folders, live + FINISHED_FOR + 60.0, on, true)
                .0
                .is_empty(),
            "quiet, it leaves"
        );
        assert!(
            cowork
                .scan(&[root.join("missing")], live, on, true)
                .0
                .is_empty()
        );
        assert!(meta(b"[]", "x").is_none());

        // Open finds a Code tab session's app id in the record the app keeps.
        let data = root.join("Claude-3p");
        let records = data
            .join("claude-code-sessions")
            .join("6887b3a4")
            .join("00000000");
        fs::create_dir_all(&records).unwrap();
        fs::write(
            records.join("local_6e026cc1.json"),
            br#"{"sessionId":"local_6e026cc1","cliSessionId":"af54c630-f416","cwd":"C:\\w"}"#,
        )
        .unwrap();
        let mut code = rows_for_lookup("af54c630-f416", Origin::Desktop);
        assert_eq!(
            app_session_id(&code, std::slice::from_ref(&data)).as_deref(),
            Some("local_6e026cc1")
        );
        code.id = "0000".into();
        assert!(app_session_id(&code, std::slice::from_ref(&data)).is_none());
        let cowork_row = rows_for_lookup("cowork:local_2e916da8", Origin::Cowork);
        assert_eq!(
            app_session_id(&cowork_row, &[]).as_deref(),
            Some("local_2e916da8")
        );
        let _ = fs::remove_dir_all(&root);
    }
}
