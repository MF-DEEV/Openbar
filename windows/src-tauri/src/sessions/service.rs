// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! All the sessions section's I/O around the pure engine
//! (ClaudeSessionService.swift; spec 02-sessions §1, §2.4, §2.8, §6): it
//! scans Claude Code's registry, tails transcripts, takes hook messages
//! from the pipe, holds permission hooks until the person answers, types
//! replies through the session's host, and publishes snapshots to the island.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Wry};

use super::agent_colors::AgentColors;
use super::codex::client::CodexClient;
use super::codex::state::Answer as CodexAnswer;
use super::engine::Engine;
use super::hook_message::{self, Decision, HookMessage};
use super::models::{
    self, ContentPolicy, HookEvent, HostKind, Session, SessionCounts, SessionEvent, SessionRecord,
    Time,
};
use super::registry::{self, RegistryFile};
use super::transcript::{self, NoPricing, TranscriptReader};
use crate::hooks::{self, transport::HookConnection};
use crate::hosts::{self, Located};

/// Ticks while anything is going on, and when all is quiet.
const ACTIVE_TICK: Duration = Duration::from_secs(1);
const QUIET_TICK: Duration = Duration::from_secs(3);
/// A permission hook is let go a little before Claude Code's 600 s timeout.
const HOOK_PATIENCE: f64 = 590.0;
/// A line longer than this is not one worth reading.
const MAX_LINE: usize = 32 * 1024 * 1024;
const CHUNK: usize = 4 * 1024 * 1024;

#[cfg(windows)]
const PID_DOMAINS: &[&str] = registry::WINDOWS_PID_DOMAINS;
#[cfg(not(windows))]
const PID_DOMAINS: &[&str] = registry::MAC_PID_DOMAINS;

pub fn now() -> Time {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// What the island receives (windows/src/sessions/types.ts `Snapshot`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Published<'a> {
    sessions: &'a [Session],
    counts: SessionCounts,
    previews: bool,
    hooks_installed: bool,
}

/// A permission hook waiting on the person's answer.
struct Pending {
    connection: HookConnection,
    session: String,
    suggestions: Option<Value>,
    question_input: Option<Map<String, Value>>,
    since: Time,
}

/// Reads what was appended to a file since the last time.
struct Tail {
    offset: u64,
    partial: Vec<u8>,
    /// The file's first bytes, to notice it was rewritten in place.
    head: Vec<u8>,
    reader: TranscriptReader,
}

impl Tail {
    fn new(policy: ContentPolicy, path: &Path) -> Self {
        Tail {
            offset: 0,
            partial: Vec::new(),
            head: Vec::new(),
            reader: TranscriptReader::new(policy, &path.to_string_lossy()),
        }
    }

    /// Complete new lines; starts over when the file shrank or was rewritten.
    fn lines(&mut self, path: &Path, policy: ContentPolicy) -> Vec<Vec<u8>> {
        let Ok(mut file) = File::open(path) else {
            return Vec::new();
        };
        let Ok(meta) = file.metadata() else {
            return Vec::new();
        };
        let len = meta.len();
        let mut head = vec![0u8; len.min(4096) as usize];
        let _ = file.read_exact(&mut head);
        let rewritten = !self.head.is_empty()
            && !head.starts_with(&self.head[..self.head.len().min(head.len())]);
        if len < self.offset || rewritten {
            *self = Tail::new(policy, path);
        }
        self.head = head;
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = vec![0u8; ((len - self.offset) as usize).min(CHUNK)];
        let Ok(read) = file.read(&mut buf) else {
            return Vec::new();
        };
        buf.truncate(read);
        self.offset += read as u64;
        self.partial.extend_from_slice(&buf);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            if line.len() <= MAX_LINE {
                lines.push(line);
            }
        }
        if self.partial.len() > MAX_LINE {
            self.partial.clear();
        }
        lines
    }
}

struct Inner {
    engine: Engine,
    agent_colors: AgentColors,
    tails: HashMap<PathBuf, Tail>,
    /// Where each session's transcript is, as the hooks said or a scan found.
    transcripts: HashMap<String, PathBuf>,
    hosts: HashMap<i32, Located>,
    last_records: HashMap<i32, SessionRecord>,
    pending: HashMap<String, Pending>,
    metas_read: HashSet<PathBuf>,
    task_stamps: HashMap<String, SystemTime>,
    sessions: Vec<Session>,
    ticks: u64,
    hooks_installed: bool,
}

pub struct Service {
    app: AppHandle<Wry>,
    inner: Mutex<Inner>,
    codex: Mutex<Option<Arc<CodexClient>>>,
    /// Wakes the ticker for an early publish (Codex says something changed).
    wake: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl Service {
    pub fn start(app: AppHandle<Wry>) -> Arc<Self> {
        let service = Arc::new(Service {
            app,
            inner: Mutex::new(Inner {
                engine: Engine::new(ContentPolicy::default()),
                agent_colors: AgentColors::default(),
                tails: HashMap::new(),
                transcripts: HashMap::new(),
                hosts: HashMap::new(),
                last_records: HashMap::new(),
                pending: HashMap::new(),
                metas_read: HashSet::new(),
                task_stamps: HashMap::new(),
                sessions: Vec::new(),
                ticks: 0,
                hooks_installed: false,
            }),
            codex: Mutex::new(None),
            wake: Mutex::new(None),
        });
        let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
        *service.wake.lock().unwrap() = Some(wake_tx.clone());
        let codex = CodexClient::start(move || {
            let _ = wake_tx.send(());
        });
        *service.codex.lock().unwrap() = Some(codex);
        let hook_side = service.clone();
        if let Err(e) = hooks::transport::serve(hooks::APP_ID, move |connection| {
            hook_side.on_hook(connection)
        }) {
            eprintln!("vorssaint: Claude Code hooks unavailable: {e}");
        }
        let ticker = service.clone();
        thread::Builder::new()
            .name("claude-sessions".into())
            .spawn(move || {
                let mut next = std::time::Instant::now();
                loop {
                    let wait = next.saturating_duration_since(std::time::Instant::now());
                    match wake_rx.recv_timeout(wait) {
                        // Codex changed something: publish now, scan files on schedule.
                        Ok(()) => {
                            while wake_rx.try_recv().is_ok() {}
                            let mut inner = ticker.inner.lock().unwrap();
                            let t = now();
                            ticker.publish(&mut inner, t);
                        }
                        Err(_) => {
                            let busy = ticker.tick();
                            next = std::time::Instant::now()
                                + if busy { ACTIVE_TICK } else { QUIET_TICK };
                        }
                    }
                }
            })
            .expect("sessions thread");
        service
    }

    // MARK: Ticks

    /// One scan; true while any session works or waits, so the next comes sooner.
    fn tick(&self) -> bool {
        let home = hooks::installer::home_dir();
        let files = home.as_deref().map(registry_files).unwrap_or_default();
        let scan = registry::scan(&files, PID_DOMAINS, process_start);
        let mut inner = self.inner.lock().unwrap();
        inner.ticks += 1;
        let t = now();

        // A record being rewritten keeps its last good version for this tick.
        let mut records = scan.records;
        for pid in scan.unreadable {
            if let Some(last) = inner.last_records.get(&pid).cloned() {
                records.push(last);
            }
        }
        let live: HashSet<i32> = records.iter().map(|r| r.pid).collect();
        inner.hosts.retain(|pid, _| live.contains(pid));
        for record in &mut records {
            let located = inner
                .hosts
                .entry(record.pid)
                .or_insert_with(|| {
                    hosts::locate(record.pid as u32).unwrap_or(Located {
                        host: hosts::Host::Unknown,
                        ancestors: vec![],
                    })
                })
                .clone();
            record.host = host_kind(&located.host);
            record.accepts_replies = located.host.accepts_replies();
        }
        inner.last_records = records.iter().map(|r| (r.pid, r.clone())).collect();
        inner.engine.ingest_records(records, t);

        let ids = inner.engine.session_ids();
        for id in &ids {
            self.read_transcript(&mut inner, id, home.as_deref(), t);
        }
        if inner.ticks % 3 == 1 {
            for id in &ids {
                self.read_subagents(&mut inner, id, t);
                if let Some(home) = &home {
                    self.read_tasks(&mut inner, id, home, t);
                }
            }
        }
        if inner.ticks % 5 == 1 {
            inner.hooks_installed = hooks::claude_hooks_status_installed();
        }
        self.release_stale_hooks(&mut inner, t);
        self.publish(&mut inner, t)
    }

    fn read_transcript(&self, inner: &mut Inner, id: &str, home: Option<&Path>, t: Time) {
        let path = match inner.transcripts.get(id).filter(|p| p.exists()) {
            Some(path) => path.clone(),
            None => {
                let Some(found) = home.and_then(|h| find_transcript(h, id)) else {
                    return;
                };
                inner.transcripts.insert(id.to_string(), found.clone());
                found
            }
        };
        let policy = inner.engine.policy();
        let tail = inner
            .tails
            .entry(path.clone())
            .or_insert_with(|| Tail::new(policy, &path));
        let mut facts = Vec::new();
        for line in tail.lines(&path, policy) {
            facts.extend(tail.reader.parse(&line, t, &NoPricing));
        }
        if !facts.is_empty() {
            inner.engine.ingest_facts(id, &facts, t);
        }
    }

    fn read_subagents(&self, inner: &mut Inner, id: &str, t: Time) {
        let Some(transcript) = inner.transcripts.get(id).cloned() else {
            return;
        };
        let folder = transcript.with_extension("").join("subagents");
        let Ok(entries) = std::fs::read_dir(&folder) else {
            return;
        };
        let policy = inner.engine.policy();
        let mut facts = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(stem) = name.strip_suffix(".jsonl") {
                let tail = inner
                    .tails
                    .entry(path.clone())
                    .or_insert_with(|| Tail::new(policy, &path));
                for line in tail.lines(&path, policy) {
                    facts.extend(tail.reader.parse(&line, t, &NoPricing));
                }
                let meta = folder.join(format!("{stem}.meta.json"));
                if !inner.metas_read.contains(&meta)
                    && let Ok(data) = std::fs::read(&meta)
                    && data.len() <= 64 * 1024
                {
                    inner.metas_read.insert(meta);
                    let agent = stem.strip_prefix("agent-").unwrap_or(stem);
                    if let Some(fact) = transcript::subagent_info(&data, agent, policy) {
                        facts.push(fact);
                    }
                }
            }
        }
        if !facts.is_empty() {
            inner.engine.ingest_facts(id, &facts, t);
        }
    }

    fn read_tasks(&self, inner: &mut Inner, id: &str, home: &Path, t: Time) {
        let policy = inner.engine.policy();
        for claude in registry::homes(home) {
            let folder = claude.join("tasks").join(id);
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            let files: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .take(500)
                .collect();
            let newest = files
                .iter()
                .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok())
                .max();
            if newest.is_none() || inner.task_stamps.get(id) == newest.as_ref() {
                return;
            }
            inner.task_stamps.insert(id.to_string(), newest.unwrap());
            let contents: Vec<Vec<u8>> =
                files.iter().filter_map(|p| std::fs::read(p).ok()).collect();
            let fact = transcript::tasks(&contents, policy);
            let fact = models::TranscriptFact::Tasks(fact.unwrap_or_default());
            inner.engine.ingest_facts(id, &[fact], t);
            return;
        }
    }

    /// Lets go of hooks whose question is gone: answered in the terminal,
    /// the turn moved on, the hook process killed, or nearly timed out.
    fn release_stale_hooks(&self, inner: &mut Inner, t: Time) {
        let stale: Vec<String> = inner
            .pending
            .iter()
            .filter(|(request, p)| {
                !inner.engine.holds(request, &p.session)
                    || p.connection
                        .peer_pid
                        .is_some_and(|pid| !hosts::process::is_running(pid))
                    || t - p.since > HOOK_PATIENCE
            })
            .map(|(request, _)| request.clone())
            .collect();
        for request in stale {
            // Dropping the connection sends nothing: Claude Code asks in the terminal.
            inner.pending.remove(&request);
            inner.engine.resolve_permission(&request);
        }
    }

    /// Sends the island a snapshot and any notices; true while busy.
    fn codex(&self) -> Option<Arc<CodexClient>> {
        self.codex.lock().unwrap().clone()
    }

    fn publish(&self, inner: &mut Inner, t: Time) -> bool {
        let mut snapshot = inner.engine.snapshot(t);
        if let Some(home) = hooks::installer::home_dir() {
            for session in &mut snapshot.sessions {
                inner.agent_colors.decorate(session, &home, t);
            }
        }
        if let Some(codex) = self.codex() {
            let (sessions, events) = codex.snapshot(t, inner.engine.policy());
            snapshot.sessions.extend(sessions);
            snapshot.events.extend(events);
        }
        let sessions = models::sorted(snapshot.sessions);
        let counts = SessionCounts::of(&sessions);
        let busy = sessions
            .iter()
            .any(|s| s.phase.is_active() || s.phase.is_waiting());
        let published = Published {
            sessions: &sessions,
            counts,
            previews: inner.engine.policy().previews,
            hooks_installed: inner.hooks_installed,
        };
        let _ = self.app.emit("sessions://snapshot", &published);
        for event in &snapshot.events {
            let _ = self.app.emit::<&SessionEvent>("sessions://event", event);
        }
        inner.sessions = sessions;
        busy
    }

    // MARK: Hooks

    fn on_hook(&self, connection: HookConnection) {
        let request_id = format!("{:032x}", rand_id());
        let mut inner = self.inner.lock().unwrap();
        let policy = inner.engine.policy();
        let Some(message) = HookMessage::decode(&connection.payload, &request_id, now(), policy)
        else {
            return;
        };
        let permission = message.is_permission_request();
        if let Some(path) = &message.transcript {
            inner
                .transcripts
                .insert(message.fact.session_id.clone(), PathBuf::from(path));
        }
        // A background job started in the same turn is seen before "Done".
        if matches!(message.fact.event, HookEvent::Stopped { .. }) {
            let id = message.fact.session_id.clone();
            let home = hooks::installer::home_dir();
            self.read_transcript(&mut inner, &id, home.as_deref(), now());
        }
        let session = message.fact.session_id.clone();
        inner.engine.ingest_hook(message.fact, now());
        if permission {
            inner.pending.insert(
                request_id,
                Pending {
                    connection,
                    session,
                    suggestions: message.suggestions,
                    question_input: message.question_input,
                    since: now(),
                },
            );
        } else {
            // Everything but a permission is answered at once, with nothing.
            drop(connection);
        }
        let t = now();
        self.publish(&mut inner, t);
    }

    // MARK: Actions

    fn answer(&self, session: &str, request: &str, decision: Decision) -> bool {
        if let Some(codex) = self.codex().filter(|c| c.owns(session)) {
            let answer = match decision {
                Decision::Allow => CodexAnswer::Allow,
                Decision::AllowAlways => CodexAnswer::AllowForSession,
                Decision::Deny => CodexAnswer::Deny,
                Decision::Answer(map) => CodexAnswer::Questions(map.into_iter().collect()),
            };
            let ok = codex.answer(session, request, answer);
            if ok {
                codex.acknowledge(session, now());
            }
            let mut inner = self.inner.lock().unwrap();
            let t = now();
            self.publish(&mut inner, t);
            return ok;
        }
        let mut inner = self.inner.lock().unwrap();
        let asks = inner
            .engine
            .permission(session)
            .is_some_and(|p| !p.questions.is_empty());
        let fits = match &decision {
            Decision::Answer(_) => asks,
            Decision::Allow | Decision::AllowAlways => !asks,
            Decision::Deny => true,
        };
        if !fits || !inner.engine.holds(request, session) {
            return false;
        }
        let Some(pending) = inner.pending.remove(request) else {
            return false;
        };
        let body = hook_message::reply(
            &decision,
            pending.suggestions.as_ref(),
            pending.question_input.as_ref(),
        );
        let sent = !body.is_empty() && pending.connection.reply(&body).is_ok();
        inner.engine.resolve_permission(request);
        let t = now();
        inner.engine.acknowledge(session, t);
        self.publish(&mut inner, t);
        sent
    }

    fn acknowledge(&self, session: &str) {
        let mut inner = self.inner.lock().unwrap();
        let t = now();
        if let Some(codex) = self.codex() {
            codex.acknowledge(session, t);
        }
        inner.engine.acknowledge(session, t);
        self.publish(&mut inner, t);
    }

    pub fn set_previews(&self, on: bool) {
        let mut inner = self.inner.lock().unwrap();
        // Turning previews on re-reads every transcript: only when it changes.
        if inner.engine.policy().previews == on {
            return;
        }
        inner.engine.set_policy(ContentPolicy { previews: on });
        if on {
            // Read everything again to fill in what was skipped.
            inner.tails.clear();
            inner.metas_read.clear();
            inner.task_stamps.clear();
        }
        let t = now();
        self.publish(&mut inner, t);
    }

    /// Never types while the session waits on a prompt: the registry is read
    /// again right before, as a dialog may be open before it says so.
    fn reply(&self, session: &str, text: &str, images: &[String]) -> Option<&'static str> {
        if let Some(codex) = self.codex().filter(|c| c.owns(session)) {
            // Codex takes replies through its own server: no terminal typing, no Esc risk.
            if codex.reply(session, text, images) {
                self.acknowledge(session);
                return None;
            }
            let inner = self.inner.lock().unwrap();
            return Some(codex_refusal(&inner.sessions, session));
        }
        let (pid, located) = {
            let inner = self.inner.lock().unwrap();
            let Some(current) = inner.sessions.iter().find(|s| s.id == session) else {
                return Some("unreachable");
            };
            if !models::can_reply(current) {
                return Some(
                    if current.phase.is_waiting() || current.waiting_for.is_some() {
                        "waiting"
                    } else {
                        "unreachable"
                    },
                );
            }
            let asking = inner.engine.permission(session).is_some()
                || inner.pending.values().any(|p| p.session == session);
            if asking {
                return Some("waiting");
            }
            let Some(located) = inner.hosts.get(&current.pid).cloned() else {
                return Some("unreachable");
            };
            (current.pid, located)
        };
        if let Some(home) = hooks::installer::home_dir() {
            let files = registry_files(&home);
            let fresh = registry::scan(&files, PID_DOMAINS, process_start);
            match fresh.records.iter().find(|r| r.pid == pid) {
                Some(r) if r.session_id != session => return Some("unreachable"),
                Some(r) if r.status == Some(models::RegistryStatus::Waiting) => {
                    return Some("waiting");
                }
                _ => {}
            }
        }
        match hosts::deliver(pid as u32, &located, text, images) {
            Ok(()) => {
                self.acknowledge(session);
                None
            }
            Err(_) => Some("unreachable"),
        }
    }

    fn open(&self, session: &str) -> bool {
        let codex_name = {
            let inner = self.inner.lock().unwrap();
            inner
                .sessions
                .iter()
                .find(|s| s.id == session && s.provider == models::Provider::Codex)
                .map(|s| s.name.clone())
        };
        if let Some(name) = codex_name {
            // Codex's terminal titles carry the thread's own name; with
            // previews off the list shows the folder's instead, which can't
            // tell two terminals in one folder apart.
            let name = self
                .codex()
                .and_then(|c| c.thread_name(session))
                .unwrap_or(name);
            let ok = hosts::focus_codex(&name);
            if ok {
                self.acknowledge(session);
            }
            return ok;
        }
        let target = {
            let inner = self.inner.lock().unwrap();
            inner
                .sessions
                .iter()
                .find(|s| s.id == session)
                .and_then(|s| inner.hosts.get(&s.pid).cloned().map(|l| (s.pid, l)))
        };
        let Some((pid, located)) = target else {
            return false;
        };
        let ok = hosts::focus(pid as u32, &located);
        if ok {
            self.acknowledge(session);
        }
        ok
    }
}

fn host_kind(host: &hosts::Host) -> HostKind {
    match host {
        hosts::Host::WindowsTerminal { .. } => HostKind::WindowsTerminal,
        hosts::Host::Vscode { .. } => HostKind::Vscode,
        hosts::Host::Conhost => HostKind::Conhost,
        hosts::Host::App { .. } => HostKind::App,
        hosts::Host::Unknown => HostKind::Unknown,
    }
}

/// Every file of every registry folder, with its contents.
fn registry_files(home: &Path) -> Vec<RegistryFile> {
    let mut files = Vec::new();
    for folder in registry::folders(home) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if registry::pid_of_file(&name).is_none() {
                continue;
            }
            let data = entry
                .metadata()
                .ok()
                .filter(|m| m.len() < registry::MAX_RECORD_BYTES as u64)
                .and_then(|_| std::fs::read(entry.path()).ok());
            files.push(RegistryFile { name, data });
        }
    }
    files
}

/// When the process with `pid` started, or None when none runs.
fn process_start(pid: i32) -> Option<Time> {
    if pid <= 0 {
        return None;
    }
    #[cfg(windows)]
    {
        hosts::process::started_at(pid as u32)
    }
    #[cfg(not(windows))]
    {
        // `ps -o lstart` in UTC is the format Claude Code records.
        let out = std::process::Command::new("ps")
            .env("TZ", "UTC")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        registry::proc_start(String::from_utf8_lossy(&out.stdout).trim())
    }
}

/// `<home>/projects/*/<id>.jsonl` in either Claude Code home.
fn find_transcript(home: &Path, id: &str) -> Option<PathBuf> {
    let name = format!("{id}.jsonl");
    for claude in registry::homes(home) {
        let Ok(projects) = std::fs::read_dir(claude.join("projects")) else {
            continue;
        };
        for project in projects.filter_map(Result::ok) {
            let candidate = project.path().join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn rand_id() -> u128 {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap_or_default();
    u128::from_le_bytes(bytes)
}

// MARK: Commands

pub type Shared = Arc<Service>;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiDecision {
    Allow,
    Always,
    Deny,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum UiAnswer {
    One(String),
    Many(Vec<String>),
}

#[tauri::command]
pub fn sessions_decide(
    service: tauri::State<'_, Shared>,
    session_id: String,
    request_id: String,
    decision: UiDecision,
) -> bool {
    let decision = match decision {
        UiDecision::Allow => Decision::Allow,
        UiDecision::Always => Decision::AllowAlways,
        UiDecision::Deny => Decision::Deny,
    };
    service.answer(&session_id, &request_id, decision)
}

#[tauri::command]
pub fn sessions_answer(
    service: tauri::State<'_, Shared>,
    session_id: String,
    request_id: String,
    answers: BTreeMap<String, UiAnswer>,
) -> bool {
    let answers = answers
        .into_iter()
        .map(|(q, a)| {
            let list = match a {
                UiAnswer::One(s) => vec![hook_message::clipped_utf16(
                    &s,
                    models::Question::OWN_ANSWER_LIMIT,
                )],
                UiAnswer::Many(v) => v
                    .into_iter()
                    .map(|s| hook_message::clipped_utf16(&s, models::Question::OWN_ANSWER_LIMIT))
                    .collect(),
            };
            (q, list)
        })
        .filter(|(_, list)| !list.is_empty())
        .collect();
    service.answer(&session_id, &request_id, Decision::Answer(answers))
}

#[tauri::command]
pub fn sessions_acknowledge(service: tauri::State<'_, Shared>, session_id: String) {
    service.acknowledge(&session_id);
}

/// Why Codex refused a reply: "waiting" while the session waits on a
/// prompt, otherwise "unreachable" (Codex went away, or the thread did).
fn codex_refusal(sessions: &[Session], session: &str) -> &'static str {
    let waiting = sessions
        .iter()
        .find(|s| s.id == session)
        .is_some_and(|s| s.phase.is_waiting() || s.waiting_for.is_some());
    if waiting { "waiting" } else { "unreachable" }
}

/// The session page the island shows (None for the list), so Codex keeps that thread joined.
#[tauri::command]
pub fn sessions_focus(service: tauri::State<'_, Shared>, session_id: Option<String>) {
    if let Some(codex) = service.codex() {
        codex.focus(session_id.as_deref());
    }
}

/// For Settings › Content › Sessions: whether the island hears Codex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexStatus {
    Connected,
    /// Installed (its folder exists) but no app-server is running.
    NotRunning,
    NotFound,
}

#[tauri::command]
pub fn sessions_codex_status(service: tauri::State<'_, Shared>) -> CodexStatus {
    if service.codex().is_some_and(|c| c.connected()) {
        CodexStatus::Connected
    } else if crate::sessions::codex::client::codex_home().is_some_and(|h| h.is_dir()) {
        CodexStatus::NotRunning
    } else {
        CodexStatus::NotFound
    }
}

/// Typing can take a second or two (images wait for Claude Code), so it runs off the main thread.
#[tauri::command]
pub async fn sessions_reply(
    service: tauri::State<'_, Shared>,
    session_id: String,
    text: String,
    images: Vec<String>,
) -> Result<Option<&'static str>, ()> {
    let service = service.inner().clone();
    let text: String = text.trim().chars().take(4000).collect();
    let images: Vec<String> = images.into_iter().take(5).collect();
    tauri::async_runtime::spawn_blocking(move || service.reply(&session_id, &text, &images))
        .await
        .map_err(|_| ())
}

#[tauri::command]
pub async fn sessions_open(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<bool, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.open(&session_id))
        .await
        .map_err(|_| ())
}

/// Images must belong to a visible message; never expose arbitrary filesystem paths.
#[tauri::command]
pub async fn sessions_image(
    service: tauri::State<'_, Shared>,
    session_id: String,
    source: String,
) -> Result<String, String> {
    {
        let inner = service.inner.lock().unwrap();
        if !inner.engine.policy().previews
            || !inner
                .sessions
                .iter()
                .filter(|s| s.id == session_id)
                .flat_map(|s| &s.chat)
                .any(|m| m.images.contains(&source))
        {
            return Err("Image unavailable".into());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        const LIMIT: u64 = 20 * 1024 * 1024;
        let bytes = if source.starts_with("data:image/") {
            let (header, data) = source.split_once(',').ok_or("Invalid image")?;
            if !header.ends_with(";base64") || data.len() as u64 > LIMIT * 4 / 3 + 4 {
                return Err("Invalid image".to_string());
            }
            STANDARD.decode(data).map_err(|_| "Invalid image")?
        } else {
            let file = File::open(&source).map_err(|_| "Image unavailable")?;
            let mut bytes = Vec::new();
            file.take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Image unavailable")?;
            bytes
        };
        if bytes.len() as u64 > LIMIT {
            return Err("Image too large".into());
        }
        let (mime, _) = image_format(&bytes).ok_or("Unsupported image")?;
        Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    })
    .await
    .map_err(|_| "Image unavailable".to_string())?
}

pub(super) fn image_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(("image/webp", "webp"))
    } else if bytes.starts_with(b"BM") {
        Some(("image/bmp", "bmp"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::codex::state::State as CodexState;
    use serde_json::json;

    #[test]
    fn a_refused_codex_reply_says_whether_it_waits() {
        let mut codex = CodexState::default();
        for id in ["idle", "asks", "terminal"] {
            codex.upsert_thread(
                &json!({"id": id, "cwd": "C:\\code\\app", "status": {"type": "idle"}}),
            );
        }
        codex.request(
            json!(1),
            "item/commandExecution/requestApproval",
            &json!({"threadId": "asks", "command": "ls"}),
            1.0,
        );
        codex.request(
            json!(2),
            "mcpServer/elicitation/request",
            &json!({"threadId": "terminal"}),
            1.0,
        );
        let (sessions, _) = codex.snapshot(1.0, ContentPolicy::default());
        assert_eq!(codex_refusal(&sessions, "asks"), "waiting");
        assert_eq!(codex_refusal(&sessions, "terminal"), "waiting");
        assert_eq!(
            codex_refusal(&sessions, "idle"),
            "unreachable",
            "an idle session refused means Codex could not be reached"
        );
        assert_eq!(codex_refusal(&sessions, "gone"), "unreachable");
        assert_eq!(codex_refusal(&[], "idle"), "unreachable");
    }
}
