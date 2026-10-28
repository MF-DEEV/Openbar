// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What Codex's app-server says about its threads, joined into the same
//! sessions the island shows for Claude Code (spec 07-codex-sessions §3.4,
//! §3.5, §5). Pure: messages and times go in, sessions and the answers to
//! send come out.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};

use crate::sessions::models::{
    ChatMessage, ChatRole, ContentPolicy, HostKind, Permission, Provider, Question, QuestionOption,
    Session, SessionEvent, SessionPhase, Time, Tokens, ToolUse, project_name,
};

/// How long an unread finished turn stays marked, as for Claude.
const FINISHED_FOR: f64 = 10.0 * 60.0;
const TIMELINE_LENGTH: usize = 12;
const TOOLS_KEPT: usize = 24;
const DETAIL_LIMIT: usize = 8000;

/// Item types that run something, so the session reads as running a tool.
const TOOL_ITEMS: [&str; 7] = [
    "commandExecution",
    "fileChange",
    "mcpToolCall",
    "dynamicToolCall",
    "webSearch",
    "imageGeneration",
    "collabAgentToolCall",
];

/// `waiting_for` of a session whose only pending requests the island
/// cannot answer: the person answers them in Codex's terminal.
pub const ANSWER_IN_TERMINAL: &str = "terminal";

/// What a notification did: whether anything the island keeps changed,
/// and a thread worth reading (one the island has not seen), if any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notified {
    pub changed: bool,
    pub read: Option<String>,
}

impl Notified {
    const UNCHANGED: Notified = Notified {
        changed: false,
        read: None,
    };
    const CHANGED: Notified = Notified {
        changed: true,
        read: None,
    };
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        waiting_on_approval: bool,
        waiting_on_input: bool,
    },
}

impl Status {
    pub fn from_json(value: &Value) -> Status {
        match value.get("type").and_then(Value::as_str) {
            Some("active") => {
                let flags: Vec<&str> = value
                    .get("activeFlags")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                Status::Active {
                    waiting_on_approval: flags.contains(&"waitingOnApproval"),
                    waiting_on_input: flags.contains(&"waitingOnUserInput"),
                }
            }
            Some("idle") => Status::Idle,
            Some("systemError") => Status::SystemError,
            _ => Status::NotLoaded,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Status::Active { .. })
    }
}

/// A request Codex is waiting on, as it came, with the JSON-RPC id to answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub rpc_id: Value,
    pub method: String,
    pub params: Value,
    pub since: Time,
}

impl Pending {
    /// The id the island knows it by.
    pub fn request_id(&self) -> String {
        format!("codex:{}", self.rpc_id)
    }

    fn asks(&self) -> bool {
        self.method == "item/tool/requestUserInput"
            || self.method == "mcpServer/elicitation/request"
    }

    /// Only these can be answered from the island; elicitations show and wait.
    pub fn answerable(&self) -> bool {
        matches!(
            self.method.as_str(),
            "item/commandExecution/requestApproval"
                | "item/fileChange/requestApproval"
                | "item/permissions/requestApproval"
                | "item/tool/requestUserInput"
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Thread {
    pub id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub preview: Option<String>,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub created: Time,
    pub updated: Time,
    pub status: Option<Status>,
    pub subscribed: bool,
    pub tools: Vec<ToolUse>,
    pub tokens: Tokens,
    pub turn_started: Option<Time>,
    /// The last turn's end and whether it completed.
    pub turn_ended: Option<(Time, bool)>,
    pub acknowledged: Time,
    pub pending: Vec<Pending>,
    pub phase: Option<(SessionPhase, Time)>,
    pub parent: Option<String>,
    /// Codex's own helper threads (titles and the like), never a person's session.
    pub ephemeral: bool,
    /// A read or a resume described it; until then only its status is known.
    pub described: bool,
    pub chat: Vec<(String, ChatMessage)>,
    next_message: i64,
}

#[derive(Default)]
pub struct State {
    pub threads: BTreeMap<String, Thread>,
    pub previews: bool,
}

impl Thread {
    fn message(&mut self, item: &Value, at: Time) -> bool {
        let Some(id) = str_at(item, "id") else {
            return false;
        };
        let (role, text) = match str_at(item, "type") {
            Some("agentMessage") => (
                ChatRole::Assistant,
                str_at(item, "text").unwrap_or("").to_string(),
            ),
            Some("userMessage") => (
                ChatRole::User,
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|c| str_at(c, "type") == Some("text"))
                    .filter_map(|c| str_at(c, "text"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => return false,
        };
        let images: Vec<String> = item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| match str_at(c, "type") {
                Some("localImage") => str_at(c, "path"),
                Some("image") => str_at(c, "url"),
                _ => None,
            })
            .take(16)
            .map(str::to_owned)
            .collect();
        if text.trim().is_empty() && images.is_empty() {
            return false;
        }
        let text = clip(&text, 16000);
        let ends_turn =
            role == ChatRole::Assistant && str_at(item, "phase") == Some("final_answer");
        if let Some((_, message)) = self.chat.iter_mut().find(|(key, _)| key == id) {
            if message.text == text && message.images == images && message.ends_turn == ends_turn {
                return false;
            }
            message.text = text;
            message.images = images;
            message.ends_turn = ends_turn;
        } else {
            self.next_message += 1;
            self.chat.push((
                id.to_string(),
                ChatMessage {
                    id: self.next_message,
                    role,
                    text,
                    images,
                    at,
                    ends_turn,
                },
            ));
            if self.chat.len() > 100 {
                self.chat.remove(0);
            }
        }
        true
    }
}

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn time_at(value: &Value, key: &str) -> Option<Time> {
    let v = value.get(key)?;
    // Seconds or milliseconds since the epoch, whichever Codex sent.
    let n = v.as_f64()?;
    Some(if n > 1e11 { n / 1000.0 } else { n })
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit).collect();
    format!("{kept}\n… {} more characters", text.chars().count() - limit)
}

impl State {
    // MARK: Rows

    /// The thread whose row shows `id`: itself, or for a subagent's thread
    /// the topmost known thread above it.
    pub fn row_of(&self, id: &str) -> String {
        let mut current = id.to_string();
        let mut seen: HashSet<String> = HashSet::from([current.clone()]);
        while let Some(parent) = self.threads.get(&current).and_then(|t| t.parent.clone()) {
            if !self.threads.contains_key(&parent) || !seen.insert(parent.clone()) {
                break;
            }
            current = parent;
        }
        current
    }

    /// `id` is `session` or one of its subagents' threads, however deep.
    pub fn belongs(&self, id: &str, session: &str) -> bool {
        let mut current = Some(id.to_string());
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(thread) = current {
            if !seen.insert(thread.clone()) {
                break;
            }
            if thread == session {
                return true;
            }
            current = self.threads.get(&thread).and_then(|t| t.parent.clone());
        }
        false
    }

    /// Every request waiting on `session`'s row: its own and its subagents',
    /// oldest first.
    pub fn pending(&self, session: &str) -> Vec<Pending> {
        ordered(
            self.threads
                .values()
                .filter(|t| !t.pending.is_empty() && self.belongs(&t.id, session))
                .flat_map(|t| t.pending.iter().cloned())
                .collect(),
        )
    }

    /// The answerable request with this island id on `session`'s row, and
    /// the thread that holds it.
    pub fn answerable(&self, session: &str, request_id: &str) -> Option<(String, Pending)> {
        self.threads
            .values()
            .filter(|t| self.belongs(&t.id, session))
            .find_map(|t| {
                t.pending
                    .iter()
                    .find(|p| p.request_id() == request_id && p.answerable())
                    .map(|p| (t.id.clone(), p.clone()))
            })
    }

    /// Removes a pending request the island answered, returning it; it may
    /// belong to one of the session's subagents.
    pub fn take_pending(&mut self, session: &str, request_id: &str) -> Option<Pending> {
        let (holder, _) = self.answerable(session, request_id)?;
        let thread = self.threads.get_mut(&holder)?;
        let index = thread
            .pending
            .iter()
            .position(|p| p.request_id() == request_id && p.answerable())?;
        Some(thread.pending.remove(index))
    }

    // MARK: Input

    /// `thread/read`'s thread, or a thread from `thread/list`. A thread read
    /// back as not loaded is gone, as `thread/status/changed` says it.
    pub fn upsert_thread(&mut self, thread: &Value) {
        let Some(id) = str_at(thread, "id") else {
            return;
        };
        if thread
            .get("status")
            .is_some_and(|status| str_at(status, "type") == Some("notLoaded"))
        {
            self.threads.remove(id);
            return;
        }
        let entry = self
            .threads
            .entry(id.to_string())
            .or_insert_with(|| Thread {
                id: id.to_string(),
                ..Default::default()
            });
        entry.described = true;
        if let Some(cwd) = str_at(thread, "cwd") {
            entry.cwd = cwd.to_string();
        }
        entry.name = str_at(thread, "name")
            .map(str::to_owned)
            .or(entry.name.take());
        entry.preview = str_at(thread, "preview")
            .map(str::to_owned)
            .or(entry.preview.take());
        entry.model = str_at(thread, "model")
            .map(str::to_owned)
            .or(entry.model.take());
        entry.branch = thread
            .get("gitInfo")
            .and_then(|g| str_at(g, "branch"))
            .map(str::to_owned)
            .or(entry.branch.take());
        entry.created = time_at(thread, "createdAt").unwrap_or(entry.created);
        entry.updated = time_at(thread, "updatedAt").unwrap_or(entry.updated);
        entry.parent = str_at(thread, "parentThreadId")
            .map(str::to_owned)
            .or(entry.parent.take());
        if let Some(ephemeral) = thread.get("ephemeral").and_then(Value::as_bool) {
            entry.ephemeral = ephemeral;
        }
        if let Some(status) = thread.get("status") {
            entry.status = Some(Status::from_json(status));
        }
        if self.previews
            && let Some(turns) = thread.get("turns").and_then(Value::as_array)
        {
            // History can omit older items after compaction. Use shared items
            // as anchors so omitted messages keep their place instead of being
            // appended after the person's newest message. Keep UI ids stable.
            let live = entry.chat.clone();
            let mut order = Vec::new();
            let mut seen = HashSet::new();
            for turn in turns {
                let at = time_at(turn, "startedAt").unwrap_or(entry.updated);
                for item in turn
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(id) = str_at(item, "id")
                        && seen.insert(id.to_string())
                    {
                        order.push(id.to_string());
                    }
                    entry.message(item, at);
                }
            }
            let mut merged: Vec<_> = order
                .iter()
                .filter_map(|id| entry.chat.iter().find(|(key, _)| key == id).cloned())
                .collect();
            let has_shared = live.iter().any(|(key, _)| seen.contains(key));
            let mut after = None;
            for (index, message) in live.iter().enumerate() {
                if let Some(position) = merged.iter().position(|(key, _)| key == &message.0) {
                    after = Some(position + 1);
                    continue;
                }
                let before = live[index + 1..]
                    .iter()
                    .find_map(|(key, _)| merged.iter().position(|(id, _)| id == key));
                let position = before
                    .or(if has_shared { after } else { None })
                    .unwrap_or_else(|| {
                        merged
                            .iter()
                            .position(|(_, m)| m.at > message.1.at)
                            .unwrap_or(merged.len())
                    });
                merged.insert(position, message.clone());
                after = Some(position + 1);
            }
            entry.chat = merged;
            if entry.chat.len() > 100 {
                entry.chat.drain(..entry.chat.len() - 100);
            }
        }
    }

    /// A notification from the daemon: what it changed, and a thread worth
    /// reading. Streamed deltas and methods the island does not follow
    /// change nothing.
    pub fn notify(&mut self, method: &str, params: &Value, now: Time) -> Notified {
        let thread_id = str_at(params, "threadId").map(str::to_owned);
        self.apply(method, params, thread_id, now)
            .unwrap_or(Notified::UNCHANGED)
    }

    /// `notify`, where None changes nothing.
    fn apply(
        &mut self,
        method: &str,
        params: &Value,
        thread_id: Option<String>,
        now: Time,
    ) -> Option<Notified> {
        match method {
            "thread/status/changed" => {
                let id = thread_id?;
                let status = Status::from_json(params.get("status").unwrap_or(&Value::Null));
                let known = self.threads.contains_key(&id);
                if status == Status::NotLoaded {
                    self.threads.remove(&id);
                    return known.then_some(Notified::CHANGED);
                }
                let thread = self.threads.entry(id.clone()).or_insert_with(|| Thread {
                    id: id.clone(),
                    ..Default::default()
                });
                let before = (thread.status.clone(), thread.turn_started);
                // A new turn starts now; turn/started, if it comes, has the exact time.
                if status.is_active() && !thread.status.as_ref().is_some_and(Status::is_active) {
                    thread.turn_started = Some(now);
                }
                thread.status = Some(status);
                let changed =
                    !known || before.0 != thread.status || before.1 != thread.turn_started;
                Some(Notified {
                    changed,
                    read: (!known).then_some(id),
                })
            }
            "thread/closed" => {
                self.threads.remove(&thread_id?)?;
                Some(Notified::CHANGED)
            }
            "thread/name/updated" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                // A null or empty name clears it.
                let name = str_at(params, "threadName").map(str::to_owned);
                if thread.name == name {
                    return None;
                }
                thread.name = name;
                Some(Notified::CHANGED)
            }
            "turn/started" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let turn = params.get("turn").unwrap_or(params);
                let started = Some(time_at(turn, "startedAt").unwrap_or(now));
                if thread.turn_started == started {
                    return None;
                }
                thread.turn_started = started;
                Some(Notified::CHANGED)
            }
            "turn/completed" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let turn = params.get("turn").unwrap_or(params);
                let completed =
                    matches!(str_at(turn, "status"), Some("completed") | Some("failed"));
                let at = time_at(turn, "completedAt").unwrap_or(now);
                thread.turn_ended = Some((at, completed));
                // A turn that ends takes its questions with it.
                thread.pending.clear();
                for tool in thread.tools.iter_mut().filter(|t| t.finished.is_none()) {
                    tool.finished = Some(at);
                }
                Some(Notified::CHANGED)
            }
            "item/started" | "item/completed" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let item = params.get("item")?;
                let kind = str_at(item, "type")?;
                if self.previews && matches!(kind, "agentMessage" | "userMessage") {
                    return thread.message(item, now).then_some(Notified::CHANGED);
                }
                if !TOOL_ITEMS.contains(&kind) {
                    return None;
                }
                let id = str_at(item, "id").unwrap_or("").to_string();
                if method == "item/started" {
                    let started = time_at(params, "startedAtMs").unwrap_or(now);
                    thread.tools.push(ToolUse {
                        id,
                        name: tool_name(kind).into(),
                        summary: tool_summary(kind, item),
                        started,
                        finished: None,
                        failed: false,
                        input_key: None,
                    });
                    if thread.tools.len() > TOOLS_KEPT {
                        let drop = thread.tools.len() - TIMELINE_LENGTH;
                        thread.tools.drain(..drop);
                    }
                } else {
                    let tool = thread.tools.iter_mut().rev().find(|t| t.id == id)?;
                    tool.finished = Some(now.max(tool.started));
                    tool.failed =
                        matches!(str_at(item, "status"), Some("failed") | Some("declined"));
                }
                Some(Notified::CHANGED)
            }
            "thread/tokenUsage/updated" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let total = params.get("tokenUsage").and_then(|u| u.get("total"))?;
                let n = |k: &str| total.get(k).and_then(Value::as_i64).unwrap_or(0);
                let cached = n("cachedInputTokens");
                let write = n("cacheWriteInputTokens");
                // Codex counts cache reads and writes inside its input.
                let tokens = Tokens {
                    input: (n("inputTokens") - cached - write).max(0),
                    cache_write: write,
                    cache_read: cached,
                    output: n("outputTokens"),
                    reasoning: n("reasoningOutputTokens"),
                };
                if thread.tokens == tokens {
                    return None;
                }
                thread.tokens = tokens;
                Some(Notified::CHANGED)
            }
            "serverRequest/resolved" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let request = params.get("requestId")?;
                let before = thread.pending.len();
                thread.pending.retain(|p| &p.rpc_id != request);
                (thread.pending.len() != before).then_some(Notified::CHANGED)
            }
            _ => None,
        }
    }

    /// A request from the daemon. True when the island keeps it, which
    /// changes what it shows; requests it does not understand are left to
    /// the terminal and never answered.
    pub fn request(&mut self, rpc_id: Value, method: &str, params: &Value, now: Time) -> bool {
        let known = matches!(
            method,
            "item/commandExecution/requestApproval"
                | "item/fileChange/requestApproval"
                | "item/permissions/requestApproval"
                | "item/tool/requestUserInput"
                | "mcpServer/elicitation/request"
        );
        let Some(id) = str_at(params, "threadId") else {
            return false;
        };
        let Some(thread) = self.threads.get_mut(id).filter(|_| known) else {
            return false;
        };
        thread.pending.retain(|p| p.rpc_id != rpc_id);
        thread.pending.push(Pending {
            rpc_id,
            method: method.to_string(),
            params: params.clone(),
            since: now,
        });
        true
    }

    pub fn acknowledge(&mut self, id: &str, now: Time) {
        if let Some(thread) = self.threads.get_mut(id) {
            thread.acknowledged = now;
        }
    }

    /// The sessions to show, and the changes worth a notice since last time.
    pub fn snapshot(
        &mut self,
        now: Time,
        policy: ContentPolicy,
    ) -> (Vec<Session>, Vec<SessionEvent>) {
        let mut sessions = Vec::new();
        let mut events = Vec::new();
        // A subagent's requests wait on its parent's row, which answers them.
        let mut waiting: HashMap<String, Vec<Pending>> = HashMap::new();
        for thread in self.threads.values().filter(|t| !t.pending.is_empty()) {
            waiting
                .entry(self.row_of(&thread.id))
                .or_default()
                .extend(thread.pending.iter().cloned());
        }
        for thread in self.threads.values_mut() {
            // A subagent's thread belongs to its parent's row.
            if thread.parent.is_some() || thread.ephemeral || thread.cwd.is_empty() {
                continue;
            }
            let pending = ordered(waiting.remove(&thread.id).unwrap_or_default());
            let phase = phase_with(thread, &pending, now);
            let since = match thread.phase {
                Some((previous, since)) if previous == phase => since,
                previous => {
                    if let Some((previous, previous_since)) = previous {
                        events.extend(event(thread, previous, previous_since, phase));
                    }
                    now
                }
            };
            thread.phase = Some((phase, since));
            sessions.push(session(thread, &pending, phase, since, policy));
        }
        (sessions, events)
    }
}

/// Oldest first; requests of the same moment keep their order.
pub fn ordered(mut pending: Vec<Pending>) -> Vec<Pending> {
    // A stable sort keeps arrival order among equal times.
    pending.sort_by(|a, b| a.since.total_cmp(&b.since));
    pending
}

fn tool_name(kind: &str) -> &'static str {
    match kind {
        "commandExecution" => "Command",
        "fileChange" => "Edit",
        "mcpToolCall" => "MCP",
        "dynamicToolCall" => "Tool",
        "webSearch" => "Search",
        "imageGeneration" => "Image",
        _ => "Agent",
    }
}

/// What the tool works on; kept only with previews (the session strips it otherwise).
fn tool_summary(kind: &str, item: &Value) -> Option<String> {
    let text = match kind {
        "commandExecution" => str_at(item, "command").map(readable_command),
        "fileChange" => item.get("changes").and_then(Value::as_array).and_then(|c| {
            let names: Vec<String> = c
                .iter()
                .filter_map(|ch| str_at(ch, "path"))
                .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string())
                .collect();
            (!names.is_empty()).then(|| names.join(", "))
        }),
        "mcpToolCall" => match (str_at(item, "server"), str_at(item, "tool")) {
            (Some(s), Some(t)) => Some(format!("{s}.{t}")),
            _ => None,
        },
        "webSearch" => str_at(item, "query").map(str::to_owned),
        _ => None,
    }?;
    let line = text.lines().next().unwrap_or("").trim().to_string();
    Some(if line.chars().count() > 80 {
        format!("{}…", line.chars().take(79).collect::<String>())
    } else {
        line
    })
}

/// Phase rules in order (spec 07 §5.1), on the thread's own requests.
pub fn phase(thread: &Thread, now: Time) -> SessionPhase {
    phase_with(thread, &thread.pending, now)
}

/// Phase rules in order, where `pending` is what waits on the thread's row,
/// its subagents' requests too.
pub fn phase_with(thread: &Thread, pending: &[Pending], now: Time) -> SessionPhase {
    if pending.iter().any(|p| !p.asks()) {
        return SessionPhase::WaitingPermission;
    }
    if pending.iter().any(Pending::asks) {
        return SessionPhase::WaitingInput;
    }
    match &thread.status {
        Some(Status::Active {
            waiting_on_approval: true,
            ..
        }) => SessionPhase::WaitingPermission,
        Some(Status::Active {
            waiting_on_input: true,
            ..
        }) => SessionPhase::WaitingInput,
        Some(Status::Active { .. }) => {
            if thread.tools.iter().any(|t| t.finished.is_none()) {
                SessionPhase::RunningTool
            } else {
                SessionPhase::Generating
            }
        }
        _ => match thread.turn_ended {
            Some((at, true)) if now - at < FINISHED_FOR && thread.acknowledged < at => {
                SessionPhase::Finished
            }
            _ => SessionPhase::Idle,
        },
    }
}

fn event(
    thread: &Thread,
    previous: SessionPhase,
    previous_since: Time,
    phase: SessionPhase,
) -> Option<SessionEvent> {
    match phase {
        SessionPhase::WaitingPermission => Some(SessionEvent::NeedsPermission {
            id: thread.id.clone(),
            tool: "Codex".into(),
        }),
        SessionPhase::WaitingInput => Some(SessionEvent::NeedsInput {
            id: thread.id.clone(),
        }),
        SessionPhase::Finished if previous.is_active() || previous.is_waiting() => {
            let (ended, _) = thread.turn_ended?;
            (ended >= previous_since).then(|| SessionEvent::Finished {
                id: thread.id.clone(),
                duration: thread.turn_started.map(|s| ended - s),
            })
        }
        _ => None,
    }
}

/// The command a person would recognise: Codex runs shell commands as
/// `"…\powershell.exe" -Command '<command>'` (or `pwsh`, `cmd /c`), and its
/// own terminal shows only the inner command, so the island does too.
pub fn readable_command(command: &str) -> String {
    let trimmed = command.trim();
    let lower = trimmed.to_ascii_lowercase();
    for shell in [
        "powershell.exe\"",
        "powershell\"",
        "pwsh.exe\"",
        "pwsh\"",
        "powershell.exe",
        "pwsh.exe",
    ] {
        let Some(at) = lower.find(shell) else {
            continue;
        };
        let rest = trimmed[at + shell.len()..].trim_start();
        let rest = rest
            .strip_prefix("-NoProfile")
            .map(str::trim_start)
            .unwrap_or(rest);
        let Some(inner) = rest.strip_prefix("-Command").map(str::trim) else {
            continue;
        };
        if let Some(quoted) = inner.strip_prefix('\'').and_then(|i| i.strip_suffix('\'')) {
            return quoted.replace("''", "'");
        }
        if let Some(quoted) = inner.strip_prefix('"').and_then(|i| i.strip_suffix('"')) {
            return quoted.to_string();
        }
        return inner.to_string();
    }
    for prefix in ["cmd.exe /c ", "cmd /c "] {
        if let Some(at) = lower.find(prefix) {
            return trimmed[at + prefix.len()..].trim().to_string();
        }
    }
    trimmed.to_string()
}

/// The card the island shows for the oldest request Codex waits on.
fn permission(pending: &Pending) -> Permission {
    let p = &pending.params;
    let (tool, summary, detail, always) = match pending.method.as_str() {
        "item/commandExecution/requestApproval" => {
            let command = readable_command(str_at(p, "command").unwrap_or(""));
            let command = command.as_str();
            let mut detail = command.to_string();
            if let Some(cwd) = str_at(p, "cwd") {
                detail.push_str(&format!("\n\nin {cwd}"));
            }
            if let Some(reason) = str_at(p, "reason") {
                detail.push_str(&format!("\n\n{reason}"));
            }
            (
                "Command",
                Some(command.lines().next().unwrap_or("").to_string()),
                detail,
                true,
            )
        }
        "item/fileChange/requestApproval" => {
            let reason = str_at(p, "reason").unwrap_or("Codex wants to change files.");
            ("Edit", None, reason.to_string(), true)
        }
        "item/permissions/requestApproval" => {
            let reason = str_at(p, "reason").unwrap_or("Codex asks for more access.");
            let asked = p
                .get("permissions")
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                .unwrap_or_default();
            ("Permissions", None, format!("{reason}\n\n{asked}"), true)
        }
        "mcpServer/elicitation/request" => {
            let server = str_at(p, "serverName").unwrap_or("MCP");
            let message = p
                .get("mode")
                .and_then(|m| str_at(m, "message"))
                .or(str_at(p, "message"))
                .unwrap_or("");
            (server, None, message.to_string(), false)
        }
        _ => ("Codex", None, String::new(), false),
    };
    let mut card = Permission::new(
        &pending.request_id(),
        tool,
        summary.as_deref(),
        always,
        pending.since,
    );
    card.detail = clip(detail.trim(), DETAIL_LIMIT);
    if pending.method == "item/tool/requestUserInput" {
        card.questions = questions(p).into_iter().map(|(_, q)| q).collect();
    }
    card
}

/// Codex's questions with their own ids, which the answer is keyed by.
pub fn questions(params: &Value) -> Vec<(String, Question)> {
    params
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let id = str_at(q, "id")?.to_string();
            let text = str_at(q, "question")?.to_string();
            let options = q
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|o| {
                    let label = o
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| str_at(o, "label").map(str::to_owned))?;
                    Some(QuestionOption {
                        label,
                        description: str_at(o, "description").map(str::to_owned),
                        preview: None,
                    })
                })
                .collect();
            Some((
                id,
                Question {
                    question: text,
                    header: str_at(q, "header").map(str::to_owned),
                    options,
                    multi_select: false,
                },
            ))
        })
        .collect()
}

fn session(
    thread: &Thread,
    pending: &[Pending],
    phase: SessionPhase,
    since: Time,
    policy: ContentPolicy,
) -> Session {
    let previews = policy.previews;
    let mut timeline: Vec<ToolUse> = thread
        .tools
        .iter()
        .rev()
        .take(TIMELINE_LENGTH)
        .rev()
        .cloned()
        .collect();
    if !previews {
        for tool in &mut timeline {
            tool.summary = None;
        }
    }
    let current = timeline
        .iter()
        .rev()
        .find(|t| t.finished.is_none())
        .cloned();
    // The card is the oldest request the island can answer. When only ones
    // it cannot answer wait, the person answers in the terminal.
    let card = pending.iter().find(|p| p.answerable()).map(permission);
    let in_terminal = card.is_none() && !pending.is_empty();
    // Codex names a thread from its first prompt unless the person renames
    // it, and the island cannot tell which, so the name is the
    // conversation's and shows only with previews.
    let name = thread.name.clone().filter(|_| previews);
    let project = project_name(&thread.cwd);
    Session {
        id: thread.id.clone(),
        provider: Provider::Codex,
        pid: 0,
        cwd: thread.cwd.clone(),
        project: project.clone(),
        name_source: name.as_ref().map(|_| "user".to_string()),
        name: name.unwrap_or(project),
        branch: thread.branch.clone(),
        model: thread.model.clone(),
        model_name: thread.model.clone(),
        // Its terminal is found when Open is pressed (no pid ties it to one).
        host: HostKind::App,
        // Replies go through Codex's own server, whatever terminal shows the session.
        accepts_replies: true,
        phase,
        phase_since: since,
        turn_started: thread.turn_started,
        last_active: Some(
            thread
                .updated
                .max(thread.turn_ended.map_or(0.0, |(t, _)| t)),
        ),
        looked_at: (thread.acknowledged > 0.0).then_some(thread.acknowledged),
        waiting_for: in_terminal.then(|| ANSWER_IN_TERMINAL.to_string()),
        current_tool: matches!(
            phase,
            SessionPhase::RunningTool | SessionPhase::WaitingPermission
        )
        .then_some(current)
        .flatten(),
        permission: card,
        timeline,
        subagents: Vec::new(),
        tasks: None,
        tokens: thread.tokens,
        cost: 0.0,
        title: if previews {
            thread.preview.clone()
        } else {
            None
        },
        last_text: thread
            .chat
            .iter()
            .rev()
            .find(|(_, m)| m.role == ChatRole::Assistant)
            .filter(|_| previews)
            .map(|(_, m)| m.text.clone()),
        hooked: true,
        chat: if previews {
            thread.chat.iter().map(|(_, m)| m.clone()).collect()
        } else {
            Vec::new()
        },
        background_label: None,
        services: 0,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Allow,
    AllowForSession,
    Deny,
    Questions(HashMap<String, Vec<String>>),
}

/// The JSON-RPC result that answers `pending`, or None when it cannot be answered that way.
pub fn result(pending: &Pending, answer: &Answer) -> Option<Value> {
    match (pending.method.as_str(), answer) {
        (
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval",
            Answer::Allow,
        ) => Some(json!({"decision": "accept"})),
        (
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval",
            Answer::AllowForSession,
        ) => Some(json!({"decision": "acceptForSession"})),
        // Decline keeps the turn going; the terminal's own n/Esc would stop it.
        (
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval",
            Answer::Deny,
        ) => Some(json!({"decision": "decline"})),
        ("item/permissions/requestApproval", Answer::Allow | Answer::AllowForSession) => {
            let scope = if *answer == Answer::Allow {
                "turn"
            } else {
                "session"
            };
            let asked = pending
                .params
                .get("permissions")
                .cloned()
                .unwrap_or(json!({}));
            Some(json!({"permissions": asked, "scope": scope}))
        }
        ("item/permissions/requestApproval", Answer::Deny) => Some(json!({"permissions": {}})),
        ("item/tool/requestUserInput", Answer::Questions(by_text)) => {
            let mut answers = serde_json::Map::new();
            for (id, question) in questions(&pending.params) {
                if let Some(picked) = by_text.get(&question.question) {
                    answers.insert(id, json!({"answers": picked}));
                }
            }
            Some(json!({"answers": answers}))
        }
        ("item/tool/requestUserInput", Answer::Deny) => Some(json!({"answers": {}})),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_thread() -> State {
        let mut state = State::default();
        state.upsert_thread(&json!({
            "id": "t1", "cwd": "C:\\Users\\vors\\Projekty\\checkout-api", "model": "gpt-5.5-codex",
            "gitInfo": {"branch": "feat/x"}, "createdAt": 100, "updatedAt": 200, "status": {"type": "idle"}
        }));
        state
    }

    #[test]
    fn image_only_messages_survive_history_and_updates() {
        let mut state = state_with_thread();
        state.previews = true;
        let item = json!({"type": "userMessage", "id": "image", "content": [
            {"type": "localImage", "path": "C:\\photo.png"},
            {"type": "image", "url": "data:image/png;base64,aGVsbG8="}
        ]});
        let event = json!({"threadId": "t1", "item": item});
        assert!(state.notify("item/completed", &event, 210.0).changed);
        assert!(!state.notify("item/completed", &event, 211.0).changed);
        let (sessions, _) = state.snapshot(212.0, ContentPolicy { previews: true });
        assert_eq!(sessions[0].chat.len(), 1);
        assert_eq!(sessions[0].chat[0].images.len(), 2);
        assert_eq!(sessions[0].chat[0].images[0], "C:\\photo.png");
        let (hidden, _) = state.snapshot(212.0, ContentPolicy { previews: false });
        assert!(hidden[0].chat.is_empty());
    }

    #[test]
    fn conversation_history_and_live_items_merge_without_duplicates() {
        let mut state = state_with_thread();
        state.previews = true;
        let answer =
            json!({"type": "agentMessage", "id": "a", "text": "Working", "phase": "commentary"});
        let event = json!({"threadId": "t1", "item": answer});
        assert!(state.notify("item/started", &event, 210.0).changed);
        assert!(!state.notify("item/completed", &event, 211.0).changed);
        state.upsert_thread(&json!({"id": "t1", "turns": [{"startedAt": 205, "items": [
            {"type": "userMessage", "id": "u", "content": [{"type": "text", "text": "Hello"}]}, answer
        ]}]}));
        let final_item = json!({"threadId": "t1", "item": {
            "type": "agentMessage", "id": "final", "text": "Done", "phase": "final_answer"
        }});
        state.notify("item/completed", &final_item, 220.0);
        let (sessions, _) = state.snapshot(221.0, ContentPolicy { previews: true });
        let s = &sessions[0];
        assert_eq!(
            s.chat.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["Hello", "Working", "Done"]
        );
        assert_eq!(s.chat[0].role, ChatRole::User);
        assert!(s.chat[2].ends_turn);
        assert_eq!(s.last_text.as_deref(), Some("Done"));
        let (hidden, _) = state.snapshot(221.0, ContentPolicy { previews: false });
        assert!(hidden[0].chat.is_empty() && hidden[0].last_text.is_none());
    }

    #[test]
    fn omitted_old_messages_stay_before_new_history_and_keep_their_ids() {
        let mut state = state_with_thread();
        state.previews = true;
        let old = json!({"type":"userMessage", "id":"old", "content":[{"type":"text", "text":"Older question"}]});
        let answer = json!({"type":"agentMessage", "id":"answer", "text":"Older answer", "phase":"final_answer"});
        let latest = json!({"type":"userMessage", "id":"latest", "content":[{"type":"text", "text":"Newest question"}]});
        for item in [&old, &answer] {
            state.notify(
                "item/completed",
                &json!({"threadId":"t1", "item":item}),
                210.0,
            );
        }
        let answer_id = state.threads["t1"].chat[1].1.id;
        let history = json!({"id":"t1", "turns":[{"startedAt":210, "items":[answer, latest]}]});
        state.upsert_thread(&history);
        state.upsert_thread(&history);
        let chat = &state.threads["t1"].chat;
        assert_eq!(
            chat.iter()
                .map(|(_, m)| m.text.as_str())
                .collect::<Vec<_>>(),
            ["Older question", "Older answer", "Newest question"]
        );
        assert_eq!(chat[1].1.id, answer_id);
        state.notify(
            "item/completed",
            &json!({"threadId":"t1", "item":old}),
            240.0,
        );
        assert_eq!(
            state.threads["t1"].chat.last().unwrap().1.text,
            "Newest question"
        );
    }

    #[test]
    fn omitted_old_suffix_stays_before_new_history_while_racing_live_messages_stay_last() {
        let mut state = state_with_thread();
        state.previews = true;
        let anchor = json!({"type":"agentMessage", "id":"anchor", "text":"Anchor"});
        let old = json!({"type":"userMessage", "id":"old", "content":[{"type":"text", "text":"Older question"}]});
        for item in [&anchor, &old] {
            state.notify(
                "item/completed",
                &json!({"threadId":"t1", "item":item}),
                210.0,
            );
        }
        state.upsert_thread(&json!({"id":"t1", "turns":[{"startedAt":210, "items":[
            anchor, {"type":"userMessage", "id":"latest", "content":[{"type":"text", "text":"Newest question"}]}
        ]}]}));
        assert_eq!(
            state.threads["t1"]
                .chat
                .iter()
                .map(|(_, m)| m.text.as_str())
                .collect::<Vec<_>>(),
            ["Anchor", "Older question", "Newest question"]
        );
        state.notify(
            "item/completed",
            &json!({"threadId":"t1", "item":{
                "type":"agentMessage", "id":"live", "text":"Live answer"
            }}),
            240.0,
        );
        state
            .upsert_thread(&json!({"id":"t1", "turns":[{"startedAt":210, "items":[anchor, old]}]}));
        assert_eq!(
            state.threads["t1"]
                .chat
                .iter()
                .map(|(_, m)| m.text.as_str())
                .collect::<Vec<_>>(),
            ["Anchor", "Older question", "Newest question", "Live answer"]
        );
    }

    #[test]
    fn history_without_shared_items_places_older_and_newer_live_messages_on_the_correct_sides() {
        let mut state = state_with_thread();
        state.previews = true;
        for (id, text, at) in [
            ("old", "Older question", 200.0),
            ("new", "Live question", 240.0),
        ] {
            state.notify(
                "item/completed",
                &json!({"threadId":"t1", "item":{
                    "type":"userMessage", "id":id, "content":[{"type":"text", "text":text}]
                }}),
                at,
            );
        }
        state.upsert_thread(&json!({"id":"t1", "turns":[{"startedAt":220, "items":[
            {"type":"agentMessage", "id":"history", "text":"Historical answer"}
        ]}]}));
        assert_eq!(
            state.threads["t1"]
                .chat
                .iter()
                .map(|(_, m)| m.text.as_str())
                .collect::<Vec<_>>(),
            ["Older question", "Historical answer", "Live question"]
        );
    }

    #[test]
    fn previews_off_never_keeps_message_content() {
        let mut state = state_with_thread();
        let item = json!({"type": "agentMessage", "id": "a", "text": "private"});
        state.upsert_thread(&json!({"id": "t1", "turns": [{"items": [item]}]}));
        assert!(
            !state
                .notify(
                    "item/completed",
                    &json!({"threadId": "t1", "item": item}),
                    210.0
                )
                .changed
        );
        assert!(state.threads["t1"].chat.is_empty());
    }

    #[test]
    fn a_thread_reads_as_a_codex_session() {
        let mut state = state_with_thread();
        let (sessions, _) = state.snapshot(300.0, ContentPolicy::default());
        let s = &sessions[0];
        assert_eq!(s.provider, Provider::Codex);
        assert_eq!(s.project, "checkout-api");
        assert_eq!(s.branch.as_deref(), Some("feat/x"));
        assert_eq!(s.phase, SessionPhase::Idle);
    }

    #[test]
    fn status_and_items_drive_the_phase() {
        let mut state = state_with_thread();
        state.notify(
            "thread/status/changed",
            &json!({"threadId": "t1", "status": {"type": "active", "activeFlags": []}}),
            300.0,
        );
        assert_eq!(phase(&state.threads["t1"], 300.0), SessionPhase::Generating);
        state.notify(
            "item/started",
            &json!({"threadId": "t1", "item": {"type": "commandExecution", "id": "i1", "command": "npm test"}}),
            301.0,
        );
        assert_eq!(
            phase(&state.threads["t1"], 301.0),
            SessionPhase::RunningTool
        );
        state.notify("item/completed", &json!({"threadId": "t1", "item": {"type": "commandExecution", "id": "i1", "status": "completed"}}), 302.0);
        assert_eq!(phase(&state.threads["t1"], 302.0), SessionPhase::Generating);
        state.notify("thread/status/changed", &json!({"threadId": "t1", "status": {"type": "active", "activeFlags": ["waitingOnApproval"]}}), 303.0);
        assert_eq!(
            phase(&state.threads["t1"], 303.0),
            SessionPhase::WaitingPermission
        );
        state.notify(
            "turn/completed",
            &json!({"threadId": "t1", "turn": {"status": "completed", "completedAt": 310}}),
            310.0,
        );
        state.notify(
            "thread/status/changed",
            &json!({"threadId": "t1", "status": {"type": "idle"}}),
            310.0,
        );
        assert_eq!(phase(&state.threads["t1"], 311.0), SessionPhase::Finished);
        state.acknowledge("t1", 312.0);
        assert_eq!(phase(&state.threads["t1"], 313.0), SessionPhase::Idle);
    }

    #[test]
    fn each_turn_seen_only_by_its_status_starts_its_own_clock() {
        let mut state = state_with_thread();
        let active = json!({"threadId": "t1", "status": {"type": "active", "activeFlags": []}});
        let idle = json!({"threadId": "t1", "status": {"type": "idle"}});
        state.notify("thread/status/changed", &active, 300.0);
        state.notify("thread/status/changed", &idle, 310.0);
        state.notify("thread/status/changed", &active, 400.0);
        assert_eq!(state.threads["t1"].turn_started, Some(400.0));
        // An exact start from turn/started still wins.
        state.notify(
            "turn/started",
            &json!({"threadId": "t1", "turn": {"startedAt": 399}}),
            401.0,
        );
        assert_eq!(state.threads["t1"].turn_started, Some(399.0));
    }

    #[test]
    fn an_approval_becomes_a_card_and_is_answered_without_stopping_the_turn() {
        let mut state = state_with_thread();
        let params = json!({"threadId": "t1", "turnId": "u", "itemId": "i", "command": "rm -rf build", "cwd": "C:\\x", "reason": "clean"});
        assert!(state.request(
            json!(7),
            "item/commandExecution/requestApproval",
            &params,
            400.0
        ));
        let (sessions, events) = state.snapshot(400.0, ContentPolicy::default());
        let card = sessions[0].permission.as_ref().unwrap();
        assert_eq!(card.request_id, "codex:7");
        assert!(card.detail.starts_with("rm -rf build"));
        assert!(card.offers_always);
        assert!(events.is_empty(), "the first snapshot raises nothing");
        let pending = &state.threads["t1"].pending[0];
        assert_eq!(
            result(pending, &Answer::Deny),
            Some(json!({"decision": "decline"}))
        );
        assert_eq!(
            result(pending, &Answer::AllowForSession),
            Some(json!({"decision": "acceptForSession"}))
        );
        state.notify(
            "serverRequest/resolved",
            &json!({"threadId": "t1", "requestId": 7}),
            401.0,
        );
        assert!(state.threads["t1"].pending.is_empty());
    }

    #[test]
    fn questions_are_answered_by_their_codex_ids() {
        let mut state = state_with_thread();
        let params = json!({"threadId": "t1", "questions": [{"id": "q1", "header": "DB", "question": "Which database?", "options": [{"label": "Postgres"}, {"label": "SQLite"}]}]});
        assert!(state.request(json!("r2"), "item/tool/requestUserInput", &params, 500.0));
        assert_eq!(
            phase(&state.threads["t1"], 500.0),
            SessionPhase::WaitingInput
        );
        let pending = &state.threads["t1"].pending[0];
        let answer = Answer::Questions(HashMap::from([(
            "Which database?".to_string(),
            vec!["SQLite".to_string()],
        )]));
        assert_eq!(
            result(pending, &answer),
            Some(json!({"answers": {"q1": {"answers": ["SQLite"]}}}))
        );
    }

    #[test]
    fn shell_wrappers_are_peeled_off_commands() {
        assert_eq!(
            readable_command(
                r#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -Command 'curl.exe -sI https://example.com'"#
            ),
            "curl.exe -sI https://example.com"
        );
        assert_eq!(
            readable_command("pwsh.exe -NoProfile -Command 'echo ''hi'''"),
            "echo 'hi'"
        );
        assert_eq!(readable_command("cmd.exe /c dir"), "dir");
        assert_eq!(readable_command("git status"), "git status");
    }

    #[test]
    fn unknown_requests_are_left_to_the_terminal() {
        let mut state = state_with_thread();
        assert!(!state.request(json!(9), "item/tool/call", &json!({"threadId": "t1"}), 1.0));
        assert!(state.threads["t1"].pending.is_empty());
    }

    #[test]
    fn token_usage_takes_cache_out_of_input() {
        let mut state = state_with_thread();
        state.notify(
            "thread/tokenUsage/updated",
            &json!({"threadId": "t1", "tokenUsage": {"total": {"inputTokens": 1000, "cachedInputTokens": 600, "cacheWriteInputTokens": 100, "outputTokens": 50, "reasoningOutputTokens": 20}}}),
            1.0,
        );
        let t = state.threads["t1"].tokens;
        assert_eq!(
            (t.input, t.cache_read, t.cache_write, t.output),
            (300, 600, 100, 50)
        );
    }

    fn active(state: &mut State, id: &str, extra: Value) {
        let mut thread = json!({"id": id, "cwd": "C:\\Users\\vors\\Projekty\\checkout-api",
            "status": {"type": "active", "activeFlags": []}});
        thread
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        state.upsert_thread(&thread);
    }

    #[test]
    fn notify_reports_only_real_changes() {
        let mut state = state_with_thread();
        let idle = json!({"threadId": "new", "status": {"type": "idle"}});
        let busy = json!({"threadId": "new", "status": {"type": "active"}});
        assert_eq!(
            state.notify("thread/status/changed", &idle, 0.0),
            Notified {
                changed: true,
                read: Some("new".into())
            },
            "a status for an unseen thread asks for a read"
        );
        assert_eq!(
            state.notify("thread/status/changed", &busy, 1.0),
            Notified::CHANGED,
            "a known thread is not read again"
        );
        assert_eq!(
            state.notify("thread/status/changed", &busy, 1.0),
            Notified::default(),
            "the same status again changes nothing"
        );
        assert_eq!(
            state.notify(
                "thread/status/changed",
                &json!({"threadId": "t1", "status": {"type": "notLoaded"}}),
                2.0
            ),
            Notified::CHANGED
        );
        assert!(
            !state.threads.contains_key("t1"),
            "notLoaded forgets the thread"
        );
        assert_eq!(
            state.notify(
                "thread/status/changed",
                &json!({"threadId": "gone", "status": {"type": "notLoaded"}}),
                2.0
            ),
            Notified::default()
        );
        assert!(
            !state.threads.contains_key("gone"),
            "notLoaded never adds a thread"
        );
        for (method, params) in [
            (
                "item/agentMessage/delta",
                json!({"threadId": "new", "delta": "hi"}),
            ),
            (
                "item/reasoning/textDelta",
                json!({"threadId": "new", "delta": "hm"}),
            ),
            ("what/ever", json!({"threadId": "new"})),
            (
                "item/started",
                json!({"threadId": "new", "item": {"type": "agentMessage", "id": "a"}}),
            ),
            (
                "item/completed",
                json!({"threadId": "new", "item": {"type": "commandExecution", "id": "never-started"}}),
            ),
            ("turn/started", json!({"threadId": "nobody"})),
            ("thread/closed", json!({"threadId": "nobody"})),
            (
                "serverRequest/resolved",
                json!({"threadId": "new", "requestId": 99}),
            ),
        ] {
            assert_eq!(
                state.notify(method, &params, 2.0),
                Notified::default(),
                "{method} changes nothing"
            );
        }
        let usage = json!({"threadId": "new", "tokenUsage": {"total": {"inputTokens": 10, "outputTokens": 2}}});
        assert!(
            state
                .notify("thread/tokenUsage/updated", &usage, 2.0)
                .changed
        );
        assert!(
            !state
                .notify("thread/tokenUsage/updated", &usage, 3.0)
                .changed,
            "the same tokens again change nothing"
        );
        let started = json!({"threadId": "new", "turn": {"startedAt": 5}});
        assert!(state.notify("turn/started", &started, 5.0).changed);
        assert!(!state.notify("turn/started", &started, 6.0).changed);
        assert!(
            state
                .notify("thread/closed", &json!({"threadId": "new"}), 7.0)
                .changed
        );
        assert!(state.threads.is_empty(), "a closed thread is forgotten");
    }

    #[test]
    fn thread_names_follow_renames_and_show_only_with_previews() {
        let mut state = state_with_thread();
        state.upsert_thread(&json!({"id": "t1", "name": "Fix checkout"}));
        let on = ContentPolicy { previews: true };
        let off = ContentPolicy { previews: false };
        let named = state.snapshot(0.0, on).0.remove(0);
        assert_eq!(
            (named.name.as_str(), named.name_source.as_deref()),
            ("Fix checkout", Some("user")),
            "with previews the thread's own name wins"
        );
        let unnamed = state.snapshot(0.0, off).0.remove(0);
        assert_eq!(
            (unnamed.name.as_str(), unnamed.name_source.as_deref()),
            ("checkout-api", None),
            "without previews the folder names it, as Codex may have made the name from the prompt"
        );
        let rename = json!({"threadId": "t1", "threadName": "Ship it"});
        assert!(state.notify("thread/name/updated", &rename, 0.0).changed);
        assert_eq!(state.threads["t1"].name.as_deref(), Some("Ship it"));
        assert_eq!(state.snapshot(0.0, on).0[0].name, "Ship it");
        assert!(
            !state.notify("thread/name/updated", &rename, 0.0).changed,
            "the same name again changes nothing"
        );
        assert!(
            state
                .notify(
                    "thread/name/updated",
                    &json!({"threadId": "t1", "threadName": null}),
                    0.0
                )
                .changed
        );
        assert_eq!(state.threads["t1"].name, None, "a cleared name goes");
        state.upsert_thread(&json!({"id": "t1", "name": "Again"}));
        assert!(
            state
                .notify(
                    "thread/name/updated",
                    &json!({"threadId": "t1", "threadName": ""}),
                    0.0
                )
                .changed
        );
        assert_eq!(
            state.threads["t1"].name, None,
            "an empty name clears it too"
        );
        assert!(
            !state
                .notify(
                    "thread/name/updated",
                    &json!({"threadId": "nobody", "threadName": "x"}),
                    0.0
                )
                .changed,
            "an unknown thread's rename changes nothing"
        );
    }

    #[test]
    fn an_elicitation_alone_is_answered_in_the_terminal() {
        let mut state = state_with_thread();
        assert!(state.request(
            json!(12),
            "mcpServer/elicitation/request",
            &json!({"threadId": "t1", "serverName": "figma", "message": "Sign in?"}),
            1.0
        ));
        let elicitation = state.threads["t1"].pending[0].clone();
        assert!(!elicitation.answerable());
        assert_eq!(result(&elicitation, &Answer::Allow), None);
        let s = state.snapshot(1.0, ContentPolicy::default()).0.remove(0);
        assert!(s.permission.is_none(), "no card the island cannot answer");
        assert_eq!(s.waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        assert_eq!(s.phase, SessionPhase::WaitingInput);
        let card = permission(&elicitation);
        assert_eq!(
            (card.tool.as_str(), card.detail.as_str(), card.offers_always),
            ("figma", "Sign in?", false),
            "its card would name the server and its message"
        );
        assert!(
            state.take_pending("t1", "codex:12").is_none(),
            "the island cannot take it"
        );
        // An approval that comes later is the card, and the phase is its own.
        state.request(
            json!(13),
            "item/commandExecution/requestApproval",
            &json!({"threadId": "t1", "command": "ls"}),
            2.0,
        );
        let s = state.snapshot(2.0, ContentPolicy::default()).0.remove(0);
        assert_eq!(s.phase, SessionPhase::WaitingPermission);
        assert_eq!(
            s.permission.as_ref().map(|p| p.request_id.as_str()),
            Some("codex:13"),
            "the answerable approval is the card, not the elicitation before it"
        );
        assert_eq!(s.waiting_for, None);
        // Once the approval is answered, the elicitation left goes back to
        // the terminal, whatever Codex's status flags say.
        let mut both = state_with_thread();
        both.upsert_thread(&json!({"id": "t1", "status": {"type": "active", "activeFlags": ["waitingOnApproval"]}}));
        both.request(
            json!(1),
            "mcpServer/elicitation/request",
            &json!({"threadId": "t1"}),
            1.0,
        );
        both.request(
            json!(2),
            "item/fileChange/requestApproval",
            &json!({"threadId": "t1"}),
            1.0,
        );
        both.take_pending("t1", "codex:2").unwrap();
        let s = both.snapshot(1.0, ContentPolicy::default()).0.remove(0);
        assert_eq!(s.waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        assert_eq!(s.phase, SessionPhase::WaitingInput);
    }

    #[test]
    fn a_subagents_approval_waits_on_its_parents_row() {
        let mut family = state_with_thread();
        active(&mut family, "c1", json!({"parentThreadId": "t1"}));
        active(&mut family, "g1", json!({"parentThreadId": "c1"}));
        assert!(family.request(
            json!(21),
            "item/fileChange/requestApproval",
            &json!({"threadId": "g1"}),
            1.0
        ));
        assert_eq!(family.row_of("g1"), "t1");
        assert!(family.belongs("g1", "t1") && !family.belongs("t1", "g1"));
        assert_eq!(family.pending("t1").len(), 1);
        let rows = family.snapshot(1.0, ContentPolicy::default()).0;
        assert_eq!(
            rows.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["t1"]
        );
        assert_eq!(rows[0].phase, SessionPhase::WaitingPermission);
        assert_eq!(
            rows[0].permission.as_ref().map(|p| p.request_id.as_str()),
            Some("codex:21"),
            "a sub-subagent's approval is the parent's card"
        );
        assert_eq!(family.answerable("t1", "codex:21").unwrap().0, "g1");
        assert!(
            family.take_pending("t1", "codex:21").is_some(),
            "and is taken from the parent's row"
        );
        assert!(family.threads["g1"].pending.is_empty());
        assert!(
            family.snapshot(2.0, ContentPolicy::default()).0[0]
                .permission
                .is_none(),
            "and goes once answered"
        );
        // Oldest first across the family.
        family.request(
            json!(31),
            "item/fileChange/requestApproval",
            &json!({"threadId": "t1"}),
            5.0,
        );
        family.request(
            json!(32),
            "item/fileChange/requestApproval",
            &json!({"threadId": "c1"}),
            4.0,
        );
        let ids: Vec<String> = family
            .pending("t1")
            .iter()
            .map(Pending::request_id)
            .collect();
        assert_eq!(ids, ["codex:32", "codex:31"]);

        let mut other = state_with_thread();
        other.upsert_thread(&json!({"id": "x1", "cwd": "C:\\a", "status": {"type": "idle"}}));
        other.request(
            json!(22),
            "item/fileChange/requestApproval",
            &json!({"threadId": "x1"}),
            1.0,
        );
        assert!(
            other.take_pending("t1", "codex:22").is_none(),
            "another row's request is not taken"
        );

        let mut cycle = State::default();
        cycle.upsert_thread(&json!({"id": "a", "cwd": "C:\\a", "parentThreadId": "b"}));
        cycle.upsert_thread(&json!({"id": "b", "cwd": "C:\\a", "parentThreadId": "a"}));
        cycle.request(
            json!(1),
            "item/fileChange/requestApproval",
            &json!({"threadId": "a"}),
            1.0,
        );
        assert!(
            cycle.snapshot(1.0, ContentPolicy::default()).0.is_empty(),
            "threads that name each other as parents end the walk"
        );
        assert!(!cycle.belongs("a", "c"));
        assert!(cycle.row_of("a") == "a" || cycle.row_of("a") == "b");
    }

    #[test]
    fn a_thread_read_back_unloaded_is_gone() {
        let mut ghost = State::default();
        ghost.upsert_thread(
            &json!({"id": "g1", "cwd": "C:\\code\\app", "status": {"type": "notLoaded"}}),
        );
        assert!(
            ghost.threads.is_empty(),
            "a thread read as notLoaded is not kept"
        );
        // It unloaded while the read was in flight.
        ghost.notify(
            "thread/status/changed",
            &json!({"threadId": "t9", "status": {"type": "active", "activeFlags": []}}),
            1.0,
        );
        ghost.notify(
            "thread/status/changed",
            &json!({"threadId": "t9", "status": {"type": "notLoaded"}}),
            2.0,
        );
        ghost.upsert_thread(
            &json!({"id": "t9", "cwd": "C:\\code\\app", "status": {"type": "notLoaded"}}),
        );
        assert!(
            ghost.snapshot(3.0, ContentPolicy::default()).0.is_empty(),
            "a late read of an unloaded thread leaves no idle ghost"
        );
        // A known thread read back unloaded goes too.
        let mut known = state_with_thread();
        known.upsert_thread(&json!({"id": "t1", "status": {"type": "notLoaded"}}));
        assert!(known.threads.is_empty());
    }

    #[test]
    fn only_a_read_describes_a_thread() {
        let mut state = State::default();
        state.notify(
            "thread/status/changed",
            &json!({"threadId": "t1", "status": {"type": "active"}}),
            1.0,
        );
        assert!(!state.threads["t1"].described);
        state.upsert_thread(&json!({"id": "t1", "cwd": "C:\\a"}));
        assert!(state.threads["t1"].described);
    }
}
