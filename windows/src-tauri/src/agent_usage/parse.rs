// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What one log line says, reduced to the few facts the island keeps
//! (AgentLogParser.swift and Harness/*Reader.swift; spec 03 §2.1.1, §3.1, §4,
//! §5). Only usage counters, model names, times and folder names leave a
//! line, besides the ids of Claude's shell commands still running; prompts,
//! replies and tool output are never kept.

use super::model::{
    Billable, LimitSource, LimitWindow, Limits, Provider, Record, Time, Tokens, WindowKind,
};
use super::pricing;
use memchr::memmem;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A turn silent this long is not shown as working (NotchAgentSupport.idleTurn).
pub const IDLE_TURN: f64 = 10.0 * 60.0;
/// OpenCode starts the next step the moment a step's tools return.
pub const OPENCODE_SETTLE: f64 = 30.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// `key` identifies the response across duplicate lines and files.
    Usage {
        key: String,
        record: Record,
        billable: Billable,
    },
    /// A later chunk names the model of an already-counted request.
    UsageModel {
        key: String,
        model: String,
    },
    Limits(Limits),
    Plan(String, Time),
    TurnBegan(Time),
    TurnContext {
        model: String,
        project: String,
    },
    /// Work goes on; None when the line was not worth decoding for its time.
    TurnActive(Option<Time>),
    /// A step ended expecting the agent to go on.
    TurnSettled(Time),
    TurnEnded {
        date: Option<Time>,
        completed: bool,
        duration: Option<f64>,
    },
    Reset,
}

/// Per-file context carried from line to line (AgentLogState).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogState {
    pub session: String,
    pub project: String,
    pub model: String,
    pub turn_open: bool,
    pub saw_usage_records: bool,
    pub last_total: Option<Tokens>,
    pub fast: bool,
    pub codex_subagent: bool,
    pub codex_header_read: bool,
    pub codex_root: String,
    pub codex_totals_session: String,
    pub copilot_totals: BTreeMap<String, Tokens>,
    pub copilot_requests: BTreeMap<String, i64>,
    pub copilot_request_models: BTreeMap<String, String>,
    pub copilot_reported_requests: BTreeMap<String, i64>,
    pub copilot_turn_id: Option<String>,
    pub copilot_final_response: bool,
    pub parent_session: String,
    /// OpenCode is never archived, so its per-session machines are not either.
    #[serde(skip)]
    pub opencode_sessions: HashMap<String, OpenCodeSession>,
    pub running_commands: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenCodeSession {
    pub project: String,
    pub model: String,
    pub turn_open: bool,
    pub turn_started: Option<Time>,
    pub active_user_message: String,
    pub last_activity: Option<Time>,
    pub settled_at: Option<Time>,
    pub writing_reply: String,
    pub active_prompt_date: Option<Time>,
    pub seen_user: Vec<String>,
    pub completed_assistant: Vec<String>,
    pub completed_user: Vec<String>,
}

pub fn parse(provider: Provider, line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    match provider {
        Provider::Claude => claude(line, state, now),
        Provider::Codex => codex(line, state, now),
        Provider::Opencode => opencode(line, state, now),
        Provider::Copilot => copilot(line, state, now),
    }
}

// MARK: Shared values

fn contains(line: &[u8], needle: &str) -> bool {
    memmem::find(line, needle.as_bytes()).is_some()
}

/// The first `"type":"…"` value at or after `start`, and where it ends. The
/// search for its closing quote stops within 64 bytes, so a long line is
/// never crossed for its type.
pub fn first_type(line: &[u8], start: usize) -> Option<(&str, usize)> {
    const KEY: &[u8] = b"\"type\":\"";
    let found = start + memmem::find(line.get(start..)?, KEY)?;
    let value = found + KEY.len();
    let limit = line.len().min(value + 64);
    let quote = value + memchr::memchr(b'"', line.get(value..limit)?)?;
    Some((std::str::from_utf8(&line[value..quote]).ok()?, quote + 1))
}

fn object(line: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice(line).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}
fn non_empty(v: Option<&Value>) -> Option<&str> {
    text(v).filter(|s| !s.is_empty())
}
fn flag(v: Option<&Value>) -> bool {
    v.and_then(Value::as_bool) == Some(true)
}

/// A counter: positive, finite, and capped so a damaged line cannot overflow the sums.
pub fn int(v: Option<&Value>) -> i64 {
    match v.and_then(Value::as_f64) {
        Some(n) if n.is_finite() && n > 0.0 => n.min(1e12) as i64,
        _ => 0,
    }
}

/// Unix seconds, or milliseconds from agents that write those.
pub fn seconds(v: Option<&Value>) -> Option<Time> {
    let raw = v?.as_f64()?;
    (raw.is_finite() && raw > 0.0).then(|| {
        if raw > 100_000_000_000.0 {
            raw / 1000.0
        } else {
            raw
        }
    })
}

pub fn timestamp(v: Option<&Value>) -> Option<Time> {
    match v? {
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.timestamp_millis() as f64 / 1000.0),
        other => seconds(Some(other)),
    }
}

/// The folder an agent ran in names the project. A worktree kept inside the
/// repository belongs to it; Windows paths split on both separators.
pub fn project_name(path: &str) -> String {
    let mut path = path.replace('\\', "/");
    if let Some(at) = path.to_ascii_lowercase().find("/.claude/worktrees/") {
        path.truncate(at);
    }
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    path.rsplit('/').next().unwrap_or("").to_string()
}

fn record(
    provider: Provider,
    date: Time,
    model: &str,
    state: &LogState,
    tokens: Tokens,
    priced: (Option<f64>, f64),
) -> Record {
    Record {
        provider,
        date,
        model: model.into(),
        project: state.project.clone(),
        session: state.session.clone(),
        requests: 1,
        tokens,
        cost: priced.0,
        savings: priced.1,
        reported_cost: false,
    }
}

// MARK: Claude Code (ClaudeReader.swift)

fn claude(line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    if contains(line, "\"type\":\"assistant\"") {
        return claude_assistant(line, state, now);
    }
    if !contains(line, "\"type\":\"user\"") {
        return vec![];
    }
    let is_user = |json: &Map<String, Value>| text(json.get("type")) == Some("user");
    // A local command prints its output without asking the model; only the
    // person's own text counts, never a tool result quoting the same words.
    if (contains(line, "[Request interrupted by user") || contains(line, "<local-command-std"))
        && let Some(json) = object(line).filter(|j| is_user(j) && claude_ends_turn(j))
    {
        let _ = json;
        let open = std::mem::take(&mut state.turn_open);
        return if open {
            vec![Entry::TurnEnded {
                date: None,
                completed: false,
                duration: None,
            }]
        } else {
            vec![]
        };
    }
    // A tool that ends the turn gets no reply after its result.
    if contains(line, "\"toolEndsTurn\":true")
        && let Some(json) = object(line)
            .filter(|j| is_user(j) && flag(j.get("toolEndsTurn")) && !flag(j.get("isSidechain")))
    {
        let open = std::mem::take(&mut state.turn_open);
        let date = timestamp(json.get("timestamp")).unwrap_or(now);
        return if open {
            vec![Entry::TurnEnded {
                date: Some(date),
                completed: true,
                duration: None,
            }]
        } else {
            vec![]
        };
    }
    if state.turn_open {
        if !state.running_commands.is_empty()
            && let Some(json) = object(line)
                .filter(|j| is_user(j) && !flag(j.get("isMeta")) && !flag(j.get("isSidechain")))
        {
            let blocks = json
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_array);
            let results: Vec<&Value> = blocks
                .map(|b| {
                    b.iter()
                        .filter(|x| text(x.get("type")) == Some("tool_result"))
                        .collect()
                })
                .unwrap_or_default();
            if results.is_empty() {
                // A prompt inside an open turn leaves a killed command behind.
                state.running_commands.clear();
            } else {
                for result in results {
                    if let Some(id) = text(result.get("tool_use_id")) {
                        state.running_commands.remove(id);
                    }
                }
            }
        }
        return vec![Entry::TurnActive(None)];
    }
    if contains(line, "\"isSidechain\":true") {
        return vec![];
    }
    let Some(json) = object(line) else {
        return vec![];
    };
    if !is_user(&json)
        || flag(json.get("isMeta"))
        || flag(json.get("isSidechain"))
        || flag(json.get("isCompactSummary"))
        || claude_command(&json)
    {
        return vec![];
    }
    claude_adopt(&json, state);
    state.turn_open = true;
    state.running_commands.clear();
    vec![Entry::TurnBegan(
        timestamp(json.get("timestamp")).unwrap_or(now),
    )]
}

fn claude_assistant(line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    let Some(json) = object(line) else {
        return vec![];
    };
    let Some(message) = json
        .get("message")
        .and_then(Value::as_object)
        .filter(|_| text(json.get("type")) == Some("assistant"))
    else {
        return vec![];
    };
    claude_adopt(&json, state);
    let date = timestamp(json.get("timestamp")).unwrap_or(now);
    let mut entries = Vec::new();
    let model = text(message.get("model")).unwrap_or("").to_string();
    if let Some(usage) = message
        .get("usage")
        .filter(|_| !model.is_empty() && !model.starts_with('<'))
    {
        state.model = model.clone();
        let billable = Billable {
            tokens: Tokens {
                input: int(usage.get("input_tokens")),
                cache_write: int(usage.get("cache_creation_input_tokens")),
                cache_read: int(usage.get("cache_read_input_tokens")),
                output: int(usage.get("output_tokens")),
                reasoning: int(usage.pointer("/output_tokens_details/thinking_tokens")),
            },
            long_cache_write: int(usage.pointer("/cache_creation/ephemeral_1h_input_tokens")),
            fast: text(usage.get("speed")) == Some("fast"),
            domestic: text(usage.get("inference_geo")) == Some("us"),
            web_searches: int(usage.pointer("/server_tool_use/web_search_requests")),
            aggregate: false,
        };
        let id = text(message.get("id")).unwrap_or("");
        let request = text(json.get("requestId")).unwrap_or("");
        let key = if id.is_empty() && request.is_empty() {
            format!("claude:{}:{}", state.session, date)
        } else {
            format!("claude:{id}:{request}")
        };
        let priced = pricing::cost(&billable, &model);
        entries.push(Entry::Usage {
            key,
            record: record(
                Provider::Claude,
                date,
                &model,
                state,
                billable.tokens,
                priced,
            ),
            billable,
        });
    }
    // A subagent's own ending is not the end of the turn it serves.
    if flag(json.get("isSidechain")) {
        return entries;
    }
    match text(message.get("stop_reason")) {
        Some("end_turn" | "stop_sequence" | "max_tokens" | "refusal") => {
            // An error written in place of a reply stops the turn without finishing it.
            let failed = flag(json.get("isApiErrorMessage")) || model.starts_with('<');
            if state.turn_open {
                entries.push(Entry::TurnEnded {
                    date: Some(date),
                    completed: !failed,
                    duration: None,
                });
            }
            state.turn_open = false;
        }
        _ => {
            for block in message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if text(block.get("type")) == Some("tool_use")
                    && text(block.get("name")) == Some("Bash")
                    && let Some(id) = text(block.get("id"))
                {
                    state.running_commands.insert(id.into());
                }
            }
            entries.insert(
                0,
                if state.turn_open {
                    Entry::TurnActive(Some(date))
                } else {
                    Entry::TurnBegan(date)
                },
            );
            state.turn_open = true;
        }
    }
    entries
}

fn claude_texts(json: &Map<String, Value>) -> Vec<&str> {
    match json.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| text(b.get("type")) == Some("text"))
            .filter_map(|b| text(b.get("text")))
            .collect(),
        _ => vec![],
    }
}

/// A slash command the person typed, which is not a prompt by itself.
fn claude_command(json: &Map<String, Value>) -> bool {
    claude_texts(json)
        .first()
        .is_some_and(|t| t.trim().starts_with("<command-name>"))
}

fn claude_ends_turn(json: &Map<String, Value>) -> bool {
    claude_texts(json).iter().any(|t| {
        t.starts_with("[Request interrupted by user") || t.starts_with("<local-command-std")
    })
}

fn claude_adopt(json: &Map<String, Value>, state: &mut LogState) {
    if let Some(session) = non_empty(json.get("sessionId")) {
        state.session = session.into();
    }
    if let Some(cwd) = non_empty(json.get("cwd")) {
        state.project = project_name(cwd);
    }
}

// MARK: Codex (CodexReader.swift)

const CODEX_EVENTS: [&str; 5] = [
    "token_count",
    "task_started",
    "task_complete",
    "turn_aborted",
    "thread_settings_applied",
];

/// Input counts include what came from the cache.
pub fn codex_tokens(usage: &Value) -> Tokens {
    let input = int(usage.get("input_tokens"));
    let cached = input.min(int(usage.get("cached_input_tokens")));
    let written = (input - cached).min(int(usage.get("cache_write_input_tokens")));
    Tokens {
        input: input - cached - written,
        cache_write: written,
        cache_read: cached,
        output: int(usage.get("output_tokens")),
        reasoning: int(usage.get("reasoning_output_tokens")),
    }
}

fn fast_tier(tier: &str) -> bool {
    matches!(tier.to_lowercase().as_str(), "fast" | "priority")
}

/// Only the main allowance is kept: `limit_id` empty or "codex".
pub fn is_main_bucket(limits: &Value) -> bool {
    let id = text(limits.get("limit_id"))
        .or(text(limits.get("limitId")))
        .unwrap_or("")
        .to_lowercase();
    id.is_empty() || id == "codex"
}

pub struct WindowKeys {
    pub used: &'static str,
    pub minutes: &'static str,
    pub resets: &'static str,
    pub individual: &'static str,
    pub remaining: &'static str,
}
pub const LOG_KEYS: WindowKeys = WindowKeys {
    used: "used_percent",
    minutes: "window_minutes",
    resets: "resets_at",
    individual: "individual_limit",
    remaining: "remaining_percent",
};
pub const ACCOUNT_KEYS: WindowKeys = WindowKeys {
    used: "usedPercent",
    minutes: "windowDurationMins",
    resets: "resetsAt",
    individual: "individualLimit",
    remaining: "remainingPercent",
};

pub fn window_kind(minutes: Option<i64>) -> WindowKind {
    match minutes {
        Some(m) if m > 0 && m <= 12 * 60 => WindowKind::Session,
        Some(m) if (6 * 1440..=8 * 1440).contains(&m) => WindowKind::Weekly,
        _ => WindowKind::Other,
    }
}

/// Windows are told apart by their length, never by their slot.
pub fn codex_windows(limits: &Value, observed: Time, keys: &WindowKeys) -> Vec<LimitWindow> {
    let mut windows = Vec::new();
    for slot in ["primary", "secondary"] {
        let Some(window) = limits.get(slot).filter(|w| w.is_object()) else {
            continue;
        };
        let Some(used) = window
            .get(keys.used)
            .and_then(Value::as_f64)
            .filter(|u| u.is_finite())
        else {
            continue;
        };
        let minutes = window
            .get(keys.minutes)
            .and_then(Value::as_f64)
            .filter(|m| m.is_finite())
            .map(|m| m as i64);
        let mut resets = seconds(window.get(keys.resets));
        if resets.is_none()
            && let Some(delay) = window
                .get("resets_in_seconds")
                .and_then(Value::as_f64)
                .filter(|d| d.is_finite())
        {
            resets = Some(observed + delay.max(0.0));
        }
        let id = format!(
            "codex.{}",
            minutes
                .map(|m| m.to_string())
                .unwrap_or_else(|| slot.into())
        );
        windows.push(LimitWindow::new(
            id,
            window_kind(minutes),
            minutes,
            None,
            used,
            resets,
        ));
    }
    if windows.is_empty()
        && let Some(individual) = limits.get(keys.individual).filter(|i| i.is_object())
        && let Some(remaining) = individual
            .get(keys.remaining)
            .and_then(Value::as_f64)
            .filter(|r| r.is_finite())
    {
        windows.push(LimitWindow::new(
            "codex.individual",
            WindowKind::Other,
            None,
            None,
            100.0 - remaining,
            seconds(individual.get(keys.resets)),
        ));
    }
    windows.sort_by_key(|w| w.minutes.unwrap_or(i64::MAX));
    windows
}

fn codex(line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    let Some((kind, end)) = first_type(line, 0) else {
        return vec![];
    };
    match kind {
        "token_usage_record" | "turn_context" | "session_meta" => {}
        "event_msg" => {
            if !first_type(line, end).is_some_and(|(event, _)| CODEX_EVENTS.contains(&event)) {
                return vec![];
            }
        }
        _ => return vec![],
    }
    let Some(json) = object(line) else {
        return vec![];
    };
    let Some(payload) = json.get("payload").filter(|p| p.is_object()) else {
        return vec![];
    };
    let date = timestamp(json.get("timestamp")).unwrap_or(now);
    match text(json.get("type")) {
        Some("session_meta") => {
            // History copied from another thread repeats that thread's metadata.
            let id = text(payload.get("id")).unwrap_or("");
            if !id.is_empty() {
                state.codex_totals_session = id.into();
            }
            if state.codex_header_read {
                return vec![];
            }
            state.codex_header_read = true;
            if !id.is_empty() {
                state.session = id.into();
            }
            if let Some(cwd) = non_empty(payload.get("cwd")) {
                state.project = project_name(cwd);
            }
            let source = payload.get("source").and_then(Value::as_object);
            state.codex_subagent =
                source.is_some_and(|s| s.contains_key("subagent") || s.contains_key("internal"));
            let root = text(payload.get("session_id"))
                .or(text(payload.get("parent_thread_id")))
                .unwrap_or("");
            if state.codex_subagent && !root.is_empty() && root != state.session {
                state.codex_root = root.into();
            }
            vec![]
        }
        Some("turn_context") => {
            if let Some(model) = non_empty(payload.get("model")) {
                state.model = model.into();
            }
            if let Some(cwd) = non_empty(payload.get("cwd")) {
                state.project = project_name(cwd);
            }
            if let Some(tier) = text(payload.get("service_tier")) {
                state.fast = fast_tier(tier);
            }
            vec![]
        }
        Some("token_usage_record") => {
            let Some(usage) = payload.get("usage").filter(|u| u.is_object()) else {
                return vec![];
            };
            state.saw_usage_records = true;
            if let Some(session) = non_empty(payload.get("session_id"))
                && state.session.is_empty()
            {
                state.session = session.into();
            }
            let response = text(payload.get("response_id")).unwrap_or("");
            let key = if response.is_empty() {
                format!("codex:{}:{}", state.session, date)
            } else {
                format!("codex:{response}")
            };
            vec![codex_usage(codex_tokens(usage), key, date, state)]
        }
        Some("event_msg") => codex_event(payload, date, state),
        _ => vec![],
    }
}

fn codex_event(payload: &Value, date: Time, state: &mut LogState) -> Vec<Entry> {
    match text(payload.get("type")) {
        Some("token_count") => {
            let mut entries = Vec::new();
            if let Some(limits) = payload.get("rate_limits").filter(|l| l.is_object()) {
                if is_main_bucket(limits) {
                    let windows = codex_windows(limits, date, &LOG_KEYS);
                    if !windows.is_empty() {
                        entries.push(Entry::Limits(Limits {
                            provider: Provider::Codex,
                            windows,
                            observed_at: date,
                            source: LimitSource::SessionLog,
                        }));
                    }
                }
                if let Some(plan) = non_empty(limits.get("plan_type")) {
                    entries.push(Entry::Plan(plan.into(), date));
                }
            }
            // Running totals repeat when only the limits changed.
            if !state.saw_usage_records
                && let Some(total_usage) = payload
                    .pointer("/info/total_token_usage")
                    .filter(|u| u.is_object())
            {
                let total = codex_tokens(total_usage);
                if Some(total) != state.last_total
                    && total.total() > state.last_total.map_or(0, |t| t.total())
                {
                    let delta = match payload
                        .pointer("/info/last_token_usage")
                        .filter(|u| u.is_object())
                    {
                        Some(last) => codex_tokens(last),
                        None => total.growth(&state.last_total.unwrap_or_default()),
                    };
                    let key = format!(
                        "codex:{}:total:{}",
                        state.codex_totals_session,
                        total.total()
                    );
                    entries.push(codex_usage(delta, key, date, state));
                }
                state.last_total = Some(total);
            }
            entries
        }
        Some("thread_settings_applied") => {
            if let Some(tier) = text(payload.pointer("/thread_settings/service_tier")) {
                state.fast = fast_tier(tier);
            }
            vec![]
        }
        Some("task_started") => {
            state.turn_open = true;
            vec![Entry::TurnBegan(
                seconds(payload.get("started_at")).unwrap_or(date),
            )]
        }
        Some(kind @ ("task_complete" | "turn_aborted")) => {
            if !std::mem::take(&mut state.turn_open) {
                return vec![];
            }
            let duration = payload
                .get("duration_ms")
                .and_then(Value::as_f64)
                .map(|ms| ms / 1000.0);
            vec![Entry::TurnEnded {
                date: Some(seconds(payload.get("completed_at")).unwrap_or(date)),
                completed: kind == "task_complete",
                duration,
            }]
        }
        _ => vec![],
    }
}

fn codex_usage(tokens: Tokens, key: String, date: Time, state: &LogState) -> Entry {
    let billable = Billable {
        tokens,
        fast: state.fast,
        ..Default::default()
    };
    let priced = pricing::cost(&billable, &state.model);
    Entry::Usage {
        key,
        record: record(Provider::Codex, date, &state.model, state, tokens, priced),
        billable,
    }
}

// MARK: GitHub Copilot (CopilotReader.swift)

pub const COPILOT_EVENTS: [&str; 9] = [
    "user.message",
    "assistant.turn_start",
    "assistant.message",
    "assistant.turn_end",
    "session.start",
    "session.context_changed",
    "session.model_change",
    "session.shutdown",
    "abort",
];

fn first_path(data: &Value) -> Option<&str> {
    ["gitRoot", "cwd", "repository"]
        .iter()
        .find_map(|k| non_empty(data.get(*k)))
}

fn copilot(line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    let Some(envelope) = object(line) else {
        return vec![];
    };
    let Some(kind) = text(envelope.get("type")).filter(|t| COPILOT_EVENTS.contains(t)) else {
        return vec![];
    };
    let Some(data) = envelope.get("data").filter(|d| d.is_object()) else {
        return vec![];
    };
    let agent = non_empty(envelope.get("agentId")).map(str::to_string);
    let root = agent.is_none();
    // Subagents share the root log: activity only, never the root task.
    if !root && kind != "assistant.message" {
        return vec![];
    }
    let date = timestamp(envelope.get("timestamp")).unwrap_or(now);
    match kind {
        "user.message" | "assistant.turn_start" => {
            state.copilot_final_response = false;
            state.copilot_turn_id = if kind == "assistant.turn_start" {
                text(data.get("turnId")).map(str::to_string)
            } else {
                None
            };
            if let Some(model) = non_empty(data.get("model")) {
                state.model = model.into();
            }
            let entry = if state.turn_open {
                Entry::TurnActive(Some(date))
            } else {
                Entry::TurnBegan(date)
            };
            state.turn_open = true;
            vec![
                entry,
                Entry::TurnContext {
                    model: state.model.clone(),
                    project: state.project.clone(),
                },
            ]
        }
        "assistant.message" => {
            let model = non_empty(data.get("model"))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if root {
                        state.model.clone()
                    } else {
                        String::new()
                    }
                });
            let mut entries =
                copilot_activity(data, &envelope, agent.as_deref(), &model, date, state);
            if !root {
                return entries;
            }
            state.model = model;
            let tools = data
                .get("toolRequests")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            state.copilot_final_response =
                !tools && !matches!(text(data.get("phase")), Some("thinking" | "commentary"));
            if state.turn_open {
                entries.push(Entry::TurnContext {
                    model: state.model.clone(),
                    project: state.project.clone(),
                });
                entries.push(Entry::TurnActive(Some(date)));
            }
            entries
        }
        "assistant.turn_end" => {
            if !state.turn_open
                || state
                    .copilot_turn_id
                    .as_deref()
                    .is_some_and(|id| Some(id) != text(data.get("turnId")))
            {
                return vec![];
            }
            if !state.copilot_final_response {
                return vec![Entry::TurnActive(Some(date))];
            }
            state.turn_open = false;
            state.copilot_final_response = false;
            vec![Entry::TurnEnded {
                date: Some(date),
                completed: true,
                duration: None,
            }]
        }
        "session.start" => {
            if let Some(session) = non_empty(data.get("sessionId")) {
                state.session = session.into();
            }
            if let Some(model) = non_empty(data.get("selectedModel")) {
                state.model = model.into();
            }
            if let Some(path) = data.get("context").and_then(first_path) {
                state.project = project_name(path);
            }
            vec![]
        }
        "session.context_changed" | "session.model_change" => {
            if kind == "session.context_changed" {
                if let Some(path) = first_path(data) {
                    state.project = project_name(path);
                }
            } else if let Some(model) = non_empty(data.get("newModel")) {
                state.model = model.into();
            }
            if state.turn_open {
                vec![Entry::TurnContext {
                    model: state.model.clone(),
                    project: state.project.clone(),
                }]
            } else {
                vec![]
            }
        }
        "abort" => {
            if !state.turn_open {
                return vec![];
            }
            state.turn_open = false;
            state.copilot_final_response = false;
            vec![Entry::TurnEnded {
                date: Some(date),
                completed: false,
                duration: None,
            }]
        }
        "session.shutdown" => {
            let mut entries = copilot_usage(
                data.get("modelMetrics"),
                text(envelope.get("id")),
                date,
                state,
            );
            if state.turn_open {
                state.turn_open = false;
                entries.push(Entry::TurnEnded {
                    date: Some(date),
                    completed: false,
                    duration: None,
                });
            }
            entries
        }
        _ => vec![],
    }
}

fn copilot_activity(
    data: &Value,
    envelope: &Map<String, Value>,
    agent: Option<&str>,
    model: &str,
    date: Time,
    state: &mut LogState,
) -> Vec<Entry> {
    let call = non_empty(data.get("apiCallId"));
    let message = non_empty(data.get("messageId"))
        .or(text(envelope.get("id")))
        .map(str::to_string)
        .unwrap_or_else(|| date.to_string());
    let request = format!(
        "{}:{}",
        agent
            .map(|a| format!("agent:{a}"))
            .unwrap_or_else(|| "root".into()),
        call.map(|c| format!("api:{c}"))
            .unwrap_or_else(|| format!("message:{message}"))
    );
    let key = format!("copilot:{}:{request}:activity", state.session);
    if let Some(previous) = state.copilot_request_models.get(&request).cloned() {
        if !previous.is_empty() || model.is_empty() {
            return vec![];
        }
        state.copilot_request_models.insert(request, model.into());
        *state.copilot_requests.entry(previous).or_default() -= 1;
        *state.copilot_requests.entry(model.into()).or_default() += 1;
        return vec![Entry::UsageModel {
            key,
            model: model.into(),
        }];
    }
    state.copilot_request_models.insert(request, model.into());
    *state.copilot_requests.entry(model.into()).or_default() += 1;
    let mut r = record(
        Provider::Copilot,
        date,
        model,
        state,
        Tokens::default(),
        (Some(0.0), 0.0),
    );
    r.requests = 1;
    vec![Entry::Usage {
        key,
        record: r,
        billable: Billable {
            aggregate: true,
            ..Default::default()
        },
    }]
}

fn copilot_tokens(metric: &Value) -> Tokens {
    let count = |detail: &str, fallback: &str| match metric
        .pointer(&format!("/tokenDetails/{detail}/tokenCount"))
    {
        Some(v) => int(Some(v)),
        None => int(metric.pointer(&format!("/usage/{fallback}"))),
    };
    Tokens {
        input: count("input", "inputTokens"),
        cache_write: count("cache_write", "cacheWriteTokens"),
        cache_read: count("cache_read", "cacheReadTokens"),
        output: count("output", "outputTokens"),
        reasoning: int(metric.pointer("/usage/reasoningTokens")),
    }
}

/// Shutdown counters are cumulative for the session: what grew since the
/// last one is stored, with request counts reconciled against activity.
fn copilot_usage(
    metrics: Option<&Value>,
    event: Option<&str>,
    date: Time,
    state: &mut LogState,
) -> Vec<Entry> {
    let Some(metrics) = metrics.and_then(Value::as_object) else {
        return vec![];
    };
    let mut growth: BTreeMap<String, Tokens> = BTreeMap::new();
    for (model, metric) in metrics {
        if !metric.is_object() {
            continue;
        }
        let total = copilot_tokens(metric);
        let grew = match state.copilot_totals.get(model) {
            Some(previous) if total.total() >= previous.total() => total.growth(previous),
            _ => total,
        };
        growth.insert(model.clone(), grew);
        state.copilot_totals.insert(model.clone(), total);
        let reported = int(metric.pointer("/requests/count"));
        let at_least = reported.max(if total.total() > 0 { 1 } else { 0 });
        let entry = state
            .copilot_reported_requests
            .entry(model.clone())
            .or_default();
        *entry = (*entry).max(at_least);
    }
    let deficits: Vec<(String, i64)> = state
        .copilot_reported_requests
        .iter()
        .map(|(m, n)| {
            (
                m.clone(),
                (n - state.copilot_requests.get(m).copied().unwrap_or(0)).max(0),
            )
        })
        .collect();
    let unresolved = state.copilot_requests.get("").copied().unwrap_or(0);
    let mut missing = (deficits.iter().map(|d| d.1).sum::<i64>() - unresolved).max(0);
    let mut requests: BTreeMap<String, i64> = BTreeMap::new();
    for (model, count) in &deficits {
        let certain = (count - unresolved).max(0);
        requests.insert(model.clone(), certain);
        missing -= certain;
    }
    if missing > 0 {
        *requests.entry(String::new()).or_default() += missing;
    }
    let models: BTreeSet<String> = growth.keys().chain(requests.keys()).cloned().collect();
    let mut entries = Vec::new();
    for model in models {
        let tokens = growth.get(&model).copied().unwrap_or_default();
        let count = requests.get(&model).copied().unwrap_or(0);
        *state.copilot_requests.entry(model.clone()).or_default() += count;
        if tokens.total() <= 0 && count <= 0 {
            continue;
        }
        let billable = Billable {
            tokens,
            aggregate: true,
            ..Default::default()
        };
        let priced = pricing::cost(&billable, &model);
        let checkpoint = event
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| date.to_string());
        let mut r = record(Provider::Copilot, date, &model, state, tokens, priced);
        r.requests = count;
        entries.push(Entry::Usage {
            key: format!("copilot:{}:{checkpoint}:{model}", state.session),
            record: r,
            billable,
        });
    }
    entries
}

// MARK: OpenCode (OpenCodeReader.swift): rows the database reader hands over as JSON

fn remember(id: &str, ids: &mut Vec<String>) {
    if ids.len() > 100 {
        ids.remove(0);
    }
    if !ids.iter().any(|i| i == id) {
        ids.push(id.into());
    }
}

fn opencode_model(json: &Map<String, Value>) -> Option<&str> {
    non_empty(json.get("model_id"))
        .or(non_empty(json.get("modelID")))
        .or(json
            .get("model")
            .and_then(|m| non_empty(m.get("modelID")).or(non_empty(m.get("id")))))
        .or(non_empty(json.get("model")))
}

fn opencode(line: &[u8], state: &mut LogState, now: Time) -> Vec<Entry> {
    if !contains(line, "\"role\":\"") && !contains(line, "\"type\":\"reset\"") {
        return vec![];
    }
    let Some(json) = object(line) else {
        return vec![];
    };
    if text(json.get("type")) == Some("reset") {
        state.opencode_sessions.clear();
        state.turn_open = false;
        return vec![Entry::Reset];
    }
    let Some(role) = text(json.get("role")) else {
        return vec![];
    };
    let id = text(json.get("id")).unwrap_or("").to_string();
    let session_id = non_empty(json.get("session_id"))
        .map(str::to_string)
        .unwrap_or_else(|| state.session.clone());
    state.session = session_id.clone();
    state.parent_session = text(json.get("parent_session_id")).unwrap_or("").into();
    let mut s = state
        .opencode_sessions
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let cwd = json
        .get("path")
        .and_then(|p| text(p.get("cwd")))
        .or(text(json.get("directory")))
        .unwrap_or("");
    if !cwd.is_empty() {
        s.project = project_name(cwd);
    }
    let date = seconds(
        json.get("time")
            .and_then(|t| t.get("created"))
            .or(json.get("time_created")),
    )
    .unwrap_or(now);
    if !state.opencode_sessions.contains_key(&session_id) {
        state.opencode_sessions.retain(|_, kept| {
            !kept.writing_reply.is_empty() || date - kept.last_activity.unwrap_or(date) < IDLE_TURN
        });
    }
    let last_activity = s.last_activity;
    s.last_activity = Some(last_activity.unwrap_or(date).max(date));
    let mut entries = Vec::new();
    match role {
        "user" => {
            if !id.is_empty() {
                if s.seen_user.contains(&id) {
                    return vec![];
                }
                remember(&id, &mut s.seen_user);
                s.active_user_message = id.clone();
            }
            let quiet = match (s.settled_at, last_activity) {
                (Some(settled), _) => date - settled >= OPENCODE_SETTLE,
                (None, Some(last)) if s.writing_reply.is_empty() => date - last >= IDLE_TURN,
                _ => false,
            };
            s.settled_at = None;
            s.active_prompt_date = Some(date);
            if s.turn_open && !quiet {
                entries.push(Entry::TurnActive(Some(date)));
            } else {
                if s.turn_open {
                    entries.push(Entry::TurnEnded {
                        date: last_activity,
                        completed: false,
                        duration: None,
                    });
                }
                s.turn_open = true;
                s.turn_started = Some(date);
                entries.push(Entry::TurnBegan(date));
            }
            if let Some(model) = opencode_model(&json) {
                s.model = model.into();
            }
            state.turn_open = true;
        }
        "assistant" => {
            if let Some(model) = opencode_model(&json) {
                s.model = model.into();
            }
            let parent = text(json.get("parentID")).unwrap_or("").to_string();
            let already = (!id.is_empty() && s.completed_assistant.contains(&id))
                || (!parent.is_empty() && s.completed_user.contains(&parent));
            let active = parent.is_empty() || parent == s.active_user_message;
            let completed = seconds(json.get("time").and_then(|t| t.get("completed")));
            let error = json
                .get("error")
                .is_some_and(|e| !e.is_null() && e != &Value::Bool(false));
            let finish = text(json.get("finish"));
            if let Some(c) = completed {
                s.last_activity = Some(s.last_activity.unwrap_or(c).max(c));
            }
            if s.settled_at.is_some_and(|settled| date >= settled) {
                s.settled_at = None;
            }
            if !id.is_empty()
                && !s.writing_reply.is_empty()
                && s.writing_reply != id
                && s.turn_open
                && !parent.is_empty()
                && parent == s.active_user_message
                && let (Some(prompt), Some(started)) = (s.active_prompt_date, s.turn_started)
                && prompt > started
            {
                entries.push(Entry::TurnEnded {
                    date: None,
                    completed: false,
                    duration: None,
                });
                entries.push(Entry::TurnBegan(prompt));
                s.turn_started = Some(prompt);
            }
            if completed.is_none() && finish.is_none() && !error && !id.is_empty() {
                s.writing_reply = id.clone();
            } else if s.writing_reply == id {
                s.writing_reply.clear();
            }
            if !already && active && !s.turn_open {
                s.turn_open = true;
                s.turn_started = Some(date);
                entries.push(Entry::TurnBegan(date));
            }
            if let Some(usage) = opencode_usage(&json, &id, &session_id, date, &s.model, &s.project)
            {
                entries.push(usage);
            }
            let ends = if error {
                true
            } else if let Some(finish) = finish {
                !matches!(finish, "tool-calls" | "unknown")
                    && !flag(json.get("tool_calls"))
                    && !flag(json.get("auto_compaction"))
            } else {
                completed.is_some()
            };
            if ends {
                if !already && active && s.turn_open {
                    let end = completed
                        .or(seconds(json.get("time_updated")))
                        .unwrap_or(date);
                    let duration = match s.turn_started {
                        Some(started) => (end >= started).then_some(end - started),
                        None => (end >= date).then_some(end - date),
                    };
                    s.turn_open = false;
                    s.turn_started = None;
                    s.active_user_message.clear();
                    s.settled_at = None;
                    entries.push(Entry::TurnEnded {
                        date: Some(end),
                        completed: !error,
                        duration,
                    });
                }
                if !id.is_empty() {
                    remember(&id, &mut s.completed_assistant);
                }
                if !parent.is_empty() {
                    remember(&parent, &mut s.completed_user);
                }
            } else if !already && active {
                match completed {
                    Some(c) if s.turn_open => {
                        if json.get("version").and_then(Value::as_i64) == Some(2) {
                            entries.push(Entry::TurnActive(Some(c)));
                        } else {
                            s.settled_at = Some(c);
                            entries.push(Entry::TurnSettled(c));
                        }
                    }
                    _ => entries.push(Entry::TurnActive(Some(date))),
                }
            }
            state.turn_open = s.turn_open;
        }
        "compaction" => {
            let own = json
                .get("model")
                .and_then(|m| non_empty(m.get("id")))
                .map(str::to_string);
            if let Some(usage) = opencode_usage(
                &json,
                &id,
                &session_id,
                date,
                own.as_deref().unwrap_or(&s.model),
                &s.project,
            ) {
                entries.push(usage);
            }
            if s.turn_open {
                let moment = date.max(seconds(json.get("time_updated")).unwrap_or(date));
                s.last_activity = Some(s.last_activity.unwrap_or(moment).max(moment));
                s.settled_at = None;
                entries.push(Entry::TurnActive(Some(moment)));
            }
            state.turn_open = s.turn_open;
        }
        "idle" => {
            s.writing_reply.clear();
            s.settled_at = None;
            if s.turn_open {
                let duration = s
                    .turn_started
                    .filter(|started| date >= *started)
                    .map(|started| date - started);
                s.turn_open = false;
                s.turn_started = None;
                s.active_user_message.clear();
                entries.push(Entry::TurnEnded {
                    date: Some(date),
                    completed: text(json.get("outcome")) == Some("succeeded"),
                    duration,
                });
            }
            state.turn_open = false;
        }
        _ => return vec![],
    }
    state.project = s.project.clone();
    state.model = s.model.clone();
    state.opencode_sessions.insert(session_id, s);
    entries
}

/// A reply's or compaction's usage, priced from the list when it knows the
/// model, else what OpenCode recorded (a recorded zero stays an estimate).
fn opencode_usage(
    json: &Map<String, Value>,
    id: &str,
    session: &str,
    date: Time,
    model: &str,
    project: &str,
) -> Option<Entry> {
    let t = json.get("tokens");
    let reasoning = int(t.and_then(|t| t.get("reasoning")));
    let tokens = Tokens {
        input: int(t.and_then(|t| t.get("input"))),
        cache_write: int(t.and_then(|t| t.pointer("/cache/write"))),
        cache_read: int(t.and_then(|t| t.pointer("/cache/read"))),
        output: int(t.and_then(|t| t.get("output"))) + reasoning,
        reasoning,
    };
    let recorded = json
        .get("cost")
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite());
    if tokens.total() <= 0 && recorded.unwrap_or(0.0) <= 0.0 {
        return None;
    }
    let billable = Billable {
        tokens,
        ..Default::default()
    };
    let priced = pricing::cost(&billable, model);
    let (cost, reported) = match (priced.0, recorded) {
        (Some(c), _) => (Some(c), false),
        (None, Some(r)) => (Some(r), r > 0.0),
        (None, None) => (None, false),
    };
    let key = format!(
        "opencode:{session}:{}",
        if id.is_empty() {
            date.to_string()
        } else {
            id.to_string()
        }
    );
    let record = Record {
        provider: Provider::Opencode,
        date,
        model: model.into(),
        project: project.into(),
        session: session.into(),
        requests: 1,
        tokens,
        cost,
        savings: priced.1,
        reported_cost: reported,
    };
    Some(Entry::Usage {
        key,
        record,
        billable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(provider: Provider, lines: &[&str]) -> (Vec<Entry>, LogState) {
        let mut state = LogState::default();
        let entries = lines
            .iter()
            .flat_map(|l| parse(provider, l.as_bytes(), &mut state, 1_000.0))
            .collect();
        (entries, state)
    }

    #[test]
    fn timestamps_and_counters_read_safely() {
        assert_eq!(
            timestamp(Some(&Value::from("2026-09-21T23:42:45.078Z"))),
            Some(1_790_034_165.078)
        );
        assert_eq!(
            timestamp(Some(&Value::from("2026-09-21T23:42:45+02:00"))),
            Some(1_790_026_965.0)
        );
        assert_eq!(
            seconds(Some(&Value::from(1_789_778_369_000u64))),
            Some(1_789_778_369.0)
        );
        assert_eq!(int(Some(&Value::from(-4))), 0);
        assert_eq!(int(Some(&Value::from(1e20))), 1_000_000_000_000);
    }

    #[test]
    fn project_names_come_from_windows_and_posix_paths() {
        assert_eq!(project_name(r"C:\Users\me\repo"), "repo");
        assert_eq!(project_name(r"C:\Users\me\repo\"), "repo");
        assert_eq!(
            project_name(r"C:\Users\me\repo\.claude\worktrees\fix-1"),
            "repo"
        );
        assert_eq!(project_name(r"D:\Work\App\.Claude\Worktrees\x\sub"), "App");
        assert_eq!(project_name("/Users/me/site/.claude/worktrees/a"), "site");
        assert_eq!(project_name("/Users/me/site"), "site");
    }

    #[test]
    fn a_claude_reply_records_usage_and_ends_its_turn() {
        let user = r#"{"type":"user","sessionId":"s1","cwd":"C:\\Users\\me\\vors","timestamp":"2026-09-21T23:40:00.000Z","message":{"role":"user","content":"fix it"}}"#;
        let tool = r#"{"type":"assistant","sessionId":"s1","cwd":"C:\\Users\\me\\vors","timestamp":"2026-09-21T23:41:00.000Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-opus-5-5","stop_reason":"tool_use","content":[{"type":"tool_use","name":"Bash","id":"toolu_1"}],"usage":{"input_tokens":10,"cache_creation_input_tokens":200,"cache_read_input_tokens":3000,"output_tokens":40,"output_tokens_details":{"thinking_tokens":5},"cache_creation":{"ephemeral_1h_input_tokens":100},"speed":"fast","inference_geo":"us","server_tool_use":{"web_search_requests":1}}}}"#;
        let result = r#"{"type":"user","sessionId":"s1","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}}"#;
        let end = r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-09-21T23:42:45.078Z","requestId":"req_2","message":{"id":"msg_2","model":"claude-opus-5-5","stop_reason":"end_turn","content":[],"usage":{"input_tokens":1,"output_tokens":2}}}"#;
        let mut state = LogState::default();
        let began = parse(Provider::Claude, user.as_bytes(), &mut state, 0.0);
        assert!(matches!(began[..], [Entry::TurnBegan(_)]));
        assert_eq!(state.project, "vors");
        let reply = parse(Provider::Claude, tool.as_bytes(), &mut state, 0.0);
        assert!(matches!(reply[0], Entry::TurnActive(Some(_))));
        let Entry::Usage {
            key,
            record,
            billable,
        } = &reply[1]
        else {
            panic!("usage")
        };
        assert_eq!(key, "claude:msg_1:req_1");
        assert_eq!(
            record.tokens,
            Tokens {
                input: 10,
                cache_write: 200,
                cache_read: 3000,
                output: 40,
                reasoning: 5
            }
        );
        assert_eq!(
            (
                billable.long_cache_write,
                billable.fast,
                billable.domestic,
                billable.web_searches
            ),
            (100, true, true, 1)
        );
        assert!(record.cost.unwrap() > 0.0);
        assert!(state.running_commands.contains("toolu_1"));
        assert!(matches!(
            parse(Provider::Claude, result.as_bytes(), &mut state, 0.0)[..],
            [Entry::TurnActive(None)]
        ));
        assert!(state.running_commands.is_empty());
        let ended = parse(Provider::Claude, end.as_bytes(), &mut state, 0.0);
        assert!(matches!(
            ended.last(),
            Some(Entry::TurnEnded {
                completed: true,
                ..
            })
        ));
        assert!(!state.turn_open);
    }

    #[test]
    fn claude_interruptions_errors_and_commands() {
        let prompt = r#"{"type":"user","message":{"content":"go"}}"#;
        let stop = r#"{"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#;
        let (entries, _) = run(Provider::Claude, &[prompt, stop]);
        assert!(matches!(
            entries.last(),
            Some(Entry::TurnEnded {
                completed: false,
                ..
            })
        ));
        let error = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"model":"<synthetic>","stop_reason":"end_turn","usage":{"input_tokens":1}}}"#;
        let (entries, _) = run(Provider::Claude, &[prompt, error]);
        assert!(
            matches!(
                entries[..],
                [
                    Entry::TurnBegan(_),
                    Entry::TurnEnded {
                        completed: false,
                        ..
                    }
                ]
            ),
            "synthetic replies record nothing"
        );
        let command =
            r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#;
        let meta = r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#;
        assert!(run(Provider::Claude, &[command, meta]).0.is_empty());
        let side = r#"{"type":"assistant","isSidechain":true,"message":{"model":"claude-opus-5-5","stop_reason":"end_turn","usage":{"output_tokens":3}}}"#;
        let (entries, state) = run(Provider::Claude, &[prompt, side]);
        assert!(state.turn_open, "a subagent ending is not the turn's end");
        assert!(matches!(entries.last(), Some(Entry::Usage { .. })));
    }

    #[test]
    fn codex_token_counts_limits_and_plan() {
        let meta = r#"{"timestamp":"2026-09-21T10:00:00.000Z","type":"session_meta","payload":{"id":"t1","cwd":"C:\\code\\api","source":"cli"}}"#;
        let ctx = r#"{"timestamp":"2026-09-21T10:00:01.000Z","type":"turn_context","payload":{"model":"gpt-5.6","cwd":"C:\\code\\api","service_tier":"priority"}}"#;
        let started = r#"{"timestamp":"2026-09-21T10:00:02.000Z","type":"event_msg","payload":{"type":"task_started","started_at":1790157602}}"#;
        let count = r#"{"timestamp":"2026-09-21T10:00:03.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":23507,"cached_input_tokens":21888,"cache_write_input_tokens":0,"output_tokens":36,"reasoning_output_tokens":0,"total_tokens":23543},"last_token_usage":{"input_tokens":23507,"cached_input_tokens":21888,"output_tokens":36}},"rate_limits":{"limit_id":"codex","primary":{"used_percent":9.0,"window_minutes":300,"resets_at":1789778369},"secondary":{"used_percent":6.0,"window_minutes":10080,"resets_at":1789845426},"individual_limit":null,"plan_type":"plus"}}}"#;
        let model_bucket = r#"{"timestamp":"2026-09-21T10:00:03.500Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"gpt-5.6-mini","primary":{"used_percent":50.0,"window_minutes":300}}}}"#;
        let done = r#"{"timestamp":"2026-09-21T10:00:09.000Z","type":"event_msg","payload":{"type":"task_complete","completed_at":1790157609,"duration_ms":7000}}"#;
        let (entries, state) = run(
            Provider::Codex,
            &[meta, ctx, started, count, model_bucket, done],
        );
        assert_eq!(state.project, "api");
        assert!(state.fast);
        let usage = entries
            .iter()
            .find_map(|e| match e {
                Entry::Usage {
                    key,
                    record,
                    billable,
                } => Some((key, record, billable)),
                _ => None,
            })
            .unwrap();
        assert_eq!(usage.0, "codex:t1:total:23543");
        assert_eq!(
            usage.1.tokens,
            Tokens {
                input: 1619,
                cache_write: 0,
                cache_read: 21888,
                output: 36,
                reasoning: 0
            }
        );
        assert!(usage.2.fast);
        let limits: Vec<&Limits> = entries
            .iter()
            .filter_map(|e| {
                if let Entry::Limits(l) = e {
                    Some(l)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(limits.len(), 1, "a model's own bucket is ignored");
        assert_eq!(
            limits[0]
                .windows
                .iter()
                .map(|w| (w.id.as_str(), w.kind))
                .collect::<Vec<_>>(),
            vec![
                ("codex.300", WindowKind::Session),
                ("codex.10080", WindowKind::Weekly)
            ]
        );
        assert_eq!(limits[0].windows[0].resets_at, Some(1_789_778_369.0));
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, Entry::Plan(p, _) if p == "plus"))
        );
        assert!(
            matches!(entries.last(), Some(Entry::TurnEnded { completed: true, duration: Some(d), .. }) if *d == 7.0)
        );
    }

    #[test]
    fn codex_records_win_over_totals_and_subagents_find_their_root() {
        let meta = r#"{"type":"session_meta","payload":{"id":"sub","session_id":"root","source":{"subagent":{"thread_spawn":{}}}}}"#;
        let record = r#"{"timestamp":"2026-09-21T10:00:03.000Z","type":"token_usage_record","payload":{"session_id":"sub","response_id":"resp_1","usage":{"input_tokens":100,"cached_input_tokens":40,"cache_write_input_tokens":10,"output_tokens":5}}}"#;
        let count = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":999,"output_tokens":9}}}}"#;
        let (entries, state) = run(Provider::Codex, &[meta, record, count]);
        assert!(state.codex_subagent);
        assert_eq!(state.codex_root, "root");
        assert_eq!(entries.len(), 1);
        let Entry::Usage { key, record, .. } = &entries[0] else {
            panic!()
        };
        assert_eq!(key, "codex:resp_1");
        assert_eq!(
            record.tokens,
            Tokens {
                input: 50,
                cache_write: 10,
                cache_read: 40,
                output: 5,
                reasoning: 0
            }
        );
        let individual = serde_json::json!({"individual_limit": {"remaining_percent": 70, "resets_at": 1790000000}});
        let windows = codex_windows(&individual, 0.0, &LOG_KEYS);
        assert_eq!(
            (windows[0].id.as_str(), windows[0].used_percent),
            ("codex.individual", 30.0)
        );
    }

    #[test]
    fn copilot_turns_and_shutdown_counters() {
        let start = r#"{"type":"session.start","id":"e0","timestamp":"2026-09-21T10:00:00Z","data":{"sessionId":"cs","selectedModel":"claude-sonnet-4.5","context":{"cwd":"C:\\src\\web"}}}"#;
        let user = r#"{"type":"user.message","id":"e1","timestamp":"2026-09-21T10:00:01Z","data":{"content":"secret"}}"#;
        let tool = r#"{"type":"assistant.message","id":"e2","timestamp":"2026-09-21T10:00:02Z","data":{"messageId":"m1","apiCallId":"a1","toolRequests":[{"name":"x"}]}}"#;
        let end1 = r#"{"type":"assistant.turn_end","id":"e3","timestamp":"2026-09-21T10:00:03Z","data":{}}"#;
        let final_msg = r#"{"type":"assistant.message","id":"e4","timestamp":"2026-09-21T10:00:04Z","data":{"messageId":"m2","apiCallId":"a2"}}"#;
        let end2 = r#"{"type":"assistant.turn_end","id":"e5","timestamp":"2026-09-21T10:00:05Z","data":{}}"#;
        let shutdown = r#"{"type":"session.shutdown","id":"e6","timestamp":"2026-09-21T10:01:00Z","data":{"modelMetrics":{"claude-sonnet-4.5":{"requests":{"count":3},"usage":{"inputTokens":1000,"cacheReadTokens":500,"outputTokens":100}}}}}"#;
        let (entries, state) = run(
            Provider::Copilot,
            &[start, user, tool, end1, final_msg, end2, shutdown],
        );
        assert_eq!(state.project, "web");
        let ended: Vec<_> = entries
            .iter()
            .filter(|e| matches!(e, Entry::TurnEnded { .. }))
            .collect();
        assert_eq!(
            ended.len(),
            1,
            "a tool iteration's turn_end does not finish the task"
        );
        let usage: Vec<&Record> = entries
            .iter()
            .filter_map(|e| {
                if let Entry::Usage { record, .. } = e {
                    Some(record)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(usage.len(), 3);
        assert_eq!(usage[2].tokens.input, 1000);
        assert_eq!(usage[2].requests, 1, "three reported, two seen as activity");
    }

    #[test]
    fn opencode_rows_open_and_close_turns_and_keep_reported_costs() {
        let user = r#"{"role":"user","id":"u1","session_id":"ses","directory":"C:\\p\\tool","time":{"created":1790000000000}}"#;
        let reply = r#"{"role":"assistant","id":"a1","session_id":"ses","parentID":"u1","modelID":"some-local-model","tokens":{"input":100,"output":10,"reasoning":5,"cache":{"read":0,"write":0}},"cost":0.5,"finish":"stop","time":{"created":1790000001000,"completed":1790000031000}}"#;
        let (entries, state) = run(Provider::Opencode, &[user, reply]);
        assert!(matches!(entries[0], Entry::TurnBegan(_)));
        let Entry::Usage { record, .. } = &entries[1] else {
            panic!()
        };
        assert_eq!(
            (record.cost, record.reported_cost, record.tokens.output),
            (Some(0.5), true, 15)
        );
        assert!(
            matches!(entries[2], Entry::TurnEnded { completed: true, duration: Some(d), .. } if d == 31.0)
        );
        assert_eq!(state.project, "tool");
        let (entries, _) = run(Provider::Opencode, &[r#"{"type":"reset"}"#]);
        assert_eq!(entries, vec![Entry::Reset]);
    }
}
