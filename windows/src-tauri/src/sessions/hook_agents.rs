// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Gemini CLI, Copilot CLI, Cursor, OpenCode, Pi, Mistral Vibe and Devin CLI
//! in the Sessions list (AgentHookMessage.swift, AgentHookSessions.swift,
//! AgentHookInstaller.swift, AgentHookSupport.swift).
//! The hook the island installs is this app run with `--hook --agent <name>
//! --event <hook>`, which wraps what the agent hands it in an envelope:
//!
//! `{"openbar": "gemini", "hook": "BeforeTool", "pid": 0, "event": {…}}`
//!
//! OpenCode's plugin and Pi's extension write the same envelope with an
//! event of their own making. None of these agents keeps a registry, so a session is listed from
//! its first event until it ends, its process quits, or (for Cursor's app,
//! whose process outlives its conversations) it has been quiet a while.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::file_diff;
use super::models::{
    ChatMessage, ChatRole, ContentPolicy, HostKind, Permission, Provider, Session, SessionEvent,
    SessionPhase, Time, Tokens, ToolUse, preview, project_name, str_of, tool_summary,
};
use super::transcript::REPLY_LIMIT;

/// The envelope's key, which no agent's own payload carries.
pub const KEY: &str = "openbar";
pub const ANSWER_IN_TERMINAL: &str = super::codex::state::ANSWER_IN_TERMINAL;
/// How long an unread finished turn stays marked, as for Claude Code.
pub const FINISHED_FOR: Time = 10.0 * 60.0;
/// A Cursor conversation nothing was heard from for this long leaves the list.
pub const QUIET_FOR: Time = 30.0 * 60.0;
const TIMELINE_LENGTH: usize = 12;
const CHAT_LENGTH: usize = 40;

impl Provider {
    pub const HOOK_AGENTS: [Provider; 7] = [
        Provider::Gemini,
        Provider::Copilot,
        Provider::Opencode,
        Provider::Cursor,
        Provider::Pi,
        Provider::Vibe,
        Provider::Devin,
    ];

    pub fn reports_through_hooks(self) -> bool {
        Self::HOOK_AGENTS.contains(&self)
    }

    /// Gemini CLI's, Copilot CLI's and Mistral Vibe's hooks only hear of a
    /// prompt, Cursor keeps its approvals, Pi asks none of its own and Devin
    /// CLI's stay in Devin; the person answers those in the agent.
    pub fn answers_permissions(self) -> bool {
        matches!(
            self,
            Provider::Claude | Provider::Codex | Provider::Opencode
        )
    }

    pub fn key(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Gemini => "gemini",
            Provider::Copilot => "copilot",
            Provider::Opencode => "opencode",
            Provider::Cursor => "cursor",
            Provider::Pi => "pi",
            Provider::Vibe => "vibe",
            Provider::Devin => "devin",
        }
    }

    /// Whether the agent's own plugin or extension, running inside it, tells
    /// the island of its sessions and takes replies back.
    pub fn reports_through_plugin(self) -> bool {
        matches!(self, Provider::Opencode | Provider::Pi)
    }

    pub fn from_key(key: &str) -> Option<Provider> {
        [
            Provider::Claude,
            Provider::Codex,
            Provider::Gemini,
            Provider::Copilot,
            Provider::Opencode,
            Provider::Cursor,
            Provider::Pi,
            Provider::Vibe,
            Provider::Devin,
        ]
        .into_iter()
        .find(|p| p.key() == key)
    }
}

// MARK: Events

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Started,
    Ended,
    /// The person sent a prompt and a turn began; its text with previews.
    Prompt(Option<String>),
    /// The agent is at work, as OpenCode's busy status or Cursor's thoughts say.
    Working,
    ToolStarted(ToolUse),
    /// Tools the agent gives no id are matched by name and input.
    ToolFinished {
        id: Option<String>,
        name: String,
        input_key: Option<u64>,
        failed: bool,
        started: Option<Time>,
    },
    Permission(Permission),
    PermissionSettled(Option<String>),
    Question,
    Reply(String),
    Stopped {
        last_text: Option<String>,
        failed: bool,
    },
    Model(String),
    Title(String),
    Usage {
        key: String,
        tokens: Tokens,
        cost: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentEvent {
    pub provider: Provider,
    pub session_id: String,
    pub pid: i32,
    pub cwd: Option<String>,
    pub date: Time,
    pub kind: Kind,
}

impl AgentEvent {
    pub fn id(&self) -> String {
        id(self.provider, &self.session_id)
    }
}

/// The id the island lists a session by, apart from every other agent's.
pub fn id(provider: Provider, session: &str) -> String {
    format!("{}:{session}", provider.key())
}

#[derive(Debug, Default)]
pub struct Decoded {
    pub events: Vec<AgentEvent>,
    /// An OpenCode permission request: (session, request), answered on the same connection.
    pub permission: Option<(String, String)>,
    /// OpenCode's plugin or Pi's extension listening for replies the person types here.
    pub listens: bool,
    /// The folder of the session's own log, where the agent names one (Vibe).
    pub transcript: Option<String>,
    pub provider: Option<Provider>,
    pub pid: i32,
}

/// Whether the input is an envelope from another agent, told cheaply.
pub fn is_envelope(data: &[u8]) -> bool {
    contains(data, b"\"openbar\":")
}

/// Whether the hook waits for the island's answer: an OpenCode permission,
/// or OpenCode's plugin or Pi's extension listening.
pub fn waits(data: &[u8]) -> bool {
    if !is_envelope(data) || !(contains(data, b"\"opencode\"") || contains(data, b"\"pi\"")) {
        return false;
    }
    let Ok(json) = serde_json::from_slice::<Value>(data) else {
        return false;
    };
    let kind = json.pointer("/event/kind").and_then(Value::as_str);
    match json.get(KEY).and_then(Value::as_str) {
        Some("opencode") => matches!(kind, Some("permission" | "listen")),
        Some("pi") => kind == Some("listen"),
        _ => false,
    }
}

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

pub fn decode(data: &[u8], now: Time, policy: ContentPolicy) -> Option<Decoded> {
    let json: Value = serde_json::from_slice(data).ok()?;
    let provider = Provider::from_key(json.get(KEY)?.as_str()?)?;
    if !provider.reports_through_hooks() {
        return None;
    }
    let event = json.get("event")?.as_object()?;
    let pid = json.get("pid").and_then(Value::as_i64).unwrap_or(0) as i32;
    let hook = json
        .get("hook")
        .and_then(Value::as_str)
        .or_else(|| str_of(event.get("hook_event_name")))
        .unwrap_or("")
        .to_owned();
    let mut decoded = Decoded {
        provider: Some(provider),
        pid,
        ..Default::default()
    };
    match provider {
        Provider::Gemini => decoded.events = gemini(&hook, event, pid, now, policy),
        Provider::Copilot => decoded.events = copilot(&hook, event, pid, now, policy),
        Provider::Cursor => decoded.events = cursor(&hook, event, pid, now, policy),
        Provider::Opencode | Provider::Pi => {
            plugin(provider, event, pid, now, policy, &mut decoded)
        }
        Provider::Vibe => {
            decoded.events = vibe(&hook, event, pid, now, policy);
            decoded.transcript = text(event, "transcript_path");
        }
        Provider::Devin => decoded.events = devin(&hook, event, pid, now, policy),
        Provider::Claude | Provider::Codex => return None,
    }
    Some(decoded)
}

fn text(json: &Map<String, Value>, key: &str) -> Option<String> {
    str_of(json.get(key))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn number(value: Option<&Value>) -> i64 {
    value.and_then(Value::as_i64).unwrap_or(0)
}

/// An ISO 8601 time, or milliseconds since 1970.
pub fn timestamp(value: Option<&Value>) -> Option<Time> {
    match value? {
        Value::Number(n) => n.as_f64().filter(|v| *v > 1e12).map(|v| v / 1000.0),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.timestamp_millis() as f64 / 1000.0),
        _ => None,
    }
}

fn reply_preview(value: Option<&Value>) -> Option<String> {
    preview(str_of(value), REPLY_LIMIT)
}

/// Tool arguments, given as an object or as JSON text.
pub fn arguments(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::String(s)) => serde_json::from_str::<Value>(s)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        _ => Map::new(),
    }
}

fn tool_use(name: &str, input: &Map<String, Value>, at: Time, policy: ContentPolicy) -> ToolUse {
    let name = if name.is_empty() { "Tool" } else { name };
    let mut tool = ToolUse::new(
        &format!("{:x}", rand_id()),
        name,
        policy.previews.then(|| summary(name, input)).flatten(),
        at,
    );
    tool.input_key = Some(tool_summary::input_key(name, input));
    tool
}

fn rand_id() -> u128 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    hasher.finish() as u128
}

fn first_line(text: &str) -> String {
    tool_summary::clip(text.lines().next().unwrap_or("").trim())
}

fn waiting_permission(
    tool: &str,
    detail: String,
    offers_always: bool,
    at: Time,
    request: &str,
) -> Permission {
    let line = first_line(&detail);
    let mut permission = Permission::new(
        request,
        if tool.is_empty() { "Tool" } else { tool },
        (!line.is_empty()).then_some(line.as_str()),
        offers_always,
        at,
    );
    permission.detail = detail;
    permission
}

// MARK: Gemini CLI

fn gemini(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "session_id") else {
        return vec![];
    };
    let cwd = text(json, "cwd");
    let date = timestamp(json.get("timestamp")).unwrap_or(now);
    let event = |kind| {
        vec![AgentEvent {
            provider: Provider::Gemini,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date,
            kind,
        }]
    };
    let tool = name(
        Provider::Gemini,
        str_of(json.get("tool_name")).unwrap_or(""),
    );
    let input = arguments(json.get("tool_input"));
    match hook {
        "SessionStart" => event(Kind::Started),
        "SessionEnd" => event(Kind::Ended),
        "BeforeAgent" => event(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        "AfterAgent" => {
            // Gemini CLI writes this when the turn ended with no text, as on an error.
            let reply =
                reply_preview(json.get("prompt_response")).filter(|t| t != "[no response text]");
            event(Kind::Stopped {
                last_text: reply.filter(|_| policy.previews),
                failed: false,
            })
        }
        "BeforeTool" => event(Kind::ToolStarted(tool_use(&tool, &input, date, policy))),
        "AfterTool" => {
            let failed = json
                .get("tool_response")
                .and_then(|r| r.get("error"))
                .is_some_and(|e| !e.is_null());
            event(Kind::ToolFinished {
                id: None,
                input_key: Some(tool_summary::input_key(&tool, &input)),
                name: tool,
                failed,
                started: None,
            })
        }
        "Notification" if str_of(json.get("notification_type")) == Some("ToolPermission") => {
            let details = json
                .get("details")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let asked = name(
                Provider::Gemini,
                str_of(details.get("toolName"))
                    .or_else(|| str_of(details.get("type")))
                    .unwrap_or(""),
            );
            let detail = ["command", "fileName", "filePath", "url"]
                .iter()
                .find_map(|k| text(&details, k))
                .or_else(|| text(json, "message"))
                .unwrap_or_default();
            let mut permission =
                waiting_permission(&asked, detail, false, date, &format!("{:x}", rand_id()));
            permission.diffs = file_diff::gemini(&details);
            event(Kind::Permission(permission))
        }
        _ => vec![],
    }
}

// MARK: Copilot CLI

fn copilot(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "sessionId").or_else(|| text(json, "session_id")) else {
        return vec![];
    };
    let cwd = text(json, "cwd");
    let date = timestamp(json.get("timestamp")).unwrap_or(now);
    let event = |kind| {
        vec![AgentEvent {
            provider: Provider::Copilot,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date,
            kind,
        }]
    };
    let tool = name(
        Provider::Copilot,
        str_of(json.get("toolName")).unwrap_or(""),
    );
    let input = arguments(json.get("toolArgs"));
    match hook {
        "sessionStart" => event(Kind::Started),
        "sessionEnd" => event(Kind::Ended),
        "userPromptSubmitted" => event(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        "preToolUse" => event(Kind::ToolStarted(tool_use(&tool, &input, date, policy))),
        "postToolUse" | "postToolUseFailure" => {
            let result = json
                .get("toolResult")
                .and_then(|r| r.get("resultType"))
                .and_then(Value::as_str);
            event(Kind::ToolFinished {
                id: None,
                input_key: Some(tool_summary::input_key(&tool, &input)),
                name: tool,
                failed: hook == "postToolUseFailure" || result.is_some_and(|r| r != "success"),
                started: None,
            })
        }
        "agentStop" => event(Kind::Stopped {
            last_text: None,
            failed: false,
        }),
        "errorOccurred" if json.get("recoverable").and_then(Value::as_bool) == Some(false) => {
            event(Kind::Stopped {
                last_text: None,
                failed: true,
            })
        }
        "notification" => match str_of(json.get("notification_type")) {
            Some("permission_prompt") => {
                let message = text(json, "message")
                    .or_else(|| text(json, "title"))
                    .unwrap_or_default();
                event(Kind::Permission(waiting_permission(
                    "Tool",
                    message,
                    false,
                    date,
                    &format!("{:x}", rand_id()),
                )))
            }
            Some("elicitation_dialog") => event(Kind::Question),
            _ => vec![],
        },
        _ => vec![],
    }
}

// MARK: Cursor

fn cursor(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "conversation_id").or_else(|| text(json, "session_id")) else {
        return vec![];
    };
    let cwd = json
        .get("workspace_roots")
        .and_then(Value::as_array)
        .and_then(|roots| {
            roots
                .iter()
                .filter_map(Value::as_str)
                .find(|r| !r.is_empty())
        })
        .map(str::to_owned)
        .or_else(|| text(json, "cwd"));
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider: Provider::Cursor,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    if let Some(model) = text(json, "model") {
        add(Kind::Model(model));
    }
    let tool = name(
        Provider::Cursor,
        str_of(json.get("tool_name")).unwrap_or(""),
    );
    match hook {
        "sessionStart" => add(Kind::Started),
        "sessionEnd" => add(Kind::Ended),
        "beforeSubmitPrompt" => add(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        "afterAgentThought" => add(Kind::Working),
        "afterAgentResponse" => match reply_preview(json.get("text")).filter(|_| policy.previews) {
            Some(text) => add(Kind::Reply(text)),
            None => add(Kind::Working),
        },
        "postToolUse" | "postToolUseFailure" => {
            let input = arguments(json.get("tool_input"));
            let seconds = json
                .get("duration")
                .and_then(Value::as_f64)
                .map(|ms| ms / 1000.0);
            add(Kind::ToolFinished {
                id: text(json, "tool_use_id"),
                input_key: Some(tool_summary::input_key(&tool, &input)),
                name: tool,
                failed: hook == "postToolUseFailure",
                started: seconds.map(|s| now - s),
            })
        }
        "stop" => add(Kind::Stopped {
            last_text: None,
            failed: str_of(json.get("status")).is_some_and(|s| s != "completed"),
        }),
        _ => {}
    }
    events
}

// MARK: OpenCode

// MARK: Mistral Vibe

/// Mistral Vibe's hooks.toml hooks (its bundled `vibe` skill, "Hooks"). A
/// notification only tells of an approval or a question, answered in Vibe;
/// the end of the person's turn is heard from it too.
fn vibe(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "session_id") else {
        return vec![];
    };
    // A subagent's hooks name the session it works for; its tools are the parent's.
    if text(json, "parent_session_id").is_some() {
        return vec![];
    }
    let cwd = text(json, "cwd");
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider: Provider::Vibe,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    let tool = name(Provider::Vibe, str_of(json.get("tool_name")).unwrap_or(""));
    let input = arguments(json.get("tool_input"));
    match hook {
        "session_start" => {
            add(Kind::Started);
            if let Some(model) = text(json, "model") {
                add(Kind::Model(model));
            }
        }
        "session_end" => add(Kind::Ended),
        "pre_agent" => add(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        "pre_tool" => {
            let mut use_ = tool_use(&tool, &input, now, policy);
            if let Some(id) = text(json, "tool_call_id") {
                use_.id = id;
            }
            add(Kind::ToolStarted(use_));
        }
        "post_tool" => add(Kind::ToolFinished {
            id: text(json, "tool_call_id"),
            input_key: Some(tool_summary::input_key(&tool, &input)),
            name: tool,
            failed: str_of(json.get("tool_status")) == Some("failure"),
            started: None,
        }),
        "notification" => match str_of(json.get("notification_type")) {
            Some("approval_requested") => {
                let asked = name(Provider::Vibe, str_of(json.get("tool_name")).unwrap_or(""));
                let message = text(json, "message").unwrap_or_default();
                add(Kind::Permission(waiting_permission(
                    &asked,
                    message,
                    false,
                    now,
                    &format!("{:x}", rand_id()),
                )));
            }
            Some("question_asked") => add(Kind::Question),
            Some("turn_completed") => add(Kind::Stopped {
                last_text: None,
                failed: false,
            }),
            Some("turn_failed") => add(Kind::Stopped {
                last_text: None,
                failed: true,
            }),
            _ => {}
        },
        _ => {}
    }
    events
}

// MARK: Devin CLI

/// Devin CLI's hooks, in Claude Code's format (docs.devin.ai, "Lifecycle
/// Hooks"). Its payloads carry no folder; the island reads the agent's. A
/// permission is heard and answered in Devin.
fn devin(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "session_id") else {
        return vec![];
    };
    let cwd = text(json, "cwd");
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider: Provider::Devin,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    let raw = str_of(json.get("tool_name")).unwrap_or("");
    let tool = name(Provider::Devin, raw);
    let input = arguments(json.get("tool_input"));
    match hook {
        "SessionStart" => add(Kind::Started),
        "SessionEnd" => add(Kind::Ended),
        "UserPromptSubmit" => add(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        // Devin's question to the person is a tool of its own.
        "PreToolUse" if raw == "ask_user_question" => add(Kind::Question),
        "PreToolUse" => add(Kind::ToolStarted(tool_use(&tool, &input, now, policy))),
        "PostToolUse" if raw == "ask_user_question" => add(Kind::PermissionSettled(None)),
        "PostToolUse" => add(Kind::ToolFinished {
            id: None,
            input_key: Some(tool_summary::input_key(&tool, &input)),
            name: tool,
            failed: json
                .get("tool_response")
                .and_then(|r| r.get("success"))
                .and_then(Value::as_bool)
                == Some(false),
            started: None,
        }),
        "PermissionRequest" => {
            let detail = ["command", "file_path", "path", "url"]
                .iter()
                .find_map(|k| text(&input, k))
                .unwrap_or_else(|| {
                    if input.is_empty() {
                        String::new()
                    } else {
                        tool_summary::arguments(&input)
                    }
                });
            add(Kind::Permission(waiting_permission(
                &tool,
                detail,
                false,
                now,
                &format!("{:x}", rand_id()),
            )));
        }
        "Stop" => add(Kind::Stopped {
            last_text: None,
            failed: false,
        }),
        _ => {}
    }
    events
}

/// Devin CLI reads the hooks in Claude Code's settings by default, so the
/// island's Claude Code hooks run in its sessions too; those are told apart
/// and let go. A hook in Claude Code's format is Devin's when its input has
/// Devin's per-turn id and none of the transcript every Claude Code hook
/// names, or when Devin's process ran it.
pub fn is_devin_payload(data: &[u8]) -> bool {
    if !contains(data, b"\"prompt_id\"") {
        return false;
    }
    serde_json::from_slice::<Value>(data).is_ok_and(|json| {
        json.get("prompt_id").is_some() && json.get("transcript_path").is_none()
    })
}

pub fn is_devin_process(name: &str) -> bool {
    let name = name.to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    name == "devin" || name.starts_with("devin-") || name.starts_with("devin.")
}

/// The last reply of a Mistral Vibe session, read from its own log, since
/// its hooks carry none: the newest JSON chunk under `chunks/` that holds
/// history ends on the latest messages (AgentVibeLog in AgentHookSupport.swift).
pub fn vibe_last_reply(folder: &Path) -> Option<String> {
    const CHUNK_LIMIT: u64 = 4 * 1024 * 1024;
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = std::fs::read_dir(folder.join("chunks"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, e.path(), meta.len()))
        })
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.0));
    files
        .into_iter()
        .take(6)
        .filter(|(_, _, size)| *size <= CHUNK_LIMIT)
        .filter_map(|(_, path, _)| std::fs::read(path).ok())
        .filter(|data| contains(data, b"\"history\""))
        .find_map(|data| vibe_chunk_reply(&data))
}

pub fn vibe_chunk_reply(data: &[u8]) -> Option<String> {
    let entries: Vec<Value> = serde_json::from_slice(data).ok()?;
    entries.iter().rev().find_map(|entry| {
        if str_of(entry.get("source")) != Some("history")
            || entry.pointer("/message/role").and_then(Value::as_str) != Some("assistant")
        {
            return None;
        }
        let text: Vec<&str> = entry
            .pointer("/message/content")
            .and_then(Value::as_array)?
            .iter()
            .filter(|p| str_of(p.get("type")) == Some("text"))
            .filter_map(|p| str_of(p.get("text")))
            .collect();
        preview(Some(&text.join("\n")), REPLY_LIMIT)
    })
}

// MARK: OpenCode and Pi

/// What the island's OpenCode plugin and Pi extension write.
fn plugin(
    provider: Provider,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
    decoded: &mut Decoded,
) {
    let kind = str_of(json.get("kind")).unwrap_or("");
    if kind == "listen" {
        decoded.listens = true;
        return;
    }
    let Some(session) = text(json, "session") else {
        return;
    };
    let cwd = text(json, "directory");
    let date = timestamp(json.get("at")).unwrap_or(now);
    let mut add = |kind| {
        decoded.events.push(AgentEvent {
            provider,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date,
            kind,
        })
    };
    match kind {
        "created" => add(Kind::Started),
        "deleted" => add(Kind::Ended),
        "prompt" => add(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("text")))
                .flatten(),
        )),
        "status" => match str_of(json.get("status")) {
            Some("busy" | "retry") => add(Kind::Working),
            Some("idle") => add(Kind::Stopped {
                last_text: None,
                failed: false,
            }),
            _ => {}
        },
        "error" => add(Kind::Stopped {
            last_text: None,
            failed: true,
        }),
        "tool" => {
            let tool = name(provider, str_of(json.get("tool")).unwrap_or(""));
            let input = arguments(json.get("input"));
            let call = text(json, "call").unwrap_or_else(|| format!("{:x}", rand_id()));
            match str_of(json.get("status")) {
                Some("running" | "pending") => {
                    let mut tool = tool_use(&tool, &input, date, policy);
                    tool.id = call;
                    add(Kind::ToolStarted(tool));
                }
                Some(status @ ("completed" | "error")) => add(Kind::ToolFinished {
                    id: Some(call),
                    name: tool,
                    input_key: None,
                    failed: status == "error",
                    started: None,
                }),
                _ => {}
            }
        }
        "permission" if provider == Provider::Opencode => {
            let Some(request) = text(json, "id") else {
                return;
            };
            let tool = name(
                Provider::Opencode,
                str_of(json.get("permission")).unwrap_or(""),
            );
            let metadata = json
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let patterns: Vec<String> = json
                .get("patterns")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let detail = text(&metadata, "command").unwrap_or_else(|| {
                if patterns.is_empty() {
                    tool_summary::arguments(&metadata)
                } else {
                    patterns.join("\n")
                }
            });
            let always = json
                .get("always")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().filter_map(Value::as_str).any(|s| !s.is_empty()));
            let detail: String = detail.chars().take(tool_summary::DETAIL_LIMIT).collect();
            decoded.permission = Some((session.clone(), request.clone()));
            let mut permission = waiting_permission(&tool, detail, always, date, &request);
            if tool == "Edit" {
                permission.diffs = file_diff::opencode(&metadata);
            }
            add(Kind::Permission(permission));
        }
        "permissionReplied" => add(Kind::PermissionSettled(text(json, "id"))),
        "question" => add(Kind::Question),
        "reply" => {
            if let Some(text) = reply_preview(json.get("text")).filter(|_| policy.previews) {
                add(Kind::Reply(text));
            }
        }
        "message" => {
            if let Some(model) = text(json, "model") {
                add(Kind::Model(model));
            }
            if let (Some(tokens), Some(message)) = (
                json.get("tokens").and_then(Value::as_object),
                text(json, "id"),
            ) {
                let cache = tokens
                    .get("cache")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let reasoning = number(tokens.get("reasoning"));
                add(Kind::Usage {
                    key: message,
                    tokens: Tokens {
                        input: number(tokens.get("input")),
                        cache_write: number(cache.get("write")),
                        cache_read: number(cache.get("read")),
                        output: number(tokens.get("output")) + reasoning,
                        reasoning,
                    },
                    cost: json.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
                });
            }
        }
        "model" => {
            if let Some(model) = text(json, "model") {
                add(Kind::Model(model));
            }
        }
        "title" => {
            if let Some(title) = reply_preview(json.get("title")).filter(|_| policy.previews) {
                add(Kind::Title(title));
            }
        }
        _ => {}
    }
}

/// What OpenCode's plugin passes on for a decision: once, always, or reject.
pub fn opencode_reply(decision: &super::hook_message::Decision) -> Option<Vec<u8>> {
    use super::hook_message::Decision;
    let reply = match decision {
        Decision::Allow | Decision::ApprovePlan(_) => "once",
        Decision::AllowAlways => "always",
        Decision::Deny => "reject",
        Decision::Answer(_) => return None,
    };
    serde_json::to_vec(&json!({ "reply": reply })).ok()
}

/// A prompt for OpenCode's listening plugin or Pi's extension to send to one of its sessions.
pub fn opencode_prompt(session: &str, text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "prompt": { "session": session, "text": text } }))
        .unwrap_or_default()
}

// MARK: Tools

/// The agents' tools by the names Claude Code gives the same ones.
pub fn name(provider: Provider, tool: &str) -> String {
    let known: &[(&str, &str)] = match provider {
        Provider::Gemini => &[
            ("run_shell_command", "Bash"),
            ("read_file", "Read"),
            ("read_many_files", "Read"),
            ("write_file", "Write"),
            ("replace", "Edit"),
            ("edit", "Edit"),
            ("glob", "Glob"),
            ("search_file_content", "Grep"),
            ("grep_search", "Grep"),
            ("list_directory", "LS"),
            ("web_fetch", "WebFetch"),
            ("google_web_search", "WebSearch"),
            ("save_memory", "Memory"),
            ("write_todos", "TodoWrite"),
            ("exec", "Bash"),
            ("info", "Tool"),
        ],
        Provider::Copilot => &[
            ("bash", "Bash"),
            ("powershell", "PowerShell"),
            ("view", "Read"),
            ("create", "Write"),
            ("edit", "Edit"),
            ("glob", "Glob"),
            ("grep", "Grep"),
            ("web_fetch", "WebFetch"),
            ("task", "Task"),
            ("ask_user", "AskUserQuestion"),
        ],
        Provider::Opencode => &[
            ("bash", "Bash"),
            ("read", "Read"),
            ("write", "Write"),
            ("edit", "Edit"),
            ("patch", "Edit"),
            ("glob", "Glob"),
            ("grep", "Grep"),
            ("list", "LS"),
            ("webfetch", "WebFetch"),
            ("websearch", "WebSearch"),
            ("task", "Task"),
            ("todowrite", "TodoWrite"),
            ("todoread", "TodoRead"),
            ("external_directory", "Folder"),
        ],
        Provider::Cursor => &[
            ("Shell", "Bash"),
            ("Read", "Read"),
            ("Write", "Write"),
            ("Edit", "Edit"),
            ("Grep", "Grep"),
            ("Glob", "Glob"),
            ("Task", "Task"),
            ("Delete", "Delete"),
        ],
        Provider::Pi => &[
            ("bash", "Bash"),
            ("read", "Read"),
            ("write", "Write"),
            ("edit", "Edit"),
            ("grep", "Grep"),
            ("find", "Glob"),
            ("ls", "LS"),
        ],
        // Vibe names a tool with its group, as `file_system.bash`; the last part is matched.
        Provider::Vibe => &[
            ("bash", "Bash"),
            ("process_start", "Bash"),
            ("read_file", "Read"),
            ("write_file", "Write"),
            ("search_replace", "Edit"),
            ("edit_file", "Edit"),
            ("grep", "Grep"),
            ("glob", "Glob"),
            ("list_dir", "LS"),
            ("web_fetch", "WebFetch"),
            ("web_search", "WebSearch"),
            ("todo", "TodoWrite"),
            ("agents_spawn", "Task"),
            ("ask_user_question", "AskUserQuestion"),
        ],
        Provider::Devin => &[
            ("exec", "Bash"),
            ("read", "Read"),
            ("write", "Write"),
            ("edit", "Edit"),
            ("apply_patch", "Edit"),
            ("grep", "Grep"),
            ("glob", "Glob"),
            ("webfetch", "WebFetch"),
            ("todo_write", "TodoWrite"),
            ("run_subagent", "Task"),
            ("notebook_read", "Read"),
            ("notebook_edit", "Edit"),
            ("exit_plan_mode", "ExitPlanMode"),
        ],
        Provider::Claude | Provider::Codex => &[],
    };
    if let Some((_, name)) = known.iter().find(|(k, _)| *k == tool) {
        return (*name).to_owned();
    }
    if provider == Provider::Vibe
        && !tool.starts_with("mcp_")
        && let Some((_, last)) = tool.rsplit_once('.')
    {
        return self::name(provider, last);
    }
    if let Some(rest) = tool.strip_prefix("MCP:") {
        return rest.to_owned();
    }
    if let Some(rest) = tool.strip_prefix("mcp__") {
        return rest.to_owned();
    }
    let mut chars = tool.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One line on what a tool works on, from the first field that says.
pub fn summary(tool: &str, input: &Map<String, Value>) -> Option<String> {
    if let Some(claude) = tool_summary::summary(tool, input) {
        return Some(claude);
    }
    for key in [
        "command",
        "file_path",
        "filePath",
        "absolute_path",
        "path",
        "pattern",
        "url",
        "query",
        "description",
    ] {
        let Some(value) = str_of(input.get(key)) else {
            continue;
        };
        let line = value.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let shown = if key.to_lowercase().contains("path") {
            line.rsplit(['/', '\\']).next().unwrap_or(line)
        } else {
            line
        };
        return Some(tool_summary::clip(shown));
    }
    None
}

// MARK: Sessions

#[derive(Clone, Debug)]
pub struct Entry {
    pub provider: Provider,
    pub session_id: String,
    pub pid: i32,
    pub cwd: String,
    pub heard: Time,
    turn_open: bool,
    turn_started: Option<Time>,
    turn_ended: Option<Time>,
    completed: bool,
    acknowledged: Option<Time>,
    tools: Vec<ToolUse>,
    pub permission: Option<Permission>,
    answerable: bool,
    asking: bool,
    model: Option<String>,
    title: Option<String>,
    last_text: Option<String>,
    chat: Vec<ChatMessage>,
    usage: BTreeMap<String, (Tokens, f64)>,
}

impl Entry {
    fn new(event: &AgentEvent, now: Time) -> Self {
        Entry {
            provider: event.provider,
            session_id: event.session_id.clone(),
            pid: event.pid,
            cwd: event.cwd.clone().unwrap_or_default(),
            heard: now,
            turn_open: false,
            turn_started: None,
            turn_ended: None,
            completed: true,
            acknowledged: None,
            tools: Vec::new(),
            permission: None,
            answerable: false,
            asking: false,
            model: None,
            title: None,
            last_text: None,
            chat: Vec::new(),
            usage: BTreeMap::new(),
        }
    }

    fn open_tool(&self) -> Option<&ToolUse> {
        self.tools.iter().rev().find(|t| t.finished.is_none())
    }

    fn clear_wait(&mut self) {
        self.permission = None;
        self.answerable = false;
        self.asking = false;
    }

    /// An event of a turn the island did not see begin opens it.
    fn open_turn(&mut self, at: Time) {
        if self.turn_open {
            return;
        }
        self.turn_open = true;
        if self.turn_started.is_none() || self.turn_ended.is_some() {
            self.turn_started = Some(at);
        }
        self.turn_ended = None;
        self.completed = false;
    }

    fn append(&mut self, role: ChatRole, text: String, at: Time, id: &mut i64) {
        *id += 1;
        self.chat.push(ChatMessage {
            id: *id,
            role,
            text,
            images: Vec::new(),
            at,
            ends_turn: false,
        });
        if self.chat.len() > CHAT_LENGTH {
            let extra = self.chat.len() - CHAT_LENGTH;
            self.chat.drain(..extra);
        }
    }

    fn trim_tools(&mut self) {
        if self.tools.len() > TIMELINE_LENGTH {
            let extra = self.tools.len() - TIMELINE_LENGTH;
            self.tools.drain(..extra);
        }
    }
}

/// The sessions of the hook agents, kept from their events.
#[derive(Default)]
pub struct Tracker {
    entries: HashMap<String, Entry>,
    phases: HashMap<String, (SessionPhase, Time)>,
    message_id: i64,
}

/// What the tracker needs of the outside world for a snapshot.
pub struct Place {
    pub host: HostKind,
    pub accepts_replies: bool,
    pub host_name: Option<String>,
}

impl Tracker {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn owns(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub fn pids(&self) -> Vec<i32> {
        self.entries.values().map(|e| e.pid).collect()
    }

    pub fn ingest(&mut self, event: AgentEvent, now: Time) {
        let id = event.id();
        if event.kind == Kind::Ended {
            self.entries.remove(&id);
            self.phases.remove(&id);
            return;
        }
        let mut message_id = self.message_id;
        let entry = self
            .entries
            .entry(id)
            .or_insert_with(|| Entry::new(&event, now));
        if event.pid > 0 {
            entry.pid = event.pid;
        }
        if let Some(cwd) = event.cwd.as_ref().filter(|c| !c.is_empty()) {
            entry.cwd = cwd.clone();
        }
        entry.heard = now;
        let date = event.date.min(now);
        match event.kind {
            Kind::Started | Kind::Ended => {}
            Kind::Prompt(text) => {
                entry.turn_open = true;
                entry.turn_started = Some(date);
                entry.turn_ended = None;
                entry.completed = false;
                entry.clear_wait();
                if let Some(text) = text {
                    entry.append(ChatRole::User, text, date, &mut message_id);
                }
            }
            Kind::Working => {
                if !entry.turn_open {
                    entry.turn_open = true;
                    entry.turn_started = Some(date);
                    entry.turn_ended = None;
                    entry.completed = false;
                }
            }
            Kind::ToolStarted(tool) => {
                if !entry.tools.iter().any(|t| t.id == tool.id) {
                    entry.open_turn(date);
                    // A tool that runs was allowed, wherever.
                    if !entry.answerable {
                        entry.clear_wait();
                    }
                    entry.tools.push(tool);
                    entry.trim_tools();
                }
            }
            Kind::ToolFinished {
                id,
                name,
                input_key,
                failed,
                started,
            } => {
                entry.open_turn(date);
                if !entry.answerable {
                    entry.clear_wait();
                }
                let index = id
                    .as_ref()
                    .and_then(|id| entry.tools.iter().rposition(|t| &t.id == id))
                    .or_else(|| {
                        entry.tools.iter().position(|t| {
                            t.finished.is_none()
                                && t.name == name
                                && (input_key.is_none() || t.input_key == input_key)
                        })
                    })
                    .or_else(|| {
                        entry
                            .tools
                            .iter()
                            .position(|t| t.finished.is_none() && t.name == name)
                    });
                match index {
                    Some(i) => {
                        let tool = &mut entry.tools[i];
                        tool.finished = Some(date.max(tool.started));
                        tool.failed = failed;
                    }
                    None => {
                        // Cursor tells of a tool only once it ran.
                        let mut tool = ToolUse::new(
                            &id.unwrap_or_else(|| format!("{:x}", rand_id())),
                            if name.is_empty() { "Tool" } else { &name },
                            None,
                            started.unwrap_or(date),
                        );
                        tool.finished = Some(date);
                        tool.failed = failed;
                        tool.input_key = input_key;
                        entry.tools.push(tool);
                        entry.trim_tools();
                    }
                }
            }
            Kind::Permission(permission) => {
                entry.open_turn(date);
                entry.permission = Some(permission);
                entry.answerable = event.provider.answers_permissions();
                entry.asking = false;
            }
            Kind::PermissionSettled(request) => {
                if request.is_none()
                    || entry.permission.as_ref().map(|p| &p.request_id) == request.as_ref()
                {
                    entry.clear_wait();
                }
            }
            Kind::Question => {
                entry.open_turn(date);
                entry.asking = true;
            }
            Kind::Reply(text) => {
                entry.last_text = Some(text.clone());
                entry.append(ChatRole::Assistant, text, date, &mut message_id);
            }
            Kind::Stopped { last_text, failed } => {
                if entry.turn_open || entry.permission.is_some() || entry.asking {
                    entry.turn_open = false;
                    entry.turn_ended = Some(date);
                    entry.completed = !failed;
                    entry.clear_wait();
                    for tool in &mut entry.tools {
                        if tool.finished.is_none() {
                            tool.finished = Some(date.max(tool.started));
                        }
                    }
                    if let Some(text) = last_text {
                        entry.last_text = Some(text.clone());
                        let repeated = entry
                            .chat
                            .last()
                            .is_some_and(|m| m.role == ChatRole::Assistant && m.text == text);
                        if !repeated {
                            entry.append(ChatRole::Assistant, text, date, &mut message_id);
                        }
                    }
                    if let Some(last) = entry
                        .chat
                        .last_mut()
                        .filter(|m| m.role == ChatRole::Assistant)
                    {
                        last.ends_turn = true;
                    }
                }
            }
            Kind::Model(model) => entry.model = Some(model),
            Kind::Title(title) => entry.title = Some(title),
            Kind::Usage { key, tokens, cost } => {
                entry.usage.insert(key, (tokens, cost));
            }
        }
        self.message_id = message_id;
    }

    /// The person answered the request here; the agent tells when it moves on.
    pub fn resolve(&mut self, id: &str, request: &str) {
        if let Some(entry) = self.entries.get_mut(id)
            && entry
                .permission
                .as_ref()
                .is_some_and(|p| p.request_id == request)
        {
            entry.clear_wait();
        }
    }

    pub fn acknowledge(&mut self, id: &str, now: Time) {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.acknowledged = Some(now);
        }
    }

    pub fn purge_content(&mut self) {
        for entry in self.entries.values_mut() {
            entry.title = None;
            entry.last_text = None;
            entry.chat.clear();
            for tool in &mut entry.tools {
                tool.summary = None;
            }
        }
    }

    /// Drops sessions whose process is gone, and Cursor conversations long quiet.
    pub fn prune(&mut self, now: Time, alive: impl Fn(i32) -> bool) {
        let gone: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                (e.pid > 0 && !alive(e.pid))
                    || (e.provider == Provider::Cursor
                        && !e.turn_open
                        && e.permission.is_none()
                        && now - e.heard > QUIET_FOR)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.entries.remove(&id);
            self.phases.remove(&id);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.phases.clear();
    }

    pub fn phase(entry: &Entry, now: Time) -> SessionPhase {
        if entry.permission.is_some() {
            return SessionPhase::WaitingPermission;
        }
        if entry.asking {
            return SessionPhase::WaitingInput;
        }
        if entry.turn_open {
            return if entry.open_tool().is_some() {
                SessionPhase::RunningTool
            } else {
                SessionPhase::Generating
            };
        }
        if let Some(ended) = entry.turn_ended
            && entry.completed
            && now - ended < FINISHED_FOR
            && entry.acknowledged.unwrap_or(f64::MIN) < ended
        {
            return SessionPhase::Finished;
        }
        SessionPhase::Idle
    }

    /// The sessions as the island shows them, with the changes worth a notice.
    pub fn snapshot(
        &mut self,
        now: Time,
        policy: ContentPolicy,
        place: impl Fn(&Entry) -> Place,
        branch: impl Fn(&str) -> Option<String>,
    ) -> (Vec<Session>, Vec<SessionEvent>) {
        let mut sessions = Vec::new();
        let mut events = Vec::new();
        let mut ids: Vec<&String> = self.entries.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &self.entries[id];
            let phase = Self::phase(entry, now);
            let previous = self.phases.get(id).copied();
            let since = match previous {
                Some((p, since)) if p == phase => since,
                _ => now,
            };
            if let Some((previous, _)) = previous
                && previous != phase
            {
                match phase {
                    SessionPhase::WaitingPermission => events.push(SessionEvent::NeedsPermission {
                        id: id.clone(),
                        tool: entry
                            .permission
                            .as_ref()
                            .map(|p| p.tool.clone())
                            .unwrap_or_default(),
                    }),
                    SessionPhase::WaitingInput => {
                        events.push(SessionEvent::NeedsInput { id: id.clone() })
                    }
                    SessionPhase::Finished if previous.is_active() || previous.is_waiting() => {
                        events.push(SessionEvent::Finished {
                            id: id.clone(),
                            duration: entry.turn_started.zip(entry.turn_ended).map(|(s, e)| e - s),
                        })
                    }
                    _ => {}
                }
            }
            sessions.push(session(
                id,
                entry,
                phase,
                since,
                policy,
                place(entry),
                branch(&entry.cwd),
            ));
        }
        for session in &sessions {
            self.phases
                .insert(session.id.clone(), (session.phase, session.phase_since));
        }
        (sessions, events)
    }
}

fn session(
    id: &str,
    entry: &Entry,
    phase: SessionPhase,
    since: Time,
    policy: ContentPolicy,
    place: Place,
    branch: Option<String>,
) -> Session {
    let previews = policy.previews;
    let mut timeline = entry.tools.clone();
    if !previews {
        for tool in &mut timeline {
            tool.summary = None;
        }
    }
    let project = project_name(&entry.cwd);
    let mut tokens = Tokens::default();
    let mut cost = 0.0;
    for (part, part_cost) in entry.usage.values() {
        tokens += *part;
        cost += part_cost;
    }
    // A request only the agent can answer is shown as waiting there.
    let in_agent = entry.permission.is_some() && !entry.answerable;
    Session {
        id: id.to_owned(),
        provider: entry.provider,
        pid: entry.pid,
        cwd: entry.cwd.clone(),
        project: project.clone(),
        name: project,
        name_source: None,
        branch,
        model: entry.model.clone(),
        model_name: entry.model.clone(),
        host: place.host,
        accepts_replies: place.accepts_replies,
        host_name: place.host_name,
        remote: None,
        phase,
        phase_since: since,
        turn_started: entry.turn_started,
        last_active: Some(entry.turn_ended.map_or(entry.heard, |t| t.max(entry.heard))),
        looked_at: entry.acknowledged,
        waiting_for: (in_agent || entry.asking).then(|| ANSWER_IN_TERMINAL.to_owned()),
        current_tool: matches!(
            phase,
            SessionPhase::RunningTool | SessionPhase::WaitingPermission
        )
        .then(|| entry.open_tool().cloned())
        .flatten(),
        permission: if in_agent {
            None
        } else {
            entry.permission.clone()
        },
        waiting_diffs: if in_agent {
            entry
                .permission
                .as_ref()
                .map(|p| p.diffs.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        timeline,
        subagents: Vec::new(),
        tasks: None,
        tokens,
        cost,
        title: entry.title.clone().filter(|_| previews),
        last_text: entry.last_text.clone().filter(|_| previews),
        hooked: true,
        chat: if previews {
            entry.chat.clone()
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
    }
}

/// The agent process a hook came from: above the hook's own process (this
/// app run with `--hook`) and the shells the agent started it in, the first
/// process that is none of these.
pub fn agent_process(chain: &[(u32, String)], hint: i32) -> i32 {
    const SHELLS: [&str; 12] = [
        "openbar.exe",
        "powershell.exe",
        "pwsh.exe",
        "cmd.exe",
        "bash.exe",
        "sh.exe",
        "conhost.exe",
        "nc",
        "sh",
        "bash",
        "zsh",
        "dash",
    ];
    let own = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    chain
        .iter()
        .find(|(_, name)| {
            let name = name.to_lowercase();
            !SHELLS.contains(&name.as_str()) && Some(&name) != own.as_ref()
        })
        .map(|(pid, _)| *pid as i32)
        .unwrap_or(hint)
}

/// The command New session starts for an agent, in a console of its own.
pub fn command_name(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Gemini => Some("gemini"),
        Provider::Copilot => Some("copilot"),
        Provider::Opencode => Some("opencode"),
        Provider::Cursor => Some("cursor-agent"),
        Provider::Pi => Some("pi"),
        Provider::Vibe => Some("vibe"),
        Provider::Devin => Some("devin"),
        Provider::Claude | Provider::Codex => None,
    }
}

/// The arguments that open the agent's terminal UI with a first prompt
/// already sent: `gemini -i`, `copilot -i`, `opencode --prompt`, `devin --`,
/// and `cursor-agent`, `pi` and `vibe` take it as it is (AgentCLI in
/// ClaudeBackgroundSessions.swift).
pub fn start_arguments(provider: Provider, prompt: &str) -> Vec<String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Vec::new();
    }
    match provider {
        Provider::Gemini | Provider::Copilot => vec!["-i".into(), prompt.into()],
        Provider::Opencode => vec!["--prompt".into(), prompt.into()],
        Provider::Cursor | Provider::Pi | Provider::Vibe => vec![prompt.into()],
        Provider::Devin => vec!["--".into(), prompt.into()],
        Provider::Claude | Provider::Codex => Vec::new(),
    }
}

/// The branch a folder's repository is on, read from its HEAD.
pub fn read_branch(folder: &str) -> Option<String> {
    let mut path = PathBuf::from(folder);
    for _ in 0..32 {
        let git = path.join(".git");
        if git.exists() {
            let head = if git.is_dir() {
                git.join("HEAD")
            } else {
                // A worktree's .git file names its own folder.
                let text = std::fs::read_to_string(&git).ok()?;
                let line = text
                    .lines()
                    .next()?
                    .strip_prefix("gitdir:")?
                    .trim()
                    .to_owned();
                let dir = PathBuf::from(&line);
                if dir.is_absolute() {
                    dir
                } else {
                    path.join(dir)
                }
                .join("HEAD")
            };
            let text = std::fs::read_to_string(head).ok()?;
            return text
                .trim()
                .strip_prefix("ref: refs/heads/")
                .map(str::to_owned);
        }
        if !path.pop() {
            return None;
        }
    }
    None
}

// MARK: Installing

pub mod installer {
    //! Puts the island's hooks into Gemini CLI, Copilot CLI and Cursor, and
    //! its plugin into OpenCode, only when the person presses Install. Gemini's
    //! settings and Cursor's hooks.json keep the island's entries beside the
    //! person's own, copied aside first; Copilot and OpenCode read a folder,
    //! where the island writes a file of its own.

    use super::*;
    use crate::hooks::installer as claude;

    pub const EVENT_TIMEOUT: u64 = 5;
    pub const GEMINI_EVENTS: [&str; 7] = [
        "SessionStart",
        "SessionEnd",
        "BeforeAgent",
        "AfterAgent",
        "BeforeTool",
        "AfterTool",
        "Notification",
    ];
    pub const COPILOT_EVENTS: [&str; 9] = [
        "sessionStart",
        "sessionEnd",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "postToolUseFailure",
        "agentStop",
        "errorOccurred",
        "notification",
    ];
    /// Cursor blocks an action when a hook that may allow or deny it answers
    /// with nothing, so only hooks that only tell are installed.
    pub const CURSOR_EVENTS: [&str; 8] = [
        "sessionStart",
        "sessionEnd",
        "beforeSubmitPrompt",
        "afterAgentThought",
        "afterAgentResponse",
        "postToolUse",
        "postToolUseFailure",
        "stop",
    ];
    pub const CURSOR_CONTINUES: [&str; 1] = ["beforeSubmitPrompt"];
    /// Mistral Vibe's hook types. `post_agent` runs after subagents' turns
    /// too, so the end of the person's turn is heard from `notification`.
    pub const VIBE_EVENTS: [&str; 6] = [
        "session_start",
        "session_end",
        "pre_agent",
        "pre_tool",
        "post_tool",
        "notification",
    ];
    /// Devin CLI's events, in Claude Code's format. `PermissionRequest` is
    /// only heard: the hook answers nothing, so Devin asks in its terminal.
    pub const DEVIN_EVENTS: [&str; 7] = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
    ];

    #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    pub enum State {
        Installed,
        Outdated,
        Missing,
        Unreadable,
    }

    fn env_dir(name: &str) -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    }

    pub fn own_name(app: &str) -> String {
        if crate::legacy::is_legacy(app) {
            format!("vorssaint-{app}")
        } else {
            format!("openbar-{app}")
        }
    }

    pub fn gemini_settings(home: &Path) -> PathBuf {
        env_dir("GEMINI_CLI_HOME")
            .unwrap_or_else(|| home.to_owned())
            .join(".gemini")
            .join("settings.json")
    }

    pub fn copilot_folder(home: &Path) -> PathBuf {
        env_dir("COPILOT_HOME").unwrap_or_else(|| home.join(".copilot"))
    }

    pub fn copilot_hooks(home: &Path, app: &str) -> PathBuf {
        copilot_folder(home)
            .join("hooks")
            .join(format!("{}.json", own_name(app)))
    }

    pub fn cursor_hooks(home: &Path) -> PathBuf {
        home.join(".cursor").join("hooks.json")
    }

    /// OpenCode reads `~/.config/opencode` on Windows too.
    pub fn opencode_folder(home: &Path) -> PathBuf {
        env_dir("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home.join(".config"))
            .join("opencode")
    }

    pub fn opencode_plugin(home: &Path, app: &str) -> PathBuf {
        opencode_folder(home)
            .join("plugins")
            .join(format!("{}.js", own_name(app)))
    }

    pub fn pi_folder(home: &Path) -> PathBuf {
        env_dir("PI_CODING_AGENT_DIR").unwrap_or_else(|| home.join(".pi").join("agent"))
    }

    pub fn pi_extension(home: &Path, app: &str) -> PathBuf {
        pi_folder(home)
            .join("extensions")
            .join(format!("{}.js", own_name(app)))
    }

    pub fn vibe_hooks(home: &Path) -> PathBuf {
        env_dir("VIBE_HOME")
            .unwrap_or_else(|| home.join(".vibe"))
            .join("hooks.toml")
    }

    /// Devin CLI keeps its user config in `%APPDATA%\devin` on Windows.
    pub fn devin_config(home: &Path) -> PathBuf {
        env_dir("APPDATA")
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("devin")
            .join("config.json")
    }

    /// Whether the agent is installed: its command (as New session finds
    /// it), or Cursor's app; for Pi, its command and the folder Pi makes on
    /// first run, since other tools take the name `pi` too. A folder like
    /// `.gemini` or `.copilot` is not enough: other apps make those too.
    pub fn is_present(provider: Provider, home: &Path) -> bool {
        if super::command_name(provider)
            .and_then(crate::agent_usage::agent_executable)
            .is_some()
        {
            return provider != Provider::Pi || home.join(".pi").join("agent").is_dir();
        }
        provider == Provider::Cursor
            && std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Local"))
                .join("Programs")
                .join("cursor")
                .join("Cursor.exe")
                .is_file()
    }

    /// What keeping the hooks in step asks (AgentHookInstaller.plan): each
    /// found agent whose switch is on gets its hooks or an update; one whose
    /// switch is off has the island's taken out. An unreadable file is left alone.
    pub fn plan(
        found: &[Provider],
        hidden: &std::collections::HashSet<Provider>,
        state: impl Fn(Provider) -> State,
    ) -> (Vec<Provider>, Vec<Provider>) {
        let mut install = Vec::new();
        let mut remove = Vec::new();
        for provider in Provider::HOOK_AGENTS {
            let now = state(provider);
            if !hidden.contains(&provider) {
                if found.contains(&provider) && matches!(now, State::Missing | State::Outdated) {
                    install.push(provider);
                }
            } else if matches!(now, State::Installed | State::Outdated) {
                remove.push(provider);
            }
        }
        (install, remove)
    }

    /// The hook for one event: this app, quoted with forward slashes, run
    /// by PowerShell (Gemini CLI and Cursor on Windows) or by any shell.
    pub fn command(provider: Provider, event: &str, exe: &Path, app: &str) -> String {
        let path = exe.to_string_lossy().replace('\\', "/");
        let continues = if provider == Provider::Cursor && CURSOR_CONTINUES.contains(&event) {
            " --continue"
        } else {
            ""
        };
        format!(
            "& \"{path}\" --hook --agent {} --event {event}{continues} {}",
            provider.key(),
            claude::marker(app)
        )
    }

    /// The hook for Mistral Vibe and Devin CLI, whose shell on Windows may be
    /// Git Bash, PowerShell or cmd: this app's path with no quotes (its short
    /// form when the path has a space), which each of them runs as it is.
    /// A path that keeps a space even so falls back to PowerShell's form.
    pub fn portable_command(provider: Provider, event: &str, exe: &Path, app: &str) -> String {
        let path = short_path(exe).to_string_lossy().replace('\\', "/");
        if path.contains(char::is_whitespace) {
            return command(provider, event, exe, app);
        }
        format!(
            "{path} --hook --agent {} --event {event} {}",
            provider.key(),
            claude::marker(app)
        )
    }

    /// The 8.3 form of a path, where Windows keeps one.
    fn short_path(path: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::{OsStrExt, OsStringExt};
            use windows::Win32::Storage::FileSystem::GetShortPathNameW;
            use windows::core::PCWSTR;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut buffer = vec![0u16; 1024];
            let length = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buffer)) } as usize;
            if length > 0 && length < buffer.len() {
                return PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length]));
            }
        }
        path.to_path_buf()
    }

    /// Copilot runs an executable directly, with no shell between.
    pub fn copilot_file(exe: &Path, app: &str) -> Vec<u8> {
        let path = exe.to_string_lossy().into_owned();
        let mut hooks = Map::new();
        for event in COPILOT_EVENTS {
            hooks.insert(
                event.to_owned(),
                json!([{
                    "type": "command",
                    "exec": path,
                    "args": ["--hook", "--agent", "copilot", "--event", event, "--marker", claude::marker_name(app), "--app", app],
                    "timeoutSec": EVENT_TIMEOUT,
                }]),
            );
        }
        let mut data =
            serde_json::to_vec_pretty(&json!({ "version": 1, "hooks": hooks })).unwrap_or_default();
        data.push(b'\n');
        data
    }

    pub fn opencode_file(endpoint: &str, app: &str) -> Vec<u8> {
        let literal = serde_json::to_string(endpoint).unwrap_or_else(|_| "\"\"".into());
        PLUGIN
            .replace("\"__OPENBAR_SOCKET__\"", &literal)
            .replace("__OPENBAR_MARKER__", &claude::marker(app))
            .into_bytes()
    }

    const PLUGIN: &str = include_str!("opencode-plugin.js");
    const PI_EXTENSION: &str = include_str!("pi-extension.js");

    pub fn pi_file(endpoint: &str, app: &str) -> Vec<u8> {
        let literal = serde_json::to_string(endpoint).unwrap_or_else(|_| "\"\"".into());
        PI_EXTENSION
            .replace("\"__OPENBAR_SOCKET__\"", &literal)
            .replace("__OPENBAR_MARKER__", &claude::marker(app))
            .into_bytes()
    }

    // MARK: Vibe's hooks.toml

    /// The lines that open and close the island's entries in hooks.toml.
    pub fn vibe_start(app: &str) -> String {
        format!("{} start: added by the island, removed by it", claude::marker(app))
    }

    pub fn vibe_end(app: &str) -> String {
        format!("{} end", claude::marker(app))
    }

    /// The island's `[[hooks]]` entries between its two marker lines; names
    /// carry the app's own, so two builds never clash.
    pub fn vibe_block(exe: &Path, app: &str) -> String {
        let mut lines = vec![vibe_start(app)];
        for event in VIBE_EVENTS {
            lines.push("[[hooks]]".into());
            lines.push(format!("name = {}", toml_string(&format!("{}-{event}", own_name(app)))));
            lines.push(format!("type = {}", toml_string(event)));
            lines.push(format!(
                "command = {}",
                toml_string(&portable_command(Provider::Vibe, event, exe, app))
            ));
            lines.push(format!("timeout = {EVENT_TIMEOUT}.0"));
        }
        lines.push(vibe_end(app));
        lines.join("\n") + "\n"
    }

    /// A TOML basic string.
    pub fn toml_string(text: &str) -> String {
        let mut out = String::from("\"");
        for c in text.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 0x20 || c as u32 == 0x7F => {
                    out.push_str(&format!("\\u{:04X}", c as u32))
                }
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// The file without the island's entries; None when it had none.
    pub fn vibe_removing(text: &str, app: &str) -> Option<String> {
        let start = vibe_start(app);
        let end = vibe_end(app);
        let mut lines: Vec<&str> = text.split('\n').collect();
        let mut changed = false;
        while let Some(first) = lines.iter().position(|l| l.trim_end_matches('\r') == start) {
            let last = lines[first..]
                .iter()
                .position(|l| l.trim_end_matches('\r') == end)
                .map(|i| first + i)
                .unwrap_or(lines.len() - 1);
            lines.drain(first..=last);
            changed = true;
        }
        if !changed {
            return None;
        }
        let mut result = lines.join("\n");
        while result.ends_with("\n\n") {
            result.pop();
        }
        Some(if result.trim().is_empty() { String::new() } else { result })
    }

    /// The file with the island's entries at its end, after the person's own.
    pub fn vibe_installing(text: Option<&str>, exe: &Path, app: &str) -> String {
        let mut base = text
            .map(|t| vibe_removing(t, app).unwrap_or_else(|| t.to_owned()))
            .unwrap_or_default();
        if !base.is_empty() && !base.ends_with('\n') {
            base.push('\n');
        }
        if !base.is_empty() {
            base.push('\n');
        }
        base + &vibe_block(exe, app)
    }

    /// Whether hooks.toml keeps its hooks as an inline array, which the
    /// island's tables can't sit beside, so the file is left alone.
    pub fn vibe_inline(text: &str) -> bool {
        text.lines().any(|line| {
            line.trim()
                .strip_prefix("hooks")
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
    }

    fn vibe_text(path: &Path) -> Result<Option<String>, ()> {
        match std::fs::read(path) {
            Ok(data) => String::from_utf8(data).map(Some).map_err(|_| ()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(()),
        }
    }

    fn groups_hold(settings: &Value, events: &[&str], command: impl Fn(&str) -> String) -> bool {
        let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
            return false;
        };
        events.iter().all(|event| {
            hooks
                .get(*event)
                .and_then(Value::as_array)
                .is_some_and(|groups| {
                    groups.iter().any(|g| {
                        g.get("hooks").and_then(Value::as_array).is_some_and(|e| {
                            e.iter().any(|h| {
                                h.get("command").and_then(Value::as_str) == Some(&command(event))
                            })
                        })
                    })
                })
        })
    }

    fn flat_has_any(settings: &Value, app: &str) -> bool {
        let marker = claude::marker(app);
        settings
            .get("hooks")
            .and_then(Value::as_object)
            .is_some_and(|hooks| {
                hooks
                    .values()
                    .filter_map(Value::as_array)
                    .flatten()
                    .any(|h| {
                        h.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| c.contains(&marker))
                    })
            })
    }

    /// Cursor's hooks without the island's; events left empty go too.
    pub fn flat_removing(settings: &Value, app: &str) -> Value {
        let marker = claude::marker(app);
        let mut settings = settings.clone();
        if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
            let events: Vec<String> = hooks.keys().cloned().collect();
            for event in events {
                let Some(list) = hooks.get(&event).and_then(Value::as_array) else {
                    continue;
                };
                let kept: Vec<Value> = list
                    .iter()
                    .filter(|h| {
                        !h.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| c.contains(&marker))
                    })
                    .cloned()
                    .collect();
                if kept.len() == list.len() {
                    continue;
                }
                if kept.is_empty() {
                    hooks.remove(&event);
                } else {
                    hooks.insert(event, Value::Array(kept));
                }
            }
        }
        settings
    }

    pub fn gemini_installing(settings: &Value, exe: &Path, app: &str) -> Value {
        let mut settings = claude::removing(settings, app);
        if !settings.is_object() {
            settings = json!({});
        }
        let object = settings.as_object_mut().unwrap();
        let hooks = object.entry("hooks").or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        let hooks = hooks.as_object_mut().unwrap();
        for event in GEMINI_EVENTS {
            let entry = json!({
                "type": "command",
                "name": "openbar",
                "timeout": EVENT_TIMEOUT * 1000,
                "command": command(Provider::Gemini, event, exe, app),
            });
            let list = hooks.entry(event).or_insert_with(|| json!([]));
            if let Some(list) = list.as_array_mut() {
                list.push(json!({ "hooks": [entry] }));
            }
        }
        settings
    }

    pub fn cursor_installing(settings: &Value, exe: &Path, app: &str) -> Value {
        let mut settings = flat_removing(settings, app);
        if !settings.is_object() {
            settings = json!({});
        }
        let object = settings.as_object_mut().unwrap();
        object.entry("version").or_insert(json!(1));
        let hooks = object.entry("hooks").or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        let hooks = hooks.as_object_mut().unwrap();
        for event in CURSOR_EVENTS {
            let entry = json!({ "command": command(Provider::Cursor, event, exe, app), "timeout": EVENT_TIMEOUT });
            let list = hooks.entry(event).or_insert_with(|| json!([]));
            if let Some(list) = list.as_array_mut() {
                list.push(entry);
            }
        }
        settings
    }

    pub fn state(provider: Provider, home: &Path, exe: &Path, endpoint: &str, app: &str) -> State {
        let read = |path: &Path| claude::read(path);
        match provider {
            Provider::Gemini => match read(&gemini_settings(home)) {
                Ok(s)
                    if groups_hold(&s, &GEMINI_EVENTS, |e| {
                        command(Provider::Gemini, e, exe, app)
                    }) =>
                {
                    State::Installed
                }
                Ok(s) if claude::has_any(&s, app) => State::Outdated,
                Ok(_) => State::Missing,
                Err(_) => State::Unreadable,
            },
            Provider::Cursor => match read(&cursor_hooks(home)) {
                Ok(s) => {
                    let current = CURSOR_EVENTS.iter().all(|event| {
                        s.pointer(&format!("/hooks/{event}"))
                            .and_then(Value::as_array)
                            .is_some_and(|l| {
                                l.iter().any(|h| {
                                    h.get("command").and_then(Value::as_str)
                                        == Some(&command(Provider::Cursor, event, exe, app))
                                })
                            })
                    });
                    if current {
                        State::Installed
                    } else if flat_has_any(&s, app) {
                        State::Outdated
                    } else {
                        State::Missing
                    }
                }
                Err(_) => State::Unreadable,
            },
            Provider::Copilot => own_state(&copilot_hooks(home, app), &copilot_file(exe, app)),
            Provider::Opencode => {
                own_state(&opencode_plugin(home, app), &opencode_file(endpoint, app))
            }
            Provider::Pi => own_state(&pi_extension(home, app), &pi_file(endpoint, app)),
            Provider::Vibe => match vibe_text(&vibe_hooks(home)) {
                Ok(None) => State::Missing,
                Ok(Some(text)) if text.contains(&vibe_block(exe, app)) => State::Installed,
                Ok(Some(text)) if text.contains(&claude::marker(app)) => State::Outdated,
                Ok(Some(text)) if vibe_inline(&text) => State::Unreadable,
                Ok(Some(_)) => State::Missing,
                Err(_) => State::Unreadable,
            },
            Provider::Devin => match read(&devin_config(home)) {
                Ok(s)
                    if groups_hold(&s, &DEVIN_EVENTS, |e| {
                        portable_command(Provider::Devin, e, exe, app)
                    }) =>
                {
                    State::Installed
                }
                Ok(s) if claude::has_any(&s, app) => State::Outdated,
                Ok(_) => State::Missing,
                Err(_) => State::Unreadable,
            },
            _ => State::Missing,
        }
    }

    pub fn devin_installing(settings: &Value, exe: &Path, app: &str) -> Value {
        let mut settings = claude::removing(settings, app);
        if !settings.is_object() {
            settings = json!({});
        }
        let object = settings.as_object_mut().unwrap();
        // Devin writes its own file with a version; a new one gets the same.
        object.entry("version").or_insert(json!(1));
        let hooks = object.entry("hooks").or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        let hooks = hooks.as_object_mut().unwrap();
        for event in DEVIN_EVENTS {
            let entry = json!({
                "type": "command",
                "timeout": EVENT_TIMEOUT,
                "command": portable_command(Provider::Devin, event, exe, app),
            });
            let list = hooks.entry(event).or_insert_with(|| json!([]));
            if let Some(list) = list.as_array_mut() {
                list.push(json!({ "hooks": [entry] }));
            }
        }
        settings
    }

    fn own_state(path: &Path, wanted: &[u8]) -> State {
        match std::fs::read(path) {
            Ok(data) if data == wanted => State::Installed,
            Ok(_) => State::Outdated,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::Missing,
            Err(_) => State::Unreadable,
        }
    }

    pub fn install(
        provider: Provider,
        home: &Path,
        exe: &Path,
        endpoint: &str,
        app: &str,
    ) -> Result<(), String> {
        match provider {
            Provider::Gemini => {
                claude::update(&gemini_settings(home), |s| gemini_installing(s, exe, app))
                    .map_err(|e| e.to_string())
            }
            Provider::Cursor => {
                claude::update(&cursor_hooks(home), |s| cursor_installing(s, exe, app))
                    .map_err(|e| e.to_string())
            }
            Provider::Copilot => write_own(&copilot_hooks(home, app), &copilot_file(exe, app), app),
            Provider::Opencode => write_own(
                &opencode_plugin(home, app),
                &opencode_file(endpoint, app),
                app,
            ),
            Provider::Pi => write_own(&pi_extension(home, app), &pi_file(endpoint, app), app),
            Provider::Vibe => {
                let path = vibe_hooks(home);
                if let Ok(Some(text)) = vibe_text(&path)
                    && vibe_inline(&text)
                {
                    return Err(format!("{} keeps its hooks inline", path.display()));
                }
                claude::update_text(&path, |text| Some(vibe_installing(text, exe, app)))
                    .map_err(|e| e.to_string())
            }
            Provider::Devin => {
                claude::update(&devin_config(home), |s| devin_installing(s, exe, app))
                    .map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }

    pub fn remove(provider: Provider, home: &Path, app: &str) -> Result<(), String> {
        match provider {
            Provider::Gemini => {
                let path = gemini_settings(home);
                if !path.exists() {
                    return Ok(());
                }
                claude::update(&path, |s| claude::removing(s, app)).map_err(|e| e.to_string())
            }
            Provider::Cursor => {
                let path = cursor_hooks(home);
                if !path.exists() {
                    return Ok(());
                }
                claude::update(&path, |s| flat_removing(s, app)).map_err(|e| e.to_string())
            }
            Provider::Copilot => remove_own(&copilot_hooks(home, app), app),
            Provider::Opencode => remove_own(&opencode_plugin(home, app), app),
            Provider::Pi => remove_own(&pi_extension(home, app), app),
            Provider::Vibe => {
                let path = vibe_hooks(home);
                if !path.exists() {
                    return Ok(());
                }
                claude::update_text(&path, |text| text.and_then(|t| vibe_removing(t, app)))
                    .map_err(|e| e.to_string())
            }
            Provider::Devin => {
                let path = devin_config(home);
                if !path.exists() {
                    return Ok(());
                }
                claude::update(&path, |s| claude::removing(s, app)).map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }

    fn holds_marker(data: &[u8], app: &str) -> bool {
        let marker = claude::marker(app);
        let copilot = format!("\"--app\",\n          \"{app}\"");
        let text = String::from_utf8_lossy(data);
        text.contains(&marker)
            || (text.contains(claude::marker_name(app))
                && (text.contains(&copilot) || text.contains(app)))
    }

    /// Writes a file of the island's own, never over one that is not.
    fn write_own(path: &Path, data: &[u8], app: &str) -> Result<(), String> {
        if let Ok(existing) = std::fs::read(path)
            && !holds_marker(&existing, app)
        {
            return Err(format!("{} is not the island's", path.display()));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, data).map_err(|e| e.to_string())
    }

    fn remove_own(path: &Path, app: &str) -> Result<(), String> {
        match std::fs::read(path) {
            Ok(existing) if holds_marker(&existing, app) => {
                std::fs::remove_file(path).map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Time = 1_800_000_000.0;
    const ON: ContentPolicy = ContentPolicy { previews: true };
    const OFF: ContentPolicy = ContentPolicy { previews: false };

    fn envelope(provider: &str, hook: &str, event: Value) -> Vec<u8> {
        serde_json::to_vec(
            &json!({ "openbar": provider, "hook": hook, "pid": 4242, "event": event }),
        )
        .unwrap()
    }

    fn feed(tracker: &mut Tracker, data: &[u8], policy: ContentPolicy) {
        for event in decode(data, NOW, policy)
            .map(|d| d.events)
            .unwrap_or_default()
        {
            tracker.ingest(event, NOW);
        }
    }

    fn snap(tracker: &mut Tracker, now: Time, previews: bool) -> (Vec<Session>, Vec<SessionEvent>) {
        tracker.snapshot(
            now,
            ContentPolicy { previews },
            |_| Place {
                host: HostKind::WindowsTerminal,
                accepts_replies: true,
                host_name: None,
            },
            |_| Some("main".into()),
        )
    }

    #[test]
    fn envelopes_are_told_apart() {
        let gemini = envelope(
            "gemini",
            "SessionStart",
            json!({ "session_id": "g", "cwd": "C:/w/app" }),
        );
        assert!(is_envelope(&gemini) && !waits(&gemini));
        let claude =
            serde_json::to_vec(&json!({ "hook_event_name": "Stop", "session_id": "c" })).unwrap();
        assert!(!is_envelope(&claude) && decode(&claude, NOW, ON).is_none());
        assert!(decode(&envelope("claude", "", json!({})), NOW, ON).is_none());
        assert!(waits(&envelope(
            "opencode",
            "",
            json!({ "kind": "permission", "session": "s", "id": "p" })
        )));
        assert!(waits(&envelope(
            "opencode",
            "",
            json!({ "kind": "listen" })
        )));
        assert!(!waits(&envelope(
            "opencode",
            "",
            json!({ "kind": "status" })
        )));
        assert_eq!(decode(&gemini, NOW, ON).unwrap().events[0].id(), "gemini:g");
    }

    #[test]
    fn a_gemini_turn_goes_through_its_phases() {
        let mut t = Tracker::default();
        let ev = |hook: &str, extra: Value| {
            let mut base =
                json!({ "session_id": "g1", "cwd": "C:/w/app", "hook_event_name": hook });
            base.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            envelope("gemini", hook, base)
        };
        feed(&mut t, &ev("SessionStart", json!({})), ON);
        assert_eq!(snap(&mut t, NOW, true).0[0].phase, SessionPhase::Idle);
        feed(
            &mut t,
            &ev("BeforeAgent", json!({ "prompt": "fix it" })),
            ON,
        );
        feed(
            &mut t,
            &ev(
                "BeforeTool",
                json!({ "tool_name": "run_shell_command", "tool_input": { "command": "npm test" } }),
            ),
            ON,
        );
        let s = &snap(&mut t, NOW, true).0[0];
        assert_eq!(s.phase, SessionPhase::RunningTool);
        assert_eq!(s.current_tool.as_ref().unwrap().name, "Bash");
        assert_eq!(
            s.current_tool.as_ref().unwrap().summary.as_deref(),
            Some("npm test")
        );
        feed(
            &mut t,
            &ev(
                "Notification",
                json!({ "notification_type": "ToolPermission", "details": { "command": "npm test" } }),
            ),
            ON,
        );
        let (sessions, events) = snap(&mut t, NOW, true);
        assert_eq!(sessions[0].phase, SessionPhase::WaitingPermission);
        assert!(sessions[0].permission.is_none());
        assert_eq!(sessions[0].waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        assert!(matches!(events[0], SessionEvent::NeedsPermission { .. }));
        assert!(sessions[0].waiting_diffs.is_empty());
        // An edit Gemini asks about shows what it changes, though it is answered there.
        feed(
            &mut t,
            &ev(
                "Notification",
                json!({ "notification_type": "ToolPermission", "details": {
                    "type": "edit", "filePath": "C:\\w\\a.ts", "fileName": "a.ts",
                    "originalContent": "a\nb\n", "newContent": "a\nc\n" } }),
            ),
            ON,
        );
        let (sessions, _) = snap(&mut t, NOW, true);
        assert!(sessions[0].permission.is_none());
        assert_eq!(sessions[0].waiting_diffs.len(), 1);
        assert_eq!(
            (
                sessions[0].waiting_diffs[0].added,
                sessions[0].waiting_diffs[0].removed
            ),
            (1, 1)
        );
        let value = serde_json::to_value(&sessions[0]).unwrap();
        assert_eq!(value["waitingDiffs"][0]["path"], "C:\\w\\a.ts");
        feed(
            &mut t,
            &ev(
                "AfterTool",
                json!({ "tool_name": "run_shell_command", "tool_input": { "command": "npm test" }, "tool_response": {} }),
            ),
            ON,
        );
        assert_eq!(snap(&mut t, NOW, true).0[0].phase, SessionPhase::Generating);
        feed(
            &mut t,
            &ev("AfterAgent", json!({ "prompt_response": "All green." })),
            ON,
        );
        let (sessions, events) = snap(&mut t, NOW, true);
        assert_eq!(sessions[0].phase, SessionPhase::Finished);
        assert_eq!(sessions[0].last_text.as_deref(), Some("All green."));
        assert!(matches!(events[0], SessionEvent::Finished { .. }));
        feed(&mut t, &ev("BeforeAgent", json!({ "prompt": "again" })), ON);
        feed(
            &mut t,
            &ev(
                "AfterAgent",
                json!({ "prompt_response": "[no response text]" }),
            ),
            ON,
        );
        assert_eq!(
            snap(&mut t, NOW, true).0[0].last_text.as_deref(),
            Some("All green.")
        );
        feed(&mut t, &ev("SessionEnd", json!({})), ON);
        assert!(snap(&mut t, NOW, true).0.is_empty());
    }

    #[test]
    fn copilot_and_cursor_read_their_own_shapes() {
        let mut t = Tracker::default();
        let base =
            json!({ "sessionId": "c1", "cwd": "C:/w/api", "timestamp": 1_800_000_000_000_i64 });
        let ev = |hook: &str, extra: Value| {
            let mut b = base.clone();
            b.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            envelope("copilot", hook, b)
        };
        feed(
            &mut t,
            &ev("userPromptSubmitted", json!({ "prompt": "go" })),
            ON,
        );
        feed(
            &mut t,
            &ev(
                "preToolUse",
                json!({ "toolName": "bash", "toolArgs": "{\"command\":\"ls\"}" }),
            ),
            ON,
        );
        assert_eq!(
            snap(&mut t, NOW, true).0[0]
                .current_tool
                .as_ref()
                .unwrap()
                .summary
                .as_deref(),
            Some("ls")
        );
        feed(
            &mut t,
            &ev(
                "notification",
                json!({ "notification_type": "permission_prompt", "message": "Allow ls?" }),
            ),
            ON,
        );
        assert_eq!(
            snap(&mut t, NOW, true).0[0].phase,
            SessionPhase::WaitingPermission
        );
        feed(
            &mut t,
            &ev(
                "postToolUseFailure",
                json!({ "toolName": "bash", "toolArgs": "{\"command\":\"ls\"}" }),
            ),
            ON,
        );
        let s = &snap(&mut t, NOW, true).0[0];
        assert_eq!(s.phase, SessionPhase::Generating);
        assert!(s.timeline[0].failed);

        let mut k = Tracker::default();
        let cursor = |hook: &str, extra: Value| {
            let mut b = json!({ "conversation_id": "k", "workspace_roots": ["C:/w/site"], "model": "composer-2" });
            b.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            envelope("cursor", hook, b)
        };
        feed(
            &mut k,
            &cursor("beforeSubmitPrompt", json!({ "prompt": "pink" })),
            ON,
        );
        feed(
            &mut k,
            &cursor(
                "postToolUse",
                json!({ "tool_name": "Shell", "tool_input": { "command": "npm run build" }, "duration": 900 }),
            ),
            ON,
        );
        feed(
            &mut k,
            &cursor("afterAgentResponse", json!({ "text": "Done." })),
            ON,
        );
        feed(
            &mut k,
            &cursor("stop", json!({ "status": "completed" })),
            ON,
        );
        let s = &snap(&mut k, NOW, true).0[0];
        assert_eq!(s.project, "site");
        assert_eq!(s.model.as_deref(), Some("composer-2"));
        assert_eq!(s.timeline[0].name, "Bash");
        assert_eq!(s.phase, SessionPhase::Finished);
        assert_eq!(s.last_text.as_deref(), Some("Done."));
        k.prune(NOW + QUIET_FOR + 60.0, |_| true);
        assert!(k.is_empty());
    }

    #[test]
    fn opencode_permissions_are_answered_here() {
        let mut t = Tracker::default();
        let ev = |fields: Value| envelope("opencode", "", fields);
        feed(
            &mut t,
            &ev(json!({ "kind": "created", "session": "s", "directory": "C:/w/cli" })),
            ON,
        );
        feed(
            &mut t,
            &ev(json!({ "kind": "status", "session": "s", "status": "busy" })),
            ON,
        );
        let ask = ev(
            json!({ "kind": "permission", "session": "s", "id": "per", "permission": "bash", "metadata": { "command": "rm -rf build" }, "always": ["rm *"] }),
        );
        assert_eq!(
            decode(&ask, NOW, OFF).unwrap().permission,
            Some(("s".into(), "per".into()))
        );
        feed(&mut t, &ask, OFF);
        let s = &snap(&mut t, NOW, false).0[0];
        assert_eq!(s.phase, SessionPhase::WaitingPermission);
        let permission = s.permission.as_ref().unwrap();
        assert_eq!(permission.detail, "rm -rf build");
        assert!(permission.offers_always);
        assert_eq!(
            opencode_reply(&super::super::hook_message::Decision::Allow).unwrap(),
            br#"{"reply":"once"}"#
        );
        assert_eq!(
            opencode_reply(&super::super::hook_message::Decision::Deny).unwrap(),
            br#"{"reply":"reject"}"#
        );
        feed(
            &mut t,
            &ev(json!({ "kind": "permissionReplied", "session": "s", "id": "per" })),
            ON,
        );
        assert_eq!(snap(&mut t, NOW, true).0[0].phase, SessionPhase::Generating);
        feed(
            &mut t,
            &ev(
                json!({ "kind": "message", "session": "s", "id": "m", "model": "big-pickle", "cost": 0.5, "tokens": { "input": 10, "output": 2, "reasoning": 1, "cache": { "read": 3, "write": 4 } } }),
            ),
            ON,
        );
        feed(
            &mut t,
            &ev(json!({ "kind": "status", "session": "s", "status": "idle" })),
            ON,
        );
        let s = &snap(&mut t, NOW, true).0[0];
        assert_eq!(s.phase, SessionPhase::Finished);
        assert_eq!(s.tokens.output, 3);
        assert_eq!(s.model.as_deref(), Some("big-pickle"));
        assert!(
            decode(&ev(json!({ "kind": "listen" })), NOW, ON)
                .unwrap()
                .listens
        );
    }

    #[test]
    fn sessions_of_a_quit_agent_leave_and_previews_stay_off() {
        let mut t = Tracker::default();
        feed(
            &mut t,
            &envelope(
                "gemini",
                "BeforeAgent",
                json!({ "session_id": "a", "prompt": "secret" }),
            ),
            OFF,
        );
        assert!(snap(&mut t, NOW, false).0[0].chat.is_empty());
        t.prune(NOW, |_| false);
        assert!(t.is_empty());
    }

    #[test]
    fn the_agent_is_the_first_process_above_the_shells() {
        let chain = vec![
            (10, "Openbar.exe".to_owned()),
            (9, "powershell.exe".to_owned()),
            (8, "node.exe".to_owned()),
            (7, "WindowsTerminal.exe".to_owned()),
        ];
        assert_eq!(agent_process(&chain, 3), 8);
        assert_eq!(agent_process(&[], 3), 3);
    }

    #[test]
    fn hooks_go_in_beside_the_persons_own_and_come_out_alone() {
        let exe = Path::new("C:\\Program Files\\Openbar\\Openbar.exe");
        let own = json!({ "hooks": { "BeforeTool": [{ "matcher": "write_file", "hooks": [{ "type": "command", "command": "mine.ps1" }] }] } });
        let installed = installer::gemini_installing(&own, exe, "app");
        let tools = installed
            .pointer("/hooks/BeforeTool")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(
            tools[0],
            own.pointer("/hooks/BeforeTool/0").unwrap().clone()
        );
        let command = installer::command(Provider::Gemini, "BeforeTool", exe, "app");
        assert!(command.starts_with(
            "& \"C:/Program Files/Openbar/Openbar.exe\" --hook --agent gemini --event BeforeTool"
        ));
        assert_eq!(crate::hooks::installer::removing(&installed, "app"), own);

        let cursor_own = json!({ "version": 1, "hooks": { "stop": [{ "command": "audit.ps1" }] } });
        let cursor = installer::cursor_installing(&cursor_own, exe, "app");
        assert_eq!(
            cursor
                .pointer("/hooks/stop")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            installer::command(Provider::Cursor, "beforeSubmitPrompt", exe, "app")
                .contains("--continue")
        );
        assert_eq!(installer::flat_removing(&cursor, "app"), cursor_own);
        assert!(!installer::CURSOR_EVENTS.contains(&"beforeShellExecution"));

        let copilot: Value = serde_json::from_slice(&installer::copilot_file(exe, "app")).unwrap();
        assert_eq!(copilot["version"], 1);
        assert!(copilot["hooks"].get("permissionRequest").is_none());
        assert_eq!(
            copilot["hooks"]["preToolUse"][0]["exec"],
            exe.to_string_lossy().as_ref()
        );

        let plugin =
            String::from_utf8(installer::opencode_file(r"\\.\pipe\openbar-x", "app")).unwrap();
        assert!(plugin.contains(r#"const SOCKET = "\\\\.\\pipe\\openbar-x""#));
        assert!(!plugin.contains("__OPENBAR"));
    }

    #[test]
    fn found_agents_are_connected_and_switched_off_ones_taken_out() {
        use installer::{State, plan};
        let none = std::collections::HashSet::new();
        let states = |p| match p {
            Provider::Gemini => State::Missing,
            Provider::Copilot => State::Outdated,
            Provider::Opencode => State::Installed,
            _ => State::Unreadable,
        };
        let all = Provider::HOOK_AGENTS;
        assert_eq!(
            plan(&all, &none, states),
            (vec![Provider::Gemini, Provider::Copilot], vec![])
        );
        assert_eq!(plan(&[], &none, states), (vec![], vec![]));
        let hidden: std::collections::HashSet<_> = all.into_iter().collect();
        assert_eq!(
            plan(&all, &hidden, states),
            (vec![], vec![Provider::Copilot, Provider::Opencode])
        );
    }

    #[test]
    fn new_session_starts_each_agent_with_its_prompt() {
        assert_eq!(
            start_arguments(Provider::Gemini, " fix it "),
            ["-i", "fix it"]
        );
        assert_eq!(
            start_arguments(Provider::Copilot, "fix it"),
            ["-i", "fix it"]
        );
        assert_eq!(
            start_arguments(Provider::Opencode, "fix it"),
            ["--prompt", "fix it"]
        );
        assert_eq!(start_arguments(Provider::Cursor, "fix it"), ["fix it"]);
        assert!(start_arguments(Provider::Gemini, "  ").is_empty());
        assert_eq!(command_name(Provider::Cursor), Some("cursor-agent"));
        assert_eq!(command_name(Provider::Claude), None);
    }

    #[test]
    fn the_plugin_is_the_one_the_mac_installs() {
        let mac = include_str!(
            "../../../../Sources/Openbar/Services/ClaudeSessions/AgentOpenCodePlugin.swift"
        );
        assert!(mac.contains(include_str!("opencode-plugin.js")));
    }

    #[test]
    fn the_pi_extension_is_the_one_the_mac_installs() {
        let mac = include_str!(
            "../../../../Sources/Openbar/Services/ClaudeSessions/AgentPiExtension.swift"
        );
        assert!(mac.contains(include_str!("pi-extension.js")));
        let file = String::from_utf8(installer::pi_file(r"\\.\pipe\openbar-x", "app")).unwrap();
        assert!(file.contains(r#"const SOCKET = "\\\\.\\pipe\\openbar-x""#));
        assert!(file.contains("pi.sendUserMessage") && !file.contains("__OPENBAR"));
    }

    #[test]
    fn a_pi_session_is_told_by_its_extension() {
        let mut tracker = Tracker::default();
        let send = |tracker: &mut Tracker, event: Value| {
            let mut event = event;
            event["session"] = json!("p1");
            event["directory"] = json!("C:/w/pi-app");
            feed(tracker, &envelope("pi", "", event), ON);
        };
        send(&mut tracker, json!({ "kind": "created" }));
        send(&mut tracker, json!({ "kind": "model", "model": "claude-sonnet-4-5" }));
        send(&mut tracker, json!({ "kind": "prompt", "text": "list the files" }));
        send(&mut tracker, json!({ "kind": "status", "status": "busy" }));
        send(
            &mut tracker,
            json!({ "kind": "tool", "call": "c1", "tool": "bash", "status": "running", "input": { "command": "ls -la" } }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].provider, Provider::Pi);
        assert_eq!(listed[0].id, "pi:p1");
        assert_eq!(listed[0].phase, SessionPhase::RunningTool);
        assert_eq!(listed[0].model.as_deref(), Some("claude-sonnet-4-5"));
        send(&mut tracker, json!({ "kind": "tool", "call": "c1", "tool": "bash", "status": "completed" }));
        send(&mut tracker, json!({ "kind": "reply", "text": "Here they are." }));
        send(&mut tracker, json!({ "kind": "question" }));
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::WaitingInput);
        assert_eq!(listed[0].waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        send(&mut tracker, json!({ "kind": "permissionReplied" }));
        send(&mut tracker, json!({ "kind": "status", "status": "idle" }));
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::Finished);
        assert_eq!(listed[0].last_text.as_deref(), Some("Here they are."));
        // Pi asks no permission of its own; only OpenCode's plugin waits on one.
        send(&mut tracker, json!({ "kind": "permission", "id": "x", "permission": "bash" }));
        assert!(snap(&mut tracker, NOW, true).0[0].permission.is_none());
        assert!(waits(&envelope("pi", "", json!({ "kind": "listen" }))));
        assert!(!waits(&envelope("pi", "", json!({ "kind": "permission", "session": "p1", "id": "x" }))));
        assert!(decode(&envelope("pi", "", json!({ "kind": "listen" })), NOW, ON).unwrap().listens);
        assert!(Provider::Pi.reports_through_plugin() && !Provider::Pi.answers_permissions());
        send(&mut tracker, json!({ "kind": "deleted" }));
        assert!(snap(&mut tracker, NOW, true).0.is_empty());
    }

    #[test]
    fn a_vibe_session_is_told_by_its_hooks() {
        let mut tracker = Tracker::default();
        let send = |tracker: &mut Tracker, hook: &str, fields: Value| {
            let mut event = json!({ "session_id": "v1", "cwd": "C:/w/vibe-app", "parent_session_id": null,
                                     "transcript_path": "C:/vibe/logs/session/unified/v1", "hook_event_name": hook });
            for (k, v) in fields.as_object().unwrap() {
                event[k] = v.clone();
            }
            feed(tracker, &envelope("vibe", hook, event), ON);
        };
        send(&mut tracker, "session_start", json!({ "model": "mistral-medium-3.5" }));
        send(&mut tracker, "pre_agent", json!({ "prompt": "fix the tests" }));
        send(
            &mut tracker,
            "pre_tool",
            json!({ "tool_name": "file_system.bash", "tool_call_id": "call_1", "tool_input": { "command": "npm test" } }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].provider, Provider::Vibe);
        assert_eq!(listed[0].phase, SessionPhase::RunningTool);
        assert_eq!(listed[0].current_tool.as_ref().unwrap().name, "Bash");
        assert_eq!(listed[0].current_tool.as_ref().unwrap().id, "call_1");
        assert_eq!(listed[0].model.as_deref(), Some("mistral-medium-3.5"));
        send(
            &mut tracker,
            "notification",
            json!({ "notification_type": "approval_requested", "tool_name": "file_system.bash", "message": "Vibe needs your approval" }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::WaitingPermission);
        assert!(listed[0].permission.is_none());
        assert_eq!(listed[0].waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        send(
            &mut tracker,
            "post_tool",
            json!({ "tool_name": "file_system.bash", "tool_call_id": "call_1", "tool_input": { "command": "npm test" }, "tool_status": "failure" }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::Generating);
        assert!(listed[0].timeline.last().unwrap().failed);
        // A subagent adds no row.
        feed(
            &mut tracker,
            &envelope(
                "vibe",
                "pre_tool",
                json!({ "session_id": "sub", "parent_session_id": "v1", "tool_name": "file_system.read_file" }),
            ),
            ON,
        );
        assert_eq!(snap(&mut tracker, NOW, true).0.len(), 1);
        send(&mut tracker, "notification", json!({ "notification_type": "turn_completed" }));
        assert_eq!(snap(&mut tracker, NOW, true).0[0].phase, SessionPhase::Finished);
        let decoded = decode(
            &envelope("vibe", "notification", json!({ "session_id": "v1", "transcript_path": "C:/x" })),
            NOW,
            ON,
        )
        .unwrap();
        assert_eq!(decoded.transcript.as_deref(), Some("C:/x"));
        send(&mut tracker, "session_end", json!({ "reason": "exit" }));
        assert!(snap(&mut tracker, NOW, true).0.is_empty());
    }

    #[test]
    fn a_devin_session_is_told_by_its_hooks_and_its_claude_hooks_are_not() {
        let mut tracker = Tracker::default();
        let send = |tracker: &mut Tracker, hook: &str, fields: Value| {
            let mut event = json!({ "session_id": "d1", "prompt_id": "t1", "hook_event_name": hook });
            for (k, v) in fields.as_object().unwrap() {
                event[k] = v.clone();
            }
            feed(tracker, &envelope("devin", hook, event), ON);
        };
        send(&mut tracker, "SessionStart", json!({ "source": "startup" }));
        send(&mut tracker, "UserPromptSubmit", json!({ "prompt": "add a test" }));
        send(&mut tracker, "PreToolUse", json!({ "tool_name": "exec", "tool_input": { "command": "cargo test" } }));
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].provider, Provider::Devin);
        assert_eq!(listed[0].current_tool.as_ref().unwrap().name, "Bash");
        send(&mut tracker, "PermissionRequest", json!({ "tool_name": "exec", "tool_input": { "command": "git push" } }));
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::WaitingPermission);
        assert!(listed[0].permission.is_none());
        send(
            &mut tracker,
            "PostToolUse",
            json!({ "tool_name": "exec", "tool_input": { "command": "cargo test" }, "tool_response": { "success": false } }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::Generating);
        assert!(listed[0].timeline[0].failed);
        send(&mut tracker, "PreToolUse", json!({ "tool_name": "ask_user_question", "tool_input": {} }));
        assert_eq!(snap(&mut tracker, NOW, true).0[0].phase, SessionPhase::WaitingInput);
        send(&mut tracker, "PostToolUse", json!({ "tool_name": "ask_user_question", "tool_input": {} }));
        assert_eq!(snap(&mut tracker, NOW, true).0[0].phase, SessionPhase::Generating);
        send(&mut tracker, "Stop", json!({}));
        assert_eq!(snap(&mut tracker, NOW, true).0[0].phase, SessionPhase::Finished);
        send(&mut tracker, "SessionEnd", json!({}));
        assert!(snap(&mut tracker, NOW, true).0.is_empty());
        assert!(!waits(&envelope("devin", "PermissionRequest", json!({ "session_id": "d1" }))));

        let devin = serde_json::to_vec(&json!({ "hook_event_name": "PreToolUse", "session_id": "d", "prompt_id": "t" })).unwrap();
        let claude = serde_json::to_vec(&json!({ "hook_event_name": "PreToolUse", "session_id": "c", "prompt_id": "t", "transcript_path": "C:/t.jsonl" })).unwrap();
        assert!(is_devin_payload(&devin) && !is_devin_payload(&claude));
        assert!(is_devin_process("devin.exe") && is_devin_process("Devin") && !is_devin_process("claude.exe"));
        assert!(!is_devin_process("devinci.exe"));
    }

    #[test]
    fn vibe_hooks_go_after_the_persons_own_and_come_out_alone() {
        let exe = Path::new("C:/Users/me/AppData/Local/Openbar/Openbar.exe");
        let own = "[[hooks]]\nname = \"lint\"\ntype = \"post_agent\"\ncommand = \"eslint .\"\n";
        let installed = installer::vibe_installing(Some(own), exe, "app");
        assert!(installed.starts_with(own));
        assert_eq!(installed.matches("[[hooks]]").count(), 1 + installer::VIBE_EVENTS.len());
        assert!(installed.contains("name = \"openbar-app-pre_tool\""));
        let again = installer::vibe_installing(Some(&installed), exe, "app");
        assert_eq!(again, installed);
        assert_eq!(installer::vibe_removing(&installed, "app").as_deref(), Some(own));
        assert_eq!(installer::vibe_removing(own, "app"), None);
        let both = installer::vibe_installing(Some(&installed), exe, "other");
        assert_eq!(installer::vibe_removing(&both, "app").unwrap().matches("[[hooks]]").count(), 1 + installer::VIBE_EVENTS.len());
        assert!(installer::vibe_inline("hooks = [{ name = \"x\" }]\n"));
        assert!(!installer::vibe_inline(own));
        assert_eq!(installer::toml_string("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
        // A path with no space runs as it is in Git Bash, PowerShell and cmd.
        let command = installer::portable_command(Provider::Vibe, "pre_tool", exe, "app");
        assert!(command.starts_with("C:/Users/me/AppData/Local/Openbar/Openbar.exe --hook --agent vibe --event pre_tool "));
        assert!(!command.contains('"') && !command.starts_with('&'));
        // One that keeps a space falls back to PowerShell's form.
        let spaced = installer::portable_command(Provider::Vibe, "pre_tool", Path::new("Z:/No Such Folder/Openbar.exe"), "app");
        assert!(spaced.starts_with("& \""));
    }

    #[test]
    fn devin_hooks_go_beside_the_persons_own_and_come_out_alone() {
        let exe = Path::new("C:/Openbar/Openbar.exe");
        let own = json!({ "version": 1, "permission_mode": "auto",
                          "hooks": { "PreToolUse": [{ "matcher": "exec", "hooks": [{ "type": "command", "command": "guard.ps1" }] }] } });
        let installed = installer::devin_installing(&own, exe, "app");
        let pre = installed.pointer("/hooks/PreToolUse").unwrap().as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0], own.pointer("/hooks/PreToolUse/0").unwrap().clone());
        assert_eq!(installed["permission_mode"], "auto");
        assert_eq!(installed["hooks"].as_object().unwrap().len(), installer::DEVIN_EVENTS.len());
        assert_eq!(crate::hooks::installer::removing(&installed, "app"), own);
        assert_eq!(installer::devin_installing(&json!({}), exe, "app")["version"], 1);
        let devin = installer::portable_command(Provider::Devin, "PermissionRequest", exe, "app");
        assert!(devin.starts_with("C:/Openbar/Openbar.exe --hook --agent devin --event PermissionRequest"));
    }

    #[test]
    fn a_vibe_log_ends_on_its_last_reply() {
        let history = br#"[{"message":{"content":[{"text":"list","type":"text"}],"role":"user"},"source":"history"},{"message":{"content":[{"type":"tool_call","name":"bash"}],"role":"assistant"},"source":"history"},{"message":{"content":[{"text":"Done: the folder holds the files. ","type":"text"}],"role":"assistant"},"source":"history"}]"#;
        assert_eq!(vibe_chunk_reply(history).as_deref(), Some("Done: the folder holds the files."));
        assert_eq!(vibe_chunk_reply(b"not json"), None);
        let folder = std::env::temp_dir().join(format!("ob-vibe-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("chunks")).unwrap();
        std::fs::write(folder.join("chunks").join("b.json"), history).unwrap();
        std::fs::write(folder.join("chunks").join("c.json"), br#"[{"type":"notice"}]"#).unwrap();
        assert_eq!(vibe_last_reply(&folder).as_deref(), Some("Done: the folder holds the files."));
        assert_eq!(vibe_last_reply(&folder.join("missing")), None);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_three_new_agents_start_with_their_prompt() {
        assert_eq!(start_arguments(Provider::Devin, "go"), vec!["--", "go"]);
        assert_eq!(start_arguments(Provider::Pi, "go"), vec!["go"]);
        assert_eq!(start_arguments(Provider::Vibe, " "), Vec::<String>::new());
        assert_eq!(command_name(Provider::Vibe), Some("vibe"));
        assert_eq!(name(Provider::Vibe, "file_system.search_replace"), "Edit");
        assert_eq!(name(Provider::Devin, "mcp__github__create_issue"), "github__create_issue");
        assert_eq!(Provider::from_key("devin"), Some(Provider::Devin));
    }

    #[test]
    fn a_folder_reads_its_branch() {
        let root = std::env::temp_dir().join(format!("vs-branch-{:x}", rand_id()));
        let nested = root.join("repo").join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.join("repo").join(".git")).unwrap();
        std::fs::write(
            root.join("repo").join(".git").join("HEAD"),
            "ref: refs/heads/feat/x\n",
        )
        .unwrap();
        assert_eq!(
            read_branch(&nested.to_string_lossy()).as_deref(),
            Some("feat/x")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
