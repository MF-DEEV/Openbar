// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Gemini CLI, Copilot CLI, Cursor, OpenCode, Pi, Mistral Vibe, Devin CLI,
//! Google Antigravity and Grok CLI in the Sessions list (AgentHookMessage.swift, AgentHookSessions.swift,
//! AgentHookInstaller.swift, AgentHookSupport.swift).
//! The hook the island installs is this app run with `--hook --agent <name>
//! --event <hook>`, which wraps what the agent hands it in an envelope:
//!
//! `{"openbar": "gemini", "hook": "BeforeTool", "pid": 0, "event": {…}}`
//!
//! OpenCode's plugin and Pi's extension write the same envelope with an
//! event of their own making. None of these agents keeps a registry, so a session is listed from
//! its first event until it ends, its process quits, or (for Cursor's app and
//! Antigravity's language server, whose process outlives its conversations)
//! it has been quiet a while.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::file_diff;
use super::models::{
    ChatMessage, ChatRole, ContentPolicy, HostKind, Permission, Provider, Question, QuestionOption,
    Session, SessionEvent, SessionPhase, Time, Tokens, ToolUse, preview, project_name, str_of,
    tool_summary,
};
use super::transcript::REPLY_LIMIT;

/// The envelope's key, which no agent's own payload carries.
pub const KEY: &str = "openbar";
pub const ANSWER_IN_TERMINAL: &str = super::codex::state::ANSWER_IN_TERMINAL;
/// How long an unread finished turn stays marked, as for Claude Code.
pub const FINISHED_FOR: Time = 10.0 * 60.0;
/// A Cursor or Antigravity conversation nothing was heard from for this long
/// leaves the list.
pub const QUIET_FOR: Time = 30.0 * 60.0;
const TIMELINE_LENGTH: usize = 12;
const CHAT_LENGTH: usize = 40;

impl Provider {
    pub const HOOK_AGENTS: [Provider; 9] = [
        Provider::Gemini,
        Provider::Copilot,
        Provider::Opencode,
        Provider::Cursor,
        Provider::Pi,
        Provider::Vibe,
        Provider::Devin,
        Provider::Antigravity,
        Provider::Grok,
    ];

    pub fn reports_through_hooks(self) -> bool {
        Self::HOOK_AGENTS.contains(&self)
    }

    /// Gemini CLI's, Copilot CLI's and Mistral Vibe's hooks only hear of a
    /// prompt, Cursor keeps its approvals, Pi asks none of its own, Devin
    /// CLI's stay in Devin and Google Antigravity's hook can't approve a call;
    /// the person answers those in the agent. Grok CLI's hooks can't approve
    /// either, but its prompt takes Return for yes and Ctrl-C for no, which
    /// the island presses in its terminal where it can (#243).
    pub fn answers_permissions(self) -> bool {
        matches!(
            self,
            Provider::Claude | Provider::Codex | Provider::Opencode | Provider::Grok
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
            Provider::Antigravity => "antigravity",
            Provider::Grok => "grok",
            Provider::Custom => "custom",
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
            Provider::Antigravity,
            Provider::Grok,
            Provider::Custom,
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
    /// A permission or question the island answers itself, over the
    /// connection that holds it (Vibe's app-server proxy).
    HeldPermission(Permission),
    PermissionSettled(Option<String>),
    Question,
    Reply(String),
    Stopped {
        last_text: Option<String>,
        failed: bool,
    },
    Model(String),
    /// What a permission would ask of the tool about to run, kept until the
    /// agent tells it asks (Grok CLI names the tool only before).
    MayAsk(Permission),
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
    /// The session's own log, where the agent names one: Vibe's folder,
    /// Antigravity's transcript.
    pub transcript: Option<String>,
    /// A session Vibe's app-server proxy runs, which then answers its approvals.
    pub attached: Option<String>,
    /// The events come from the proxy, not from Vibe's hooks.
    pub from_proxy: bool,
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
    if !is_envelope(data)
        || !(contains(data, b"\"opencode\"")
            || contains(data, b"\"pi\"")
            || contains(data, b"\"proxy\""))
    {
        return false;
    }
    let Ok(json) = serde_json::from_slice::<Value>(data) else {
        return false;
    };
    let kind = json.pointer("/event/kind").and_then(Value::as_str);
    match json.get(KEY).and_then(Value::as_str) {
        Some("opencode") => matches!(kind, Some("permission" | "listen")),
        Some("pi") => kind == Some("listen"),
        // Vibe's proxy holds the connection of an approval or a question, which the island answers.
        Some("vibe") => {
            json.get("hook").and_then(Value::as_str) == Some("proxy")
                && matches!(kind, Some("approval" | "question"))
        }
        _ => false,
    }
}

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

/// The envelope's key for the config folder's tag or tool id a hook was
/// written for (#236).
pub const VIA: &str = "via";

/// Whether a tag is one the island made: only these go into a command.
pub fn is_tag(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 40
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The config folder's tag or tool id of a session heard through hooks the
/// Hooks page wrote (#236): Codex's from a second Codex home, a tool's.
pub fn via_of(provider: Provider, session: &str) -> Option<&str> {
    matches!(provider, Provider::Codex | Provider::Custom)
        .then(|| session.split_once(':').map(|(via, _)| via))
        .flatten()
}

pub fn decode(data: &[u8], now: Time, policy: ContentPolicy) -> Option<Decoded> {
    let json: Value = serde_json::from_slice(data).ok()?;
    let provider = Provider::from_key(json.get(KEY)?.as_str()?)?;
    let via = json
        .get(VIA)
        .and_then(Value::as_str)
        .filter(|v| is_tag(v))
        .map(str::to_owned);
    // Codex and the tools added by hand are heard only through hooks naming their folder or tool.
    if !provider.reports_through_hooks()
        && !(matches!(provider, Provider::Codex | Provider::Custom) && via.is_some())
    {
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
        Provider::Vibe if hook == "proxy" => vibe_proxy(event, now, &mut decoded),
        Provider::Vibe => {
            decoded.events = vibe(&hook, event, pid, now, policy);
            decoded.transcript = text(event, "transcript_path");
        }
        Provider::Devin => {
            decoded.events = claude_style(Provider::Devin, &hook, event, pid, now, policy, None)
        }
        Provider::Codex | Provider::Custom => {
            decoded.events = claude_style(provider, &hook, event, pid, now, policy, via.as_deref())
        }
        Provider::Antigravity => {
            decoded.events = antigravity(&hook, event, pid, now, policy);
            decoded.transcript = text(event, "transcriptPath");
        }
        Provider::Grok => {
            decoded.events = grok(&hook, event, pid, now, policy);
            decoded.transcript = text(event, "transcriptPath");
        }
        Provider::Claude => return None,
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

// MARK: Mistral Vibe's app-server proxy

/// What the proxy a New session runs Vibe's server through tells the island
/// (`vibe_proxy`): a session it serves, an approval or question the island
/// can answer, and one answered elsewhere.
fn vibe_proxy(json: &Map<String, Value>, now: Time, decoded: &mut Decoded) {
    decoded.from_proxy = true;
    let kind = text(json, "kind").unwrap_or_default();
    let Some(session) = text(json, "session") else {
        return;
    };
    let request = text(json, "request");
    if request.is_none() && kind != "attach" {
        return;
    }
    let cwd = text(json, "cwd");
    // The proxy's pid is not the agent's: the hooks name that.
    let mut add = |kind| {
        decoded.events.push(AgentEvent {
            provider: Provider::Vibe,
            session_id: session.clone(),
            pid: 0,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    match kind.as_str() {
        "attach" => decoded.attached = Some(session.clone()),
        "settled" => add(Kind::PermissionSettled(request.clone())),
        "approval" => {
            let raw = text(json, "tool").unwrap_or_default();
            let tool = name(Provider::Vibe, &raw);
            let input = json
                .get("input")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let detail = ["command", "file_path", "path", "url"]
                .iter()
                .filter_map(|key| str_of(input.get(*key)).filter(|s| !s.is_empty()))
                .next()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    if input.is_empty() {
                        String::new()
                    } else {
                        tool_summary::arguments(&input)
                    }
                });
            let request = request.clone().unwrap_or_default();
            decoded.permission = Some((session.clone(), request.clone()));
            let always = json.get("always").and_then(Value::as_bool).unwrap_or(false);
            let mut permission = waiting_permission(&tool, String::new(), always, now, &request);
            permission.summary = Some(first_line(&detail)).filter(|s| !s.is_empty());
            permission.set_content(&detail, true, Some(detail.chars().count()));
            add(Kind::HeldPermission(permission));
        }
        "question" => {
            let questions: Vec<Question> = json
                .get("questions")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let item = item.as_object()?;
                            let asked = text(item, "question").filter(|s| !s.is_empty())?;
                            Some(Question {
                                question: asked,
                                header: text(item, "header"),
                                options: item
                                    .get("options")
                                    .and_then(Value::as_array)
                                    .map(|options| {
                                        options
                                            .iter()
                                            .filter_map(|option| {
                                                let option = option.as_object()?;
                                                let label = text(option, "label")
                                                    .filter(|s| !s.is_empty())?;
                                                Some(QuestionOption {
                                                    label,
                                                    description: text(option, "description"),
                                                    preview: None,
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                multi_select: item
                                    .get("multiSelect")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let Some(first) = questions.first() else {
                return;
            };
            let request = request.clone().unwrap_or_default();
            decoded.permission = Some((session.clone(), request.clone()));
            let mut permission =
                waiting_permission(Question::TOOL, String::new(), false, now, &request);
            permission.summary = Some(tool_summary::clip(&first.question));
            permission.questions = questions;
            add(Kind::HeldPermission(permission));
        }
        _ => {}
    }
}

/// What the proxy passes on to Vibe for the person's decision on a request.
pub fn vibe_reply(
    decision: &super::hook_message::Decision,
    questions: &[Question],
) -> Option<Vec<u8>> {
    use super::hook_message::{DENY_MESSAGE, Decision};
    let reply = match decision {
        Decision::Allow | Decision::ApprovePlan(_) => json!({ "decision": "approve" }),
        Decision::AllowAlways => json!({ "decision": "approve_for_session" }),
        Decision::Deny if questions.is_empty() => {
            json!({ "decision": "deny", "feedback": DENY_MESSAGE })
        }
        Decision::Deny => json!({ "cancelled": true }),
        Decision::Answer(answers) => {
            let given: Vec<Value> = questions
                .iter()
                .filter_map(|question| {
                    let picked = answers.get(&question.question).filter(|p| !p.is_empty())?;
                    let listed: Vec<&str> =
                        question.options.iter().map(|o| o.label.as_str()).collect();
                    Some(json!({
                        "question": question.question,
                        "answer": picked.join(", "),
                        "isOther": !picked.iter().all(|p| listed.contains(&p.as_str())),
                    }))
                })
                .collect();
            if given.is_empty() || given.len() != questions.len() {
                return None;
            }
            json!({ "answers": given })
        }
    };
    serde_json::to_vec(&reply).ok()
}

// MARK: Devin CLI

/// Devin CLI's hooks, in Claude Code's format (docs.devin.ai, "Lifecycle
/// Hooks"). Its payloads carry no folder; the island reads the agent's. A
/// permission is heard and answered in Devin.
/// Hooks in Claude Code's format: Devin CLI's, whose payloads carry no
/// folder (the island reads the agent's), and those the Hooks page writes
/// for a second Codex home or a tool added by hand (#236), whose session ids
/// carry `via` so they stay apart from Codex's server and from each other.
/// A permission is heard and answered in the agent.
fn claude_style(
    provider: Provider,
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
    via: Option<&str>,
) -> Vec<AgentEvent> {
    let Some(own) = text(json, "session_id") else {
        return vec![];
    };
    let session = match via {
        Some(via) => format!("{via}:{own}"),
        None => own,
    };
    let cwd = text(json, "cwd");
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    let raw = str_of(json.get("tool_name")).unwrap_or("");
    let tool = name(provider, raw);
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
        "PostToolUseFailure" => add(Kind::ToolFinished {
            id: None,
            input_key: Some(tool_summary::input_key(&tool, &input)),
            name: tool,
            failed: true,
            started: None,
        }),
        "Stop" => add(Kind::Stopped {
            last_text: policy
                .previews
                .then(|| reply_preview(json.get("last_assistant_message")))
                .flatten(),
            failed: false,
        }),
        _ => {}
    }
    if provider != Provider::Devin
        && let Some(model) = text(json, "model")
    {
        add(Kind::Model(model));
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
    serde_json::from_slice::<Value>(data)
        .is_ok_and(|json| json.get("prompt_id").is_some() && json.get("transcript_path").is_none())
}

pub fn is_devin_process(name: &str) -> bool {
    let name = name.to_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    name == "devin" || name.starts_with("devin-") || name.starts_with("devin.")
}

// MARK: Grok CLI

/// Grok CLI's hooks (xAI's Grok Build, user guide "Hooks"): camelCase input
/// with snake_case event names. A permission is told in two steps, the
/// tool's `PreToolUse` and then a `permission_prompt` notification with no
/// tool in it; the island answers it with the keys of Grok's prompt. A
/// question is a tool of its own, answered in Grok. A turn ends with
/// `Stop`, or without a done with `StopFailure` or `StopCancelled`; the
/// `Stop` Grok runs as it shuts down ends nothing.
fn grok(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "sessionId") else {
        return vec![];
    };
    let cwd = text(json, "cwd").or_else(|| text(json, "workspaceRoot"));
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider: Provider::Grok,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    let raw = str_of(json.get("toolName")).unwrap_or("");
    let tool = name(Provider::Grok, raw);
    let input = arguments(json.get("toolInput"));
    let event = text(json, "hookEventName").unwrap_or_else(|| hook.to_owned());
    match event.as_str() {
        "session_start" | "SessionStart" => {
            add(Kind::Started);
            if let Some(model) = text(json, "modelId") {
                add(Kind::Model(model));
            }
        }
        "session_end" | "SessionEnd" => add(Kind::Ended),
        "user_prompt_submit" | "UserPromptSubmit" => add(Kind::Prompt(
            policy
                .previews
                .then(|| reply_preview(json.get("prompt")))
                .flatten(),
        )),
        "pre_tool_use" | "PreToolUse" if raw == "ask_user_question" => add(Kind::Question),
        "pre_tool_use" | "PreToolUse" => {
            let mut use_ = tool_use(&tool, &input, now, policy);
            if let Some(id) = text(json, "toolUseId") {
                use_.id = id;
            }
            add(Kind::ToolStarted(use_));
            let truncated = json.get("toolInputTruncated").and_then(Value::as_bool) == Some(true);
            add(Kind::MayAsk(grok_ask(
                &tool,
                &input,
                cwd.as_deref(),
                truncated,
                now,
            )));
        }
        "post_tool_use" | "PostToolUse" | "post_tool_use_failure" | "PostToolUseFailure" => {
            // A tool that ran or failed was let through, here or in Grok.
            add(Kind::PermissionSettled(None));
            if raw != "ask_user_question" {
                add(Kind::ToolFinished {
                    id: text(json, "toolUseId"),
                    input_key: Some(tool_summary::input_key(&tool, &input)),
                    name: tool,
                    failed: matches!(
                        event.as_str(),
                        "post_tool_use_failure" | "PostToolUseFailure"
                    ),
                    started: None,
                });
            }
        }
        "permission_denied" | "PermissionDenied" => add(Kind::PermissionSettled(None)),
        "notification" | "Notification" => match text(json, "notificationType").as_deref() {
            // Filled in from the tool the session's last PreToolUse named.
            Some("permission_prompt") => add(Kind::Permission(Permission::new(
                &format!("{:x}", rand_id()),
                "",
                None,
                false,
                now,
            ))),
            Some("elicitation_dialog") => add(Kind::Question),
            _ => {}
        },
        "stop" | "Stop" if text(json, "reason").as_deref() == Some("shutdown") => {}
        "stop" | "Stop" => add(Kind::Stopped {
            last_text: policy
                .previews
                .then(|| reply_preview(json.get("lastAssistantMessage")))
                .flatten(),
            failed: false,
        }),
        // Interrupted, refused or failed: the turn ends, with nothing done.
        "stop_failure" | "StopFailure" | "stop_cancelled" | "StopCancelled" => add(Kind::Stopped {
            last_text: None,
            failed: true,
        }),
        _ => {}
    }
    events
}

/// What Grok's prompt would ask about the tool: the whole command or file,
/// as Claude Code's request carries it, since Grok's shell, write and
/// search_replace take Claude Code's arguments. The island allows only what
/// it can show whole (#224).
pub fn grok_ask(
    tool: &str,
    input: &Map<String, Value>,
    cwd: Option<&str>,
    truncated: bool,
    at: Time,
) -> Permission {
    let full = tool_summary::permission_detail(tool, input);
    let complete = !truncated && (tool != "Bash" || text(input, "command").is_some());
    let mut permission = Permission::new(
        &format!("{:x}", rand_id()),
        if tool.is_empty() { "Tool" } else { tool },
        Some(first_line(&full)).filter(|l| !l.is_empty()).as_deref(),
        false,
        at,
    );
    permission.set_content(&full, complete, complete.then(|| full.chars().count()));
    permission.input_key = Some(tool_summary::input_key(tool, input));
    permission.diffs = file_diff::diffs(tool, input, cwd);
    permission
}

/// Grok CLI runs the hooks in Claude Code's settings too, handing them its
/// own input; those are let go, since Grok's sessions come through hooks of
/// their own (#243). Grok's input names the workspace root and its event in
/// camelCase, which no Claude Code hook input does.
pub fn is_grok_payload(data: &[u8]) -> bool {
    if !contains(data, b"\"workspaceRoot\"") || !contains(data, b"\"hookEventName\"") {
        return false;
    }
    serde_json::from_slice::<Value>(data).is_ok_and(|json| {
        json.get("workspaceRoot").is_some()
            && json.get("hookEventName").is_some()
            && json.get("sessionId").is_some()
    })
}

/// The process of a Grok CLI session, from the `active_sessions.json` Grok
/// keeps in its folder (`<GROK_HOME>/sessions/<folder>/<id>/updates.jsonl` is
/// the transcript its hooks name). On Windows Grok runs a hook through
/// `powershell -Command`, which doesn't wait for this app (a windowed
/// program), so the hook's parent is gone by the time the island reads it
/// and the agent can't be found above the hook.
pub fn grok_session_pid(transcript: &Path, session: &str) -> Option<i32> {
    let home = transcript.parent()?.parent()?.parent()?.parent()?;
    let path = home.join("active_sessions.json");
    if std::fs::metadata(&path).ok()?.len() > 1024 * 1024 {
        return None;
    }
    grok_session_pid_in(&std::fs::read(path).ok()?, session)
}

pub fn grok_session_pid_in(data: &[u8], session: &str) -> Option<i32> {
    let list: Vec<Value> = serde_json::from_slice(data).ok()?;
    list.iter()
        .rev()
        .find(|e| e.get("session_id").and_then(Value::as_str) == Some(session))
        .and_then(|e| e.get("pid"))
        .and_then(Value::as_i64)
        .and_then(|p| i32::try_from(p).ok())
        .filter(|p| *p > 0)
}

/// The model of a Grok CLI session, from the summary Grok keeps beside the
/// conversation its hooks name (`…/sessions/<folder>/<id>/updates.jsonl`).
pub fn grok_model(transcript: &Path) -> Option<String> {
    let summary = transcript.parent()?.join("summary.json");
    if std::fs::metadata(&summary).ok()?.len() > 1024 * 1024 {
        return None;
    }
    grok_model_in(&std::fs::read(summary).ok()?)
}

pub fn grok_model_in(data: &[u8]) -> Option<String> {
    let json: Value = serde_json::from_slice(data).ok()?;
    json.get("current_model_id")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .map(str::to_owned)
}

// MARK: Google Antigravity

/// Google Antigravity's lifecycle hooks (the hooks.json guide its language
/// server carries), the same in its app, its IDE and `agy`. The input is
/// camelCase, a session is a conversation, and there is no hook for a prompt
/// or a permission: a turn's first model invocation (`invocationNum` 0, with
/// the prompt as `lastUserInput`) opens it, `Stop` ends it, and approvals are
/// asked and answered in Antigravity. A tool Antigravity refused never
/// reports back; `Stop` closes it.
fn antigravity(
    hook: &str,
    json: &Map<String, Value>,
    pid: i32,
    now: Time,
    policy: ContentPolicy,
) -> Vec<AgentEvent> {
    let Some(session) = text(json, "conversationId") else {
        return vec![];
    };
    let cwd = json
        .get("workspacePaths")
        .and_then(Value::as_array)
        .and_then(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .find(|p| !p.is_empty())
        })
        .map(str::to_owned)
        .or_else(|| text(json, "cwd"));
    let mut events = Vec::new();
    let mut add = |kind| {
        events.push(AgentEvent {
            provider: Provider::Antigravity,
            session_id: session.clone(),
            pid,
            cwd: cwd.clone(),
            date: now,
            kind,
        })
    };
    if let Some(model) = text(json, "modelName").filter(|m| !m.eq_ignore_ascii_case("auto")) {
        add(Kind::Model(model));
    }
    // A tool's hooks share the step it runs as, which ties the two together.
    let step = json
        .get("stepIdx")
        .and_then(Value::as_i64)
        .map(|i| format!("step-{i}"));
    match hook {
        "PreInvocation" if json.get("invocationNum").and_then(Value::as_i64) == Some(0) => {
            add(Kind::Prompt(
                policy
                    .previews
                    .then(|| reply_preview(json.get("lastUserInput")))
                    .flatten(),
            ))
        }
        "PreInvocation" => add(Kind::Working),
        "PreToolUse" => {
            let call = json.get("toolCall");
            let tool = name(
                Provider::Antigravity,
                str_of(call.and_then(|c| c.get("name"))).unwrap_or(""),
            );
            let input = antigravity_input(&arguments(call.and_then(|c| c.get("args"))));
            let mut use_ = tool_use(&tool, &input, now, policy);
            if let Some(step) = step {
                use_.id = step;
            }
            add(Kind::ToolStarted(use_));
        }
        // It names the tool too, for a call whose start the island missed.
        "PostToolUse" => add(Kind::ToolFinished {
            id: step,
            name: name(
                Provider::Antigravity,
                str_of(json.get("toolCall").and_then(|c| c.get("name"))).unwrap_or(""),
            ),
            input_key: None,
            failed: text(json, "error").is_some(),
            started: None,
        }),
        // As "NO_TOOL_CALL" for a turn that ended well; a failed one says so.
        "Stop" => add(Kind::Stopped {
            last_text: None,
            failed: str_of(json.get("terminationReason"))
                .is_some_and(|r| r.to_lowercase().contains("error"))
                || text(json, "error").is_some(),
        }),
        _ => {}
    }
    events
}

/// Antigravity's tool arguments under the names Claude Code gives them, so
/// they are summed up alike; its own one-line summary, when it wrote one,
/// comes last.
pub fn antigravity_input(args: &Map<String, Value>) -> Map<String, Value> {
    let mut input = args.clone();
    for (from, to) in [
        ("CommandLine", "command"),
        ("AbsolutePath", "file_path"),
        ("TargetFile", "file_path"),
        ("Query", "pattern"),
        ("Pattern", "pattern"),
        ("SearchPath", "path"),
        ("DirectoryPath", "path"),
        ("SearchDirectory", "path"),
        ("Url", "url"),
        ("toolSummary", "description"),
    ] {
        if !input.contains_key(to)
            && let Some(value) = args.get(from)
        {
            input.insert(to.to_owned(), value.clone());
        }
    }
    input
}

/// The agent's last reply in an Antigravity conversation, read from the end
/// of the transcript its hooks name (`transcript_full.jsonl`), since the
/// hooks carry none: `PLANNER_RESPONSE` steps hold the model's text, empty on
/// a step that only calls tools (AgentAntigravityLog in
/// AgentHookSupport.swift). Read only with previews on.
pub fn antigravity_last_reply(path: &Path) -> Option<String> {
    antigravity_last(path, "PLANNER_RESPONSE").and_then(|text| preview(Some(&text), REPLY_LIMIT))
}

fn antigravity_last(path: &Path, kind: &str) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 1024 * 1024;
    if !matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some("transcript_full.jsonl" | "transcript.jsonl")
    ) {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let start = size.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    // A cut first line is not a step.
    if start > 0
        && let Some(newline) = data.iter().position(|b| *b == b'\n')
    {
        data.drain(..=newline);
    }
    antigravity_last_in(&data, kind)
}

pub fn antigravity_last_in(data: &[u8], kind: &str) -> Option<String> {
    data.split(|b| *b == b'\n').rev().find_map(|line| {
        let step: Value = serde_json::from_slice(line).ok()?;
        if str_of(step.get("type")) != Some(kind) {
            return None;
        }
        str_of(step.get("content"))
            .filter(|c| !c.trim().is_empty())
            .map(str::to_owned)
    })
}

/// The last reply of a Mistral Vibe session, read from its own log, since
/// its hooks carry none: the newest JSON chunk under `chunks/` that holds
/// history ends on the latest messages (AgentVibeLog in AgentHookSupport.swift).
pub fn vibe_last_reply(folder: &Path) -> Option<String> {
    const CHUNK_LIMIT: u64 = 4 * 1024 * 1024;
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> =
        std::fs::read_dir(folder.join("chunks"))
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

            decoded.permission = Some((session.clone(), request.clone()));
            let mut permission = waiting_permission(&tool, String::new(), always, date, &request);
            let command = metadata.get("command").and_then(Value::as_str);
            let original_length = metadata
                .get("commandLength")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok());
            let shell = tool == "Bash" || metadata.contains_key("command");
            let complete_detail = json.get("permissionContentComplete") == Some(&Value::Bool(true))
                && json.get("metadata").is_none_or(Value::is_object)
                && json
                    .get("patterns")
                    .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
                && json
                    .get("permissionDetailLength")
                    .is_none_or(|v| v.as_u64() == Some(detail.chars().count() as u64));
            let complete = if shell {
                metadata.get("commandComplete") == Some(&Value::Bool(true))
                    && command.is_some_and(|s| {
                        !s.is_empty() && original_length == Some(s.chars().count())
                    })
            } else {
                complete_detail
            };
            permission.set_content(
                &detail,
                complete,
                if shell {
                    original_length
                } else {
                    json.get("permissionDetailLength")
                        .and_then(Value::as_u64)
                        .and_then(|n| usize::try_from(n).ok())
                        .or_else(|| complete.then(|| detail.chars().count()))
                },
            );
            if tool == "Edit" && !shell {
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
        Provider::Grok => &[
            ("run_terminal_command", "Bash"),
            ("read_file", "Read"),
            ("write", "Write"),
            ("search_replace", "Edit"),
            ("list_dir", "LS"),
            ("grep", "Grep"),
            ("todo_write", "TodoWrite"),
            ("spawn_subagent", "Task"),
            ("get_command_or_subagent_output", "BashOutput"),
            ("kill_command_or_subagent", "KillShell"),
            ("web_fetch", "WebFetch"),
            ("web_search", "WebSearch"),
            ("ask_user_question", "AskUserQuestion"),
            ("enter_plan_mode", "EnterPlanMode"),
            ("exit_plan_mode", "ExitPlanMode"),
        ],
        Provider::Antigravity => &[
            ("run_command", "Bash"),
            ("command_status", "BashOutput"),
            ("manage_task", "BashOutput"),
            ("send_command_input", "Bash"),
            ("view_file", "Read"),
            ("view_file_outline", "Read"),
            ("view_code_item", "Read"),
            ("write_to_file", "Write"),
            ("replace_file_content", "Edit"),
            ("multi_replace_file_content", "Edit"),
            ("grep_search", "Grep"),
            ("find_by_name", "Glob"),
            ("list_dir", "LS"),
            ("read_url_content", "WebFetch"),
            ("search_web", "WebSearch"),
        ],
        // Codex's hooks name its shell `Bash` already; its patch tool is an edit.
        Provider::Codex => &[
            ("apply_patch", "Edit"),
            ("shell", "Bash"),
            ("local_shell", "Bash"),
            ("exec_command", "Bash"),
            ("update_plan", "TodoWrite"),
            ("view_image", "Read"),
            ("web_search", "WebSearch"),
        ],
        Provider::Claude | Provider::Custom => &[],
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
    /// What the tool about to run would ask, for an agent that names it
    /// only before it asks (Grok CLI).
    next_ask: Option<Permission>,
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
            next_ask: None,
            answerable: false,
            asking: false,
            model: None,
            title: None,
            last_text: None,
            chat: Vec::new(),
            usage: BTreeMap::new(),
        }
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
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
    /// Whether the island can press keys in the host (Grok CLI's prompt, #243).
    pub takes_keys: bool,
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
                entry.next_ask = None;
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
            Kind::Permission(mut permission) => {
                entry.open_turn(date);
                // Grok's prompt names no tool: the one its last PreToolUse named waits.
                if permission.tool.is_empty() {
                    let mut asked = entry.next_ask.clone().unwrap_or_else(|| {
                        Permission::new(
                            &permission.request_id,
                            "Tool",
                            None,
                            false,
                            permission.since,
                        )
                    });
                    asked.request_id = permission.request_id.clone();
                    asked.since = permission.since;
                    permission = asked;
                }
                entry.permission = Some(permission);
                // Codex through hooks only hears of a request: Codex asks in its terminal (#236).
                entry.answerable = event.provider.answers_permissions()
                    && via_of(event.provider, &event.session_id).is_none();
                entry.asking = false;
            }
            Kind::HeldPermission(permission) => {
                entry.open_turn(date);
                entry.permission = Some(permission);
                entry.answerable = true;
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
                    entry.next_ask = None;
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
            Kind::MayAsk(permission) => entry.next_ask = Some(permission),
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

    /// Drops sessions whose process is gone, and Cursor and Antigravity
    /// conversations long quiet.
    pub fn prune(&mut self, now: Time, alive: impl Fn(i32) -> bool) {
        let gone: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                (e.pid > 0 && !alive(e.pid))
                    || (matches!(e.provider, Provider::Cursor | Provider::Antigravity)
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
    // Grok's is answered with keys, where its terminal takes them.
    let in_agent = entry.permission.is_some()
        && (!entry.answerable || (entry.provider == Provider::Grok && !place.takes_keys));
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
        effort: None,
        edits: None,
        failed_at: None,
        started_at: None,
        turn_ended_at: entry.turn_ended,
        turn_completed: phase == SessionPhase::Finished,
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
        auto_mode: false,
        let_through: Vec::new(),
        later_asks: Vec::new(),
        via: via_of(entry.provider, &entry.session_id).map(str::to_owned),
        origin: None,
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
        Provider::Antigravity => Some("agy"),
        Provider::Grok => Some("grok"),
        Provider::Claude | Provider::Codex | Provider::Custom => None,
    }
}

/// The arguments that open the agent's terminal UI with a first prompt
/// already sent: `gemini -i`, `copilot -i`, `agy -i`, `opencode --prompt`, `devin --`, `grok --`,
/// and `cursor-agent`, `pi` and `vibe` take it as it is (AgentCLI in
/// ClaudeBackgroundSessions.swift).
pub fn start_arguments(provider: Provider, prompt: &str) -> Vec<String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Vec::new();
    }
    match provider {
        Provider::Gemini | Provider::Copilot | Provider::Antigravity => {
            vec!["-i".into(), prompt.into()]
        }
        Provider::Opencode => vec!["--prompt".into(), prompt.into()],
        Provider::Cursor | Provider::Pi | Provider::Vibe => vec![prompt.into()],
        Provider::Devin | Provider::Grok => vec!["--".into(), prompt.into()],
        Provider::Claude | Provider::Codex | Provider::Custom => Vec::new(),
    }
}

/// What a folder's checkout is on, read from its repository's HEAD (#233).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    /// HEAD names a commit, not a branch.
    Detached,
    /// No repository found, or its HEAD couldn't be read.
    Unknown,
}

/// The HEAD of the repository a folder is in, a linked worktree's own one
/// included.
pub fn read_head(folder: &str) -> Head {
    if folder.is_empty() {
        return Head::Unknown;
    }
    let mut path = PathBuf::from(folder);
    for _ in 0..32 {
        let git = path.join(".git");
        if git.exists() {
            let head = if git.is_dir() {
                git.join("HEAD")
            } else {
                // A worktree's .git file names its own folder.
                let Some(line) = std::fs::read_to_string(&git).ok().and_then(|text| {
                    Some(
                        text.lines()
                            .next()?
                            .strip_prefix("gitdir:")?
                            .trim()
                            .to_owned(),
                    )
                }) else {
                    return Head::Unknown;
                };
                let dir = PathBuf::from(&line);
                if dir.is_absolute() {
                    dir
                } else {
                    path.join(dir)
                }
                .join("HEAD")
            };
            let Ok(text) = std::fs::read_to_string(head) else {
                return Head::Unknown;
            };
            let text = text.trim();
            return match text.strip_prefix("ref: refs/heads/") {
                Some(branch) if !branch.is_empty() => Head::Branch(branch.to_owned()),
                // An unborn or odd ref still isn't a commit: say nothing new.
                _ if text.starts_with("ref:") => Head::Unknown,
                _ => Head::Detached,
            };
        }
        if !path.pop() {
            return Head::Unknown;
        }
    }
    Head::Unknown
}

/// The branch a folder's repository is on, read from its HEAD.
pub fn read_branch(folder: &str) -> Option<String> {
    match read_head(folder) {
        Head::Branch(branch) => Some(branch),
        Head::Detached | Head::Unknown => None,
    }
}

/// A session's branch as its checkout has it now: a checkout inside the
/// session shows at once, a detached HEAD shows none, and a folder this
/// machine can't read keeps what the agent said.
pub fn live_branch(reported: Option<String>, head: &Head) -> Option<String> {
    match head {
        Head::Branch(branch) => Some(branch.clone()),
        Head::Detached => None,
        Head::Unknown => reported,
    }
}

// MARK: Installing

pub mod installer {
    //! Puts the island's hooks into Gemini CLI, Copilot CLI, Cursor, Mistral
    //! Vibe, Devin CLI and Google Antigravity, and its plugin into OpenCode
    //! and Pi, only when the person presses Install. A file of the agent's
    //! keeps the island's entries beside the person's own, copied aside
    //! first; Copilot, OpenCode and Pi read a folder, where the island writes
    //! a file of its own.

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

    /// Grok CLI's events (xAI's Grok Build, user guide "Hooks"), in a file of
    /// the island's own in its `hooks` folder, without a matcher, which Grok
    /// refuses on events that aren't a tool's. Each only tells: Grok goes on
    /// whatever the hook answers, and asks in its terminal.
    pub const GROK_EVENTS: [&str; 11] = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionDenied",
        "Notification",
        "Stop",
        "StopFailure",
        "StopCancelled",
    ];

    /// Google Antigravity's lifecycle hooks (its hooks.json guide). It has no
    /// session or prompt hooks: a turn is heard from the model's invocations.
    pub const ANTIGRAVITY_EVENTS: [&str; 5] = [
        "PreInvocation",
        "PreToolUse",
        "PostToolUse",
        "PostInvocation",
        "Stop",
    ];
    /// Antigravity's tool hooks sit in groups with a matcher; the rest are flat.
    pub const ANTIGRAVITY_TOOL_EVENTS: [&str; 2] = ["PreToolUse", "PostToolUse"];
    /// Codex's hooks (codex-rs/hooks), in Claude Code's format, written into
    /// a Codex home other than the one the island hears through Codex's
    /// app-server (#236). A permission is only heard: Codex asks in its terminal.
    pub const CODEX_EVENTS: [&str; 7] = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
        "Stop",
    ];
    /// Events Codex and Claude Code match against a tool name.
    pub const CLAUDE_TOOL_EVENTS: [&str; 4] = [
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
    ];
    /// What a tool with Claude Code style hooks gets (#236): Claude Code's
    /// events, each only heard.
    pub const CUSTOM_EVENTS: [&str; 9] = [
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
        "Notification",
        "Stop",
    ];
    /// The agents whose config folder can be named by hand, beside the one
    /// their home variable or default names (#236).
    pub const FOLDERS_BY_HAND: [Provider; 6] = [
        Provider::Codex,
        Provider::Gemini,
        Provider::Copilot,
        Provider::Pi,
        Provider::Vibe,
        Provider::Grok,
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

    /// Grok CLI's folder: `GROK_HOME`, or `~/.grok` (`%USERPROFILE%\.grok`).
    pub fn grok_folder(home: &Path) -> PathBuf {
        env_dir("GROK_HOME").unwrap_or_else(|| home.join(".grok"))
    }

    pub fn grok_hooks(home: &Path, app: &str) -> PathBuf {
        grok_folder(home)
            .join("hooks")
            .join(format!("{}.json", own_name(app)))
    }

    /// Devin CLI keeps its user config in `%APPDATA%\devin` on Windows, and
    /// in `$XDG_CONFIG_HOME/devin` (`~/.config/devin`) elsewhere.
    pub fn devin_config(home: &Path) -> PathBuf {
        let base = if cfg!(windows) {
            env_dir("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"))
        } else {
            env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"))
        };
        base.join("devin").join("config.json")
    }

    /// Codex's own home, `CODEX_HOME` or `~/.codex`, which the island hears
    /// through Codex's app-server.
    pub fn codex_home(home: &Path) -> PathBuf {
        env_dir("CODEX_HOME").unwrap_or_else(|| home.join(".codex"))
    }

    /// The folder whose presence says the agent was used here, and the one
    /// the Hooks page shows for its own row (#236).
    pub fn agent_folder(provider: Provider, home: &Path) -> Option<PathBuf> {
        Some(match provider {
            Provider::Codex => codex_home(home),
            Provider::Gemini => gemini_settings(home).parent()?.to_path_buf(),
            Provider::Copilot => copilot_folder(home),
            Provider::Opencode => opencode_folder(home),
            Provider::Cursor => home.join(".cursor"),
            Provider::Pi => pi_folder(home),
            Provider::Vibe => vibe_hooks(home).parent()?.to_path_buf(),
            Provider::Devin => devin_config(home).parent()?.to_path_buf(),
            Provider::Antigravity => antigravity_hooks(home).parent()?.to_path_buf(),
            Provider::Grok => grok_folder(home),
            Provider::Claude | Provider::Custom => return None,
        })
    }

    /// The agent's hook file in a config folder the person added (#236): the
    /// folder Gemini keeps as `.gemini`, Copilot's `COPILOT_HOME`, Pi's
    /// `PI_CODING_AGENT_DIR`, Vibe's `VIBE_HOME`, a `CODEX_HOME`, a `GROK_HOME`.
    pub fn file_in(provider: Provider, folder: &Path, app: &str) -> Option<PathBuf> {
        Some(match provider {
            Provider::Gemini => folder.join("settings.json"),
            Provider::Copilot => folder.join("hooks").join(format!("{}.json", own_name(app))),
            Provider::Pi => folder
                .join("extensions")
                .join(format!("{}.js", own_name(app))),
            Provider::Vibe => folder.join("hooks.toml"),
            Provider::Codex => folder.join("hooks.json"),
            Provider::Grok => folder.join("hooks").join(format!("{}.json", own_name(app))),
            _ => return None,
        })
    }

    /// The agent's hook file: in `folder` when one is given, else where the
    /// agent reads it by default; None for an agent with nothing installed.
    pub fn file(
        provider: Provider,
        home: &Path,
        app: &str,
        folder: Option<&Path>,
    ) -> Option<PathBuf> {
        match folder {
            Some(folder) => file_in(provider, folder, app),
            None => {
                crate::diagnostics::hook_records::hook_file(provider, home, app).map(|(f, _)| f)
            }
        }
    }

    /// A short name for a config folder, which its hooks carry so the island
    /// knows which folder a session runs from without writing the path into
    /// a command (#236).
    pub fn folder_tag(folder: &Path) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(crate::sessions::accounts::canonical(folder).as_bytes());
        digest[..6].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Antigravity's global hooks, read by its app, its IDE and `agy` alike,
    /// in the same place on every system.
    pub fn antigravity_hooks(home: &Path) -> PathBuf {
        home.join(".gemini").join("config").join("hooks.json")
    }

    /// Where Antigravity's app and IDE are put: the per-user installs on
    /// Windows, the packages' folders on Linux.
    fn antigravity_apps(home: &Path) -> Vec<PathBuf> {
        if cfg!(windows) {
            let local = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Local"))
                .join("Programs");
            vec![
                local.join("Antigravity").join("Antigravity.exe"),
                local.join("Antigravity IDE").join("Antigravity IDE.exe"),
            ]
        } else {
            vec![
                PathBuf::from("/usr/share/antigravity/antigravity"),
                PathBuf::from("/usr/share/antigravity-ide/antigravity-ide"),
                PathBuf::from("/opt/Antigravity/antigravity"),
                PathBuf::from("/opt/Antigravity IDE/antigravity-ide"),
            ]
        }
    }

    /// Whether the agent is installed: its command (as New session finds
    /// it), or Cursor's or Antigravity's app; for Pi, its command and the folder Pi makes on
    /// first run, since other tools take the name `pi` too. A folder like
    /// `.gemini` or `.copilot` is not enough: other apps make those too.
    pub fn is_present(provider: Provider, home: &Path) -> bool {
        if super::command_name(provider)
            .and_then(crate::agent_usage::agent_executable)
            .is_some()
        {
            return provider != Provider::Pi || home.join(".pi").join("agent").is_dir();
        }
        if provider == Provider::Antigravity {
            return antigravity_apps(home).iter().any(|p| p.is_file());
        }
        provider == Provider::Cursor && cursor_apps(home).iter().any(|p| p.is_file())
    }

    /// Where Cursor's app is put: the per-user install on Windows, the
    /// packages' folders on Linux.
    fn cursor_apps(home: &Path) -> Vec<PathBuf> {
        if cfg!(windows) {
            vec![
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join("AppData").join("Local"))
                    .join("Programs")
                    .join("cursor")
                    .join("Cursor.exe"),
            ]
        } else {
            vec![
                PathBuf::from("/usr/share/cursor/cursor"),
                PathBuf::from("/opt/Cursor/cursor"),
                PathBuf::from("/opt/cursor/cursor"),
            ]
        }
    }

    /// What keeping the hooks in step asks (AgentHookInstaller.plan): each
    /// found agent whose switch is on gets its hooks or an update; one whose
    /// switch is off has the island's taken out. An unreadable file is left alone.
    pub fn plan(
        found: &[Provider],
        hidden: &std::collections::HashSet<Provider>,
        state: impl Fn(Provider) -> State,
    ) -> (Vec<Provider>, Vec<Provider>) {
        use crate::diagnostics::hook_records::derive;
        plan_with(
            found,
            hidden,
            |p| derive(state(p), None, None),
            &std::collections::HashSet::new(),
            true,
        )
    }

    /// The same, once the records say more (#236): hooks the island put in
    /// that someone took out come back only while lost hooks are put back
    /// by themselves, hooks changed by hand are the person's, and an agent
    /// the person pressed Remove for on the Hooks page is left as it is.
    pub fn plan_with(
        found: &[Provider],
        hidden: &std::collections::HashSet<Provider>,
        state: impl Fn(Provider) -> crate::diagnostics::hook_records::State,
        opted_out: &std::collections::HashSet<Provider>,
        repair: bool,
    ) -> (Vec<Provider>, Vec<Provider>) {
        use crate::diagnostics::hook_records::State as Derived;
        let mut install = Vec::new();
        let mut remove = Vec::new();
        for provider in Provider::HOOK_AGENTS {
            let now = state(provider);
            if !hidden.contains(&provider) {
                if !found.contains(&provider) || opted_out.contains(&provider) {
                    continue;
                }
                match now {
                    Derived::Missing | Derived::Outdated => install.push(provider),
                    Derived::RemovedByHand if repair => install.push(provider),
                    _ => {}
                }
            } else if matches!(
                now,
                Derived::Installed | Derived::Outdated | Derived::ChangedByHand
            ) {
                remove.push(provider);
            }
        }
        (install, remove)
    }

    /// The hook for one event: this app, quoted with forward slashes, run
    /// by PowerShell (Gemini CLI and Cursor on Windows); elsewhere they run
    /// it with `bash -c` or `sh -c`, so the path goes in single quotes.
    pub fn command(provider: Provider, event: &str, exe: &Path, app: &str) -> String {
        command_via(provider, event, exe, app, None)
    }

    /// The hook's arguments after the program; `via` names the config
    /// folder (Codex) or the tool the hook was written for (#236).
    fn arguments_via(provider: Provider, event: &str, app: &str, via: Option<&str>) -> String {
        let continues = if provider == Provider::Cursor && CURSOR_CONTINUES.contains(&event) {
            " --continue"
        } else {
            ""
        };
        let via = via
            .filter(|v| super::is_tag(v))
            .map(|v| format!(" --via {v}"))
            .unwrap_or_default();
        format!(
            "--hook --agent {} --event {event}{continues}{via} {}",
            provider.key(),
            claude::marker(app)
        )
    }

    pub fn command_via(
        provider: Provider,
        event: &str,
        exe: &Path,
        app: &str,
        via: Option<&str>,
    ) -> String {
        let arguments = arguments_via(provider, event, app, via);
        if cfg!(windows) {
            let path = exe.to_string_lossy().replace('\\', "/");
            format!("& \"{path}\" {arguments}")
        } else {
            let path = exe.to_string_lossy().replace('\'', "'\\''");
            format!("'{path}' {arguments}")
        }
    }

    /// The hook for Mistral Vibe and Devin CLI, whose shell on Windows may be
    /// Git Bash, PowerShell or cmd: this app's path with no quotes (its short
    /// form when the path has a space), which each of them runs as it is.
    /// A path that keeps a space even so falls back to PowerShell's form.
    pub fn portable_command(provider: Provider, event: &str, exe: &Path, app: &str) -> String {
        portable_command_via(provider, event, exe, app, None)
    }

    pub fn portable_command_via(
        provider: Provider,
        event: &str,
        exe: &Path,
        app: &str,
        via: Option<&str>,
    ) -> String {
        let path = short_path(exe).to_string_lossy().replace('\\', "/");
        if path.contains(char::is_whitespace) {
            return command_via(provider, event, exe, app, via);
        }
        format!("{path} {}", arguments_via(provider, event, app, via))
    }

    /// The hook for Google Antigravity, which runs it with `cmd /c` on
    /// Windows and `sh -c` elsewhere: the portable form there, and the path
    /// in single quotes here. The app itself prints Antigravity's answer
    /// (hooks::transport::fixed_answer).
    pub fn antigravity_command(event: &str, exe: &Path, app: &str) -> String {
        if cfg!(windows) {
            return portable_command(Provider::Antigravity, event, exe, app);
        }
        let path = exe.to_string_lossy().replace('\'', "'\\''");
        format!(
            "'{path}' --hook --agent {} --event {event} {}",
            Provider::Antigravity.key(),
            claude::marker(app)
        )
    }

    /// The island's named hook in Antigravity's hooks.json: every event's
    /// command, the tool events in a group matching every tool.
    pub fn antigravity_entry(exe: &Path, app: &str) -> Value {
        let mut entry = Map::new();
        for event in ANTIGRAVITY_EVENTS {
            let handler = json!({
                "type": "command",
                "timeout": EVENT_TIMEOUT,
                "command": antigravity_command(event, exe, app),
            });
            let value = if ANTIGRAVITY_TOOL_EVENTS.contains(&event) {
                json!([{ "matcher": "*", "hooks": [handler] }])
            } else {
                json!([handler])
            };
            entry.insert(event.to_owned(), value);
        }
        Value::Object(entry)
    }

    /// The hooks.json without the island's named hooks: every top-level
    /// entry one of whose commands carries the marker, whatever it is called.
    pub fn antigravity_removing(settings: &Value, app: &str) -> Value {
        fn carries(value: &Value, marker: &str) -> bool {
            match value {
                Value::Object(object) => {
                    object
                        .get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|c| c.contains(marker))
                        || object.values().any(|v| carries(v, marker))
                }
                Value::Array(list) => list.iter().any(|v| carries(v, marker)),
                _ => false,
            }
        }
        let marker = claude::marker(app);
        match settings {
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .filter(|(_, v)| !carries(v, &marker))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
            _ => json!({}),
        }
    }

    pub fn antigravity_installing(settings: &Value, exe: &Path, app: &str) -> Value {
        let mut settings = antigravity_removing(settings, app);
        if let Some(object) = settings.as_object_mut() {
            object.insert(own_name(app), antigravity_entry(exe, app));
        }
        settings
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
            let length =
                unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buffer)) } as usize;
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

    /// Grok CLI's hooks file of the island's own: every event's command in
    /// a group with no matcher. On Windows Grok runs a command with
    /// arguments through PowerShell (or Git Bash), which both take the
    /// portable form.
    pub fn grok_file(exe: &Path, app: &str) -> Vec<u8> {
        let mut hooks = Map::new();
        for event in GROK_EVENTS {
            hooks.insert(
                event.to_owned(),
                json!([{ "hooks": [{
                    "type": "command",
                    "timeout": EVENT_TIMEOUT,
                    "command": portable_command(Provider::Grok, event, exe, app),
                }] }]),
            );
        }
        let mut data = serde_json::to_vec_pretty(&json!({ "hooks": hooks })).unwrap_or_default();
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

    /// The lines that open and close the island's entries are TOML comments;
    /// on Windows the marker is the hook's arguments, so `#` starts them.
    pub fn vibe_start(app: &str) -> String {
        format!(
            "# {} start: added by the island, removed by it",
            claude::marker(app)
        )
    }

    pub fn vibe_end(app: &str) -> String {
        format!("# {} end", claude::marker(app))
    }

    /// The island's `[[hooks]]` entries between its two marker lines; names
    /// carry the app's own, so two builds never clash.
    pub fn vibe_block(exe: &Path, app: &str) -> String {
        let mut lines = vec![vibe_start(app)];
        for event in VIBE_EVENTS {
            lines.push("[[hooks]]".into());
            lines.push(format!(
                "name = {}",
                toml_string(&format!("{}-{event}", own_name(app)))
            ));
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
        Some(if result.trim().is_empty() {
            String::new()
        } else {
            result
        })
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
        state_in(provider, home, exe, endpoint, app, None)
    }

    /// The state of the agent's hooks where it reads them by default, or in
    /// `folder`, one the person added (#236).
    pub fn state_in(
        provider: Provider,
        home: &Path,
        exe: &Path,
        endpoint: &str,
        app: &str,
        folder: Option<&Path>,
    ) -> State {
        let Some(path) = file(provider, home, app, folder) else {
            return State::Missing;
        };
        let read = |path: &Path| claude::read(path);
        match provider {
            Provider::Gemini => match read(&path) {
                Ok(s) => groups_state(&s, &GEMINI_EVENTS, app, |e| {
                    command(Provider::Gemini, e, exe, app)
                }),
                Err(_) => State::Unreadable,
            },
            Provider::Cursor => match read(&path) {
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
            Provider::Copilot => own_state(&path, &copilot_file(exe, app)),
            Provider::Grok => own_state(&path, &grok_file(exe, app)),
            Provider::Opencode => own_state(&path, &opencode_file(endpoint, app)),
            Provider::Pi => own_state(&path, &pi_file(endpoint, app)),
            Provider::Vibe => match vibe_text(&path) {
                Ok(None) => State::Missing,
                Ok(Some(text)) if text.contains(&vibe_block(exe, app)) => State::Installed,
                Ok(Some(text)) if text.contains(&claude::marker(app)) => State::Outdated,
                Ok(Some(text)) if vibe_inline(&text) => State::Unreadable,
                Ok(Some(_)) => State::Missing,
                Err(_) => State::Unreadable,
            },
            Provider::Devin => match read(&path) {
                Ok(s) => groups_state(&s, &DEVIN_EVENTS, app, |e| {
                    portable_command(Provider::Devin, e, exe, app)
                }),
                Err(_) => State::Unreadable,
            },
            Provider::Antigravity => match read(&path) {
                Ok(s) if s.get(own_name(app)) == Some(&antigravity_entry(exe, app)) => {
                    State::Installed
                }
                Ok(s) if antigravity_removing(&s, app) != s && s.is_object() => State::Outdated,
                Ok(_) => State::Missing,
                Err(_) => State::Unreadable,
            },
            Provider::Codex => {
                let Some(folder) = folder else {
                    return State::Missing;
                };
                let tag = folder_tag(folder);
                match read(&path) {
                    Ok(s) => groups_state(&s, &CODEX_EVENTS, app, |e| {
                        portable_command_via(Provider::Codex, e, exe, app, Some(&tag))
                    }),
                    Err(_) => State::Unreadable,
                }
            }
            Provider::Claude | Provider::Custom => State::Missing,
        }
    }

    /// Installed when every event holds the current command; outdated when
    /// some other command of the island's is there.
    fn groups_state(
        settings: &Value,
        events: &[&str],
        app: &str,
        command: impl Fn(&str) -> String,
    ) -> State {
        if groups_hold(settings, events, command) {
            State::Installed
        } else if claude::has_any(settings, app) {
            State::Outdated
        } else {
            State::Missing
        }
    }

    /// The island's groups after the person's own; events in `matched` get
    /// a matcher for every tool, as Claude Code and Codex want.
    fn groups_installing(
        settings: &Value,
        events: &[&str],
        app: &str,
        matched: &[&str],
        entry: impl Fn(&str) -> Value,
    ) -> Value {
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
        for event in events {
            let mut group = json!({ "hooks": [entry(event)] });
            if matched.contains(event) {
                group["matcher"] = json!("*");
            }
            let list = hooks.entry(*event).or_insert_with(|| json!([]));
            if !list.is_array() {
                *list = json!([]);
            }
            if let Some(list) = list.as_array_mut() {
                list.push(group);
            }
        }
        settings
    }

    /// Codex's hooks in a Codex home the person added, each naming the folder.
    pub fn codex_installing(settings: &Value, exe: &Path, app: &str, folder: &Path) -> Value {
        let tag = folder_tag(folder);
        groups_installing(settings, &CODEX_EVENTS, app, &CLAUDE_TOOL_EVENTS, |event| {
            // Codex holds SessionEnd to 3 seconds and warns of more.
            json!({
                "type": "command",
                "timeout": if event == "SessionEnd" { 3 } else { EVENT_TIMEOUT },
                "command": portable_command_via(Provider::Codex, event, exe, app, Some(&tag)),
            })
        })
    }

    // MARK: Tools with Claude Code style hooks (#236)

    /// A tool's hooks: Claude Code's events in Claude Code's groups, each
    /// command naming the tool.
    pub fn custom_state(path: &Path, tool: &str, exe: &Path, app: &str) -> State {
        match claude::read(path) {
            Ok(s) => groups_state(&s, &CUSTOM_EVENTS, app, |e| {
                portable_command_via(Provider::Custom, e, exe, app, Some(tool))
            }),
            Err(_) => State::Unreadable,
        }
    }

    pub fn custom_installing(settings: &Value, tool: &str, exe: &Path, app: &str) -> Value {
        groups_installing(
            settings,
            &CUSTOM_EVENTS,
            app,
            &CLAUDE_TOOL_EVENTS,
            |event| {
                json!({
                    "type": "command",
                    "timeout": EVENT_TIMEOUT,
                    "command": portable_command_via(Provider::Custom, event, exe, app, Some(tool)),
                })
            },
        )
    }

    pub fn install_custom(path: &Path, tool: &str, exe: &Path, app: &str) -> Result<(), String> {
        claude::update(path, |s| custom_installing(s, tool, exe, app)).map_err(|e| e.to_string())
    }

    pub fn remove_custom(path: &Path, app: &str) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        claude::update(path, |s| claude::removing(s, app)).map_err(|e| e.to_string())
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
        install_in(provider, home, exe, endpoint, app, None)
    }

    pub fn install_in(
        provider: Provider,
        home: &Path,
        exe: &Path,
        endpoint: &str,
        app: &str,
        folder: Option<&Path>,
    ) -> Result<(), String> {
        let Some(path) = file(provider, home, app, folder) else {
            return Ok(());
        };
        let failure = |e: claude::Failure| e.to_string();
        match provider {
            Provider::Gemini => {
                claude::update(&path, |s| gemini_installing(s, exe, app)).map_err(failure)
            }
            Provider::Cursor => {
                claude::update(&path, |s| cursor_installing(s, exe, app)).map_err(failure)
            }
            Provider::Copilot => write_own(&path, &copilot_file(exe, app), app),
            Provider::Grok => write_own(&path, &grok_file(exe, app), app),
            Provider::Opencode => write_own(&path, &opencode_file(endpoint, app), app),
            Provider::Pi => write_own(&path, &pi_file(endpoint, app), app),
            Provider::Vibe => {
                if let Ok(Some(text)) = vibe_text(&path)
                    && vibe_inline(&text)
                {
                    return Err(format!("{} keeps its hooks inline", path.display()));
                }
                claude::update_text(&path, |text| Some(vibe_installing(text, exe, app)))
                    .map_err(failure)
            }
            Provider::Devin => {
                claude::update(&path, |s| devin_installing(s, exe, app)).map_err(failure)
            }
            Provider::Antigravity => {
                claude::update(&path, |s| antigravity_installing(s, exe, app)).map_err(failure)
            }
            Provider::Codex => match folder {
                Some(folder) => claude::update(&path, |s| codex_installing(s, exe, app, folder))
                    .map_err(failure),
                None => Ok(()),
            },
            Provider::Claude | Provider::Custom => Ok(()),
        }
    }

    pub fn remove(provider: Provider, home: &Path, app: &str) -> Result<(), String> {
        remove_in(provider, home, app, None)
    }

    pub fn remove_in(
        provider: Provider,
        home: &Path,
        app: &str,
        folder: Option<&Path>,
    ) -> Result<(), String> {
        let Some(path) = file(provider, home, app, folder) else {
            return Ok(());
        };
        let failure = |e: claude::Failure| e.to_string();
        match provider {
            Provider::Copilot | Provider::Opencode | Provider::Pi | Provider::Grok => {
                remove_own(&path, app)
            }
            _ if !path.exists() => Ok(()),
            Provider::Gemini | Provider::Devin | Provider::Codex => {
                claude::update(&path, |s| claude::removing(s, app)).map_err(failure)
            }
            Provider::Cursor => claude::update(&path, |s| flat_removing(s, app)).map_err(failure),
            Provider::Vibe => {
                claude::update_text(&path, |text| text.and_then(|t| vibe_removing(t, app)))
                    .map_err(failure)
            }
            Provider::Antigravity => {
                claude::update(&path, |s| antigravity_removing(s, app)).map_err(failure)
            }
            Provider::Claude | Provider::Custom => Ok(()),
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

    /// Writes a file of the island's own, never over one that is not, and
    /// keeps a copy of what was there first (#236).
    fn write_own(path: &Path, data: &[u8], app: &str) -> Result<(), String> {
        let existing = std::fs::read(path).ok();
        if let Some(existing) = &existing
            && !holds_marker(existing, app)
        {
            return Err(format!("{} is not the island's", path.display()));
        }
        if existing.as_deref() == Some(data) {
            return Ok(());
        }
        claude::write_with_backup(path, existing.as_deref(), data).map_err(|e| e.to_string())
    }

    /// Takes a file of the island's own away, a copy kept beside it.
    fn remove_own(path: &Path, app: &str) -> Result<(), String> {
        match std::fs::read(path) {
            Ok(existing) if holds_marker(&existing, app) => {
                claude::keep_copy(&existing, path).map_err(|e| e.to_string())?;
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
                takes_keys: true,
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
            json!({ "kind": "permission", "session": "s", "id": "per", "permission": "bash", "metadata": { "command": "rm -rf build", "commandComplete": true, "commandLength": 12 }, "always": ["rm *"] }),
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
        if cfg!(windows) {
            assert!(command.starts_with(
                "& \"C:/Program Files/Openbar/Openbar.exe\" --hook --agent gemini --event BeforeTool"
            ));
        } else {
            assert!(command.starts_with(
                "'C:\\Program Files\\Openbar\\Openbar.exe' --hook --agent gemini --event BeforeTool"
            ));
        }
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
        send(
            &mut tracker,
            json!({ "kind": "model", "model": "claude-sonnet-4-5" }),
        );
        send(
            &mut tracker,
            json!({ "kind": "prompt", "text": "list the files" }),
        );
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
        send(
            &mut tracker,
            json!({ "kind": "tool", "call": "c1", "tool": "bash", "status": "completed" }),
        );
        send(
            &mut tracker,
            json!({ "kind": "reply", "text": "Here they are." }),
        );
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
        send(
            &mut tracker,
            json!({ "kind": "permission", "id": "x", "permission": "bash" }),
        );
        assert!(snap(&mut tracker, NOW, true).0[0].permission.is_none());
        assert!(waits(&envelope("pi", "", json!({ "kind": "listen" }))));
        assert!(!waits(&envelope(
            "pi",
            "",
            json!({ "kind": "permission", "session": "p1", "id": "x" })
        )));
        assert!(
            decode(&envelope("pi", "", json!({ "kind": "listen" })), NOW, ON)
                .unwrap()
                .listens
        );
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
        send(
            &mut tracker,
            "session_start",
            json!({ "model": "mistral-medium-3.5" }),
        );
        send(
            &mut tracker,
            "pre_agent",
            json!({ "prompt": "fix the tests" }),
        );
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
        send(
            &mut tracker,
            "notification",
            json!({ "notification_type": "turn_completed" }),
        );
        assert_eq!(
            snap(&mut tracker, NOW, true).0[0].phase,
            SessionPhase::Finished
        );
        let decoded = decode(
            &envelope(
                "vibe",
                "notification",
                json!({ "session_id": "v1", "transcript_path": "C:/x" }),
            ),
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
            let mut event =
                json!({ "session_id": "d1", "prompt_id": "t1", "hook_event_name": hook });
            for (k, v) in fields.as_object().unwrap() {
                event[k] = v.clone();
            }
            feed(tracker, &envelope("devin", hook, event), ON);
        };
        send(&mut tracker, "SessionStart", json!({ "source": "startup" }));
        send(
            &mut tracker,
            "UserPromptSubmit",
            json!({ "prompt": "add a test" }),
        );
        send(
            &mut tracker,
            "PreToolUse",
            json!({ "tool_name": "exec", "tool_input": { "command": "cargo test" } }),
        );
        let (listed, _) = snap(&mut tracker, NOW, true);
        assert_eq!(listed[0].provider, Provider::Devin);
        assert_eq!(listed[0].current_tool.as_ref().unwrap().name, "Bash");
        send(
            &mut tracker,
            "PermissionRequest",
            json!({ "tool_name": "exec", "tool_input": { "command": "git push" } }),
        );
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
        send(
            &mut tracker,
            "PreToolUse",
            json!({ "tool_name": "ask_user_question", "tool_input": {} }),
        );
        assert_eq!(
            snap(&mut tracker, NOW, true).0[0].phase,
            SessionPhase::WaitingInput
        );
        send(
            &mut tracker,
            "PostToolUse",
            json!({ "tool_name": "ask_user_question", "tool_input": {} }),
        );
        assert_eq!(
            snap(&mut tracker, NOW, true).0[0].phase,
            SessionPhase::Generating
        );
        send(&mut tracker, "Stop", json!({}));
        assert_eq!(
            snap(&mut tracker, NOW, true).0[0].phase,
            SessionPhase::Finished
        );
        send(&mut tracker, "SessionEnd", json!({}));
        assert!(snap(&mut tracker, NOW, true).0.is_empty());
        assert!(!waits(&envelope(
            "devin",
            "PermissionRequest",
            json!({ "session_id": "d1" })
        )));

        let devin = serde_json::to_vec(
            &json!({ "hook_event_name": "PreToolUse", "session_id": "d", "prompt_id": "t" }),
        )
        .unwrap();
        let claude = serde_json::to_vec(&json!({ "hook_event_name": "PreToolUse", "session_id": "c", "prompt_id": "t", "transcript_path": "C:/t.jsonl" })).unwrap();
        assert!(is_devin_payload(&devin) && !is_devin_payload(&claude));
        assert!(
            is_devin_process("devin.exe")
                && is_devin_process("Devin")
                && !is_devin_process("claude.exe")
        );
        assert!(!is_devin_process("devinci.exe"));
    }

    #[test]
    fn vibe_hooks_go_after_the_persons_own_and_come_out_alone() {
        let exe = Path::new("C:/Users/me/AppData/Local/Openbar/Openbar.exe");
        let own = "[[hooks]]\nname = \"lint\"\ntype = \"post_agent\"\ncommand = \"eslint .\"\n";
        let installed = installer::vibe_installing(Some(own), exe, "app");
        assert!(installed.starts_with(own));
        assert_eq!(
            installed.matches("[[hooks]]").count(),
            1 + installer::VIBE_EVENTS.len()
        );
        assert!(installed.contains("name = \"openbar-app-pre_tool\""));
        // Every line the island adds is a TOML table, key or comment, never a bare word.
        for line in installed.lines().skip(own.lines().count()) {
            let line = line.trim();
            assert!(
                line.is_empty()
                    || line.starts_with('#')
                    || line.starts_with("[[hooks]]")
                    || line.contains(" = "),
                "not TOML: {line}"
            );
        }
        let again = installer::vibe_installing(Some(&installed), exe, "app");
        assert_eq!(again, installed);
        assert_eq!(
            installer::vibe_removing(&installed, "app").as_deref(),
            Some(own)
        );
        assert_eq!(installer::vibe_removing(own, "app"), None);
        let both = installer::vibe_installing(Some(&installed), exe, "other");
        assert_eq!(
            installer::vibe_removing(&both, "app")
                .unwrap()
                .matches("[[hooks]]")
                .count(),
            1 + installer::VIBE_EVENTS.len()
        );
        assert!(installer::vibe_inline("hooks = [{ name = \"x\" }]\n"));
        assert!(!installer::vibe_inline(own));
        assert_eq!(installer::toml_string("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
        // A path with no space runs as it is in Git Bash, PowerShell and cmd.
        let command = installer::portable_command(Provider::Vibe, "pre_tool", exe, "app");
        assert!(command.starts_with(
            "C:/Users/me/AppData/Local/Openbar/Openbar.exe --hook --agent vibe --event pre_tool "
        ));
        assert!(!command.contains('"') && !command.starts_with('&'));
        // One that keeps a space falls back to the quoted form: PowerShell's
        // on Windows, a POSIX shell's elsewhere.
        let spaced = installer::portable_command(
            Provider::Vibe,
            "pre_tool",
            Path::new("Z:/No Such Folder/Openbar.exe"),
            "app",
        );
        assert!(spaced.starts_with(if cfg!(windows) { "& \"" } else { "'Z:/No Such" }));
    }

    #[test]
    fn devin_hooks_go_beside_the_persons_own_and_come_out_alone() {
        let exe = Path::new("C:/Openbar/Openbar.exe");
        let own = json!({ "version": 1, "permission_mode": "auto",
                          "hooks": { "PreToolUse": [{ "matcher": "exec", "hooks": [{ "type": "command", "command": "guard.ps1" }] }] } });
        let installed = installer::devin_installing(&own, exe, "app");
        let pre = installed
            .pointer("/hooks/PreToolUse")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0], own.pointer("/hooks/PreToolUse/0").unwrap().clone());
        assert_eq!(installed["permission_mode"], "auto");
        assert_eq!(
            installed["hooks"].as_object().unwrap().len(),
            installer::DEVIN_EVENTS.len()
        );
        assert_eq!(crate::hooks::installer::removing(&installed, "app"), own);
        assert_eq!(
            installer::devin_installing(&json!({}), exe, "app")["version"],
            1
        );
        let devin = installer::portable_command(Provider::Devin, "PermissionRequest", exe, "app");
        assert!(
            devin.starts_with(
                "C:/Openbar/Openbar.exe --hook --agent devin --event PermissionRequest"
            )
        );
    }

    #[test]
    fn a_vibe_log_ends_on_its_last_reply() {
        let history = br#"[{"message":{"content":[{"text":"list","type":"text"}],"role":"user"},"source":"history"},{"message":{"content":[{"type":"tool_call","name":"bash"}],"role":"assistant"},"source":"history"},{"message":{"content":[{"text":"Done: the folder holds the files. ","type":"text"}],"role":"assistant"},"source":"history"}]"#;
        assert_eq!(
            vibe_chunk_reply(history).as_deref(),
            Some("Done: the folder holds the files.")
        );
        assert_eq!(vibe_chunk_reply(b"not json"), None);
        let folder = std::env::temp_dir().join(format!("ob-vibe-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("chunks")).unwrap();
        std::fs::write(folder.join("chunks").join("b.json"), history).unwrap();
        std::fs::write(
            folder.join("chunks").join("c.json"),
            br#"[{"type":"notice"}]"#,
        )
        .unwrap();
        assert_eq!(
            vibe_last_reply(&folder).as_deref(),
            Some("Done: the folder holds the files.")
        );
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
        assert_eq!(
            name(Provider::Devin, "mcp__github__create_issue"),
            "github__create_issue"
        );
        assert_eq!(Provider::from_key("devin"), Some(Provider::Devin));
    }

    fn antigravity(hook: &str, extra: Value) -> Vec<u8> {
        let mut event = json!({
            "conversationId": "a1",
            "workspacePaths": ["/w/rocket"],
            "cwd": "/w/rocket",
            "transcriptPath": "/h/.gemini/antigravity-cli/brain/a1/.system_generated/logs/transcript_full.jsonl",
            "modelName": "gemini-3.8-flash-high",
            "lastUserInput": "run the tests",
        });
        for (key, value) in extra.as_object().unwrap() {
            event[key] = value.clone();
        }
        envelope("antigravity", hook, event)
    }

    #[test]
    fn an_antigravity_conversation_is_told_by_its_hooks() {
        let mut t = Tracker::default();
        let decoded = decode(&antigravity("PreInvocation", json!({})), NOW, ON).unwrap();
        assert_eq!(decoded.provider, Some(Provider::Antigravity));
        assert!(
            decoded
                .transcript
                .unwrap()
                .ends_with("logs/transcript_full.jsonl")
        );
        feed(
            &mut t,
            &antigravity("PreInvocation", json!({ "invocationNum": 0 })),
            ON,
        );
        let (listed, _) = snap(&mut t, NOW, true);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "antigravity:a1");
        assert_eq!(listed[0].provider, Provider::Antigravity);
        assert_eq!(listed[0].phase, SessionPhase::Generating);
        assert_eq!(listed[0].model.as_deref(), Some("gemini-3.8-flash-high"));
        assert_eq!(listed[0].chat.len(), 1);
        assert_eq!(listed[0].chat[0].text, "run the tests");
        feed(
            &mut t,
            &antigravity("PreInvocation", json!({ "invocationNum": 1 })),
            ON,
        );
        assert_eq!(snap(&mut t, NOW, true).0[0].chat.len(), 1);
        feed(
            &mut t,
            &antigravity(
                "PreToolUse",
                json!({ "stepIdx": 4, "toolCall": { "name": "run_command",
                        "args": { "CommandLine": "npm test", "toolSummary": "Run tests" } } }),
            ),
            ON,
        );
        let (listed, _) = snap(&mut t, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::RunningTool);
        let tool = listed[0].current_tool.clone().unwrap();
        assert_eq!(tool.name, "Bash");
        assert_eq!(tool.summary.as_deref(), Some("npm test"));
        feed(
            &mut t,
            &antigravity("PostToolUse", json!({ "stepIdx": 4 })),
            ON,
        );
        feed(
            &mut t,
            &antigravity(
                "PreToolUse",
                json!({ "stepIdx": 5, "toolCall": { "name": "write_to_file",
                        "args": { "TargetFile": "/w/rocket/src/a.ts" } } }),
            ),
            ON,
        );
        feed(
            &mut t,
            &antigravity("PostToolUse", json!({ "stepIdx": 5, "error": "denied" })),
            ON,
        );
        let (listed, _) = snap(&mut t, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::Generating);
        assert_eq!(listed[0].timeline.len(), 2);
        assert!(listed[0].timeline[0].finished.is_some() && !listed[0].timeline[0].failed);
        assert_eq!(listed[0].timeline[1].name, "Write");
        assert!(listed[0].timeline[1].failed);
        feed(
            &mut t,
            &antigravity(
                "Stop",
                json!({ "terminationReason": "NO_TOOL_CALL", "fullyIdle": true, "error": "" }),
            ),
            ON,
        );
        let (listed, _) = snap(&mut t, NOW, true);
        assert_eq!(listed[0].phase, SessionPhase::Finished);
        assert!(listed[0].permission.is_none());
        assert!(!waits(&antigravity("PreToolUse", json!({}))));
        // The model picker's "auto" names no model; no conversation, no session.
        let auto = decode(
            &antigravity("PreInvocation", json!({ "modelName": "auto" })),
            NOW,
            ON,
        )
        .unwrap();
        assert!(!auto.events.iter().any(|e| matches!(e.kind, Kind::Model(_))));
        let none = decode(
            &envelope(
                "antigravity",
                "PreInvocation",
                json!({ "workspacePaths": ["/w"] }),
            ),
            NOW,
            ON,
        )
        .unwrap();
        assert!(none.events.is_empty());
        let hidden = decode(
            &antigravity("PreInvocation", json!({ "invocationNum": 0 })),
            NOW,
            OFF,
        )
        .unwrap();
        assert!(hidden.events.iter().any(|e| e.kind == Kind::Prompt(None)));
        let bare = decode(
            &envelope(
                "antigravity",
                "PreInvocation",
                json!({ "conversationId": "a3", "cwd": "/w/other" }),
            ),
            NOW,
            ON,
        )
        .unwrap();
        assert_eq!(bare.events[0].cwd.as_deref(), Some("/w/other"));
        // Without previews a command line is not kept.
        let quiet = decode(
            &antigravity(
                "PreToolUse",
                json!({ "stepIdx": 1, "toolCall": { "name": "run_command", "args": { "CommandLine": "rm -rf x" } } }),
            ),
            NOW,
            OFF,
        )
        .unwrap();
        assert!(
            quiet
                .events
                .iter()
                .any(|e| matches!(&e.kind, Kind::ToolStarted(u) if u.summary.is_none()))
        );
        // Its language server outlives the conversation: a quiet one leaves.
        t.prune(NOW + 60.0, |_| true);
        assert_eq!(snap(&mut t, NOW, true).0.len(), 1);
        t.prune(NOW + QUIET_FOR + 1.0, |_| true);
        assert!(snap(&mut t, NOW, true).0.is_empty());
    }

    #[test]
    fn antigravity_hooks_go_beside_the_persons_own_and_come_out_alone() {
        let exe = Path::new("/opt/Openbar/openbar");
        let own = json!({
            "lint-checker": { "PostToolUse": [{ "matcher": "run_command", "hooks": [{ "type": "command", "command": "./lint.sh" }] }] },
            "safety-gate": { "enabled": false, "PreToolUse": [{ "matcher": "run_command", "hooks": [{ "command": "./safety.sh" }] }] },
        });
        let installed = installer::antigravity_installing(&own, exe, "app");
        let mine = &installed[installer::own_name("app")];
        assert_eq!(installed["lint-checker"], own["lint-checker"]);
        assert_eq!(installed["safety-gate"], own["safety-gate"]);
        assert_eq!(
            mine.as_object().unwrap().len(),
            installer::ANTIGRAVITY_EVENTS.len()
        );
        assert_eq!(mine.pointer("/PreToolUse/0/matcher"), Some(&json!("*")));
        assert_eq!(mine.pointer("/Stop/0/type"), Some(&json!("command")));
        assert_eq!(installer::antigravity_removing(&installed, "app"), own);
        // Another app's hook stays when this one's goes.
        let both = installer::antigravity_installing(&installed, exe, "other");
        assert_eq!(
            installer::antigravity_removing(&both, "app")
                .as_object()
                .unwrap()
                .len(),
            3
        );
        let command = installer::antigravity_command("PreToolUse", exe, "app");
        if cfg!(windows) {
            assert!(
                command.starts_with(
                    "/opt/Openbar/openbar --hook --agent antigravity --event PreToolUse"
                )
            );
        } else {
            assert!(command.starts_with(
                "'/opt/Openbar/openbar' --hook --agent antigravity --event PreToolUse"
            ));
            let spaced =
                installer::antigravity_command("Stop", Path::new("/opt/My App/it's"), "app");
            assert!(spaced.starts_with(r"'/opt/My App/it'\''s' --hook"));
        }
        assert!(command.contains(&crate::hooks::installer::marker("app")));
        // State, from a file on disk.
        let home = std::env::temp_dir().join(format!("ob-ag-{}", rand_id()));
        let path = installer::antigravity_hooks(&home);
        assert!(path.ends_with(".gemini/config/hooks.json"));
        assert_eq!(
            installer::state(Provider::Antigravity, &home, exe, "", "app"),
            installer::State::Missing
        );
        installer::install(Provider::Antigravity, &home, exe, "", "app").unwrap();
        assert_eq!(
            installer::state(Provider::Antigravity, &home, exe, "", "app"),
            installer::State::Installed
        );
        assert_eq!(
            installer::state(
                Provider::Antigravity,
                &home,
                Path::new("/elsewhere/openbar"),
                "",
                "app"
            ),
            installer::State::Outdated
        );
        installer::remove(Provider::Antigravity, &home, "app").unwrap();
        assert_eq!(
            installer::state(Provider::Antigravity, &home, exe, "", "app"),
            installer::State::Missing
        );
        std::fs::write(&path, "{ not json").unwrap();
        assert!(installer::install(Provider::Antigravity, &home, exe, "", "app").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        assert_eq!(
            installer::state(Provider::Antigravity, &home, exe, "", "app"),
            installer::State::Unreadable
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_antigravity_transcript_gives_the_last_reply() {
        let folder = std::env::temp_dir().join(format!("ob-ag-log-{}", rand_id()));
        std::fs::create_dir_all(&folder).unwrap();
        let log = folder.join("transcript_full.jsonl");
        let lines = [
            r#"{"type":"USER_INPUT","status":"DONE","content":"<USER_REQUEST>\nlist the files\n</USER_REQUEST>"}"#,
            r#"{"type":"PLANNER_RESPONSE","status":"DONE","tool_calls":[{"name":"run_command"}]}"#,
            r#"{"type":"PLANNER_RESPONSE","status":"DONE","content":"The folder holds a.txt."}"#,
            "not json",
        ];
        std::fs::write(&log, lines.join("\n")).unwrap();
        assert_eq!(
            antigravity_last_reply(&log).as_deref(),
            Some("The folder holds a.txt.")
        );
        let other = folder.join("notes.jsonl");
        std::fs::write(&other, lines.join("\n")).unwrap();
        assert!(antigravity_last_reply(&other).is_none());
        assert!(antigravity_last_reply(&folder.join("gone/transcript.jsonl")).is_none());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn antigravity_is_named_and_started_with_its_prompt() {
        assert!(Provider::HOOK_AGENTS.contains(&Provider::Antigravity));
        assert!(!Provider::Antigravity.answers_permissions());
        assert_eq!(
            Provider::from_key("antigravity"),
            Some(Provider::Antigravity)
        );
        assert_eq!(command_name(Provider::Antigravity), Some("agy"));
        assert_eq!(start_arguments(Provider::Antigravity, "go"), ["-i", "go"]);
        assert_eq!(name(Provider::Antigravity, "view_file"), "Read");
        let late = decode(
            &envelope(
                "antigravity",
                "PostToolUse",
                json!({ "conversationId": "a1", "stepIdx": 9,
                      "toolCall": { "name": "manage_task", "args": {} } }),
            ),
            NOW,
            ON,
        )
        .unwrap();
        assert!(
            matches!(&late.events[0].kind, Kind::ToolFinished { name, .. } if name == "BashOutput")
        );
        assert_eq!(
            name(Provider::Antigravity, "browser_click"),
            "Browser_click"
        );
        assert_eq!(
            crate::hooks::transport::fixed_answer("antigravity", "PreToolUse"),
            Some(r#"{"decision":"ask"}"#)
        );
        assert_eq!(
            crate::hooks::transport::fixed_answer("antigravity", "Stop"),
            Some("{}")
        );
        assert_eq!(
            crate::hooks::transport::fixed_answer("gemini", "BeforeTool"),
            None
        );
    }

    #[test]
    fn a_checkout_says_its_branch_detached_head_or_nothing() {
        let root = std::env::temp_dir().join(format!("vs-head-{:x}", rand_id()));
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git").join("worktrees").join("wt")).unwrap();
        let head = repo.join(".git").join("HEAD");
        std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
        let folder = repo.to_string_lossy().into_owned();
        assert_eq!(read_head(&folder), Head::Branch("main".into()));
        // A checkout inside the session shows on the next read.
        std::fs::write(&head, "ref: refs/heads/feat/y\n").unwrap();
        assert_eq!(read_head(&folder), Head::Branch("feat/y".into()));
        std::fs::write(&head, "3f2c1a9e8d7b6c5a4f3e2d1c0b9a8f7e6d5c4b3a\n").unwrap();
        assert_eq!(read_head(&folder), Head::Detached);
        // A linked worktree reads its own HEAD through its .git file.
        let wt = root.join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        let gitdir = repo.join(".git").join("worktrees").join("wt");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/worktree-wt\n").unwrap();
        assert_eq!(
            read_head(&wt.to_string_lossy()),
            Head::Branch("worktree-wt".into())
        );
        assert_eq!(read_head(""), Head::Unknown);
        assert_eq!(
            read_head(&root.join("nowhere").to_string_lossy()),
            Head::Unknown
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_session_keeps_its_branch_only_where_head_cant_be_read() {
        let said = Some("feat/a".to_string());
        assert_eq!(
            live_branch(said.clone(), &Head::Branch("feat/b".into())).as_deref(),
            Some("feat/b")
        );
        assert_eq!(live_branch(said.clone(), &Head::Detached), None);
        assert_eq!(live_branch(said, &Head::Unknown).as_deref(), Some("feat/a"));
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

    // MARK: Vibe's app-server proxy (#251)

    const VIBE_SESSION: &str = "6267026e-5700-b9cb-3d47-f84eb25621f1";
    // As Vibe 3.1.2 sent it (trimmed of what the proxy does not read).
    const VIBE_APPROVAL: &str = r#"{"jsonrpc":"2.0","id":1,"method":"callback/call","params":{"callback":{"id":"approval-call_1-da12e44f","sessionId":"6267026e-5700-b9cb-3d47-f84eb25621f1","turnId":"turn-aee2","type":"callback","callbackId":"approval-call_1-da12e44f","title":"Approve file_system.bash","detail":{"kind":"approval","effect":{"toolName":"file_system.bash","display":{"summary":"bash: touch approved.txt","message":"touch approved.txt"},"kind":"shell","input":{"command":"touch approved.txt"}},"choices":["approve","approve_for_session","approve_permanently","deny","cancel_turn"],"pathScopeChoices":[],"reason":null},"state":{"status":"open"}}}}"#;
    const VIBE_QUESTION: &str = r#"{"jsonrpc":"2.0","id":2,"method":"callback/call","params":{"callback":{"id":"user-input-call_q-716120f1","sessionId":"30f956f5","type":"callback","callbackId":"user-input-call_q-716120f1","title":"Question","detail":{"kind":"user_input","request":{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multiSelect":false,"hideOther":false}],"footerNote":null}},"state":{"status":"open"}}}}"#;

    fn proxy_message(event: Value) -> Vec<u8> {
        super::super::vibe_proxy::envelope(&event, 4242)
    }

    #[test]
    fn a_proxy_approval_is_held_and_answered_here() {
        use super::super::vibe_proxy as proxy;
        let callback = proxy::callback_in_server_line(VIBE_APPROVAL.as_bytes()).unwrap();
        let sent = proxy::envelope(&proxy::event_for(&callback), 4242);
        assert!(is_envelope(&sent) && waits(&sent));
        let decoded = decode(&sent, NOW, ON).unwrap();
        assert!(decoded.from_proxy);
        assert_eq!(
            decoded.permission,
            Some((
                VIBE_SESSION.to_owned(),
                "approval-call_1-da12e44f".to_owned()
            ))
        );
        let Kind::HeldPermission(permission) = &decoded.events[0].kind else {
            panic!(
                "an approval is a request the island holds: {:?}",
                decoded.events
            )
        };
        assert_eq!(permission.tool, "Bash");
        assert_eq!(permission.detail, "touch approved.txt");
        assert!(permission.offers_always);
        assert_eq!(permission.request_id, "approval-call_1-da12e44f");
        assert!(permission.content.is_some());
        assert_eq!(
            decoded.events[0].pid, 0,
            "the proxy's pid is not the agent's"
        );

        let mut t = Tracker::default();
        feed(
            &mut t,
            &envelope(
                "vibe",
                "session_start",
                json!({ "hook_event_name": "session_start", "session_id": VIBE_SESSION, "cwd": "C:/w/vibe-app", "model": "devstral" }),
            ),
            ON,
        );
        feed(&mut t, &sent, ON);
        let listed = snap(&mut t, NOW, true).0;
        assert_eq!(listed[0].phase, SessionPhase::WaitingPermission);
        assert_eq!(
            listed[0].permission.as_ref().map(|p| p.tool.as_str()),
            Some("Bash")
        );
        assert_eq!(
            listed[0].waiting_for, None,
            "answered here, not in the terminal"
        );
        assert_eq!(listed[0].cwd, "C:/w/vibe-app");
        // Answered in the terminal, the card goes.
        feed(
            &mut t,
            &proxy_message(
                json!({ "kind": "settled", "session": VIBE_SESSION, "request": "approval-call_1-da12e44f" }),
            ),
            ON,
        );
        let listed = snap(&mut t, NOW, true).0;
        assert!(listed[0].permission.is_none());
        assert_ne!(listed[0].phase, SessionPhase::WaitingPermission);
    }

    #[test]
    fn a_session_started_by_hand_is_still_answered_in_the_terminal() {
        let mut t = Tracker::default();
        feed(
            &mut t,
            &envelope(
                "vibe",
                "notification",
                json!({ "hook_event_name": "notification", "session_id": "hand", "cwd": "C:/w/x",
                        "notification_type": "approval_requested", "tool_name": "file_system.bash",
                        "message": "Vibe needs your approval" }),
            ),
            ON,
        );
        let listed = snap(&mut t, NOW, true).0;
        assert!(listed[0].permission.is_none());
        assert_eq!(listed[0].waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        assert!(!waits(&envelope(
            "vibe",
            "notification",
            json!({ "notification_type": "approval_requested", "session_id": "hand" })
        )));
    }

    #[test]
    fn a_proxy_question_is_asked_like_claude_codes() {
        use super::super::vibe_proxy as proxy;
        let callback = proxy::callback_in_server_line(VIBE_QUESTION.as_bytes()).unwrap();
        let sent = proxy::envelope(&proxy::event_for(&callback), 4242);
        assert!(waits(&sent));
        let decoded = decode(&sent, NOW, ON).unwrap();
        let Kind::HeldPermission(permission) = &decoded.events[0].kind else {
            panic!("a question is a request the island holds")
        };
        assert!(permission.asks_questions());
        assert_eq!(permission.tool, Question::TOOL);
        assert_eq!(permission.questions[0].question, "Which color?");
        assert_eq!(
            permission.questions[0]
                .options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            ["Red", "Blue"]
        );
        assert_eq!(
            permission.questions[0].options[0].description.as_deref(),
            Some("warm")
        );
        let reply = |d, q: &[Question]| -> Value {
            serde_json::from_slice(&vibe_reply(&d, q).unwrap()).unwrap()
        };
        use super::super::hook_message::Decision;
        let picked = reply(
            Decision::Answer([("Which color?".to_owned(), vec!["Blue".to_owned()])].into()),
            &permission.questions,
        );
        assert_eq!(picked["answers"][0]["answer"], "Blue");
        assert_eq!(picked["answers"][0]["isOther"], false);
        let typed = reply(
            Decision::Answer([("Which color?".to_owned(), vec!["green".to_owned()])].into()),
            &permission.questions,
        );
        assert_eq!(typed["answers"][0]["isOther"], true);
        assert_eq!(
            reply(Decision::Deny, &permission.questions)["cancelled"],
            true
        );
        assert!(
            vibe_reply(
                &Decision::Answer([("Other?".to_owned(), vec!["x".to_owned()])].into()),
                &permission.questions
            )
            .is_none()
        );
        assert!(vibe_reply(&Decision::Answer(Default::default()), &permission.questions).is_none());
    }

    #[test]
    fn what_the_island_answers_to_an_approval_is_vibes_own_decisions() {
        use super::super::hook_message::{DENY_MESSAGE, Decision};
        let reply = |d: Decision| -> Value {
            serde_json::from_slice(&vibe_reply(&d, &[]).unwrap()).unwrap()
        };
        assert_eq!(reply(Decision::Allow)["decision"], "approve");
        assert_eq!(
            reply(Decision::AllowAlways)["decision"],
            "approve_for_session"
        );
        assert_eq!(reply(Decision::Deny)["decision"], "deny");
        assert_eq!(reply(Decision::Deny)["feedback"], DENY_MESSAGE);
    }

    #[test]
    fn the_proxy_tells_of_a_session_and_of_one_answered_elsewhere() {
        let attach =
            proxy_message(json!({ "kind": "attach", "session": VIBE_SESSION, "cwd": "C:/w/app" }));
        let decoded = decode(&attach, NOW, ON).unwrap();
        assert!(!waits(&attach));
        assert_eq!(decoded.attached.as_deref(), Some(VIBE_SESSION));
        assert!(decoded.events.is_empty());
        let settled =
            proxy_message(json!({ "kind": "settled", "session": VIBE_SESSION, "request": "r1" }));
        assert!(!waits(&settled));
        assert_eq!(
            decode(&settled, NOW, ON).unwrap().events[0].kind,
            Kind::PermissionSettled(Some("r1".to_owned()))
        );
        // Vibe's hooks are not the proxy.
        let hook = envelope(
            "vibe",
            "notification",
            json!({ "session_id": "v1", "notification_type": "turn_completed" }),
        );
        assert!(!decode(&hook, NOW, ON).unwrap().from_proxy);
        // A proxy message with nothing to say is read without a crash.
        assert!(
            decode(&proxy_message(json!({ "kind": "approval" })), NOW, ON)
                .unwrap()
                .events
                .is_empty()
        );
        assert!(
            decode(
                &proxy_message(
                    json!({ "kind": "question", "session": "s", "request": "r", "questions": [] })
                ),
                NOW,
                ON
            )
            .unwrap()
            .events
            .is_empty()
        );
    }

    /// Gemini CLI and Cursor run a hook with `bash -c` (or `sh -c`) on Linux,
    /// where PowerShell's `& "…"` is a syntax error: the hook must be one a
    /// POSIX shell runs, the path quoted, quotes in it and all (#236).
    #[cfg(not(windows))]
    #[test]
    fn a_hook_command_runs_in_a_posix_shell() {
        let folder = std::env::temp_dir().join(format!("ob236 it's-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let exe = folder.join("open bar");
        let out = folder.join("args.txt");
        std::fs::write(&exe, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$OB236_OUT\"\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        for (provider, event) in [
            (Provider::Gemini, "BeforeTool"),
            (Provider::Cursor, "beforeSubmitPrompt"),
        ] {
            let command = installer::command(provider, event, &exe, "app");
            assert!(command.starts_with('\''), "{command}");
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .env("OB236_OUT", &out)
                .status()
                .unwrap();
            assert!(status.success(), "{command}");
            let args = std::fs::read_to_string(&out).unwrap();
            let mut wanted = vec!["--hook", "--agent", provider.key(), "--event", event];
            if provider == Provider::Cursor {
                wanted.push("--continue");
            }
            wanted.extend(["--marker", "openbar-hook", "--app", "app"]);
            assert_eq!(args.lines().collect::<Vec<_>>(), wanted);
        }
        // A tool's hook names the tool, and only with a tag the island makes.
        let command =
            installer::portable_command_via(Provider::Custom, "Stop", &exe, "app", Some("x y"));
        assert!(!command.contains("--via"));
        let command = installer::command_via(Provider::Codex, "Stop", &exe, "app", Some("a1b2c3"));
        assert!(command.contains("--event Stop --via a1b2c3 --marker"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Codex heard through a second home's hooks is a session of its own,
    /// apart from Codex's server, answered in Codex's terminal (#236).
    #[test]
    fn codex_through_hooks_is_a_session_of_its_own() {
        let payload = json!({ "session_id": "t1", "turn_id": "u1", "cwd": "/w/app",
            "hook_event_name": "PermissionRequest", "model": "gpt-5-codex",
            "permission_mode": "default", "tool_name": "Bash",
            "tool_input": { "command": "rm -rf build" }, "transcript_path": null });
        let data = serde_json::to_vec(&json!({ "openbar": "codex", "hook": "PermissionRequest",
            "pid": 9, "via": "a1b2c3d4e5f6", "event": payload }))
        .unwrap();
        let decoded = decode(&data, NOW, ON).unwrap();
        assert_eq!(decoded.provider, Some(Provider::Codex));
        assert_eq!(decoded.events[0].id(), "codex:a1b2c3d4e5f6:t1");
        assert!(decoded.permission.is_none());
        // A Codex envelope that names no folder, or a bad one, is not taken.
        let unnamed = envelope("codex", "Stop", payload.clone());
        assert!(decode(&unnamed, NOW, ON).is_none());
        let bad = serde_json::to_vec(&json!({ "openbar": "codex", "hook": "Stop",
            "via": "../x", "event": payload }))
        .unwrap();
        assert!(decode(&bad, NOW, ON).is_none());
        let mut tracker = Tracker::default();
        for event in decoded.events {
            tracker.ingest(event, NOW);
        }
        let (listed, _) = snap(&mut tracker, NOW, true);
        let session = &listed[0];
        assert_eq!(session.provider, Provider::Codex);
        assert_eq!(session.via.as_deref(), Some("a1b2c3d4e5f6"));
        assert!(!session.codex_server());
        assert!(session.permission.is_none());
        assert_eq!(session.waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
        assert_eq!(session.model.as_deref(), Some("gpt-5-codex"));
        // A failed tool and the last reply are heard as Claude Code's are.
        let stop = serde_json::to_vec(
            &json!({ "openbar": "codex", "hook": "Stop", "via": "a1b2c3d4e5f6",
            "event": { "session_id": "t1", "last_assistant_message": "All done." } }),
        )
        .unwrap();
        let events = decode(&stop, NOW, ON).unwrap().events;
        assert!(
            matches!(&events[0].kind, Kind::Stopped { last_text: Some(t), .. } if t == "All done.")
        );
        let failed = serde_json::to_vec(&json!({ "openbar": "custom", "hook": "PostToolUseFailure",
            "via": "tool-1", "event": { "session_id": "k", "tool_name": "Bash", "tool_input": {} } }))
        .unwrap();
        let events = decode(&failed, NOW, ON).unwrap().events;
        assert!(matches!(
            &events[0].kind,
            Kind::ToolFinished { failed: true, .. }
        ));
        assert_eq!(
            installer::devin_config(Path::new("/h"))
                .file_name()
                .unwrap(),
            "config.json"
        );
    }

    // MARK: Grok CLI (#243)

    /// The fields every hook of grok 1.0.89 sent in a live run (macOS, 2026-11-18).
    fn grok(hook: &str, event: &str, extra: Value) -> Vec<u8> {
        let mut json = json!({
            "hookEventName": event, "sessionId": "01a1ef72", "session_id": "01a1ef72",
            "cwd": "C:/w/rocket", "workspaceRoot": "C:/w/rocket", "permissionMode": "default",
            "transcriptPath": "C:/h/.grok/sessions/C%3A%2Fw%2Frocket/01a1ef72/updates.jsonl",
        });
        for (key, value) in extra.as_object().unwrap() {
            json[key] = value.clone();
        }
        envelope("grok", hook, json)
    }

    fn shell() -> Value {
        json!({ "toolName": "run_terminal_command", "toolUseId": "call_1",
                "toolInput": { "command": "rm -rf build", "description": "Clean the build" } })
    }

    #[test]
    fn a_grok_session_is_told_by_its_hooks() {
        let decoded = decode(&grok("Stop", "stop", json!({})), NOW, ON).unwrap();
        assert_eq!(decoded.provider, Some(Provider::Grok));
        assert!(
            decoded
                .transcript
                .unwrap()
                .ends_with("/01a1ef72/updates.jsonl")
        );
        let mut t = Tracker::default();
        feed(
            &mut t,
            &grok(
                "SessionStart",
                "session_start",
                json!({ "modelId": "grok-4.5" }),
            ),
            ON,
        );
        let listed = snap(&mut t, NOW, true).0;
        assert_eq!(listed[0].id, "grok:01a1ef72");
        assert_eq!(listed[0].provider, Provider::Grok);
        assert_eq!(listed[0].phase, SessionPhase::Idle);
        assert_eq!(listed[0].model.as_deref(), Some("grok-4.5"));
        feed(
            &mut t,
            &grok(
                "UserPromptSubmit",
                "user_prompt_submit",
                json!({ "prompt": "clean up" }),
            ),
            ON,
        );
        feed(&mut t, &grok("PreToolUse", "pre_tool_use", shell()), ON);
        let listed = snap(&mut t, NOW, true).0;
        assert_eq!(listed[0].phase, SessionPhase::RunningTool);
        assert_eq!(listed[0].current_tool.as_ref().unwrap().name, "Bash");
        feed(&mut t, &grok("PostToolUse", "post_tool_use", shell()), ON);
        feed(
            &mut t,
            &grok(
                "Stop",
                "stop",
                json!({ "reason": "end_turn", "lastAssistantMessage": "Cleaned." }),
            ),
            ON,
        );
        let listed = snap(&mut t, NOW, true).0;
        assert_eq!(listed[0].phase, SessionPhase::Finished);
        assert_eq!(listed[0].last_text.as_deref(), Some("Cleaned."));
        // As Grok shuts down it runs Stop once more, beside SessionEnd.
        let shutdown = decode(
            &grok("Stop", "stop", json!({ "reason": "shutdown" })),
            NOW,
            ON,
        )
        .unwrap();
        assert!(shutdown.events.is_empty());
        feed(
            &mut t,
            &grok("SessionEnd", "session_end", json!({ "reason": "shutdown" })),
            ON,
        );
        assert!(snap(&mut t, NOW, true).0.is_empty());
        // Interrupted or refused: the turn ends with no done.
        feed(
            &mut t,
            &grok("UserPromptSubmit", "user_prompt_submit", json!({})),
            ON,
        );
        feed(
            &mut t,
            &grok(
                "StopCancelled",
                "stop_cancelled",
                json!({ "reason": "user_interrupt" }),
            ),
            ON,
        );
        assert_eq!(snap(&mut t, NOW, true).0[0].phase, SessionPhase::Idle);
    }

    #[test]
    fn a_grok_prompt_waits_on_the_tool_named_before_it() {
        let mut t = Tracker::default();
        feed(&mut t, &grok("PreToolUse", "pre_tool_use", shell()), OFF);
        feed(
            &mut t,
            &grok(
                "Notification",
                "notification",
                json!({ "notificationType": "permission_prompt" }),
            ),
            OFF,
        );
        let listed = snap(&mut t, NOW, false).0;
        assert_eq!(listed[0].phase, SessionPhase::WaitingPermission);
        let asked = listed[0].permission.clone().unwrap();
        assert_eq!(asked.tool, "Bash");
        assert_eq!(asked.detail, "rm -rf build");
        assert!(asked.content.as_ref().is_some_and(|c| c.complete));
        assert!(listed[0].waiting_for.is_none());
        // Where the terminal takes no keys, the person answers in Grok.
        let elsewhere = t
            .snapshot(
                NOW,
                OFF,
                |_| Place {
                    host: HostKind::Vscode,
                    takes_keys: false,
                    accepts_replies: true,
                    host_name: None,
                },
                |_| None,
            )
            .0;
        assert!(elsewhere[0].permission.is_none());
        assert_eq!(
            elsewhere[0].waiting_for.as_deref(),
            Some(ANSWER_IN_TERMINAL)
        );
        // Answered in Grok's own terminal: the tool runs, and the wait ends.
        feed(&mut t, &grok("PostToolUse", "post_tool_use", shell()), OFF);
        assert!(snap(&mut t, NOW, false).0[0].permission.is_none());
        // A cut command can't be allowed from the island (#224).
        let cut = grok_ask(
            "Bash",
            shell()["toolInput"].as_object().unwrap(),
            None,
            true,
            NOW,
        );
        assert!(!cut.content.unwrap().complete);
        // A question waits in Grok.
        feed(
            &mut t,
            &grok(
                "PreToolUse",
                "pre_tool_use",
                json!({ "toolName": "ask_user_question" }),
            ),
            OFF,
        );
        let listed = snap(&mut t, NOW, false).0;
        assert_eq!(listed[0].phase, SessionPhase::WaitingInput);
        assert_eq!(listed[0].waiting_for.as_deref(), Some(ANSWER_IN_TERMINAL));
    }

    #[test]
    fn grok_hooks_are_a_file_of_the_islands_own() {
        let exe = Path::new("C:/Openbar/Openbar.exe");
        let file: Value = serde_json::from_slice(&installer::grok_file(exe, "app")).unwrap();
        let hooks = file["hooks"].as_object().unwrap();
        assert_eq!(hooks.len(), installer::GROK_EVENTS.len());
        let pre = &hooks["PreToolUse"][0];
        assert!(pre.get("matcher").is_none());
        assert_eq!(pre["hooks"][0]["timeout"], installer::EVENT_TIMEOUT);
        let command = pre["hooks"][0]["command"].as_str().unwrap();
        assert!(
            command.starts_with("C:/Openbar/Openbar.exe --hook --agent grok --event PreToolUse")
        );
        assert!(
            installer::grok_hooks(Path::new("/h"), "app").ends_with("hooks/openbar-app.json")
                || std::env::var_os("GROK_HOME").is_some()
        );
        assert_eq!(
            installer::file_in(Provider::Grok, Path::new("/g"), "app"),
            Some(Path::new("/g").join("hooks").join("openbar-app.json"))
        );
        assert!(installer::FOLDERS_BY_HAND.contains(&Provider::Grok));
        assert_eq!(command_name(Provider::Grok), Some("grok"));
        assert_eq!(start_arguments(Provider::Grok, "-go"), vec!["--", "-go"]);
        assert!(
            Provider::Grok.answers_permissions()
                && Provider::HOOK_AGENTS.last() == Some(&Provider::Grok)
        );
    }

    #[test]
    fn grok_runs_claude_codes_hooks_with_its_own_input() {
        let grok_input = serde_json::to_vec(&json!({ "hookEventName": "user_prompt_submit",
            "sessionId": "s", "workspaceRoot": "C:/w", "prompt": "hi" }))
        .unwrap();
        let claude = serde_json::to_vec(&json!({ "hook_event_name": "UserPromptSubmit",
            "session_id": "s", "transcript_path": "C:/t.jsonl" }))
        .unwrap();
        assert!(is_grok_payload(&grok_input) && !is_grok_payload(&claude));
        assert_eq!(
            grok_model_in(br#"{"current_model_id":"grok-code-fast","session_summary":"hi"}"#)
                .as_deref(),
            Some("grok-code-fast")
        );
        assert_eq!(grok_model_in(br#"{"current_model_id":" "}"#), None);
        let active = br#"[{"session_id":"a","pid":11,"cwd":"C:/w"},{"session_id":"b","pid":22,"cwd":"C:/w"}]"#;
        assert_eq!(grok_session_pid_in(active, "b"), Some(22));
        assert_eq!(grok_session_pid_in(active, "c"), None);
        assert_eq!(grok_session_pid_in(b"{}", "a"), None);
    }
}
