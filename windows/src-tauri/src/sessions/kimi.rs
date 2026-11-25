// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Moonshot AI's Kimi Code (`kimi`, #240), as the Mac's AgentKimi.swift: its
//! terminal UI, `kimi web` and the Kimi Code app. Its hooks are `[[hooks]]`
//! tables in `$KIMI_CODE_HOME/config.toml` (`~/.kimi-code` by default), each
//! with only `event`, `matcher`, `command` and `timeout`: any other key stops
//! Kimi from loading the file. Kimi writes that file back line by line, so
//! the island's tables, between its two marker lines at the end, stay put.
//!
//! Kimi's hooks can't approve anything: `PermissionRequest` and
//! `PermissionResult` only tell of a request, and what `UserPromptSubmit`
//! prints goes into the model's context, so the island's hook prints
//! nothing. Where the session runs decides how the island answers:
//! - in the terminal UI, which holds the session in its own process, with
//!   keys: `1` approves once, `2` for the session, `3` rejects; a plan takes
//!   `1` to approve and `2` to reject; a question each answer's number (a
//!   multiple choice its numbers and Tab), then `1` submits;
//! - in `kimi web` and the Kimi Code app, a local server (KAP) holds it and
//!   takes every answer and reply over HTTP. That server runs the hooks, so
//!   the hook's process is the one `server/instances/*.json` names;
//! - in Kimi Code for VS Code nothing outside can answer: the person does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use super::hook_message::{DENY_MESSAGE, Decision, PLAN_DENY_MESSAGE};
use super::models::{Permission, Question, QuestionOption, Time, tool_summary};
use crate::hooks::installer as claude;
use crate::hosts::Key;

/// Every event the island hears. A turn the person stops ends with
/// `Interrupt` instead of `Stop`; one that fails with `StopFailure`.
pub const EVENTS: [&str; 12] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "TurnStarted",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionResult",
    "Stop",
    "StopFailure",
    "Interrupt",
];

pub const QUESTION_TOOL: &str = "AskUserQuestion";
pub const PLAN_TOOL: &str = "ExitPlanMode";

/// Where a session is shown, as the island keeps it: a KAP server (`kimi
/// web`, the app) or VS Code. A terminal session has none.
pub const KAP_CLIENT: &str = "kap";
pub const VSCODE_CLIENT: &str = "vscode";

/// The Kimi Code app's server, as its instance file names it.
pub const APP_PRODUCT: &str = "kimi-code-desktop";

// MARK: Places

/// Kimi Code's folder: `KIMI_CODE_HOME`, or `.kimi-code` in the home folder.
pub fn folder(home: &Path) -> PathBuf {
    folder_with(home, &|name| std::env::var(name).ok())
}

pub fn folder_with(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    match env("KIMI_CODE_HOME").filter(|v| !v.trim().is_empty()) {
        Some(value) => match value.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => PathBuf::from(value),
        },
        None => home.join(".kimi-code"),
    }
}

pub fn config(home: &Path) -> PathBuf {
    folder(home).join("config.toml")
}

// MARK: Hooks

/// The island's `[[hooks]]` tables between its two marker lines: one per
/// event, matching everything, the timeout a whole number as Kimi wants it.
/// The command is the portable form: Kimi runs it with cmd on Windows and
/// sh elsewhere.
pub fn block(exe: &Path, app: &str) -> String {
    use super::hook_agents::installer::{portable_command, toml_string, vibe_end, vibe_start};
    let mut lines = vec![vibe_start(app)];
    for event in EVENTS {
        lines.push("[[hooks]]".into());
        lines.push(format!("event = {}", toml_string(event)));
        lines.push(format!(
            "command = {}",
            toml_string(&portable_command(
                super::models::Provider::Kimi,
                event,
                exe,
                app
            ))
        ));
        lines.push(format!(
            "timeout = {}",
            super::hook_agents::installer::EVENT_TIMEOUT
        ));
    }
    lines.push(vibe_end(app));
    lines.join("\n") + "\n"
}

/// The file with the island's tables at its end, after the person's own.
pub fn installing(text: Option<&str>, exe: &Path, app: &str) -> String {
    use super::hook_agents::installer::vibe_removing;
    let mut base = text
        .map(|t| vibe_removing(t, app).unwrap_or_else(|| t.to_owned()))
        .unwrap_or_default();
    if !base.is_empty() && !base.ends_with('\n') {
        base.push('\n');
    }
    if !base.is_empty() {
        base.push('\n');
    }
    base + &block(exe, app)
}

pub fn state(text: Option<&str>, exe: &Path, app: &str) -> super::hook_agents::installer::State {
    use super::hook_agents::installer::{State, vibe_inline};
    match text {
        None => State::Missing,
        Some(text) if text.contains(&block(exe, app)) => State::Installed,
        Some(text) if text.contains(&claude::marker(app)) => State::Outdated,
        // `hooks = [...]` can't take tables beside it: the file is the person's.
        Some(text) if vibe_inline(text) => State::Unreadable,
        Some(_) => State::Missing,
    }
}

// MARK: Asks

fn text(json: &Map<String, Value>, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Kimi's file tools name the file `path`; Claude Code's `file_path`.
pub fn claude_input(input: &Map<String, Value>) -> Map<String, Value> {
    let mut input = input.clone();
    if !input.contains_key("file_path")
        && let Some(path) = input.get("path").and_then(Value::as_str).map(str::to_owned)
    {
        input.insert("file_path".into(), Value::String(path));
    }
    input
}

/// The card for a request Kimi's `PermissionRequest` tells of. A plan
/// carries its text; a command or a file change reads as Claude Code's.
/// Kimi's own `id` names it, the same id its server answers it by.
pub fn ask(
    tool: &str,
    raw: &str,
    request: &str,
    input: &Map<String, Value>,
    display: Option<&Map<String, Value>>,
    cwd: Option<&str>,
    at: Time,
) -> Permission {
    if raw == PLAN_TOOL || display.and_then(|d| text(d, "kind")).as_deref() == Some("plan_review") {
        let plan = display.and_then(|d| text(d, "plan")).unwrap_or_default();
        let first = plan.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let mut permission = Permission::new(
            request,
            tool_summary::PLAN_TOOL,
            Some(tool_summary::clip(first)).as_deref(),
            false,
            at,
        );
        permission.set_content(&plan, true, Some(plan.chars().count()));
        return permission;
    }
    let mut asked = super::hook_agents::grok_ask(tool, &claude_input(input), cwd, false, at);
    asked.request_id = request.to_owned();
    // "Approve for this session" is Kimi's own second choice.
    asked.offers_always = true;
    asked
}

/// The questions an `AskUserQuestion` call asks; None when one has no
/// text, so the card would be answered in Kimi.
pub fn questions(input: &Map<String, Value>) -> Option<Vec<Question>> {
    let list = input.get("questions")?.as_array()?;
    let questions: Vec<Question> = list
        .iter()
        .filter_map(|item| {
            let item = item.as_object()?;
            let question = text(item, "question").filter(|q| !q.trim().is_empty())?;
            let options = item
                .get("options")
                .and_then(Value::as_array)
                .map(|options| {
                    options
                        .iter()
                        .filter_map(|option| {
                            let option = option.as_object()?;
                            Some(QuestionOption {
                                label: text(option, "label")?,
                                description: text(option, "description"),
                                preview: None,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Question {
                question,
                header: text(item, "header"),
                options,
                multi_select: item
                    .get("multi_select")
                    .or_else(|| item.get("multiSelect"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    (!questions.is_empty() && questions.len() == list.len()).then_some(questions)
}

/// The card for a question its `PreToolUse` names: its tool call's id names
/// it, the same id its server's interaction carries.
pub fn question(call: &str, input: &Map<String, Value>, at: Time) -> Option<Permission> {
    let questions = questions(input)?;
    let mut permission = Permission::new(
        call,
        Question::TOOL,
        Some(tool_summary::clip(&questions[0].question)).as_deref(),
        false,
        at,
    );
    permission.questions = questions;
    Some(permission)
}

/// The name a model alias is shown by: Kimi names its models
/// `<provider>/<model>`, as `kimi-code/k3`.
pub fn model_name(alias: &str) -> String {
    alias
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(alias)
        .to_owned()
}

// MARK: Keys

/// The keys that answer Kimi's terminal dialog, None for an answer it can't
/// be given by keys. Each choice takes its number at once.
pub fn keys(decision: &Decision, asked: &Permission) -> Option<Vec<Key>> {
    if asked.tool == tool_summary::PLAN_TOOL {
        return match decision {
            Decision::Allow | Decision::AllowAlways | Decision::ApprovePlan(_) => {
                Some(vec![Key::Digit(1)])
            }
            Decision::Deny => Some(vec![Key::Digit(2)]),
            Decision::Answer(_) => None,
        };
    }
    if !asked.questions.is_empty() {
        return match decision {
            // Escape cancels the question; Kimi goes on without an answer.
            Decision::Deny => Some(vec![Key::Escape]),
            Decision::Answer(answers) => question_keys(answers, &asked.questions),
            _ => None,
        };
    }
    match decision {
        Decision::Allow => Some(vec![Key::Digit(1)]),
        Decision::AllowAlways => Some(vec![Key::Digit(2)]),
        Decision::Deny => Some(vec![Key::Digit(3)]),
        _ => None,
    }
}

/// Each question's answer in the dialog's order: a single choice is its
/// number, which moves on by itself; a multiple choice ticks each number,
/// then Tab moves on. The review that follows takes `1` to submit.
pub fn question_keys(
    answers: &BTreeMap<String, Vec<String>>,
    questions: &[Question],
) -> Option<Vec<Key>> {
    let picks = picks(answers, questions)?;
    let mut keys = Vec::new();
    for (question, numbers) in questions.iter().zip(picks) {
        keys.extend(numbers.iter().map(|n| Key::Digit(*n)));
        if question.multi_select {
            keys.push(Key::Tab);
        }
    }
    keys.push(Key::Digit(1));
    Some(keys)
}

/// The number of each picked choice, question by question; None when one
/// is unanswered, not listed, past nine, or several for a single choice.
pub fn picks(
    answers: &BTreeMap<String, Vec<String>>,
    questions: &[Question],
) -> Option<Vec<Vec<u8>>> {
    if questions.is_empty() {
        return None;
    }
    let mut result = Vec::new();
    for question in questions {
        let picked: Vec<&String> = answers
            .get(&question.question)
            .or_else(|| {
                (questions.len() == 1 && answers.len() == 1)
                    .then(|| answers.values().next())
                    .flatten()
            })?
            .iter()
            .filter(|a| !a.is_empty())
            .collect();
        if picked.is_empty() {
            return None;
        }
        let mut numbers = Vec::new();
        for label in picked {
            let index = question.options.iter().position(|o| &o.label == label)?;
            if index >= 9 {
                return None;
            }
            numbers.push(index as u8 + 1);
        }
        numbers.sort_unstable();
        numbers.dedup();
        if !question.multi_select && numbers.len() != 1 {
            return None;
        }
        result.push(numbers);
    }
    Some(result)
}

// MARK: Log

/// The end of a file, at most `limit` bytes.
fn tail(path: &Path, limit: u64) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(limit)))
        .ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    Some(data)
}

/// A session's folder, from Kimi's session index.
pub fn session_folder(session: &str, home: &Path) -> Option<PathBuf> {
    session_folder_in(session, &folder(home))
}

pub fn session_folder_in(session: &str, kimi: &Path) -> Option<PathBuf> {
    let data = tail(&kimi.join("session_index.jsonl"), 4 * 1024 * 1024)?;
    let text = String::from_utf8_lossy(&data);
    text.lines()
        .rev()
        .filter(|line| line.contains(session))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|json| json["sessionId"].as_str() == Some(session))
        .and_then(|json| json["sessionDir"].as_str().map(PathBuf::from))
}

pub fn wire(session_folder: &Path) -> PathBuf {
    session_folder
        .join("agents")
        .join("main")
        .join("wire.jsonl")
}

/// What the last turn of the main agent's log ended on: the model that
/// replied and, read only when asked, the text of its last answer.
pub fn last_turn(wire: &Path, wanted: bool) -> (Option<String>, Option<String>) {
    let Some(data) = tail(wire, 1024 * 1024) else {
        return (None, None);
    };
    last_turn_in(&String::from_utf8_lossy(&data), wanted)
}

pub fn last_turn_in(text: &str, wanted: bool) -> (Option<String>, Option<String>) {
    let mut model = None;
    let mut turn: Option<String> = None;
    let mut step: Option<Option<String>> = None;
    let mut parts: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        let Ok(json) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = json["type"].as_str().unwrap_or("");
        if kind == "turn.ended" && turn.is_none() {
            model = json["replyModel"]["model"].as_str().map(str::to_owned);
            turn = json["turnId"]
                .as_i64()
                .map(|n| n.to_string())
                .or_else(|| json["turnId"].as_str().map(str::to_owned));
            if !wanted {
                break;
            }
            continue;
        }
        if kind == "turn.prompt" && turn.is_some() {
            break;
        }
        let event = &json["event"];
        if !wanted
            || turn.is_none()
            || kind != "context.append_loop_event"
            || event["type"].as_str() != Some("content.part")
            || event["turnId"].as_str() != turn.as_deref()
            || event["part"]["type"].as_str() != Some("text")
        {
            continue;
        }
        let Some(part) = event["part"]["text"].as_str() else {
            continue;
        };
        // Only the step the turn ended on: earlier ones led to its tools.
        let own = event["stepUuid"].as_str().map(str::to_owned);
        match &step {
            None => step = Some(own),
            Some(seen) if *seen != own => break,
            _ => {}
        }
        parts.insert(0, part.to_owned());
    }
    let reply = parts.concat();
    let reply = (wanted && !reply.trim().is_empty())
        .then(|| super::hook_agents::reply_preview(Some(&Value::String(reply))))
        .flatten();
    (model, reply)
}

// MARK: Server

/// Kimi's local server (KAP, `kimi web` and the Kimi Code app): where it
/// listens, and the answers and replies it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub product: String,
}

impl Instance {
    /// Where to reach it on this machine.
    pub fn address(&self) -> String {
        match self.host.as_str() {
            "" | "0.0.0.0" | "127.0.0.1" | "localhost" => format!("127.0.0.1:{}", self.port),
            "::" | "::1" | "[::1]" => format!("[::1]:{}", self.port),
            other => format!("{other}:{}", self.port),
        }
    }

    /// Only a server on this machine is spoken to.
    pub fn is_local(&self) -> bool {
        matches!(
            self.host.as_str(),
            "" | "0.0.0.0" | "127.0.0.1" | "localhost" | "::" | "::1" | "[::1]"
        )
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.address())
    }
}

pub fn instance(data: &[u8]) -> Option<Instance> {
    let json: Value = serde_json::from_slice(data).ok()?;
    let pid = json["pid"].as_u64().filter(|p| *p > 0)? as u32;
    let port = json["port"].as_u64().filter(|p| *p > 0 && *p <= 65535)? as u16;
    Some(Instance {
        pid,
        host: json["host"].as_str().unwrap_or("127.0.0.1").to_owned(),
        port,
        product: json["product"].as_str().unwrap_or("").to_owned(),
    })
}

/// The servers Kimi says are running.
pub fn instances(kimi: &Path) -> Vec<Instance> {
    let Ok(entries) = std::fs::read_dir(kimi.join("server").join("instances")) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|data| instance(&data))
        .collect()
}

/// The server that runs as `pid`, which then holds the sessions whose hooks
/// it runs.
pub fn serving(pid: u32, kimi: &Path, alive: &dyn Fn(u32) -> bool) -> Option<Instance> {
    if pid == 0 {
        return None;
    }
    instances(kimi)
        .into_iter()
        .find(|i| i.pid == pid && i.is_local() && alive(pid))
}

/// The server that holds a session: the one its hooks' process is, or for
/// the Kimi Code app (whose hooks may run in a helper) the app's.
pub fn server(pid: u32, client: Option<&str>, kimi: &Path) -> Option<Instance> {
    let alive = |pid: u32| crate::hosts::process::is_running(pid);
    serving(pid, kimi, &alive).or_else(|| {
        (client == Some(KAP_CLIENT))
            .then(|| {
                instances(kimi)
                    .into_iter()
                    .find(|i| i.product == APP_PRODUCT && i.is_local() && alive(i.pid))
            })
            .flatten()
    })
}

/// Brings the Kimi Code app's window forward by its own process.
pub fn raise_app(pid: u32) -> bool {
    #[cfg(windows)]
    return crate::hosts::console::raise_process_window(pid);
    #[cfg(target_os = "linux")]
    return crate::linux::x11::raise(&[pid], "");
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        false
    }
}

pub fn token(kimi: &Path) -> Option<String> {
    let text = std::fs::read_to_string(kimi.join("server.token")).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// A session's own page in `kimi web`, signed in by the server's token.
pub fn page(server: &Instance, session: &str, token: Option<&str>) -> String {
    let base = format!("{}/sessions/{}", server.origin(), escaped(session));
    match token {
        Some(token) => format!("{base}#token={token}"),
        None => base,
    }
}

fn escaped(part: &str) -> String {
    part.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The answer to an approval or a plan, None for one it doesn't take.
pub fn approval(decision: &Decision, plan: bool) -> Option<Value> {
    Some(match decision {
        Decision::Allow | Decision::ApprovePlan(_) => {
            json!({"kind": "approval", "decision": "approved"})
        }
        Decision::AllowAlways if plan => json!({"kind": "approval", "decision": "approved"}),
        Decision::AllowAlways => {
            json!({"kind": "approval", "decision": "approved", "scope": "session"})
        }
        Decision::Deny => json!({
            "kind": "approval", "decision": "rejected",
            "feedback": if plan { PLAN_DENY_MESSAGE } else { DENY_MESSAGE },
        }),
        Decision::Answer(_) => return None,
    })
}

/// The answers to a question the server lists (`request.questions`),
/// matched by text and choice label. Deny dismisses it; None when an
/// answer can't be given.
pub fn answers(decision: &Decision, interaction: &Value) -> Option<Value> {
    let answers = match decision {
        Decision::Deny => return Some(json!({"kind": "question", "dismissed": true})),
        Decision::Answer(answers) => answers,
        _ => return None,
    };
    let listed = interaction["request"]["questions"].as_array()?;
    if listed.is_empty() {
        return None;
    }
    let mut out = Map::new();
    for item in listed {
        let id = item["id"].as_str()?;
        let question = item["question"].as_str()?;
        let picked: Vec<&String> = answers
            .get(question)
            .or_else(|| {
                (listed.len() == 1 && answers.len() == 1)
                    .then(|| answers.values().next())
                    .flatten()
            })?
            .iter()
            .filter(|a| !a.is_empty())
            .collect();
        if picked.is_empty() {
            return None;
        }
        let options = item["options"].as_array().cloned().unwrap_or_default();
        let mut ids = Vec::new();
        let mut other = Vec::new();
        for label in picked {
            match options
                .iter()
                .find(|o| o["label"].as_str() == Some(label.as_str()))
                .and_then(|o| o["id"].as_str())
            {
                Some(id) => ids.push(id.to_owned()),
                None => other.push(label.clone()),
            }
        }
        if !other.is_empty() && item["allow_other"].as_bool() == Some(false) {
            return None;
        }
        let multi = item["multi_select"].as_bool().unwrap_or(false);
        let answer = if multi {
            if other.is_empty() {
                json!({"kind": "multi", "option_ids": ids})
            } else {
                json!({"kind": "multi_with_other", "option_ids": ids, "other_text": other.join(", ")})
            }
        } else if ids.len() == 1 && other.is_empty() {
            json!({"kind": "single", "option_id": ids[0]})
        } else if ids.is_empty() && other.len() == 1 {
            json!({"kind": "other", "text": other[0]})
        } else {
            return None;
        };
        out.insert(id.to_owned(), answer);
    }
    Some(json!({"kind": "question", "answers": out}))
}

pub fn prompt(text: &str) -> Value {
    json!({
        "prompt_id": format!("openbar-{:x}", super::hook_agents::rand_id()),
        "content": [{"type": "text", "text": text}],
    })
}

/// Answers a request the server holds: an approval by its id, a question
/// by its tool call's.
pub fn answer(
    decision: &Decision,
    asked: &Permission,
    session: &str,
    server: &Instance,
    token: &str,
) -> bool {
    let base = format!("/api/v1/sessions/{}", escaped(session));
    let timeout = Duration::from_secs(4);
    if !asked.questions.is_empty() {
        let Some(listed) = call(
            server,
            token,
            "GET",
            &format!("{base}/interactions"),
            None,
            timeout,
        ) else {
            return false;
        };
        let Some(item) = listed["data"]["items"].as_array().and_then(|items| {
            items.iter().find(|i| {
                i["tool_call_id"].as_str() == Some(asked.request_id.as_str())
                    && i["kind"].as_str() == Some("question")
            })
        }) else {
            return false;
        };
        let (Some(id), Some(body)) = (item["interaction_id"].as_str(), answers(decision, item))
        else {
            return false;
        };
        return ok(call(
            server,
            token,
            "POST",
            &format!("{base}/interactions/{}", escaped(id)),
            Some(&body),
            timeout,
        ));
    }
    let Some(body) = approval(decision, asked.tool == tool_summary::PLAN_TOOL) else {
        return false;
    };
    ok(call(
        server,
        token,
        "POST",
        &format!("{base}/interactions/{}", escaped(&asked.request_id)),
        Some(&body),
        timeout,
    ))
}

/// Sends a reply as a new prompt: queued after the turn under way.
pub fn send(text: &str, session: &str, server: &Instance, token: &str) -> bool {
    ok(call(
        server,
        token,
        "POST",
        &format!("/api/v1/sessions/{}/prompts", escaped(session)),
        Some(&prompt(text)),
        Duration::from_secs(4),
    ))
}

fn ok(reply: Option<Value>) -> bool {
    reply.is_some_and(|r| r["code"].as_i64() == Some(0))
}

/// One request to a loopback server, over HTTP/1.1, closed after it.
fn call(
    server: &Instance,
    token: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Option<Value> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    if !server.is_local() {
        return None;
    }
    let address = server.address().to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&address, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {token}\r\nAccept: application/json\r\nConnection: close\r\n",
        server.address()
    );
    if method != "GET" {
        request.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    request.push_str("\r\n");
    request.push_str(&body);
    stream.write_all(request.as_bytes()).ok()?;
    let mut data = Vec::new();
    // A server that keeps the connection open past its answer is read until the timeout.
    let _ = stream.read_to_end(&mut data);
    response_body(&data).and_then(|b| serde_json::from_slice(&b).ok())
}

/// The body of an HTTP/1.1 response, joined back when it came in chunks.
pub fn response_body(data: &[u8]) -> Option<Vec<u8>> {
    let split = data.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&data[..split]).to_ascii_lowercase();
    let mut body = &data[split + 4..];
    if !head.contains("transfer-encoding: chunked") {
        if let Some(length) = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            body = &body[..length.min(body.len())];
        }
        return Some(body.to_vec());
    }
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n")?;
        let size_text = String::from_utf8_lossy(&body[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        if body.len() < size {
            return None;
        }
        out.extend_from_slice(&body[..size]);
        body = body.get(size + 2..).unwrap_or(&[]);
    }
}

#[cfg(test)]
mod tests;
