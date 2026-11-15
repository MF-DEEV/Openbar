// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Mistral Vibe's terminal UI talks to its own `vibe-app-server` over a pipe
//! (JSON-RPC, one message a line). A session started from New session runs
//! that server through this proxy: the lines pass in both directions as they
//! are, and a `callback/call` that asks the client for an approval or an
//! answer is also told to the island, which can answer it with a
//! `callback/result` of its own. The first answer wins, as in Codex: a late
//! one comes back from the server as a conflict and is dropped. With the
//! island not running, or nothing answering, the terminal asks as before.
//!
//!     Openbar --vibe-proxy <app> <vibe-app-server> [arguments]
//!
//! (VibeAppServerProxy.swift is the same on a Mac.)

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use interprocess::local_socket::{GenericFilePath, Stream, ToFsName, prelude::*};
use serde_json::{Map, Value, json};

use crate::hooks::transport;

pub const FLAG: &str = "--vibe-proxy";

pub fn is_requested(arguments: &[String]) -> bool {
    arguments.get(1).map(String::as_str) == Some(FLAG)
}

// MARK: Wire

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackKind {
    Approval,
    Question,
}

impl CallbackKind {
    pub fn key(self) -> &'static str {
        match self {
            CallbackKind::Approval => "approval",
            CallbackKind::Question => "question",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VibeQuestion {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// What the island is asked about: one approval or question of the session.
#[derive(Clone, Debug, PartialEq)]
pub struct VibeCallback {
    pub id: String,
    pub session: String,
    pub kind: CallbackKind,
    /// Vibe's own name for the tool, as `file_system.bash`.
    pub tool: String,
    pub input: BTreeMap<String, String>,
    /// Whether "for this session" is one of the choices the server offers.
    pub offers_session: bool,
    pub questions: Vec<VibeQuestion>,
}

fn contains(data: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty() && data.windows(needle.len()).any(|w| w == needle)
}

fn object(line: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(line).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// The scalar members of an object, as text; what a person can read in a card.
fn strings(value: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(Value::Object(map)) = value {
        for (key, item) in map {
            match item {
                Value::String(text) => {
                    out.insert(key.clone(), text.clone());
                }
                Value::Number(number) => {
                    out.insert(key.clone(), number.to_string());
                }
                _ => {}
            }
        }
    }
    out
}

fn text<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    map.get(key).and_then(Value::as_str)
}

/// The approval or question a `callback/call` carries; other callbacks (a
/// connector's sign-in) are the terminal's.
pub fn callback_in_server_line(line: &[u8]) -> Option<VibeCallback> {
    if !contains(line, "\"callback/call\"") {
        return None;
    }
    let json = object(line)?;
    if text(&json, "method") != Some("callback/call") {
        return None;
    }
    let callback = json.get("params")?.get("callback")?.as_object()?;
    let detail = callback.get("detail")?.as_object()?;
    let id = text(callback, "callbackId")
        .or_else(|| text(callback, "id"))
        .filter(|s| !s.is_empty())?;
    let session = text(callback, "sessionId").filter(|s| !s.is_empty())?;
    match text(detail, "kind")? {
        "approval" => {
            let effect = detail.get("effect").and_then(Value::as_object);
            let choices: Vec<&str> = detail
                .get("choices")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            // An approval with no choice to give is the terminal's.
            if !choices.is_empty() && !choices.contains(&"approve") {
                return None;
            }
            Some(VibeCallback {
                id: id.to_owned(),
                session: session.to_owned(),
                kind: CallbackKind::Approval,
                tool: effect
                    .and_then(|e| text(e, "toolName"))
                    .unwrap_or_default()
                    .to_owned(),
                input: strings(effect.and_then(|e| e.get("input"))),
                offers_session: choices.contains(&"approve_for_session"),
                questions: Vec::new(),
            })
        }
        "user_input" => {
            let questions: Vec<VibeQuestion> = detail
                .get("request")
                .and_then(|r| r.get("questions"))
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|item| {
                            let item = item.as_object()?;
                            let question = text(item, "question").filter(|s| !s.is_empty())?;
                            let options = item
                                .get("options")
                                .and_then(Value::as_array)
                                .map(|options| {
                                    options
                                        .iter()
                                        .filter_map(|option| {
                                            let option = option.as_object()?;
                                            let label =
                                                text(option, "label").filter(|s| !s.is_empty())?;
                                            Some(QuestionOption {
                                                label: label.to_owned(),
                                                description: text(option, "description")
                                                    .unwrap_or_default()
                                                    .to_owned(),
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            Some(VibeQuestion {
                                question: question.to_owned(),
                                header: text(item, "header").unwrap_or_default().to_owned(),
                                options,
                                multi_select: item
                                    .get("multiSelect")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            if questions.is_empty() {
                return None;
            }
            Some(VibeCallback {
                id: id.to_owned(),
                session: session.to_owned(),
                kind: CallbackKind::Question,
                tool: String::new(),
                input: BTreeMap::new(),
                offers_session: false,
                questions,
            })
        }
        _ => None,
    }
}

/// The callback a server line says is no longer open: answered, cancelled or
/// expired, by the terminal or anywhere else.
pub fn settled_callback(line: &[u8]) -> Option<String> {
    if !contains(line, "\"history/entryUpdated\"") {
        return None;
    }
    let json = object(line)?;
    if text(&json, "method") != Some("history/entryUpdated") {
        return None;
    }
    let params = json.get("params")?.as_object()?;
    let entry = text(params, "entryId")?;
    for change in params.get("patch")?.as_array()? {
        if change.get("path").and_then(Value::as_str) != Some("/state") {
            continue;
        }
        let status = change
            .get("value")
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str);
        if status.is_some_and(|s| s != "open") {
            return Some(entry.to_owned());
        }
    }
    None
}

/// A session the server speaks of, with its folder when the line has it.
pub fn session_in_server_line(line: &[u8]) -> Option<(String, Option<String>)> {
    if !contains(line, "\"sessionId\"") && !contains(line, "\"session\":") {
        return None;
    }
    let json = object(line)?;
    if let Some(id) = json
        .get("params")
        .and_then(|p| p.get("sessionId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return Some((id.to_owned(), None));
    }
    let session = json
        .get("result")?
        .get("state")?
        .get("session")?
        .as_object()?;
    let id = text(session, "id").filter(|s| !s.is_empty())?;
    Some((id.to_owned(), text(session, "cwd").map(str::to_owned)))
}

/// The callback a terminal line answers.
pub fn answered_callback(line: &[u8]) -> Option<String> {
    if !contains(line, "\"callback/result\"") {
        return None;
    }
    let json = object(line)?;
    if text(&json, "method") != Some("callback/result") {
        return None;
    }
    json.get("params")?
        .get("result")?
        .get("callbackId")?
        .as_str()
        .map(str::to_owned)
}

/// The `callback/result` request for the island's reply:
/// `{"decision": "approve" | "approve_for_session" | "deny", "feedback": "…"}`
/// for an approval, `{"answers": [{"question", "answer", "isOther"}]}` or
/// `{"cancelled": true}` for questions.
pub fn result(id: &str, callback: &VibeCallback, reply: &Value) -> Option<Vec<u8>> {
    let output = match callback.kind {
        CallbackKind::Approval => {
            let decision = reply.get("decision")?.as_str()?;
            if !["approve", "approve_for_session", "deny"].contains(&decision)
                || (decision == "approve_for_session" && !callback.offers_session)
            {
                return None;
            }
            let mut body = json!({ "type": "approval", "decision": { "type": decision } });
            if let Some(feedback) = reply
                .get("feedback")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                body["feedback"] = json!(feedback);
            }
            body
        }
        CallbackKind::Question => {
            if reply.get("cancelled") == Some(&Value::Bool(true)) {
                json!({ "type": "user_input", "result": { "answers": [], "cancelled": true } })
            } else {
                let answers = reply.get("answers")?.as_array()?;
                if answers.is_empty() {
                    return None;
                }
                let known: HashSet<&str> = callback
                    .questions
                    .iter()
                    .map(|q| q.question.as_str())
                    .collect();
                let mut kept = Vec::new();
                for answer in answers {
                    let question = answer.get("question")?.as_str()?;
                    let given = answer.get("answer")?.as_str()?;
                    if !known.contains(question) {
                        return None;
                    }
                    kept.push(json!({
                        "question": question,
                        "answer": given,
                        "isOther": answer.get("isOther").and_then(Value::as_bool).unwrap_or(false),
                    }));
                }
                json!({ "type": "user_input", "result": { "answers": kept, "cancelled": false } })
            }
        }
    };
    serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "callback/result",
        "params": {
            "sessionId": callback.session,
            "result": { "callbackId": callback.id, "output": output },
        },
    }))
    .ok()
}

/// The envelope for one of the proxy's messages (`hook` is `proxy`).
pub fn envelope(event: &Value, pid: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "openbar": "vibe",
        "hook": "proxy",
        "pid": pid,
        "event": event,
    }))
    .unwrap_or_default()
}

pub fn event_for(callback: &VibeCallback) -> Value {
    let mut event = json!({
        "kind": callback.kind.key(),
        "session": callback.session,
        "request": callback.id,
    });
    match callback.kind {
        CallbackKind::Approval => {
            event["tool"] = json!(callback.tool);
            event["input"] = json!(callback.input);
            event["always"] = json!(callback.offers_session);
        }
        CallbackKind::Question => {
            event["questions"] = Value::Array(
                callback
                    .questions
                    .iter()
                    .map(|q| {
                        json!({
                            "question": q.question,
                            "header": q.header,
                            "multiSelect": q.multi_select,
                            "options": q.options.iter()
                                .map(|o| json!({ "label": o.label, "description": o.description }))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
            );
        }
    }
    event
}

// MARK: Launching

/// Vibe's Python and its `vibe-app-server`.
#[derive(Clone, Debug, PartialEq)]
pub struct Install {
    pub python: PathBuf,
    pub server: PathBuf,
}

/// The code the Python runs: the first argument is the server's command as
/// JSON, the rest is what `vibe` is given.
pub const CODE: &str = "import json,sys;from vibe.cli.launcher import exec_rust_cli;exec_rust_cli(sys.argv[2:],app_server_cmd=json.loads(sys.argv[1]))";

/// The interpreter a console script names: uv's and pipx's start with
/// `#!/bin/sh` and a line `'''exec' '<python>' "$0" "$@"`, others with the
/// interpreter as the shebang.
pub fn python_in_script(text: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for line in text.lines().take(4) {
        if let Some(path) = line.strip_prefix("#!") {
            let path = path.trim();
            if !path.ends_with("/sh") && !path.ends_with("/bash") && !path.contains(' ') {
                found.push(PathBuf::from(path));
            }
        } else if line.starts_with("'''exec'")
            && let Some(path) = line
                .split('\'')
                .find(|p| p.starts_with('/') && p.contains("python"))
        {
            found.push(PathBuf::from(path));
        }
    }
    found
}

/// Where Vibe's Python and server are, from the `vibe` found on the path and
/// the folders uv puts its tools in. `exists` and `read` look at files.
pub fn install(
    vibe: &Path,
    tool_folders: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
    read: impl Fn(&Path) -> Option<String>,
) -> Option<Install> {
    let real = std::fs::canonicalize(vibe).unwrap_or_else(|_| vibe.to_path_buf());
    let mut folders: Vec<PathBuf> = real.parent().map(Path::to_path_buf).into_iter().collect();
    for tools in tool_folders {
        let tool = tools.join("mistral-vibe");
        folders.push(tool.join("bin"));
        folders.push(tool.join("Scripts"));
    }
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    for folder in folders {
        let server = folder.join(format!("vibe-app-server{suffix}"));
        if !exists(&server) {
            continue;
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        for script in ["vibe", "vibe.exe"] {
            if let Some(text) = read(&folder.join(script)) {
                candidates.extend(python_in_script(&text));
            }
        }
        candidates.extend(
            ["python", "python3"]
                .iter()
                .map(|name| folder.join(format!("{name}{suffix}"))),
        );
        if let Some(python) = candidates.into_iter().find(|p| exists(p)) {
            return Some(Install { python, server });
        }
    }
    None
}

/// uv's tool folders on this system, as `uv tool dir` would say.
pub fn tool_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(dir) = std::env::var_os("UV_TOOL_DIR").filter(|d| !d.is_empty()) {
        folders.push(PathBuf::from(dir));
    }
    #[cfg(windows)]
    if let Some(data) = std::env::var_os("APPDATA") {
        folders.push(PathBuf::from(data).join("uv").join("tools"));
    }
    #[cfg(not(windows))]
    {
        if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
            folders.push(PathBuf::from(data).join("uv").join("tools"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("uv")
                    .join("tools"),
            );
        }
    }
    folders
}

/// The arguments the terminal runs Vibe's Python with: the terminal UI
/// starts with this program as its server, then comes the first prompt, as
/// `vibe` takes it.
pub fn start_arguments(install: &Install, proxy: &Path, app: &str, prompt: &str) -> Vec<String> {
    let command = json!([
        proxy.to_string_lossy(),
        FLAG,
        app,
        install.server.to_string_lossy(),
    ]);
    let mut arguments = vec![
        "-I".to_owned(),
        "-c".to_owned(),
        CODE.to_owned(),
        command.to_string(),
    ];
    let prompt = prompt.trim();
    if !prompt.is_empty() {
        arguments.push(prompt.to_owned());
    }
    arguments
}

/// What a terminal runs for a New session of Vibe: its Python, started so
/// that the terminal UI's server is this program. None where Vibe is not set
/// up as this expects: the plain `vibe` runs and its hooks report as before.
pub fn launch(vibe: &Path, app: &str, prompt: &str) -> Option<(PathBuf, Vec<String>)> {
    let proxy = std::env::current_exe().ok()?;
    let found = install(
        vibe,
        &tool_folders(),
        |path| path.is_file(),
        |path| {
            std::fs::read(path)
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        },
    )?;
    let arguments = start_arguments(&found, &proxy, app, prompt);
    Some((found.python, arguments))
}

// MARK: The proxy process

struct Shared {
    /// What the island is asked, by callback.
    waiting: Mutex<HashMap<String, VibeCallback>>,
    told: Mutex<HashSet<String>>,
    counter: Mutex<u64>,
    to_server: Mutex<Box<dyn Write + Send>>,
    app: String,
}

/// Runs the server and relays; the exit code is the server's.
pub fn run(app: &str, server: &str, arguments: &[String]) -> i32 {
    let mut command = Command::new(server);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The server is a console program: no window of its own beside the terminal's.
        command.creation_flags(0x0800_0000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("vibe proxy: cannot start {server}: {error}");
            return 127;
        }
    };
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return 127;
    };
    let shared = Arc::new(Shared {
        waiting: Mutex::new(HashMap::new()),
        told: Mutex::new(HashSet::new()),
        counter: Mutex::new(0),
        to_server: Mutex::new(Box::new(stdin)),
        app: app.to_owned(),
    });
    {
        let shared = shared.clone();
        std::thread::spawn(move || read_client(&shared));
    }
    read_server(&shared, stdout);
    child.wait().ok().and_then(|s| s.code()).unwrap_or(1)
}

fn send(shared: &Shared, line: &[u8]) {
    let mut out = shared.to_server.lock().unwrap();
    let _ = out.write_all(line);
    if !line.ends_with(b"\n") {
        let _ = out.write_all(b"\n");
    }
    let _ = out.flush();
}

/// The terminal's lines to the server, until the terminal closes its end.
fn read_client(shared: &Arc<Shared>) {
    let mut input = BufReader::new(io::stdin().lock());
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                send(shared, &line);
                if let Some(answered) = answered_callback(&line) {
                    settle(shared, &answered);
                }
            }
        }
    }
    // The server ends when its input does.
    *shared.to_server.lock().unwrap() = Box::new(io::sink());
}

/// The server's lines to the terminal, as they come.
fn read_server(shared: &Arc<Shared>, stdout: impl Read) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    let mut out = io::stdout();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let _ = out.write_all(&line);
                let _ = out.flush();
                inspect(shared, &line);
            }
        }
    }
}

fn inspect(shared: &Arc<Shared>, line: &[u8]) {
    if let Some(settled) = settled_callback(line) {
        settle(shared, &settled);
    }
    if let Some(callback) = callback_in_server_line(line) {
        let shared = shared.clone();
        std::thread::spawn(move || ask(&shared, callback));
    } else if let Some((session, cwd)) = session_in_server_line(line)
        && shared.told.lock().unwrap().insert(session.clone())
    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            let mut event = json!({ "kind": "attach", "session": session });
            if let Some(cwd) = cwd {
                event["cwd"] = json!(cwd);
            }
            notify(&shared, &event);
        });
    }
}

fn connect(app: &str) -> Option<Stream> {
    let name = transport::endpoint(app)
        .to_fs_name::<GenericFilePath>()
        .ok()?;
    for _ in 0..10 {
        match Stream::connect(name.borrow()) {
            Ok(stream) => return Some(stream),
            // A busy pipe (all instances taken) is retried briefly; a missing one is not.
            Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(231) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
    None
}

/// Tells the island of a callback and waits for its answer, which goes to the
/// server as a `callback/result`. Nothing answered: the terminal's.
fn ask(shared: &Arc<Shared>, callback: VibeCallback) {
    let Some(mut stream) = connect(&shared.app) else {
        return;
    };
    {
        let mut waiting = shared.waiting.lock().unwrap();
        if waiting.contains_key(&callback.id) {
            return;
        }
        waiting.insert(callback.id.clone(), callback.clone());
    }
    let message = envelope(&event_for(&callback), std::process::id());
    let sent = stream.write_all(&message).is_ok() && stream.flush().is_ok();
    let mut reply = Vec::new();
    if sent {
        // The island closes the connection when it has answered, or when the request is settled.
        let _ = stream.read_to_end(&mut reply);
    }
    let open = shared
        .waiting
        .lock()
        .unwrap()
        .remove(&callback.id)
        .is_some();
    if !open || reply.iter().all(u8::is_ascii_whitespace) {
        return;
    }
    let Ok(reply) = serde_json::from_slice::<Value>(&reply) else {
        return;
    };
    let id = {
        let mut counter = shared.counter.lock().unwrap();
        *counter += 1;
        format!("openbar-{counter}")
    };
    if let Some(request) = result(&id, &callback, &reply) {
        send(shared, &request);
    }
}

/// The callback was answered elsewhere: the island is told, and takes the card down.
fn settle(shared: &Arc<Shared>, id: &str) {
    let Some(callback) = shared.waiting.lock().unwrap().remove(id) else {
        return;
    };
    let shared = shared.clone();
    std::thread::spawn(move || {
        notify(
            &shared,
            &json!({ "kind": "settled", "session": callback.session, "request": callback.id }),
        );
    });
}

/// One message the island answers nothing to.
fn notify(shared: &Shared, event: &Value) {
    if let Some(mut stream) = connect(&shared.app) {
        let _ = stream.write_all(&envelope(event, std::process::id()));
        let _ = stream.flush();
    }
}

/// `Openbar --vibe-proxy <app> <vibe-app-server> [arguments]`.
pub fn main(arguments: &[String]) -> ! {
    if arguments.len() < 4 {
        eprintln!("usage: {FLAG} <app> <vibe-app-server> [arguments]");
        std::process::exit(64);
    }
    std::process::exit(run(&arguments[2], &arguments[3], &arguments[4..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // As Vibe 3.1.2 sent them (trimmed of what the proxy does not read).
    const APPROVAL: &str = r#"{"jsonrpc":"2.0","id":1,"method":"callback/call","params":{"callback":{"id":"approval-call_1-da12e44f","sessionId":"6267026e-5700-b9cb-3d47-f84eb25621f1","turnId":"turn-aee2","type":"callback","callbackId":"approval-call_1-da12e44f","title":"Approve file_system.bash","detail":{"kind":"approval","effect":{"toolName":"file_system.bash","display":{"summary":"bash: touch approved.txt","message":"touch approved.txt"},"kind":"shell","input":{"command":"touch approved.txt"}},"choices":["approve","approve_for_session","approve_permanently","deny","cancel_turn"],"pathScopeChoices":[],"reason":null},"state":{"status":"open"}}}}"#;
    const QUESTION: &str = r#"{"jsonrpc":"2.0","id":2,"method":"callback/call","params":{"callback":{"id":"user-input-call_q-716120f1","sessionId":"30f956f5","type":"callback","callbackId":"user-input-call_q-716120f1","title":"Question","detail":{"kind":"user_input","request":{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multiSelect":false,"hideOther":false}],"footerNote":null}},"state":{"status":"open"}}}}"#;
    const SESSION: &str = "6267026e-5700-b9cb-3d47-f84eb25621f1";

    #[test]
    fn callbacks_are_read() {
        let asked = callback_in_server_line(APPROVAL.as_bytes()).unwrap();
        assert_eq!(asked.kind, CallbackKind::Approval);
        assert_eq!(asked.id, "approval-call_1-da12e44f");
        assert_eq!(asked.session, SESSION);
        assert_eq!(asked.tool, "file_system.bash");
        assert_eq!(asked.input["command"], "touch approved.txt");
        assert!(asked.offers_session);
        let input = callback_in_server_line(QUESTION.as_bytes()).unwrap();
        assert_eq!(input.kind, CallbackKind::Question);
        assert_eq!(input.questions[0].question, "Which color?");
        assert_eq!(
            input.questions[0]
                .options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            ["Red", "Blue"]
        );
        assert!(!input.questions[0].multi_select);
        // Other callbacks, and other lines, are the terminal's.
        let connector = APPROVAL.replace(
            r#""kind":"approval","effect""#,
            r#""kind":"connector_auth","effect""#,
        );
        assert!(callback_in_server_line(connector.as_bytes()).is_none());
        let no_approve = APPROVAL.replace(
            r#""choices":["approve","approve_for_session","approve_permanently","deny","cancel_turn"]"#,
            r#""choices":["deny","cancel_turn"]"#,
        );
        assert!(callback_in_server_line(no_approve.as_bytes()).is_none());
        let once = APPROVAL.replace(r#""approve_for_session","#, "");
        assert!(
            !callback_in_server_line(once.as_bytes())
                .unwrap()
                .offers_session
        );
        assert!(
            callback_in_server_line(
                br#"{"jsonrpc":"2.0","method":"session/updated","params":{"sessionId":"x"}}"#
            )
            .is_none()
        );
        assert!(callback_in_server_line(b"not json callback/call").is_none());
        assert!(callback_in_server_line(b"").is_none());
        let no_session =
            APPROVAL.replace(r#""sessionId":"6267026e-5700-b9cb-3d47-f84eb25621f1","#, "");
        assert!(callback_in_server_line(no_session.as_bytes()).is_none());
    }

    #[test]
    fn a_callback_settled_anywhere_is_told() {
        let answered = r#"{"jsonrpc":"2.0","method":"history/entryUpdated","params":{"eventId":12,"sessionId":"s","turnId":"t","entryId":"approval-call_1-da12e44f","patch":[{"op":"replace","path":"/generationStatus","value":"completed"},{"op":"replace","path":"/state","value":{"status":"answered","output":{"type":"approval","decision":{"type":"approve"}}}}]}}"#;
        assert_eq!(
            settled_callback(answered.as_bytes()).as_deref(),
            Some("approval-call_1-da12e44f")
        );
        let cancelled = answered.replace(r#""status":"answered""#, r#""status":"cancelled""#);
        assert!(settled_callback(cancelled.as_bytes()).is_some());
        let open = answered.replace(r#""status":"answered""#, r#""status":"open""#);
        assert!(settled_callback(open.as_bytes()).is_none());
        let effect = r#"{"jsonrpc":"2.0","method":"history/entryUpdated","params":{"entryId":"effect-dc76","patch":[{"op":"replace","path":"/generationStatus","value":"completed"},{"op":"add","path":"/state/decision","value":"execute"}]}}"#;
        assert!(settled_callback(effect.as_bytes()).is_none());
        let terminal = r#"{"jsonrpc":"2.0","id":17,"method":"callback/result","params":{"sessionId":"s","result":{"callbackId":"approval-call_1-da12e44f","output":{"type":"approval","decision":{"type":"approve"},"feedback":null}}}}"#;
        assert_eq!(
            answered_callback(terminal.as_bytes()).as_deref(),
            Some("approval-call_1-da12e44f")
        );
        assert!(
            answered_callback(br#"{"jsonrpc":"2.0","id":3,"method":"turn/start","params":{}}"#)
                .is_none()
        );
        let started = br#"{"jsonrpc":"2.0","id":3,"result":{"lastEventId":0,"state":{"session":{"id":"abc","cwd":"/w/app","status":{"type":"idle"}}}}}"#;
        assert_eq!(
            session_in_server_line(started),
            Some(("abc".to_owned(), Some("/w/app".to_owned())))
        );
        assert_eq!(
            session_in_server_line(
                br#"{"jsonrpc":"2.0","method":"turn/started","params":{"sessionId":"def"}}"#
            ),
            Some(("def".to_owned(), None))
        );
    }

    fn output(request: &Option<Vec<u8>>) -> Value {
        let request: Value = serde_json::from_slice(request.as_ref().unwrap()).unwrap();
        request["params"]["result"]["output"].clone()
    }

    #[test]
    fn the_islands_answer_goes_back_as_a_callback_result() {
        let callback = callback_in_server_line(APPROVAL.as_bytes()).unwrap();
        let approve = result("openbar-1", &callback, &json!({ "decision": "approve" }));
        let request: Value = serde_json::from_slice(approve.as_ref().unwrap()).unwrap();
        assert_eq!(request["method"], "callback/result");
        assert_eq!(request["id"], "openbar-1");
        assert_eq!(request["params"]["sessionId"], SESSION);
        assert_eq!(
            request["params"]["result"]["callbackId"],
            "approval-call_1-da12e44f"
        );
        assert_eq!(request["params"]["result"]["output"]["type"], "approval");
        assert_eq!(
            request["params"]["result"]["output"]["decision"]["type"],
            "approve"
        );
        let always = result(
            "x",
            &callback,
            &json!({ "decision": "approve_for_session" }),
        );
        assert_eq!(output(&always)["decision"]["type"], "approve_for_session");
        let deny = result(
            "x",
            &callback,
            &json!({ "decision": "deny", "feedback": "Denied from the Dynamic Island." }),
        );
        assert_eq!(output(&deny)["decision"]["type"], "deny");
        assert_eq!(output(&deny)["feedback"], "Denied from the Dynamic Island.");
        assert!(
            result(
                "x",
                &callback,
                &json!({ "decision": "approve_permanently" })
            )
            .is_none()
        );
        assert!(result("x", &callback, &json!({ "decision": "maybe" })).is_none());
        assert!(result("x", &callback, &json!({})).is_none());
        let once =
            callback_in_server_line(APPROVAL.replace(r#""approve_for_session","#, "").as_bytes())
                .unwrap();
        assert!(result("x", &once, &json!({ "decision": "approve_for_session" })).is_none());

        let asked = callback_in_server_line(QUESTION.as_bytes()).unwrap();
        let picked = result(
            "x",
            &asked,
            &json!({ "answers": [{ "question": "Which color?", "answer": "Red", "isOther": false }] }),
        );
        assert_eq!(output(&picked)["result"]["answers"][0]["answer"], "Red");
        assert_eq!(output(&picked)["result"]["cancelled"], false);
        assert_eq!(
            output(&result("x", &asked, &json!({ "cancelled": true })))["result"]["cancelled"],
            true
        );
        assert!(
            result(
                "x",
                &asked,
                &json!({ "answers": [{ "question": "Another?", "answer": "Red" }] })
            )
            .is_none()
        );
        assert!(result("x", &asked, &json!({ "decision": "approve" })).is_none());
        assert!(result("x", &asked, &json!({ "answers": [] })).is_none());
    }

    #[test]
    fn what_the_island_is_told_names_the_request() {
        let callback = callback_in_server_line(APPROVAL.as_bytes()).unwrap();
        let sent: Value = serde_json::from_slice(&envelope(&event_for(&callback), 7)).unwrap();
        assert_eq!(sent["openbar"], "vibe");
        assert_eq!(sent["hook"], "proxy");
        assert_eq!(sent["event"]["kind"], "approval");
        assert_eq!(sent["event"]["request"], "approval-call_1-da12e44f");
        assert_eq!(sent["event"]["input"]["command"], "touch approved.txt");
        assert_eq!(sent["event"]["always"], true);
    }

    #[test]
    fn vibes_python_and_server_are_found_beside_its_console_script() {
        let uv = "#!/bin/sh\n'''exec' '/Users/me/.local/share/uv/tools/mistral-vibe/bin/python' \"$0\" \"$@\"\n' '''\n# -*- coding: utf-8 -*-\n";
        assert_eq!(
            python_in_script(uv),
            vec![PathBuf::from(
                "/Users/me/.local/share/uv/tools/mistral-vibe/bin/python"
            )]
        );
        assert_eq!(
            python_in_script("#!/usr/bin/python3.13\nimport sys\n"),
            vec![PathBuf::from("/usr/bin/python3.13")]
        );
        assert!(python_in_script("#!/bin/sh\necho hi\n").is_empty());
        assert!(python_in_script("").is_empty());

        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let tool = PathBuf::from("/vibe-tools/mistral-vibe/bin");
        let present: Vec<PathBuf> = [
            format!("vibe-app-server{suffix}"),
            format!("python{suffix}"),
        ]
        .iter()
        .map(|name| tool.join(name))
        .collect();
        let found = install(
            Path::new("/nowhere/vibe"),
            &[PathBuf::from("/vibe-tools")],
            |p| present.contains(&p.to_path_buf()),
            |_| None,
        )
        .unwrap();
        assert_eq!(found.python, tool.join(format!("python{suffix}")));
        assert_eq!(found.server, tool.join(format!("vibe-app-server{suffix}")));
        // With no server, or no Python, there is nothing to run the proxy with: plain vibe runs.
        assert!(
            install(
                Path::new("/nowhere/vibe"),
                &[PathBuf::from("/vibe-tools")],
                |p| p.ends_with(format!("python{suffix}")),
                |_| None
            )
            .is_none()
        );
        assert!(
            install(
                Path::new("/nowhere/vibe"),
                &[PathBuf::from("/vibe-tools")],
                |p| p.ends_with(format!("vibe-app-server{suffix}")),
                |_| None
            )
            .is_none()
        );

        let arguments = start_arguments(
            &found,
            Path::new("C:/Program Files/Openbar/Openbar.exe"),
            "openbar",
            "  it's a prompt ",
        );
        assert_eq!(&arguments[..3], ["-I", "-c", CODE]);
        let command: Value = serde_json::from_str(&arguments[3]).unwrap();
        assert_eq!(command[0], "C:/Program Files/Openbar/Openbar.exe");
        assert_eq!(command[1], FLAG);
        assert_eq!(command[2], "openbar");
        assert_eq!(arguments[4], "it's a prompt");
        assert_eq!(
            start_arguments(&found, Path::new("/p/Openbar"), "openbar", " ").len(),
            4
        );
        assert!(is_requested(&["Openbar".into(), "--vibe-proxy".into()]));
        assert!(!is_requested(&["Openbar".into(), "--hook".into()]));
        assert!(!is_requested(&["Openbar".into()]));
    }
}
