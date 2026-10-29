// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's connection to Codex's app-server daemon (spec 07 §3): a
//! WebSocket over the daemon's user-only AF_UNIX socket, speaking its
//! JSON-RPC dialect. It never starts the daemon; while there is none it
//! looks again every few seconds. It joins a thread (`thread/resume`) only
//! while the thread works or waits, so a closed terminal's thread can unload.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tungstenite::Message;

use super::state::{Answer, State};
use crate::sessions::models::{ContentPolicy, Session, SessionEvent, Time};

const RETRY: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

/// Streamed text and output the island never shows, asked not to be sent
/// (`capabilities.optOutNotificationMethods`): a turn streams them by the
/// token. Names as Codex 0.163 and 0.164 send them.
pub const QUIET_NOTIFICATIONS: [&str; 10] = [
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/reasoning/textDelta",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/summaryPartAdded",
    "item/commandExecution/outputDelta",
    "item/fileChange/outputDelta",
    "thread/realtime/item/transcript/delta",
    "thread/realtime/transcript/delta",
    "thread/realtime/outputAudio/delta",
];

/// `initialize`'s params: who the island is, and what it would rather not hear.
fn initialize_params() -> Value {
    json!({
        "clientInfo": {"name": "vorssaint", "title": "Vorssaint", "version": env!("CARGO_PKG_VERSION")},
        "capabilities": {"experimentalApi": true, "optOutNotificationMethods": QUIET_NOTIFICATIONS}
    })
}

/// `%CODEX_HOME%`, or `%USERPROFILE%\.codex`.
pub fn codex_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(home));
    }
    crate::hooks::installer::home_dir().map(|h| h.join(".codex"))
}

fn socket_path() -> Option<PathBuf> {
    Some(
        codex_home()?
            .join("app-server-control")
            .join("app-server-control.sock"),
    )
}

/// What a response answers.
enum Waiting {
    Initialize,
    LoadedList,
    Read,
    Resume,
    /// The newest turn of a thread joined while it worked (`running_turn`).
    Turns(String),
    Other,
}

struct Shared {
    state: State,
    next_id: i64,
    waiting: HashMap<i64, Waiting>,
    outbox: Option<Sender<Value>>,
    /// Session pages open in the island, kept joined while open.
    focused: HashSet<String>,
    connected: bool,
    version: Option<String>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            state: State::default(),
            next_id: 0,
            waiting: HashMap::new(),
            outbox: None,
            focused: HashSet::new(),
            connected: false,
            version: None,
        }
    }

    /// The daemon went away: nothing it said holds any more.
    fn reset(&mut self) {
        self.connected = false;
        self.outbox = None;
        self.waiting.clear();
        self.focused.clear();
        self.state = State::default();
    }

    fn send(&mut self, message: Value) {
        if let Some(outbox) = &self.outbox {
            let _ = outbox.send(message);
        }
    }

    fn call(&mut self, method: &str, params: Value, waiting: Waiting) {
        self.next_id += 1;
        let id = self.next_id;
        self.waiting.insert(id, waiting);
        self.send(json!({"id": id, "method": method, "params": params}));
    }

    /// Joins threads that work or wait, and leaves the quiet ones nobody
    /// looks at. Codex's helper threads are never joined, nor a thread
    /// before a read says what it is.
    fn follow(&mut self) {
        let mut join = Vec::new();
        let mut leave = Vec::new();
        for (id, thread) in &self.state.threads {
            let active =
                thread.status.as_ref().is_some_and(|s| s.is_active()) || !thread.pending.is_empty();
            let wanted =
                thread.described && !thread.ephemeral && (active || self.focused.contains(id));
            if wanted && !thread.subscribed {
                join.push(id.clone());
            } else if !wanted && thread.subscribed {
                leave.push(id.clone());
            }
        }
        for id in join {
            if let Some(t) = self.state.threads.get_mut(&id) {
                t.subscribed = true;
            }
            // No overrides: with them Codex would change the person's thread.
            self.call(
                "thread/resume",
                json!({"threadId": id, "excludeTurns": !self.state.previews}),
                Waiting::Resume,
            );
        }
        for id in leave {
            if let Some(t) = self.state.threads.get_mut(&id) {
                t.subscribed = false;
            }
            self.call(
                "thread/unsubscribe",
                json!({"threadId": id}),
                Waiting::Other,
            );
        }
    }
}

pub struct CodexClient {
    shared: Arc<Mutex<Shared>>,
}

impl CodexClient {
    /// Starts looking for the daemon; `changed` runs whenever sessions may have changed.
    pub fn start(changed: impl Fn() + Send + Sync + 'static) -> Arc<CodexClient> {
        let client = Arc::new(CodexClient {
            shared: Arc::new(Mutex::new(Shared::new())),
        });
        let shared = client.shared.clone();
        let changed = Arc::new(changed);
        thread::Builder::new()
            .name("codex-app-server".into())
            .spawn(move || {
                loop {
                    if let Some(path) = socket_path().filter(|p| p.exists()) {
                        let _ = session(&path, &shared, &*changed);
                        shared.lock().unwrap().reset();
                        changed();
                    }
                    thread::sleep(RETRY);
                }
            })
            .expect("codex thread");
        client
    }

    pub fn snapshot(&self, now: Time, policy: ContentPolicy) -> (Vec<Session>, Vec<SessionEvent>) {
        let mut shared = self.shared.lock().unwrap();
        if shared.state.previews != policy.previews {
            shared.state.previews = policy.previews;
            if policy.previews {
                let ids: Vec<_> = shared.state.threads.keys().cloned().collect();
                for id in ids {
                    shared.call(
                        "thread/read",
                        json!({"threadId": id, "includeTurns": true}),
                        Waiting::Read,
                    );
                }
            } else {
                for thread in shared.state.threads.values_mut() {
                    thread.chat.clear();
                }
            }
        }
        shared.state.snapshot(now, policy)
    }

    pub fn owns(&self, session: &str) -> bool {
        self.shared
            .lock()
            .unwrap()
            .state
            .threads
            .contains_key(session)
    }

    pub fn acknowledge(&self, session: &str, now: Time) {
        self.shared.lock().unwrap().state.acknowledge(session, now);
    }

    /// The name Codex gives the thread, whatever the previews: Open looks
    /// for it in terminal titles, where Codex writes it, and never shows it.
    pub fn thread_name(&self, session: &str) -> Option<String> {
        self.shared.lock().unwrap().thread_name(session)
    }

    /// Keeps a thread joined while its page is open, so its tools and turns show.
    pub fn focus(&self, session: Option<&str>) {
        let mut s = self.shared.lock().unwrap();
        s.focused = session.into_iter().map(str::to_owned).collect();
        s.follow();
    }

    /// Answers the request with this island id, the session's own or one of
    /// its subagents'; false when it is gone or of another kind.
    pub fn answer(&self, session: &str, request_id: &str, answer: Answer) -> bool {
        self.shared
            .lock()
            .unwrap()
            .answer(session, request_id, &answer)
    }

    /// A prompt for an idle thread, or one queued after the current turn of a working one.
    pub fn reply(&self, session: &str, text: &str, images: &[String]) -> bool {
        self.shared.lock().unwrap().reply(session, text, images)
    }

    /// Joined to Codex's app-server right now.
    pub fn connected(&self) -> bool {
        self.shared.lock().unwrap().connected
    }

    pub fn version(&self) -> Option<String> {
        self.shared.lock().unwrap().version.clone()
    }
}

impl Shared {
    fn thread_name(&self, session: &str) -> Option<String> {
        self.state.threads.get(session)?.name.clone()
    }

    fn answer(&mut self, session: &str, request_id: &str, answer: &Answer) -> bool {
        let Some((_, pending)) = self.state.answerable(session, request_id) else {
            return false;
        };
        let Some(body) = super::state::result(&pending, answer) else {
            return false;
        };
        if self.state.take_pending(session, request_id).is_none() {
            return false;
        }
        self.send(json!({"id": pending.rpc_id, "result": body}));
        true
    }

    fn reply(&mut self, session: &str, text: &str, images: &[String]) -> bool {
        if !self.connected || !self.state.pending(session).is_empty() {
            return false;
        }
        let Some(thread) = self.state.threads.get(session) else {
            return false;
        };
        let active = thread.status.as_ref().is_some_and(|st| st.is_active());
        let mut input = Vec::new();
        if !text.trim().is_empty() {
            input.push(json!({"type": "text", "text": text, "text_elements": []}));
        }
        input.extend(
            images
                .iter()
                .take(5)
                .map(|path| json!({"type": "localImage", "path": path})),
        );
        if input.is_empty() {
            return false;
        }
        if active {
            let id = uuid_v7();
            self.call(
                "thread/queue/add",
                json!({"threadId": session, "input": input, "clientUserMessageId": id}),
                Waiting::Other,
            );
        } else {
            self.call(
                "turn/start",
                json!({"threadId": session, "input": input}),
                Waiting::Other,
            );
        }
        true
    }
}

/// One connection, from handshake to the daemon going away.
fn session(
    path: &PathBuf,
    shared: &Arc<Mutex<Shared>>,
    changed: &(dyn Fn() + Send + Sync),
) -> Result<(), String> {
    let stream = connect(path)?;
    let (mut socket, _) =
        tungstenite::client::client("ws://localhost/rpc", stream).map_err(|e| e.to_string())?;
    let (tx, rx): (Sender<Value>, Receiver<Value>) = channel();
    {
        let mut s = shared.lock().unwrap();
        s.outbox = Some(tx);
        s.connected = true;
        s.call("initialize", initialize_params(), Waiting::Initialize);
    }
    loop {
        // Everything queued for the daemon goes out between reads.
        while let Ok(message) = rx.try_recv() {
            socket
                .send(Message::text(message.to_string()))
                .map_err(|e| e.to_string())?;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if handle(&text, shared) {
                    changed();
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Handles one message; true when what the island shows may have changed.
fn handle(text: &str, shared: &Arc<Mutex<Shared>>) -> bool {
    let now = crate::sessions::service::now();
    handle_at(text, &mut shared.lock().unwrap(), now)
}

fn handle_at(text: &str, s: &mut Shared, now: Time) -> bool {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    match (method, message.get("id").cloned()) {
        // A request: approvals and questions the island may answer.
        (Some(method), Some(id)) => {
            let kept = s.state.request(id, &method, &params, now);
            s.follow();
            kept
        }
        (Some(method), None) => {
            let notified = s.state.notify(&method, &params, now);
            if let Some(new) = notified.read {
                s.call(
                    "thread/read",
                    json!({"threadId": new, "includeTurns": s.state.previews}),
                    Waiting::Read,
                );
            }
            // A page's thread that closed or unloaded is no longer kept.
            if let Some(thread) = params.get("threadId").and_then(Value::as_str)
                && !s.state.threads.contains_key(thread)
            {
                s.focused.remove(thread);
            }
            s.follow();
            notified.changed
        }
        (None, Some(id)) => {
            let waiting = id.as_i64().and_then(|id| s.waiting.remove(&id));
            let result = message.get("result").cloned().unwrap_or(Value::Null);
            match &waiting {
                Some(Waiting::Initialize) => {
                    s.version = result
                        .get("userAgent")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    s.send(json!({"method": "initialized"}));
                    s.call("thread/loaded/list", json!({}), Waiting::LoadedList);
                    false
                }
                Some(Waiting::LoadedList) => {
                    let ids: Vec<String> = result
                        .get("data")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect();
                    for id in ids {
                        s.call(
                            "thread/read",
                            json!({"threadId": id, "includeTurns": s.state.previews}),
                            Waiting::Read,
                        );
                    }
                    if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                        s.call(
                            "thread/loaded/list",
                            json!({"cursor": cursor}),
                            Waiting::LoadedList,
                        );
                    }
                    false
                }
                Some(Waiting::Read) | Some(Waiting::Resume) => {
                    let Some(thread) = result.get("thread") else {
                        return false;
                    };
                    s.state.upsert_thread(thread);
                    // Joined without its history (previews off): ask for the
                    // newest turn alone, without items, for when it started.
                    let no_turns = thread
                        .get("turns")
                        .and_then(Value::as_array)
                        .is_none_or(Vec::is_empty);
                    if matches!(waiting, Some(Waiting::Resume))
                        && no_turns
                        && let Some(id) = thread.get("id").and_then(Value::as_str)
                        && s.state
                            .threads
                            .get(id)
                            .is_some_and(|t| t.status.as_ref().is_some_and(|s| s.is_active()))
                    {
                        s.call(
                            "thread/turns/list",
                            json!({"threadId": id, "limit": 1, "sortDirection": "desc", "itemsView": "notLoaded"}),
                            Waiting::Turns(id.to_string()),
                        );
                    }
                    // Read back unloaded, it left the list: a page no longer keeps it.
                    if let Some(id) = thread.get("id").and_then(Value::as_str)
                        && !s.state.threads.contains_key(id)
                    {
                        s.focused.remove(id);
                    }
                    s.follow();
                    true
                }
                Some(Waiting::Turns(id)) => {
                    let turns = result.get("data").and_then(Value::as_array);
                    s.state.running_turn(id, turns.map_or(&[], Vec::as_slice))
                }
                _ => false,
            }
        }
        _ => false,
    }
}

#[cfg(windows)]
fn connect(path: &PathBuf) -> Result<impl Read + Write, String> {
    let stream = uds_windows::UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(POLL))
        .map_err(|e| e.to_string())?;
    Ok(stream)
}

#[cfg(unix)]
fn connect(path: &PathBuf) -> Result<impl Read + Write, String> {
    let stream = std::os::unix::net::UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(POLL))
        .map_err(|e| e.to_string())?;
    Ok(stream)
}

/// A UUIDv7 for Codex's client message ids: time-ordered, the rest random.
fn uuid_v7() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap_or_default();
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// `Vorssaint.exe --codex-probe`: connects like the island does and prints
/// every message for a few seconds, to see what Codex says on this machine.
pub fn probe() -> ! {
    let code = (|| -> Result<(), String> {
        let path = socket_path().ok_or("no CODEX_HOME")?;
        println!("socket {}", path.display());
        let stream = connect(&path)?;
        println!("connected");
        let (mut socket, _) =
            tungstenite::client::client("ws://localhost/rpc", stream).map_err(|e| e.to_string())?;
        println!("websocket up");
        let send = |socket: &mut tungstenite::WebSocket<_>, v: Value| {
            socket
                .send(Message::text(v.to_string()))
                .map_err(|e: tungstenite::Error| e.to_string())
        };
        send(
            &mut socket,
            json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "vorssaint-probe", "title": "Vorssaint", "version": env!("CARGO_PKG_VERSION")}, "capabilities": {"experimentalApi": true}}}),
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    let line: String = text.chars().take(1500).collect();
                    println!("<- {line}");
                    if let Ok(v) = serde_json::from_str::<Value>(&text)
                        && v.get("id") == Some(&json!(1))
                    {
                        send(&mut socket, json!({"method": "initialized"}))?;
                        send(
                            &mut socket,
                            json!({"id": 2, "method": "thread/loaded/list", "params": {}}),
                        )?;
                    }
                    if let Ok(v) = serde_json::from_str::<Value>(&text)
                        && v.get("id") == Some(&json!(2))
                    {
                        let ids: Vec<Value> =
                            v["result"]["data"].as_array().cloned().unwrap_or_default();
                        for (i, id) in ids.into_iter().enumerate() {
                            send(
                                &mut socket,
                                json!({"id": 100 + i, "method": "thread/read", "params": {"threadId": id, "includeTurns": false}}),
                            )?;
                        }
                    }
                }
                Ok(other) => println!("<- {other:?}"),
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    })();
    if let Err(e) = &code {
        println!("error: {e}");
    }
    std::process::exit(if code.is_ok() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client as if joined to a daemon, with what it sends kept in a channel.
    /// No socket: messages from the daemon go straight to `handle_at`.
    fn joined() -> (Shared, Receiver<Value>) {
        let (tx, rx) = channel();
        let mut s = Shared::new();
        s.outbox = Some(tx);
        s.connected = true;
        (s, rx)
    }

    fn sent(rx: &Receiver<Value>) -> Vec<Value> {
        rx.try_iter().collect()
    }

    #[test]
    fn enabling_previews_loads_history_and_disabling_discards_it() {
        let (mut shared, rx) = joined();
        shared
            .state
            .upsert_thread(&json!({"id": "t1", "cwd": "C:\\code", "status": {"type": "idle"}}));
        let client = CodexClient {
            shared: Arc::new(Mutex::new(shared)),
        };
        client.snapshot(1.0, ContentPolicy { previews: true });
        let reads = sent(&rx);
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0]["method"], "thread/read");
        assert_eq!(reads[0]["params"]["includeTurns"], true);
        client.shared.lock().unwrap().state.upsert_thread(
            &json!({"id": "t1", "turns": [{"items": [
                {"type": "agentMessage", "id": "a", "text": "Answer"}
            ]}]}),
        );
        assert_eq!(
            client.snapshot(2.0, ContentPolicy { previews: true }).0[0]
                .chat
                .len(),
            1
        );
        assert!(
            sent(&rx).is_empty(),
            "unchanged preference does not reload history"
        );
        client.snapshot(3.0, ContentPolicy { previews: false });
        let shared = client.shared.lock().unwrap();
        assert!(!shared.state.previews);
        assert!(shared.state.threads["t1"].chat.is_empty());
    }

    fn methods(messages: &[Value]) -> Vec<&str> {
        messages
            .iter()
            .filter_map(|m| m.get("method").and_then(Value::as_str))
            .collect()
    }

    fn deliver(s: &mut Shared, message: Value) -> bool {
        handle_at(&message.to_string(), s, 1_000.0)
    }

    /// Answers the call named `method` among `messages` with `result`.
    fn respond(s: &mut Shared, messages: &[Value], method: &str, result: Value) -> bool {
        let call = messages
            .iter()
            .find(|m| m.get("method").and_then(Value::as_str) == Some(method))
            .unwrap_or_else(|| panic!("no {method} in {messages:?}"));
        deliver(s, json!({"id": call["id"], "result": result}))
    }

    fn read_thread(s: &mut Shared, rx: &Receiver<Value>, thread: Value) -> Vec<Value> {
        let id = thread["id"].clone();
        deliver(
            s,
            json!({"method": "thread/status/changed", "params": {"threadId": id, "status": thread["status"]}}),
        );
        let reads = sent(rx);
        assert_eq!(methods(&reads), ["thread/read"]);
        respond(s, &reads, "thread/read", json!({"thread": thread}));
        sent(rx)
    }

    #[test]
    fn initialize_opts_out_of_streamed_deltas() {
        let params = initialize_params();
        assert_eq!(params["capabilities"]["experimentalApi"], json!(true));
        let opted: Vec<&str> = params["capabilities"]["optOutNotificationMethods"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(opted, QUIET_NOTIFICATIONS);
        assert!(opted.contains(&"item/agentMessage/delta"));
        assert_eq!(params["clientInfo"]["name"], json!("vorssaint"));
    }

    #[test]
    fn deltas_and_unknown_methods_raise_no_change() {
        let (mut s, rx) = joined();
        read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code\\app", "status": {"type": "idle"}}),
        );
        for index in 0..20 {
            assert!(!deliver(
                &mut s,
                json!({"method": "item/agentMessage/delta",
                       "params": {"threadId": "t1", "turnId": "u", "itemId": "m", "delta": format!("word{index} ")}})
            ));
        }
        assert!(!deliver(
            &mut s,
            json!({"method": "what/ever", "params": {"threadId": "t1"}})
        ));
        assert!(!deliver(
            &mut s,
            json!({"method": "turn/started", "params": {"threadId": "nobody"}})
        ));
        assert!(
            !deliver(
                &mut s,
                json!({"method": "thread/status/changed", "params": {"threadId": "t1", "status": {"type": "idle"}}})
            ),
            "the same status again changes nothing"
        );
        assert!(sent(&rx).is_empty(), "and nothing is sent for them");
        assert!(deliver(
            &mut s,
            json!({"method": "thread/name/updated", "params": {"threadId": "t1", "threadName": "Ship it"}})
        ));
    }

    #[test]
    fn only_described_threads_that_are_not_helpers_are_joined() {
        let (mut s, rx) = joined();
        // A working thread seen only by its status is read before it is joined.
        assert!(deliver(
            &mut s,
            json!({"method": "thread/status/changed", "params": {"threadId": "t1", "status": {"type": "active", "activeFlags": []}}})
        ));
        let first = sent(&rx);
        assert_eq!(methods(&first), ["thread/read"], "no resume before a read");
        respond(
            &mut s,
            &first,
            "thread/read",
            json!({"thread": {"id": "t1", "cwd": "C:\\code\\app", "status": {"type": "active", "activeFlags": []}}}),
        );
        assert_eq!(
            methods(&sent(&rx)),
            ["thread/resume"],
            "a read one is joined"
        );
        // An ephemeral helper that works is never joined.
        let after = read_thread(
            &mut s,
            &rx,
            json!({"id": "e1", "cwd": "C:\\code\\app", "ephemeral": true, "status": {"type": "active", "activeFlags": []}}),
        );
        assert!(
            !methods(&after).contains(&"thread/resume"),
            "an ephemeral helper thread is not joined: {after:?}"
        );
        // Nor when a page asks for it.
        s.focused.insert("e1".into());
        s.follow();
        assert!(sent(&rx).is_empty());
    }

    #[test]
    fn a_thread_joined_midway_asks_for_its_running_turns_start() {
        let (mut s, rx) = joined();
        let thread = json!({"id": "t1", "cwd": "C:\\code\\app", "status": {"type": "active", "activeFlags": []}});
        let after = read_thread(&mut s, &rx, thread.clone());
        assert_eq!(methods(&after), ["thread/resume"]);
        // Previews off: the join has no history, so only the newest turn is asked for.
        let mut resumed = thread.clone();
        resumed["turns"] = json!([]);
        respond(&mut s, &after, "thread/resume", json!({"thread": resumed}));
        let asked = sent(&rx);
        assert_eq!(methods(&asked), ["thread/turns/list"]);
        assert_eq!(
            asked[0]["params"],
            json!({"threadId": "t1", "limit": 1, "sortDirection": "desc", "itemsView": "notLoaded"})
        );
        assert!(respond(
            &mut s,
            &asked,
            "thread/turns/list",
            json!({"data": [{"id": "u1", "status": "inProgress", "startedAt": 900, "items": []}]})
        ));
        assert_eq!(s.state.threads["t1"].turn_started, Some(900.0));
    }

    #[test]
    fn a_closed_threads_page_no_longer_keeps_it() {
        let (mut s, rx) = joined();
        read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code\\app", "status": {"type": "idle"}}),
        );
        s.focused = HashSet::from(["t1".to_string()]);
        s.follow();
        assert_eq!(methods(&sent(&rx)), ["thread/resume"], "focus joins t1");
        assert!(deliver(
            &mut s,
            json!({"method": "thread/closed", "params": {"threadId": "t1"}})
        ));
        assert!(
            !s.focused.contains("t1"),
            "closing lets the page's thread go"
        );
        // It comes back idle: read, but not joined for a page long gone.
        let back = read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code\\app", "status": {"type": "idle"}}),
        );
        assert!(!methods(&back).contains(&"thread/resume"), "{back:?}");

        // A read that finds the thread unloaded lets it go too.
        s.focused = HashSet::from(["t1".to_string()]);
        s.follow();
        let resume = sent(&rx);
        assert_eq!(methods(&resume), ["thread/resume"]);
        assert!(respond(
            &mut s,
            &resume,
            "thread/resume",
            json!({"thread": {"id": "t1", "status": {"type": "notLoaded"}}})
        ));
        assert!(!s.state.threads.contains_key("t1"));
        assert!(s.focused.is_empty());

        // A reply with no thread changes nothing.
        s.call("thread/read", json!({"threadId": "x"}), Waiting::Read);
        let read = sent(&rx);
        assert!(!respond(&mut s, &read, "thread/read", json!({})));
    }

    #[test]
    fn open_looks_for_the_threads_own_name_with_previews_off() {
        let (mut s, rx) = joined();
        read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code\\app", "name": "Fix checkout", "status": {"type": "idle"}}),
        );
        let shown = s
            .state
            .snapshot(1_000.0, ContentPolicy { previews: false })
            .0;
        // The list names it by its folder, which Open used to look for in
        // window titles, where Codex writes "<spinner> Fix checkout | app".
        assert_eq!(shown[0].name, "app");
        assert_eq!(s.thread_name("t1").as_deref(), Some("Fix checkout"));
        assert_eq!(s.thread_name("gone"), None);
    }

    #[test]
    fn a_disconnect_forgets_the_focus() {
        let (mut s, _rx) = joined();
        s.focused.insert("t1".into());
        s.waiting.insert(1, Waiting::Other);
        s.reset();
        assert!(!s.connected && s.outbox.is_none());
        assert!(s.focused.is_empty() && s.waiting.is_empty() && s.state.threads.is_empty());
    }

    #[test]
    fn replies_deliver_images_to_codex_for_idle_and_running_threads() {
        let (mut s, rx) = joined();
        read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code", "status": {"type": "idle"}}),
        );
        let paths = vec!["C:\\Temp\\photo.png".to_string()];
        assert!(s.reply("t1", "", &paths));
        let messages = sent(&rx);
        assert_eq!(messages[0]["method"], "turn/start");
        assert_eq!(
            messages[0]["params"]["input"],
            json!([{"type": "localImage", "path": paths[0]}])
        );
        s.state
            .upsert_thread(&json!({"id": "t1", "status": {"type": "active", "activeFlags": []}}));
        assert!(s.reply("t1", "Look at this", &paths));
        let messages = sent(&rx);
        assert_eq!(messages[0]["method"], "thread/queue/add");
        assert_eq!(
            messages[0]["params"]["input"],
            json!([
                {"type": "text", "text": "Look at this", "text_elements": []},
                {"type": "localImage", "path": paths[0]}
            ])
        );
        assert!(!s.reply("t1", "", &[]));
    }

    #[test]
    fn subagent_requests_are_answered_through_the_parents_row() {
        let (mut s, rx) = joined();
        let active = json!({"type": "active", "activeFlags": []});
        read_thread(
            &mut s,
            &rx,
            json!({"id": "t1", "cwd": "C:\\code\\app", "status": active}),
        );
        read_thread(
            &mut s,
            &rx,
            json!({"id": "c1", "cwd": "C:\\code\\app", "parentThreadId": "t1", "status": active}),
        );
        assert!(deliver(
            &mut s,
            json!({"id": 21, "method": "item/fileChange/requestApproval", "params": {"threadId": "c1"}})
        ));
        sent(&rx);
        assert!(!s.reply("t1", "go on", &[]), "no reply while the row waits");
        assert!(sent(&rx).is_empty());
        assert!(!s.answer("t1", "codex:99", &Answer::Allow), "an unknown id");
        assert!(
            !s.answer("t1", "codex:21", &Answer::Questions(HashMap::new())),
            "an answer of another kind"
        );
        assert!(s.answer("t1", "codex:21", &Answer::Allow));
        assert_eq!(
            sent(&rx),
            [json!({"id": 21, "result": {"decision": "accept"}})]
        );
        assert!(s.state.threads["c1"].pending.is_empty());
        assert!(!s.answer("t1", "codex:21", &Answer::Allow), "answered once");
        assert!(s.reply("t1", "go on", &[]), "the row takes replies again");
        assert_eq!(methods(&sent(&rx)), ["thread/queue/add"]);
        s.connected = false;
        assert!(!s.reply("t1", "go on", &[]), "not while disconnected");
    }
}
