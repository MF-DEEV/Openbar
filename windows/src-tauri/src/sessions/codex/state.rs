// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What Codex's app-server says about its threads, joined into the same
//! sessions the island shows for Claude Code (spec 07-codex-sessions §3.4,
//! §3.5, §5). Pure: messages and times go in, sessions and the answers to
//! send come out.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use crate::sessions::models::{
    ContentPolicy, HostKind, Permission, Provider, Question, QuestionOption, Session, SessionEvent,
    SessionPhase, Time, Tokens, ToolUse, project_name,
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
}

#[derive(Default)]
pub struct State {
    pub threads: BTreeMap<String, Thread>,
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
    /// `thread/read`'s thread, or a thread from `thread/list`.
    pub fn upsert_thread(&mut self, thread: &Value) {
        let Some(id) = str_at(thread, "id") else {
            return;
        };
        let entry = self
            .threads
            .entry(id.to_string())
            .or_insert_with(|| Thread {
                id: id.to_string(),
                ..Default::default()
            });
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
    }

    /// A notification from the daemon. Returns a thread id worth reading
    /// (one the island has not seen), if any.
    pub fn notify(&mut self, method: &str, params: &Value, now: Time) -> Option<String> {
        let thread_id = str_at(params, "threadId").map(str::to_owned);
        match method {
            "thread/status/changed" => {
                let id = thread_id?;
                let status = Status::from_json(params.get("status").unwrap_or(&Value::Null));
                let known = self.threads.contains_key(&id);
                if status == Status::NotLoaded {
                    self.threads.remove(&id);
                    return None;
                }
                let thread = self.threads.entry(id.clone()).or_insert_with(|| Thread {
                    id: id.clone(),
                    ..Default::default()
                });
                if status.is_active() && !thread.status.as_ref().is_some_and(Status::is_active) {
                    thread.turn_started.get_or_insert(now);
                }
                thread.status = Some(status);
                return (!known).then_some(id);
            }
            "thread/closed" => {
                if let Some(id) = thread_id {
                    self.threads.remove(&id);
                }
            }
            "turn/started" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let turn = params.get("turn").unwrap_or(params);
                thread.turn_started = Some(time_at(turn, "startedAt").unwrap_or(now));
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
            }
            "item/started" | "item/completed" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let item = params.get("item")?;
                let kind = str_at(item, "type")?;
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
                } else if let Some(tool) = thread.tools.iter_mut().rev().find(|t| t.id == id) {
                    tool.finished = Some(now.max(tool.started));
                    tool.failed =
                        matches!(str_at(item, "status"), Some("failed") | Some("declined"));
                }
            }
            "thread/tokenUsage/updated" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                let total = params.get("tokenUsage").and_then(|u| u.get("total"))?;
                let n = |k: &str| total.get(k).and_then(Value::as_i64).unwrap_or(0);
                let cached = n("cachedInputTokens");
                let write = n("cacheWriteInputTokens");
                // Codex counts cache reads and writes inside its input.
                thread.tokens = Tokens {
                    input: (n("inputTokens") - cached - write).max(0),
                    cache_write: write,
                    cache_read: cached,
                    output: n("outputTokens"),
                    reasoning: n("reasoningOutputTokens"),
                };
            }
            "serverRequest/resolved" => {
                let thread = self.threads.get_mut(&thread_id?)?;
                if let Some(request) = params.get("requestId") {
                    thread.pending.retain(|p| &p.rpc_id != request);
                }
            }
            _ => {}
        }
        None
    }

    /// A request from the daemon. True when the island keeps it; requests it
    /// does not understand are left to the terminal and never answered.
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
        for thread in self.threads.values_mut() {
            // A subagent's thread belongs to its parent's row.
            if thread.parent.is_some() || thread.ephemeral || thread.cwd.is_empty() {
                continue;
            }
            let phase = phase(thread, now);
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
            sessions.push(session(thread, phase, since, policy));
        }
        (sessions, events)
    }
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
        "commandExecution" => str_at(item, "command").map(str::to_owned),
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

/// Phase rules in order (spec 07 §5.1).
pub fn phase(thread: &Thread, now: Time) -> SessionPhase {
    if let Some(p) = thread.pending.iter().find(|p| !p.asks()) {
        let _ = p;
        return SessionPhase::WaitingPermission;
    }
    if thread.pending.iter().any(Pending::asks) {
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

/// The card the island shows for the oldest request Codex waits on.
fn permission(pending: &Pending) -> Permission {
    let p = &pending.params;
    let (tool, summary, detail, always) = match pending.method.as_str() {
        "item/commandExecution/requestApproval" => {
            let command = str_at(p, "command").unwrap_or("");
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

fn session(thread: &Thread, phase: SessionPhase, since: Time, policy: ContentPolicy) -> Session {
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
    let card = thread.pending.first().map(permission);
    let project = project_name(&thread.cwd);
    Session {
        id: thread.id.clone(),
        provider: Provider::Codex,
        pid: 0,
        cwd: thread.cwd.clone(),
        project: project.clone(),
        name: thread.name.clone().unwrap_or(project),
        name_source: thread.name.as_ref().map(|_| "user".to_string()),
        branch: thread.branch.clone(),
        model: thread.model.clone(),
        model_name: thread.model.clone(),
        host: HostKind::Unknown,
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
        waiting_for: None,
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
        last_text: None,
        hooked: true,
        chat: Vec::new(),
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
}
