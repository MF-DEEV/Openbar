// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Turns the lines of a Claude Code transcript into the facts the Sessions
//! section needs, ported from `ClaudeTranscriptReader.swift`, with the usage
//! and timestamp readers it borrows from `Harness/ClaudeReader.swift` and
//! `AgentLogParser.swift`. Prompts, replies and tool arguments are decoded
//! only when the content policy allows previews; otherwise a line yields its
//! times, tool names and identifiers, branch, model and usage.

use serde_json::{Map, Value};

use super::models::{
    ContentPolicy, TaskProgress, Time, Tokens, ToolUse, TranscriptFact, background_kind,
    char_count, days_from_civil, dicts, is_true, last_path_component, preview, str_of,
    tool_summary, trim,
};

pub const PREVIEW_LIMIT: usize = 280;
/// A reply is kept whole enough to read in the session's window.
pub const REPLY_LIMIT: usize = 6000;

/// Everything a reply is billed by, as the AI section's pricing reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Billable {
    pub tokens: Tokens,
    /// The part of `tokens.cache_write` kept for an hour.
    pub long_cache_write: i64,
    pub fast: bool,
    /// Inference pinned to the United States.
    pub domestic: bool,
    pub web_searches: i64,
}

/// What a reply cost, so a session's cost matches the AI section's. The
/// pricing table is ported separately; `NoPricing` stands in until then.
pub trait Pricing {
    /// Zero for a model the table does not know.
    fn cost(&self, model: &str, billable: &Billable) -> f64;
}

pub struct NoPricing;

impl Pricing for NoPricing {
    fn cost(&self, _model: &str, _billable: &Billable) -> f64 {
        0.0
    }
}

/// Stop reasons that end a reply, and with it, for the main agent, a turn.
const TURN_ENDS: [&str; 4] = ["end_turn", "stop_sequence", "max_tokens", "refusal"];

/// Statuses a task notification reports a finished task with.
pub const FINISHED_STATUSES: [&str; 6] = [
    "completed",
    "failed",
    "killed",
    "stopped",
    "cancelled",
    "error",
];

/// Reads one transcript, line by line; one per file, since a subagent's
/// file is read differently and usage keys fall back to the session.
#[derive(Clone, Debug)]
pub struct TranscriptReader {
    pub policy: ContentPolicy,
    /// The subagent a `subagents/agent-<id>.jsonl` file belongs to.
    subagent: Option<String>,
    /// The session the lines name, for usage keys of replies without ids.
    session: String,
}

impl TranscriptReader {
    pub fn new(policy: ContentPolicy, path: &str) -> Self {
        Self {
            policy,
            subagent: subagent_of(path),
            session: String::new(),
        }
    }

    pub fn subagent(&self) -> Option<&str> {
        self.subagent.as_deref()
    }

    pub fn parse(&mut self, line: &[u8], now: Time, pricing: &dyn Pricing) -> Vec<TranscriptFact> {
        let mut facts = Vec::new();
        let json: Option<Map<String, Value>> =
            serde_json::from_slice::<Value>(line)
                .ok()
                .and_then(|value| match value {
                    Value::Object(map) => Some(map),
                    _ => None,
                });
        if contains(line, br#""type":"assistant""#)
            && let Some(json) = &json
            && let Some(usage) = self.usage(json, now, pricing)
        {
            facts.push(usage);
        }
        let Some(json) = json else { return facts };
        let Some(kind) = str_of(json.get("type")) else {
            return facts;
        };
        let date = timestamp(json.get("timestamp")).unwrap_or(now);
        // A subagent run in the background ends without closing its own
        // transcript; the session hears of it as a task notification. Only
        // the records that deliver one count, never a reply quoting one.
        if self.subagent.is_none()
            && contains(line, b"<task-notification>")
            && delivers_notification(&json, kind)
            && let Some((id, _)) = finished_task(line)
        {
            facts.push(TranscriptFact::SubagentEnded { id, at: date });
        }
        if let Some(branch) =
            str_of(json.get("gitBranch")).filter(|b| !b.is_empty() && *b != "HEAD")
        {
            facts.push(TranscriptFact::Branch(branch.to_owned()));
        }
        if let Some(subagent) = &self.subagent {
            facts.extend(subagent_facts(&json, kind, subagent, date));
            return facts;
        }
        let previews = self.policy.previews;
        match kind {
            "assistant" => {
                let Some(message) = json.get("message").and_then(Value::as_object) else {
                    return facts;
                };
                if is_true(json.get("isSidechain")) {
                    return facts;
                }
                let model = str_of(message.get("model")).unwrap_or("");
                if !model.is_empty() && !model.starts_with('<') {
                    facts.push(TranscriptFact::Model(model.to_owned()));
                }
                let empty = Map::new();
                for block in dicts(message.get("content")) {
                    match str_of(block.get("type")) {
                        Some("tool_use") => {
                            let (Some(id), Some(name)) =
                                (str_of(block.get("id")), str_of(block.get("name")))
                            else {
                                continue;
                            };
                            let input = block
                                .get("input")
                                .and_then(Value::as_object)
                                .unwrap_or(&empty);
                            let summary = if previews {
                                tool_summary::summary(name, input)
                            } else {
                                None
                            };
                            let mut tool = ToolUse::new(id, name, summary, date);
                            tool.input_key = Some(tool_summary::input_key(name, input));
                            facts.push(TranscriptFact::ToolStarted(tool));
                            if name == "Bash" && is_true(input.get("run_in_background")) {
                                let command = str_of(input.get("command")).unwrap_or("");
                                let description = str_of(input.get("description"));
                                let label = if previews {
                                    preview(Some(description.unwrap_or(command)), PREVIEW_LIMIT)
                                } else {
                                    None
                                };
                                facts.push(TranscriptFact::BackgroundStarted {
                                    tool_id: id.to_owned(),
                                    service: background_kind::is_service(command, description),
                                    label,
                                    at: date,
                                });
                            }
                            // Stopping a background command ends it as its notice would.
                            if matches!(name, "TaskStop" | "KillShell" | "KillBash") {
                                // The first key present decides, as Swift's `??` of optionals.
                                let named = ["task_id", "shell_id", "bash_id"]
                                    .iter()
                                    .find_map(|key| input.get(*key));
                                if let Some(task) =
                                    named.and_then(Value::as_str).filter(|t| !t.is_empty())
                                {
                                    facts.push(TranscriptFact::SubagentEnded {
                                        id: task.to_owned(),
                                        at: date,
                                    });
                                }
                            }
                        }
                        Some("text") if previews => {
                            if let Some(text) = preview(str_of(block.get("text")), REPLY_LIMIT) {
                                facts.push(TranscriptFact::Text {
                                    text,
                                    at: date,
                                    ends_turn: false,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                let stop = str_of(message.get("stop_reason"));
                if let Some(stop) = stop.filter(|s| TURN_ENDS.contains(s)) {
                    let failed = is_true(json.get("isApiErrorMessage")) || model.starts_with('<');
                    // The last text of a turn that ended as it should sums it
                    // up; a refusal or a cut-off reply stops short of that.
                    if !failed
                        && matches!(stop, "end_turn" | "stop_sequence")
                        && let Some(TranscriptFact::Text { ends_turn, .. }) = facts
                            .iter_mut()
                            .rev()
                            .find(|fact| matches!(fact, TranscriptFact::Text { .. }))
                    {
                        *ends_turn = true;
                    }
                    facts.push(TranscriptFact::TurnEnded {
                        at: date,
                        completed: !failed,
                    });
                }
            }
            "user" => {
                if is_true(json.get("isSidechain")) {
                    return facts;
                }
                let content = json.get("message").and_then(|m| m.get("content"));
                let blocks = dicts(content);
                let results: Vec<_> = blocks
                    .iter()
                    .filter(|b| str_of(b.get("type")) == Some("tool_result"))
                    .collect();
                if !results.is_empty() {
                    let outcome = json.get("toolUseResult").and_then(Value::as_object);
                    for result in results {
                        let Some(id) = str_of(result.get("tool_use_id")) else {
                            continue;
                        };
                        facts.push(TranscriptFact::ToolFinished {
                            id: id.to_owned(),
                            at: date,
                            failed: is_true(result.get("is_error")),
                        });
                        let field = |key: &str| outcome.and_then(|o| str_of(o.get(key)));
                        if let Some(task) = field("backgroundTaskId").filter(|t| !t.is_empty()) {
                            facts.push(TranscriptFact::BackgroundTask {
                                tool_id: id.to_owned(),
                                task: task.to_owned(),
                            });
                        }
                        // A subagent sent to the background answers at once with
                        // its id; Claude waits for it as for a render.
                        let launched = outcome.is_some_and(|o| is_true(o.get("isAsync")))
                            || field("status") == Some("async_launched");
                        if launched && let Some(agent) = field("agentId").filter(|a| !a.is_empty())
                        {
                            let label = if previews {
                                preview(field("description"), PREVIEW_LIMIT)
                            } else {
                                None
                            };
                            facts.push(TranscriptFact::BackgroundStarted {
                                tool_id: id.to_owned(),
                                service: false,
                                label,
                                at: date,
                            });
                            facts.push(TranscriptFact::BackgroundTask {
                                tool_id: id.to_owned(),
                                task: agent.to_owned(),
                            });
                        }
                    }
                    if is_true(json.get("toolEndsTurn")) {
                        facts.push(TranscriptFact::TurnEnded {
                            at: date,
                            completed: true,
                        });
                    }
                    return facts;
                }
                // The summary an idle compaction writes is no prompt and begins no turn.
                if is_true(json.get("isMeta")) || is_true(json.get("isCompactSummary")) {
                    return facts;
                }
                let texts = texts_of(content);
                if texts.iter().any(|t| {
                    t.starts_with("[Request interrupted by user")
                        || t.starts_with("<local-command-std")
                }) {
                    facts.push(TranscriptFact::TurnEnded {
                        at: date,
                        completed: false,
                    });
                } else if texts.iter().any(|t| t.starts_with("<command-name>")) {
                    // A slash command typed in the terminal, like /recap or /cost,
                    // asks the model nothing by itself; one that does, like a
                    // skill, begins its turn with the reply or the registry.
                } else if !texts.is_empty() || (previews && !images_of(content).is_empty()) {
                    facts.push(TranscriptFact::TurnBegan(date));
                    // Task notifications and the summary a compaction leaves
                    // arrive as user lines but were never typed.
                    let notified = json.get("origin").and_then(|o| str_of(o.get("kind")))
                        == Some("task-notification");
                    if previews
                        && !notified
                        && let Some(prompt) = typed(&texts)
                    {
                        facts.push(TranscriptFact::Prompt {
                            text: prompt,
                            at: date,
                        });
                    }
                    if previews && !notified {
                        let images = images_of(content);
                        if !images.is_empty() {
                            facts.push(TranscriptFact::PromptImages { images, at: date });
                        }
                    }
                }
            }
            "attachment" => {
                // A prompt typed while Claude worked joins the running turn.
                if !previews || is_true(json.get("isSidechain")) {
                    return facts;
                }
                let Some(attachment) = json.get("attachment").and_then(Value::as_object) else {
                    return facts;
                };
                if str_of(attachment.get("type")) != Some("queued_command") {
                    return facts;
                }
                if let Some(text) = typed(&texts_of(attachment.get("prompt"))) {
                    facts.push(TranscriptFact::Prompt { text, at: date });
                }
                let images = images_of(attachment.get("prompt"));
                if !images.is_empty() {
                    facts.push(TranscriptFact::PromptImages { images, at: date });
                }
            }
            "ai-title" => {
                if previews && let Some(title) = preview(str_of(json.get("aiTitle")), PREVIEW_LIMIT)
                {
                    facts.push(TranscriptFact::Title(title));
                }
            }
            _ => {}
        }
        facts
    }

    /// The usage of a reply, read as the AI section reads it, sidechain or
    /// not: a subagent's tokens count toward its session.
    fn usage(
        &mut self,
        json: &Map<String, Value>,
        now: Time,
        pricing: &dyn Pricing,
    ) -> Option<TranscriptFact> {
        if str_of(json.get("type")) != Some("assistant") {
            return None;
        }
        let message = json.get("message")?.as_object()?;
        if let Some(session) = str_of(json.get("sessionId")).filter(|s| !s.is_empty()) {
            self.session = session.to_owned();
        }
        let date = timestamp(json.get("timestamp")).unwrap_or(now);
        let model = str_of(message.get("model")).unwrap_or("");
        let usage = message.get("usage")?.as_object()?;
        if model.is_empty() || model.starts_with('<') {
            return None;
        }
        let nested = |outer: &str, inner: &str| usage.get(outer).and_then(|o| o.get(inner));
        let billable = Billable {
            tokens: Tokens {
                input: count(usage.get("input_tokens")),
                cache_write: count(usage.get("cache_creation_input_tokens")),
                cache_read: count(usage.get("cache_read_input_tokens")),
                output: count(usage.get("output_tokens")),
                reasoning: count(nested("output_tokens_details", "thinking_tokens")),
            },
            long_cache_write: count(nested("cache_creation", "ephemeral_1h_input_tokens")),
            fast: str_of(usage.get("speed")) == Some("fast"),
            domestic: str_of(usage.get("inference_geo")) == Some("us"),
            web_searches: count(nested("server_tool_use", "web_search_requests")),
        };
        let id = str_of(message.get("id")).unwrap_or("");
        let request = str_of(json.get("requestId")).unwrap_or("");
        let key = if id.is_empty() && request.is_empty() {
            format!("claude:{}:{date:?}", self.session)
        } else {
            format!("claude:{id}:{request}")
        };
        Some(TranscriptFact::Usage {
            key,
            tokens: billable.tokens,
            cost: pricing.cost(model, &billable),
        })
    }
}

/// `<session>/subagents/agent-<id>.jsonl` names its subagent.
pub fn subagent_of(path: &str) -> Option<String> {
    if !path.contains("/subagents/") && !path.contains("\\subagents\\") {
        return None;
    }
    let file = last_path_component(path);
    let name = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file.as_str(),
    };
    Some(name.strip_prefix("agent-").unwrap_or(name).to_owned())
}

/// Whether the record delivers a task notification, rather than quoting one.
pub fn delivers_notification(json: &Map<String, Value>, kind: &str) -> bool {
    match kind {
        "queue-operation" => true,
        "user" => {
            json.get("origin").and_then(|o| str_of(o.get("kind"))) == Some("task-notification")
        }
        "attachment" => {
            json.get("attachment").and_then(|a| str_of(a.get("type"))) == Some("queued_command")
        }
        _ => false,
    }
}

/// The task a notification line reports finished, and how.
pub fn finished_task(line: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(line);
    let id = between(&text, "<task-id>", "</task-id>")?;
    let status = between(&text, "<status>", "</status>")?;
    FINISHED_STATUSES
        .contains(&status.as_str())
        .then_some((id, status))
}

fn between(text: &str, start: &str, end: &str) -> Option<String> {
    let open = text.find(start)? + start.len();
    let close = open + text[open..].find(end)?;
    let value = trim(&text[open..close]);
    (!value.is_empty() && char_count(value) <= 128).then(|| value.to_owned())
}

fn subagent_facts(
    json: &Map<String, Value>,
    kind: &str,
    id: &str,
    date: Time,
) -> Vec<TranscriptFact> {
    if kind != "assistant" && kind != "user" {
        return Vec::new();
    }
    if kind == "assistant"
        && let Some(message) = json.get("message").and_then(Value::as_object)
        && str_of(message.get("stop_reason")).is_some_and(|s| TURN_ENDS.contains(&s))
    {
        return vec![TranscriptFact::SubagentEnded {
            id: id.to_owned(),
            at: date,
        }];
    }
    vec![TranscriptFact::SubagentActivity {
        id: id.to_owned(),
        at: date,
    }]
}

/// A string's text, or the text blocks of a list of blocks.
fn images_of(content: Option<&Value>) -> Vec<String> {
    dicts(content)
        .into_iter()
        .filter(|block| str_of(block.get("type")) == Some("image"))
        .filter_map(|block| {
            let source = block.get("source")?;
            match source.get("type")?.as_str()? {
                "base64" => {
                    let mime = source.get("media_type")?.as_str()?;
                    let data = source.get("data")?.as_str()?;
                    (mime.starts_with("image/") && data.len() <= 28 * 1024 * 1024)
                        .then(|| format!("data:{mime};base64,{data}"))
                }
                _ => None,
            }
        })
        .take(16)
        .collect()
}

fn texts_of(content: Option<&Value>) -> Vec<String> {
    if let Some(text) = content.and_then(Value::as_str) {
        return vec![text.to_owned()];
    }
    dicts(content)
        .into_iter()
        .filter(|block| str_of(block.get("type")) == Some("text"))
        .filter_map(|block| str_of(block.get("text")).map(str::to_owned))
        .collect()
}

/// Text the person wrote; commands and notifications come wrapped in tags.
fn typed(texts: &[String]) -> Option<String> {
    let joined = texts.join("\n");
    let text = trim(&joined);
    if text.starts_with('<') {
        return None;
    }
    preview(Some(text), REPLY_LIMIT)
}

/// A token count: never negative, and capped so a damaged line cannot
/// overflow the sums it joins.
fn count(value: Option<&Value>) -> i64 {
    match value.and_then(Value::as_f64) {
        Some(number) if number.is_finite() && number > 0.0 => number.min(1e12) as i64,
        _ => 0,
    }
}

/// Structural keys are written without spaces and never appear unescaped
/// inside a string value, so a byte match is a safe first filter.
fn contains(line: &[u8], needle: &[u8]) -> bool {
    memchr::memmem::find(line, needle).is_some()
}

// MARK: Side files

/// The type and task of a subagent, from the `agent-<id>.meta.json` file
/// beside its transcript.
pub fn subagent_info(data: &[u8], id: &str, policy: ContentPolicy) -> Option<TranscriptFact> {
    let value: Value = serde_json::from_slice(data).ok()?;
    let json = value.as_object()?;
    let agent_type = str_of(json.get("agentType"))
        .filter(|t| !t.is_empty())
        .unwrap_or("Agent");
    let description = if policy.previews {
        str_of(json.get("description")).map(str::to_owned)
    } else {
        None
    };
    Some(TranscriptFact::SubagentInfo {
        id: id.to_owned(),
        agent_type: agent_type.to_owned(),
        description,
    })
}

/// Progress of the task list Claude Code keeps in `tasks/<session>/`, one
/// file per task.
pub fn tasks<D: AsRef<[u8]>>(files: &[D], policy: ContentPolicy) -> Option<TaskProgress> {
    let mut progress = TaskProgress::default();
    for data in files {
        let Ok(Value::Object(json)) = serde_json::from_slice::<Value>(data.as_ref()) else {
            continue;
        };
        let Some(status) = str_of(json.get("status")).filter(|s| *s != "deleted") else {
            continue;
        };
        progress.total += 1;
        if status == "completed" {
            progress.done += 1;
        }
        if status == "in_progress" && policy.previews && progress.current.is_none() {
            progress.current = str_of(json.get("activeForm"))
                .or_else(|| str_of(json.get("subject")))
                .map(str::to_owned);
        }
    }
    (progress.total > 0).then_some(progress)
}

// MARK: Times

/// ISO 8601 text, or Unix seconds, or milliseconds from agents that write those.
pub fn timestamp(value: Option<&Value>) -> Option<Time> {
    match value? {
        Value::String(text) => parse_iso(text),
        Value::Number(number) => {
            let raw = number.as_f64()?;
            (raw.is_finite() && raw > 0.0).then(|| {
                if raw > 100_000_000_000.0 {
                    raw / 1000.0
                } else {
                    raw
                }
            })
        }
        _ => None,
    }
}

/// ISO 8601 times as Claude Code writes them, "2026-09-21T23:42:45.078Z",
/// with or without the fraction, in UTC or at an `+hh:mm` offset. The Mac
/// falls back to `ISO8601DateFormatter` for anything else; nothing Claude
/// Code writes needs it.
pub fn parse_iso(text: &str) -> Option<Time> {
    let b = text.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        b[range].iter().try_fold(0i64, |value, byte| {
            byte.is_ascii_digit()
                .then(|| value * 10 + i64::from(byte - b'0'))
        })
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 61
    {
        return None;
    }
    let mut index = 19;
    let mut fraction = 0.0;
    if b[index] == b'.' {
        let mut scale = 0.1;
        index += 1;
        while index < b.len() && b[index].is_ascii_digit() {
            fraction += f64::from(b[index] - b'0') * scale;
            scale /= 10.0;
            index += 1;
        }
    }
    if index >= b.len() {
        return None;
    }
    let offset = match b[index] {
        b'Z' if index == b.len() - 1 => 0,
        sign @ (b'+' | b'-') if b.len() == index + 6 && b[index + 3] == b':' => {
            let seconds = number(index + 1..index + 3)? * 3600 + number(index + 4..index + 6)? * 60;
            if sign == b'+' { seconds } else { -seconds }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3600 + minute * 60 + second - offset) as f64 + fraction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::models::tool_summary::input_key;
    use serde_json::json;

    const PROMPT: &str = r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"Use the Bash tool to run exactly: echo SPIKE_OK > spike.txt"},"timestamp":"2026-10-21T17:50:38.622Z","origin":{"kind":"human"},"cwd":"/tmp/spike/work","sessionId":"s1","gitBranch":"feat/sessions"}"#;
    const TOOL_CALL: &str = r#"{"isSidechain":false,"message":{"model":"claude-haiku-5-5","id":"msg_1","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo SPIKE_OK > spike.txt","description":"Write"}}],"stop_reason":"tool_use","usage":{"input_tokens":2,"cache_creation_input_tokens":48651,"cache_read_input_tokens":28058,"output_tokens":103}},"requestId":"req_1","type":"assistant","timestamp":"2026-10-21T17:50:44.000Z","sessionId":"s1","cwd":"/tmp/spike/work","gitBranch":"feat/sessions"}"#;
    const TOOL_RESULT: &str = r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"(Bash completed with no output)","is_error":true}]},"timestamp":"2026-10-21T17:50:50.507Z","sessionId":"s1"}"#;
    const REPLY: &str = r#"{"isSidechain":false,"message":{"model":"claude-haiku-5-5","id":"msg_2","type":"message","role":"assistant","content":[{"type":"text","text":"I ran it. SENTINEL-reply"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":76000,"output_tokens":40}},"requestId":"req_2","type":"assistant","timestamp":"2026-10-21T17:50:52.900Z","sessionId":"s1"}"#;
    const TITLE: &str = r#"{"type":"ai-title","aiTitle":"SENTINEL-title","sessionId":"s1"}"#;
    const INTERRUPT: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"2026-10-21T17:51:00.000Z","sessionId":"s1"}"#;
    const META: &str = r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<system-reminder>x</system-reminder>"},"timestamp":"2026-10-21T17:51:01.000Z","sessionId":"s1"}"#;

    fn parse_at(lines: &[&str], previews: bool, path: &str) -> Vec<TranscriptFact> {
        let mut reader = TranscriptReader::new(ContentPolicy { previews }, path);
        lines
            .iter()
            .flat_map(|line| reader.parse(line.as_bytes(), 0.0, &NoPricing))
            .collect()
    }

    fn parse(lines: &[&str], previews: bool) -> Vec<TranscriptFact> {
        parse_at(lines, previews, "/x/projects/p/s1.jsonl")
    }

    fn date(text: &str) -> f64 {
        parse_iso(text).unwrap()
    }

    fn began(facts: &[TranscriptFact]) -> bool {
        facts
            .iter()
            .any(|f| matches!(f, TranscriptFact::TurnBegan(_)))
    }

    fn prompted(facts: &[TranscriptFact]) -> bool {
        facts
            .iter()
            .any(|f| matches!(f, TranscriptFact::Prompt { .. }))
    }

    // ClaudeSessionReaderTests.swift transcript.
    #[test]
    fn transcript() {
        let facts = parse(
            &[
                PROMPT,
                TOOL_CALL,
                TOOL_RESULT,
                REPLY,
                TITLE,
                INTERRUPT,
                META,
            ],
            false,
        );
        assert!(
            facts.contains(&TranscriptFact::TurnBegan(date("2026-10-21T17:50:38.622Z"))),
            "a typed prompt begins a turn"
        );
        assert!(
            facts.contains(&TranscriptFact::Branch("feat/sessions".into())),
            "the branch comes from the transcript"
        );
        assert!(
            facts.contains(&TranscriptFact::Model("claude-haiku-5-5".into())),
            "and the model"
        );
        let key = input_key(
            "Bash",
            json!({"command": "echo SPIKE_OK > spike.txt", "description": "Write"})
                .as_object()
                .unwrap(),
        );
        let mut started = ToolUse::new("toolu_1", "Bash", None, date("2026-10-21T17:50:44.000Z"));
        started.input_key = Some(key);
        assert!(
            facts.contains(&TranscriptFact::ToolStarted(started)),
            "a tool call starts that tool, without its command while previews are off"
        );
        assert!(
            facts.contains(&TranscriptFact::ToolFinished {
                id: "toolu_1".into(),
                at: date("2026-10-21T17:50:50.507Z"),
                failed: true
            }),
            "its result finishes it and says when it failed"
        );
        assert!(
            facts.contains(&TranscriptFact::TurnEnded {
                at: date("2026-10-21T17:50:52.900Z"),
                completed: true
            }),
            "end_turn ends the turn"
        );
        assert!(
            facts.contains(&TranscriptFact::TurnEnded {
                at: date("2026-10-21T17:51:00.000Z"),
                completed: false
            }),
            "an interruption ends it unfinished"
        );
        assert!(
            !facts.contains(&TranscriptFact::TurnBegan(date("2026-10-21T17:51:01.000Z"))),
            "meta lines begin nothing"
        );
        assert!(
            !format!("{facts:?}").contains("SENTINEL"),
            "with previews off no text or title is read"
        );
        let usage: Vec<Tokens> = facts
            .iter()
            .filter_map(|f| match f {
                TranscriptFact::Usage { tokens, .. } => Some(*tokens),
                _ => None,
            })
            .collect();
        assert!(
            usage.len() == 2 && usage[0].output == 103,
            "usage is read for each reply"
        );
        assert!(facts.iter().any(
            |f| matches!(f, TranscriptFact::Usage { key, .. } if key == "claude:msg_1:req_1")
        ));

        let open = parse(&[TOOL_CALL, REPLY, TITLE], true);
        assert!(
            open.contains(&TranscriptFact::Text {
                text: "I ran it. SENTINEL-reply".into(),
                at: date("2026-10-21T17:50:52.900Z"),
                ends_turn: true
            }) && open.contains(&TranscriptFact::Title("SENTINEL-title".into())),
            "with previews on replies and titles are read"
        );
        let on_the_way = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Checking the build."}],"stop_reason":"tool_use"},"timestamp":"2026-10-21T17:50:43.000Z"}"#;
        let two_texts = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First."},{"type":"text","text":"Last."}],"stop_reason":"end_turn"},"timestamp":"2026-10-21T17:50:53.000Z"}"#;
        let failed = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"API Error: 529"}],"stop_reason":"stop_sequence"},"timestamp":"2026-10-21T17:50:54.000Z"}"#;
        let refused = r#"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"Now the calibration."}],"stop_reason":"refusal"},"timestamp":"2026-10-21T17:50:55.000Z"}"#;
        let cut = r#"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"The list goes"}],"stop_reason":"max_tokens"},"timestamp":"2026-10-21T17:50:56.000Z"}"#;
        let all = parse(&[on_the_way, two_texts, failed, refused, cut], true);
        let ends: Vec<String> = all
            .iter()
            .filter_map(|f| match f {
                TranscriptFact::Text {
                    text, ends_turn, ..
                } => Some(format!("{text}{}", if *ends_turn { "!" } else { "" })),
                _ => None,
            })
            .collect();
        assert_eq!(
            ends,
            [
                "Checking the build.",
                "First.",
                "Last.!",
                "API Error: 529",
                "Now the calibration.",
                "The list goes"
            ],
            "only the last text of a turn that ended as it should ends it, not a refusal or a cut-off"
        );
        assert!(all.contains(&TranscriptFact::TurnEnded {
            at: date("2026-10-21T17:50:54.000Z"),
            completed: false
        }));
        assert!(all.contains(&TranscriptFact::TurnEnded {
            at: date("2026-10-21T17:50:55.000Z"),
            completed: true
        }));
        assert!(
            !all.iter()
                .any(|f| matches!(f, TranscriptFact::Model(m) if m.starts_with('<')))
        );
        assert!(
            open.iter().any(|f| matches!(f, TranscriptFact::ToolStarted(t) if t.summary.as_deref() == Some("echo SPIKE_OK > spike.txt"))),
            "and tool commands"
        );

        assert!(!prompted(&facts), "with previews off no prompt is read");
        let typed = parse(&[PROMPT], true);
        assert!(
            typed.contains(&TranscriptFact::Prompt {
                text: "Use the Bash tool to run exactly: echo SPIKE_OK > spike.txt".into(),
                at: date("2026-10-21T17:50:38.622Z")
            }),
            "with previews on a typed prompt joins the conversation"
        );
        let mid_turn = r#"{"isSidechain":false,"type":"attachment","attachment":{"type":"queued_command","prompt":[{"type":"text","text":"Also run lint"},{"type":"image","source":{}}]},"timestamp":"2026-10-21T17:52:00.000Z"}"#;
        let summary = r#"{"type":"user","isCompactSummary":true,"message":{"role":"user","content":"This session is being continued"},"timestamp":"2026-10-21T17:53:00.000Z"}"#;
        let command = r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"},"timestamp":"2026-10-21T17:54:00.000Z"}"#;
        let long_reply = format!(
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{}"}}]}},"timestamp":"2026-10-21T17:55:00.000Z"}}"#,
            "a".repeat(7000)
        );
        let said: Vec<String> = parse(&[mid_turn, summary, command, &long_reply], true)
            .into_iter()
            .filter_map(|f| match f {
                TranscriptFact::Prompt { text, .. } | TranscriptFact::Text { text, .. } => {
                    Some(text)
                }
                _ => None,
            })
            .collect();
        assert!(
            !began(&parse(&[summary], false)),
            "an idle compaction's summary begins no turn"
        );
        let slash = r#"{"type":"user","message":{"role":"user","content":"<command-name>/recap</command-name>\n<command-message>recap</command-message>"},"timestamp":"2026-10-22T10:26:11.114Z"}"#;
        assert!(
            !began(&parse(&[slash], true)),
            "a slash command like /recap begins no turn"
        );
        assert!(
            said.first().map(String::as_str) == Some("Also run lint") && said.len() == 2,
            "a prompt typed while Claude works counts; summaries and commands do not"
        );
        let last = said.last().unwrap();
        assert!(
            last.chars().count() == REPLY_LIMIT && last.ends_with('…'),
            "a very long reply is cut at its limit"
        );

        let started = r#"{"type":"assistant","timestamp":"2026-10-22T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"tb1","name":"Bash","input":{"command":"npx expo start --port 8088","description":"Run own Metro","run_in_background":true}}]}}"#;
        let given = r#"{"type":"user","timestamp":"2026-10-22T10:00:01.000Z","toolUseResult":{"backgroundTaskId":"b9x"},"message":{"role":"user","content":[{"tool_use_id":"tb1","type":"tool_result","content":"Command running in background with ID: b9x."}]}}"#;
        let stopped = r#"{"type":"assistant","timestamp":"2026-10-22T10:05:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"tk","name":"TaskStop","input":{"task_id":"b9x"}}]}}"#;
        let jobs = parse(&[started, given, stopped], true);
        assert!(
            jobs.contains(&TranscriptFact::BackgroundStarted {
                tool_id: "tb1".into(),
                service: true,
                label: Some("Run own Metro".into()),
                at: date("2026-10-22T10:00:00.000Z")
            }) && jobs.contains(&TranscriptFact::BackgroundTask {
                tool_id: "tb1".into(),
                task: "b9x".into()
            }) && jobs.contains(&TranscriptFact::SubagentEnded {
                id: "b9x".into(),
                at: date("2026-10-22T10:05:00.000Z")
            }),
            "a background command, the id it was given and its stop are read"
        );
        assert!(
            parse(&[started], false).contains(&TranscriptFact::BackgroundStarted {
                tool_id: "tb1".into(),
                service: true,
                label: None,
                at: date("2026-10-22T10:00:00.000Z")
            }),
            "with previews off it is still told apart, without its words"
        );
        let sent = r#"{"type":"user","timestamp":"2026-10-22T13:14:15.571Z","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a758ef93113160fa0","description":"Map friends tab and add flow"},"message":{"role":"user","content":[{"tool_use_id":"ta1","type":"tool_result","content":[{"type":"text","text":"Async agent launched successfully."}]}]}}"#;
        let waited = r#"{"type":"user","timestamp":"2026-10-22T13:14:20.000Z","toolUseResult":{"status":"completed","agentId":"a1","description":"Look around"},"message":{"role":"user","content":[{"tool_use_id":"ta2","type":"tool_result","content":"Done."}]}}"#;
        let agents = parse(&[sent, waited], true);
        assert!(
            agents.contains(&TranscriptFact::BackgroundStarted {
                tool_id: "ta1".into(),
                service: false,
                label: Some("Map friends tab and add flow".into()),
                at: date("2026-10-22T13:14:15.571Z")
            }) && agents.contains(&TranscriptFact::BackgroundTask {
                tool_id: "ta1".into(),
                task: "a758ef93113160fa0".into()
            }),
            "a subagent sent to the background is work Claude waits for, under its own id"
        );
        assert!(
            !agents.iter().any(|f| matches!(f, TranscriptFact::BackgroundStarted { tool_id, .. } if tool_id == "ta2")),
            "a subagent that answered before the turn went on is not"
        );
        assert!(
            parse(&[sent], false).contains(&TranscriptFact::BackgroundStarted {
                tool_id: "ta1".into(),
                service: false,
                label: None,
                at: date("2026-10-22T13:14:15.571Z")
            }),
            "with previews off without what it does"
        );

        let notice = r#"{"type":"user","origin":{"kind":"task-notification"},"timestamp":"2026-10-21T19:06:34.760Z","message":{"role":"user","content":"<task-notification>\n<task-id>a05df</task-id>\n<status>completed</status>\n</task-notification>"},"sessionId":"s1"}"#;
        let queued = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-21T19:06:30.000Z","content":"<task-notification><task-id>b77</task-id><status>failed</status></task-notification>"}"#;
        let quoted = r#"{"type":"assistant","timestamp":"2026-10-21T19:07:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"<task-notification><task-id>zz</task-id><status>completed</status>"}]}}"#;
        let running = r#"{"type":"queue-operation","timestamp":"2026-10-21T19:06:00.000Z","content":"<task-notification><task-id>c1</task-id><status>running</status></task-notification>"}"#;
        let notified = parse(&[notice, queued, quoted, running], false);
        assert!(
            !prompted(&parse(&[notice], true)),
            "a task notification is not something the person typed"
        );
        assert!(
            notified.contains(&TranscriptFact::SubagentEnded {
                id: "a05df".into(),
                at: date("2026-10-21T19:06:34.760Z")
            }) && notified.contains(&TranscriptFact::SubagentEnded {
                id: "b77".into(),
                at: date("2026-10-21T19:06:30.000Z")
            }),
            "a task notification ends the background subagent it names"
        );
        assert!(
            !notified
                .iter()
                .any(|f| matches!(f, TranscriptFact::SubagentEnded { id, .. } if id == "zz" || id == "c1")),
            "a reply quoting one, or a task still running, ends nothing"
        );

        let path = "/x/projects/p/s1/subagents/agent-a7.jsonl";
        assert_eq!(
            subagent_of(path).as_deref(),
            Some("a7"),
            "a subagent file names its subagent"
        );
        assert_eq!(
            subagent_of("C:\\Users\\me\\.claude\\projects\\p\\s1\\subagents\\agent-b2.jsonl")
                .as_deref(),
            Some("b2")
        );
        assert_eq!(subagent_of("/x/projects/p/s1.jsonl"), None);
        let side = |line: &str| line.replace(r#""isSidechain":false"#, r#""isSidechain":true"#);
        let (call, reply) = (side(TOOL_CALL), side(REPLY));
        let sub = parse_at(&[&call, &reply], true, path);
        assert!(
            sub.contains(&TranscriptFact::SubagentActivity {
                id: "a7".into(),
                at: date("2026-10-21T17:50:44.000Z")
            }) && sub.contains(&TranscriptFact::SubagentEnded {
                id: "a7".into(),
                at: date("2026-10-21T17:50:52.900Z")
            }),
            "a subagent's lines tell when it works and ends"
        );
        assert!(
            !sub.iter()
                .any(|f| matches!(f, TranscriptFact::ToolStarted(_))),
            "its tools stay out of the session's timeline"
        );
        assert!(
            sub.iter()
                .any(|f| matches!(f, TranscriptFact::Usage { .. })),
            "its usage counts toward the session"
        );
        let main_side = parse(&[&call], true);
        assert!(
            main_side
                .iter()
                .any(|f| matches!(f, TranscriptFact::Usage { .. }))
                && !main_side
                    .iter()
                    .any(|f| matches!(f, TranscriptFact::ToolStarted(_))),
            "a sidechain line in the main file counts its usage and nothing else"
        );

        let meta = br#"{"agentType":"Explore","description":"Find NotchModule"}"#;
        assert_eq!(
            subagent_info(meta, "a7", ContentPolicy::default()),
            Some(TranscriptFact::SubagentInfo {
                id: "a7".into(),
                agent_type: "Explore".into(),
                description: None
            }),
            "a subagent's type is read without its task"
        );
        assert_eq!(
            subagent_info(meta, "a7", ContentPolicy { previews: true }),
            Some(TranscriptFact::SubagentInfo {
                id: "a7".into(),
                agent_type: "Explore".into(),
                description: Some("Find NotchModule".into())
            })
        );
        assert_eq!(
            subagent_info(br#"{"agentType":""}"#, "a", ContentPolicy::default()),
            Some(TranscriptFact::SubagentInfo {
                id: "a".into(),
                agent_type: "Agent".into(),
                description: None
            })
        );
        assert_eq!(subagent_info(b"[]", "a", ContentPolicy::default()), None);
        let progress = tasks(
            &[
                &br#"{"id":"1","subject":"A","activeForm":"Doing A","status":"completed"}"#[..],
                br#"{"id":"2","subject":"B","activeForm":"Doing B","status":"in_progress"}"#,
                br#"{"id":"3","subject":"C","status":"pending"}"#,
                br#"{"id":"4","status":"deleted"}"#,
                b"{",
            ],
            ContentPolicy { previews: true },
        );
        assert_eq!(
            progress,
            Some(TaskProgress {
                done: 1,
                total: 3,
                current: Some("Doing B".into())
            }),
            "task progress counts finished tasks and names the one under way"
        );
        assert_eq!(
            tasks::<&[u8]>(&[], ContentPolicy::default()),
            None,
            "no tasks shows no progress"
        );
        let quiet = tasks(
            &[br#"{"subject":"B","status":"in_progress"}"#],
            ContentPolicy::default(),
        );
        assert_eq!(
            quiet,
            Some(TaskProgress {
                done: 0,
                total: 1,
                current: None
            })
        );
    }

    #[test]
    fn usage_keys_and_costs() {
        struct Flat;
        impl Pricing for Flat {
            fn cost(&self, model: &str, billable: &Billable) -> f64 {
                assert_eq!(model, "claude-opus-5-5");
                billable.tokens.output as f64
                    + if billable.fast { 1000.0 } else { 0.0 }
                    + billable.web_searches as f64 * 10.0
            }
        }
        let line = r#"{"type":"assistant","sessionId":"s9","timestamp":12,"message":{"model":"claude-opus-5-5","usage":{"input_tokens":-5,"output_tokens":7,"speed":"fast","output_tokens_details":{"thinking_tokens":3},"server_tool_use":{"web_search_requests":2},"cache_creation":{"ephemeral_1h_input_tokens":4}}}}"#;
        let mut reader = TranscriptReader::new(ContentPolicy::default(), "/p/s9.jsonl");
        let facts = reader.parse(line.as_bytes(), 0.0, &Flat);
        let TranscriptFact::Usage { key, tokens, cost } = &facts[0] else {
            panic!("usage first: {facts:?}")
        };
        assert_eq!(
            key, "claude:s9:12.0",
            "a reply without ids is keyed by its session and time"
        );
        assert_eq!(
            *tokens,
            Tokens {
                input: 0,
                cache_write: 0,
                cache_read: 0,
                output: 7,
                reasoning: 3
            }
        );
        assert_eq!(*cost, 7.0 + 1000.0 + 20.0);
        let synthetic =
            r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"output_tokens":7}}}"#;
        assert!(
            reader.parse(synthetic.as_bytes(), 0.0, &Flat).is_empty(),
            "a synthetic reply bills nothing"
        );
        let spaced = r#"{"type": "assistant","message":{"model":"claude-opus-5-5","usage":{"output_tokens":7}}}"#;
        assert!(
            !reader
                .parse(spaced.as_bytes(), 0.0, &Flat)
                .iter()
                .any(|f| matches!(f, TranscriptFact::Usage { .. })),
            "usage is found by the compact key, as Claude Code writes it"
        );
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_iso("2026-10-21T17:50:25Z"), Some(1_792_605_025.0));
        let fractional = parse_iso("2026-10-21T17:50:38.622Z").unwrap();
        assert!((fractional - 1_792_605_038.622).abs() < 1e-6);
        assert_eq!(
            parse_iso("2026-10-21T19:50:25+02:00"),
            Some(1_792_605_025.0)
        );
        assert_eq!(
            parse_iso("2026-10-21T15:50:25-02:00"),
            Some(1_792_605_025.0)
        );
        assert_eq!(
            parse_iso("2026-10-21T17:50:25"),
            None,
            "a time without a zone is not read"
        );
        assert_eq!(parse_iso("2026-13-21T17:50:25Z"), None);
        assert_eq!(parse_iso("2026-10-21T17:50:25Zx"), None);
        assert_eq!(parse_iso("2026-10-21T17:50:25.5"), None);
        assert_eq!(
            timestamp(Some(&json!(1_792_605_025))),
            Some(1_792_605_025.0),
            "seconds"
        );
        assert_eq!(
            timestamp(Some(&json!(1_792_605_025_500i64))),
            Some(1_792_605_025.5),
            "or milliseconds"
        );
        assert_eq!(timestamp(Some(&json!(-1))), None);
        assert_eq!(timestamp(Some(&json!(true))), None);
        assert_eq!(timestamp(None), None);
    }

    #[test]
    fn notification_lines() {
        let line = br#"<task-notification><task-id>  ab  </task-id><status>killed</status>"#;
        assert_eq!(finished_task(line), Some(("ab".into(), "killed".into())));
        let long = format!(
            "<task-id>{}</task-id><status>completed</status>",
            "x".repeat(129)
        );
        assert_eq!(
            finished_task(long.as_bytes()),
            None,
            "an id past 128 characters is no id"
        );
        assert_eq!(
            finished_task(b"<task-id></task-id><status>completed</status>"),
            None
        );
        assert_eq!(
            finished_task(b"<status>completed</status><task-id>a</task-id>"),
            Some(("a".into(), "completed".into()))
        );
        let attached = br#"{"type":"attachment","attachment":{"type":"queued_command","prompt":"<task-notification><task-id>q1</task-id><status>stopped</status></task-notification>"},"timestamp":"2026-10-21T19:06:30.000Z"}"#;
        let mut reader = TranscriptReader::new(ContentPolicy { previews: true }, "/p/s.jsonl");
        let facts = reader.parse(attached, 0.0, &NoPricing);
        assert!(facts.contains(&TranscriptFact::SubagentEnded {
            id: "q1".into(),
            at: date("2026-10-21T19:06:30.000Z")
        }));
        assert!(!prompted(&facts), "a queued notification is not a prompt");
    }

    #[test]
    fn stops_and_prompts() {
        for (name, key) in [
            ("KillShell", "shell_id"),
            ("KillBash", "bash_id"),
            ("TaskStop", "task_id"),
        ] {
            let line = format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"k","name":"{name}","input":{{"{key}":"j1"}}}}]}}}}"#
            );
            let facts = parse(&[&line], false);
            assert!(
                facts
                    .iter()
                    .any(|f| matches!(f, TranscriptFact::SubagentEnded { id, .. } if id == "j1")),
                "{name}"
            );
        }
        let numbered = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"k","name":"TaskStop","input":{"task_id":5,"shell_id":"j1"}}]}}"#;
        assert!(
            !parse(&[numbered], false)
                .iter()
                .any(|f| matches!(f, TranscriptFact::SubagentEnded { .. })),
            "the first id given decides, as on the Mac"
        );
        let local = r#"{"type":"user","message":{"content":"<local-command-stdout>ok</local-command-stdout>"},"timestamp":"2026-10-21T17:51:00.000Z"}"#;
        assert!(parse(&[local], false).contains(&TranscriptFact::TurnEnded {
            at: date("2026-10-21T17:51:00.000Z"),
            completed: false
        }));
        let ends = r#"{"type":"user","toolEndsTurn":true,"message":{"content":[{"type":"tool_result","tool_use_id":"t"}]},"timestamp":"2026-10-21T17:51:00.000Z"}"#;
        assert!(parse(&[ends], false).contains(&TranscriptFact::TurnEnded {
            at: date("2026-10-21T17:51:00.000Z"),
            completed: true
        }));
        let tagged = r#"{"type":"user","message":{"content":"  <bash-input>ls</bash-input>"},"timestamp":"2026-10-21T17:51:00.000Z"}"#;
        let facts = parse(&[tagged], true);
        assert!(
            began(&facts) && !prompted(&facts),
            "a tagged line begins a turn but is not shown as typed"
        );
        let head = r#"{"type":"user","gitBranch":"HEAD","message":{"content":"hi"}}"#;
        let facts = parse(&[head], true);
        assert!(
            !facts.iter().any(|f| matches!(f, TranscriptFact::Branch(_))),
            "a detached HEAD is no branch"
        );
        assert!(
            facts.contains(&TranscriptFact::Prompt {
                text: "hi".into(),
                at: 0.0
            }),
            "a line without a time is now"
        );
        assert!(parse(&["not json", "[1]", r#"{"no":"type"}"#], true).is_empty());
    }

    #[test]
    fn task_ids_count_characters() {
        let id = "e\u{301}".repeat(100);
        let line = format!(
            r#"{{"type":"queue-operation","content":"<task-notification><task-id>{id}</task-id><status>completed</status>"}}"#
        );
        let facts = parse(&[&line], false);
        assert!(
            facts.iter().any(
                |f| matches!(f, TranscriptFact::SubagentEnded { id: ended, .. } if *ended == id)
            ),
            "an id of 100 characters, 200 scalars, is within the limit, as Swift counts"
        );
    }

    #[test]
    fn image_only_prompts_keep_attachments_only_with_previews() {
        let line = r#"{"type":"user","message":{"content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"aGVsbG8="}}]}}"#;
        let facts = parse(&[line], true);
        assert!(
            facts
                .iter()
                .any(|f| matches!(f, TranscriptFact::PromptImages { images, .. }
            if images == &["data:image/png;base64,aGVsbG8="]))
        );
        assert!(
            !parse(&[line], false)
                .iter()
                .any(|f| matches!(f, TranscriptFact::PromptImages { .. }))
        );
    }
}
