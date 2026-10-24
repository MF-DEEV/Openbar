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

    /// Joins threads that work or wait, and leaves the quiet ones nobody looks at.
    fn follow(&mut self) {
        let mut join = Vec::new();
        let mut leave = Vec::new();
        for (id, thread) in &self.state.threads {
            let active =
                thread.status.as_ref().is_some_and(|s| s.is_active()) || !thread.pending.is_empty();
            let wanted = active || self.focused.contains(id);
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
                json!({"threadId": id, "excludeTurns": true}),
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
            shared: Arc::new(Mutex::new(Shared {
                state: State::default(),
                next_id: 0,
                waiting: HashMap::new(),
                outbox: None,
                focused: HashSet::new(),
                connected: false,
                version: None,
            })),
        });
        let shared = client.shared.clone();
        let changed = Arc::new(changed);
        thread::Builder::new()
            .name("codex-app-server".into())
            .spawn(move || {
                loop {
                    if let Some(path) = socket_path().filter(|p| p.exists()) {
                        let _ = session(&path, &shared, &*changed);
                        let mut s = shared.lock().unwrap();
                        s.connected = false;
                        s.outbox = None;
                        s.waiting.clear();
                        s.state = State::default();
                        drop(s);
                        changed();
                    }
                    thread::sleep(RETRY);
                }
            })
            .expect("codex thread");
        client
    }

    pub fn snapshot(&self, now: Time, policy: ContentPolicy) -> (Vec<Session>, Vec<SessionEvent>) {
        self.shared.lock().unwrap().state.snapshot(now, policy)
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

    /// Keeps a thread joined while its page is open, so its tools and turns show.
    pub fn focus(&self, session: Option<&str>) {
        let mut s = self.shared.lock().unwrap();
        s.focused = session.into_iter().map(str::to_owned).collect();
        s.follow();
    }

    /// Answers the request with this island id; false when it is gone or of another kind.
    pub fn answer(&self, session: &str, request_id: &str, answer: Answer) -> bool {
        let mut s = self.shared.lock().unwrap();
        let Some(thread) = s.state.threads.get_mut(session) else {
            return false;
        };
        let Some(index) = thread
            .pending
            .iter()
            .position(|p| p.request_id() == request_id && p.answerable())
        else {
            return false;
        };
        let Some(body) = super::state::result(&thread.pending[index], &answer) else {
            return false;
        };
        let pending = thread.pending.remove(index);
        s.send(json!({"id": pending.rpc_id, "result": body}));
        true
    }

    /// A prompt for an idle thread, or one queued after the current turn of a working one.
    pub fn reply(&self, session: &str, text: &str) -> bool {
        let mut s = self.shared.lock().unwrap();
        if !s.connected {
            return false;
        }
        let Some(thread) = s.state.threads.get(session) else {
            return false;
        };
        if !thread.pending.is_empty() {
            return false;
        }
        let input = json!([{"type": "text", "text": text, "text_elements": []}]);
        if thread.status.as_ref().is_some_and(|st| st.is_active()) {
            let id = uuid_v7();
            s.call(
                "thread/queue/add",
                json!({"threadId": session, "input": input, "clientUserMessageId": id}),
                Waiting::Other,
            );
        } else {
            s.call(
                "turn/start",
                json!({"threadId": session, "input": input}),
                Waiting::Other,
            );
        }
        true
    }

    pub fn version(&self) -> Option<String> {
        self.shared.lock().unwrap().version.clone()
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
        s.call(
            "initialize",
            json!({
                "clientInfo": {"name": "vorssaint", "title": "Vorssaint", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }),
            Waiting::Initialize,
        );
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
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    let now = crate::sessions::service::now();
    let mut s = shared.lock().unwrap();
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
            if let Some(new) = s.state.notify(&method, &params, now) {
                s.call(
                    "thread/read",
                    json!({"threadId": new, "includeTurns": false}),
                    Waiting::Read,
                );
            }
            s.follow();
            true
        }
        (None, Some(id)) => {
            let waiting = id.as_i64().and_then(|id| s.waiting.remove(&id));
            let result = message.get("result").cloned().unwrap_or(Value::Null);
            match waiting {
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
                            json!({"threadId": id, "includeTurns": false}),
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
                    if let Some(thread) = result.get("thread") {
                        s.state.upsert_thread(thread);
                    }
                    s.follow();
                    true
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
