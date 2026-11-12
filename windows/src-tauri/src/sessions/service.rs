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
use tauri::{AppHandle, Emitter, Manager, Wry};

use super::agent_colors::AgentColors;
use super::background;
use super::codex::client::CodexClient;
use super::codex::state::Answer as CodexAnswer;
use super::engine::Engine;
use super::hook_agents::{self, Tracker as HookAgents};
use super::hook_message::{self, Decision, HookMessage};
use super::models::{
    self, ContentPolicy, HookEvent, HostKind, Session, SessionCounts, SessionEvent, SessionRecord,
    Time,
};
use super::plan_modes::PlanMode;
use super::registry::{self, RegistryFile};
use super::remote;
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
#[cfg(not(any(windows, target_os = "linux")))]
const PID_DOMAINS: &[&str] = registry::MAC_PID_DOMAINS;

/// The `pidDomain`s of records written on this machine.
fn pid_domains() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    return crate::linux::sessions::pid_domains();
    #[cfg(not(target_os = "linux"))]
    PID_DOMAINS
}

/// How often Agent View is asked: more often while it has sessions.
const JOBS_EVERY: Time = 10.0;
const JOBS_EVERY_QUIETLY: Time = 60.0;

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
    /// Claude Code is installed, so the island can start sessions in a terminal.
    can_start: bool,
    /// Its Agent View is on, so it can run them in the background too.
    background_available: bool,
    /// Codex is installed, so New session can start it too.
    can_start_codex: bool,
    /// Gemini CLI, Copilot CLI, OpenCode and `cursor-agent` found on this PC.
    startable_agents: Vec<&'static str>,
    hooks_installed: bool,
}

/// A permission hook waiting on the person's answer.
struct Pending {
    connection: HookConnection,
    session: String,
    suggestions: Option<Value>,
    question_input: Option<Map<String, Value>>,
    approval_input: Option<Map<String, Value>>,
    plan_modes: Vec<PlanMode>,
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

/// An OpenCode permission request whose plugin waits for the island's answer.
struct AgentPending {
    connection: HookConnection,
    session: String,
    since: Time,
}

struct Inner {
    #[cfg(windows)]
    local_codex: Vec<super::codex::local::LocalSession>,
    /// Gemini CLI's, Copilot CLI's, OpenCode's and Cursor's sessions.
    hook_agents: HookAgents,
    agent_pending: HashMap<String, AgentPending>,
    /// Each OpenCode's plugin listening for replies, by its process.
    listeners: HashMap<i32, HookConnection>,
    /// The agents whose sessions the list shows.
    hidden_agents: HashSet<models::Provider>,
    branches: HashMap<String, (Option<String>, Time)>,
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
    /// What Agent View last listed, and when each ended job ended.
    background_jobs: Vec<background::Job>,
    background_endings: HashMap<String, Time>,
    background_branches: HashMap<String, String>,
    listing_jobs: bool,
    jobs_listed: Option<Time>,
    can_start: bool,
    can_start_codex: bool,
    startable_agents: Vec<&'static str>,
    background_available: bool,
    remote: RemoteHosts,
}

/// The SSH hosts the person listed (sessions::remote), and what each last
/// showed.
#[derive(Default)]
struct RemoteHosts {
    links: HashMap<String, Arc<remote::Link>>,
    snapshots: HashMap<String, remote::Snapshot>,
    /// Remote transcripts being read, by alias and path on the host.
    tails: HashMap<(String, String), (remote::Tail, TranscriptReader)>,
    busy: HashSet<String>,
    failures: HashMap<String, String>,
    /// The loopback port forwarded hooks arrive on (Windows).
    bridge: Option<u16>,
}

impl RemoteHosts {
    fn is_peer(&self, pid: Option<u32>) -> bool {
        pid.is_some_and(|pid| self.links.values().any(|l| l.peer() == Some(pid)))
    }
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
                #[cfg(windows)]
                local_codex: Vec::new(),
                hook_agents: HookAgents::default(),
                agent_pending: HashMap::new(),
                listeners: HashMap::new(),
                hidden_agents: HashSet::new(),
                branches: HashMap::new(),
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
                background_jobs: Vec::new(),
                background_endings: HashMap::new(),
                background_branches: HashMap::new(),
                listing_jobs: false,
                jobs_listed: None,
                can_start: false,
                can_start_codex: false,
                startable_agents: Vec::new(),
                background_available: false,
                remote: RemoteHosts::default(),
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
            eprintln!("openbar: Claude Code hooks unavailable: {e}");
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
                            Service::list_background_jobs(&ticker, false);
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
        #[cfg(windows)]
        let local_codex = super::codex::local::discover();
        let home = hooks::installer::home_dir();
        let by_home = home
            .as_deref()
            .map(registry_files_by_home)
            .unwrap_or_default();
        note_account_folder(&by_home);
        let files: Vec<RegistryFile> = by_home.into_iter().flat_map(|(_, files)| files).collect();
        let scan = registry::scan(&files, pid_domains(), process_start);
        let mut inner = self.inner.lock().unwrap();
        #[cfg(windows)]
        {
            inner.local_codex = local_codex;
        }
        inner.ticks += 1;
        let t = now();

        // A record being rewritten keeps its last good version for this tick.
        let mut records = scan.records;
        for pid in scan.unreadable {
            if let Some(last) = inner.last_records.get(&pid).cloned() {
                records.push(last);
            }
        }
        self.release_agents(&mut inner, t);
        let mut live: HashSet<i32> = records.iter().map(|r| r.pid).collect();
        live.extend(inner.hook_agents.pids());
        inner.hosts.retain(|pid, _| live.contains(pid));
        for record in &mut records {
            // A background session has no terminal to find.
            if record.job.is_some() {
                record.host = HostKind::Background;
                record.accepts_replies = true;
                continue;
            }
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
            record.host_name = located.host.display_name();
        }
        inner.last_records = records.iter().map(|r| (r.pid, r.clone())).collect();
        // Sessions on remote hosts, as their connections last saw them; their
        // process numbers are the host's, so none of the lookups above apply.
        let mut aliases: Vec<&String> = inner.remote.snapshots.keys().collect();
        aliases.sort();
        let remote: Vec<SessionRecord> = aliases
            .into_iter()
            .flat_map(|a| inner.remote.snapshots[a].records.clone())
            .collect();
        let remote_ids: HashSet<String> = remote.iter().map(|r| r.session_id.clone()).collect();
        records.extend(remote);
        inner.engine.ingest_records(records, t);

        let ids: Vec<String> = inner
            .engine
            .session_ids()
            .into_iter()
            .filter(|id| !remote_ids.contains(id))
            .collect();
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
    /// Drops hook agents' sessions whose process quit, and lets go of
    /// OpenCode requests and listeners nobody waits on any more.
    fn release_agents(&self, inner: &mut Inner, t: Time) {
        inner
            .hook_agents
            .prune(t, |pid| hosts::process::is_running(pid as u32));
        let stale: Vec<String> = inner
            .agent_pending
            .iter()
            .filter(|(request, p)| {
                inner
                    .hook_agents
                    .entry(&p.session)
                    .and_then(|e| e.permission.as_ref())
                    .is_none_or(|perm| &perm.request_id != *request)
                    || t - p.since > HOOK_PATIENCE
            })
            .map(|(r, _)| r.clone())
            .collect();
        for request in stale {
            if let Some(p) = inner.agent_pending.remove(&request) {
                inner.hook_agents.resolve(&p.session, &request);
            }
        }
        inner
            .listeners
            .retain(|pid, _| hosts::process::is_running(*pid as u32));
    }

    /// One event of Gemini CLI, Copilot CLI, Cursor or OpenCode.
    fn on_agent(&self, connection: HookConnection) {
        let t = now();
        let mut inner = self.inner.lock().unwrap();
        let Some(decoded) = hook_agents::decode(&connection.payload, t, inner.engine.policy())
        else {
            return;
        };
        let Some(provider) = decoded.provider else {
            return;
        };
        if inner.hidden_agents.contains(&provider) {
            return;
        }
        // The agent is the process above the hook and its shells; OpenCode's
        // plugin writes from inside it.
        let chain: Vec<(u32, String)> = connection
            .peer_pid
            .and_then(hosts::process::ancestors)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.pid, e.name))
            .collect();
        let pid = hook_agents::agent_process(&chain, decoded.pid);
        if decoded.listens {
            inner.listeners.insert(pid, connection);
            return;
        }
        let mut connection = Some(connection);
        if let Some((session, request)) = &decoded.permission
            && let Some(connection) = connection.take()
        {
            inner.agent_pending.insert(
                request.clone(),
                AgentPending {
                    connection,
                    session: hook_agents::id(provider, session),
                    since: t,
                },
            );
        }
        drop(connection);
        for mut event in decoded.events {
            event.pid = if pid > 0 { pid } else { event.pid };
            if let hook_agents::Kind::PermissionSettled(request) = &event.kind {
                // Answered in OpenCode itself: its plugin stops waiting too.
                let id = event.id();
                let settled: Vec<String> = inner
                    .agent_pending
                    .iter()
                    .filter(|(key, p)| {
                        p.session == id && request.as_ref().is_none_or(|r| r == *key)
                    })
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in settled {
                    inner.agent_pending.remove(&key);
                }
            }
            inner.hook_agents.ingest(event, t);
        }
        self.publish(&mut inner, t);
    }

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
        #[cfg(windows)]
        for local in &inner.local_codex {
            if !snapshot.sessions.iter().any(|s| s.id == local.id) {
                let mut session = super::codex::local::as_session(local, inner.engine.policy());
                if let Some(located) = hosts::locate(local.pid) {
                    session.host = host_kind(&located.host);
                    session.host_name = located.host.display_name();
                    session.accepts_replies = session.phase == models::SessionPhase::Idle
                        && matches!(
                            located.host,
                            hosts::Host::Conhost | hosts::Host::WindowsTerminal { .. }
                        );
                }
                snapshot.sessions.push(session);
            }
        }
        {
            let Inner {
                hook_agents,
                hosts: located,
                branches,
                hidden_agents,
                engine,
                ..
            } = &mut *inner;
            let policy = engine.policy();
            for entry_pid in hook_agents.pids() {
                if entry_pid > 0
                    && !located.contains_key(&entry_pid)
                    && let Some(found) = hosts::locate(entry_pid as u32)
                {
                    located.insert(entry_pid, found);
                }
            }
            let located = &*located;
            let branch = |folder: &str| -> Option<String> {
                let fresh = branches
                    .get(folder)
                    .filter(|(_, at)| t - at < 10.0)
                    .map(|(b, _)| b.clone());
                fresh.unwrap_or_else(|| hook_agents::read_branch(folder))
            };
            let (sessions, events) = hook_agents.snapshot(
                t,
                policy,
                |entry| {
                    let host = located.get(&entry.pid);
                    let kind = host
                        .map(|l| host_kind(&l.host))
                        .unwrap_or(HostKind::Unknown);
                    // OpenCode takes replies through its plugin; Cursor's app keeps its own chat.
                    let accepts = match entry.provider {
                        models::Provider::Opencode => true,
                        models::Provider::Cursor => {
                            matches!(
                                kind,
                                HostKind::WindowsTerminal
                                    | HostKind::Conhost
                                    | HostKind::Wezterm
                                    | HostKind::Tmux
                                    | HostKind::Kitty
                            )
                        }
                        _ => host.is_some_and(|l| l.host.accepts_replies()),
                    };
                    hook_agents::Place {
                        host: kind,
                        accepts_replies: accepts,
                        host_name: host.and_then(|l| l.host.display_name()),
                    }
                },
                branch,
            );
            for session in &sessions {
                branches.insert(session.cwd.clone(), (session.branch.clone(), t));
            }
            snapshot.sessions.extend(
                sessions
                    .into_iter()
                    .filter(|s| !hidden_agents.contains(&s.provider)),
            );
            snapshot.events.extend(events);
        }
        let sessions = models::sorted(self.with_background(inner, snapshot.sessions, t));
        let counts = SessionCounts::of(&sessions);
        let busy = sessions
            .iter()
            .any(|s| s.phase.is_active() || s.phase.is_waiting());
        let published = Published {
            sessions: &sessions,
            counts,
            previews: inner.engine.policy().previews,
            can_start: inner.can_start,
            background_available: inner.background_available,
            can_start_codex: inner.can_start_codex,
            startable_agents: inner.startable_agents.clone(),
            hooks_installed: inner.hooks_installed,
        };
        let _ = self.app.emit("sessions://snapshot", &published);
        for event in &snapshot.events {
            let _ = self.app.emit::<&SessionEvent>("sessions://event", event);
        }
        inner.sessions = sessions;
        busy
    }

    // MARK: Background sessions

    /// Asks Agent View for its sessions, when it is time to or `force`d after
    /// an action. `claude` takes a moment, so it runs off the ticker's thread.
    fn list_background_jobs(service: &Arc<Self>, force: bool) {
        let homes = {
            let mut inner = service.inner.lock().unwrap();
            let t = now();
            let every = if inner.background_jobs.is_empty() {
                JOBS_EVERY_QUIETLY
            } else {
                JOBS_EVERY
            };
            if inner.listing_jobs
                || (!force && inner.jobs_listed.is_some_and(|listed| t - listed < every))
            {
                return;
            }
            inner.listing_jobs = true;
            inner.jobs_listed = Some(t);
            hooks::installer::home_dir()
                .map(|home| registry::homes(&home))
                .unwrap_or_default()
        };
        let service = service.clone();
        thread::spawn(move || {
            let can_start = background::executable().is_some();
            let can_start_codex = crate::agent_usage::codex_executable().is_some();
            // Each found agent is connected, and a switched-off one taken out, by itself.
            // The switches are read from the settings themselves, so a sync
            // before they reach the service never connects one turned off.
            if let Some(prefs) = service.app.try_state::<crate::prefs::Prefs>() {
                sync_agent_hooks(&crate::prefs::hidden_agents(&prefs.get()));
            }
            let startable_agents: Vec<&'static str> = models::Provider::HOOK_AGENTS
                .iter()
                .filter(|p| {
                    hook_agents::command_name(**p)
                        .is_some_and(|name| crate::agent_usage::agent_executable(name).is_some())
                })
                .map(|p| p.key())
                .collect();
            let turned_off =
                background::is_on(std::env::var(background::DISABLE_VARIABLE).ok().as_deref())
                    || homes.iter().any(|home| {
                        std::fs::read(home.join("settings.json"))
                            .is_ok_and(|data| background::settings_disable(&data))
                    });
            let mut jobs = Vec::new();
            let mut listed = false;
            if !turned_off {
                let arguments = ["agents", "--json", "--all"].map(str::to_owned);
                for home in homes.iter().filter(|home| home.join("sessions").is_dir()) {
                    let path = home.to_string_lossy().into_owned();
                    let Some(output) =
                        background::claude(&arguments, Some(&path), None, Duration::from_secs(15))
                    else {
                        continue;
                    };
                    if !output.ok {
                        continue;
                    }
                    if let Some(found) = background::decode(&output.out, Some(&path)) {
                        listed = true;
                        jobs.extend(found);
                    }
                }
            }
            let mut inner = service.inner.lock().unwrap();
            let t = now();
            let alive: Vec<String> = inner
                .background_jobs
                .iter()
                .filter(|job| !job.ended())
                .map(|job| job.id.clone())
                .collect();
            let endings = background::endings(&jobs, &inner.background_endings, &alive, t);
            inner.background_endings = endings;
            inner.background_jobs = jobs;
            inner.listing_jobs = false;
            inner.can_start = can_start;
            inner.can_start_codex = can_start_codex;
            inner.startable_agents = startable_agents;
            inner.background_available = listed;
            service.publish(&mut inner, t);
        });
    }

    /// The sessions with what Agent View adds: each one's state, and rows for
    /// those whose process has ended.
    fn with_background(
        &self,
        inner: &mut Inner,
        mut sessions: Vec<Session>,
        t: Time,
    ) -> Vec<Session> {
        for session in &mut sessions {
            let id = session.id.clone();
            let Some(details) = session.background.as_mut() else {
                continue;
            };
            if let Some(job) = inner
                .background_jobs
                .iter()
                .find(|job| job.session_id == id)
            {
                details.state = job.state;
                details.config_home = job.config_home.clone();
            }
            if let Some(branch) = &session.branch {
                inner.background_branches.insert(id, branch.clone());
            }
        }
        let listed: HashSet<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        let others: Vec<background::Job> = inner
            .background_jobs
            .iter()
            .filter(|job| !listed.contains(job.session_id.as_str()))
            .cloned()
            .collect();
        let ended: Vec<Session> = background::kept(&others, &inner.background_endings, t)
            .into_iter()
            .map(|job| {
                let at = inner.background_endings.get(&job.id).copied().unwrap_or(t);
                background::ended_session(
                    job,
                    at,
                    inner.background_branches.get(&job.session_id).cloned(),
                )
            })
            .collect();
        sessions.extend(ended);
        sessions
    }

    fn background_of(&self, session: &str) -> Option<background::BackgroundDetails> {
        let inner = self.inner.lock().unwrap();
        inner
            .sessions
            .iter()
            .find(|s| s.id == session)
            .and_then(|s| s.background.clone())
    }

    // MARK: Hooks

    fn on_hook(&self, connection: HookConnection) {
        if hook_agents::is_envelope(&connection.payload) {
            return self.on_agent(connection);
        }
        let request_id = format!("{:032x}", rand_id());
        let mut inner = self.inner.lock().unwrap();
        let policy = inner.engine.policy();
        let Some(message) = HookMessage::decode(&connection.payload, &request_id, now(), policy)
        else {
            return;
        };
        let permission = message.is_permission_request();
        // A remote session's transcript is on its host, read through its connection.
        let remote = inner.remote.is_peer(connection.peer_pid);
        if !remote && let Some(path) = &message.transcript {
            inner
                .transcripts
                .insert(message.fact.session_id.clone(), PathBuf::from(path));
        }
        // A background job started in the same turn is seen before "Done".
        if !remote && matches!(message.fact.event, HookEvent::Stopped { .. }) {
            let id = message.fact.session_id.clone();
            let home = hooks::installer::home_dir();
            self.read_transcript(&mut inner, &id, home.as_deref(), now());
        }
        let session = message.fact.session_id.clone();
        let modes = match &message.fact.event {
            HookEvent::PermissionRequested(asked) => asked.plan_modes.clone(),
            _ => Vec::new(),
        };
        inner.engine.ingest_hook(message.fact, now());
        if permission {
            inner.pending.insert(
                request_id,
                Pending {
                    connection,
                    session,
                    suggestions: message.suggestions,
                    question_input: message.question_input,
                    approval_input: message.approval_input,
                    plan_modes: modes,
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
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.hook_agents.owns(session) {
                // OpenCode's plugin waits for the answer; the other agents ask in their own terminal.
                let Some(reply) = hook_agents::opencode_reply(&decision) else {
                    return false;
                };
                let Some(pending) = inner
                    .agent_pending
                    .remove(request)
                    .filter(|p| p.session == session)
                else {
                    return false;
                };
                let sent = pending.connection.reply(&reply).is_ok();
                let t = now();
                inner.hook_agents.resolve(session, request);
                inner.hook_agents.acknowledge(session, t);
                self.publish(&mut inner, t);
                return sent;
            }
        }
        if let Some(codex) = self.codex().filter(|c| c.owns(session)) {
            let answer = match decision {
                Decision::Allow | Decision::ApprovePlan(_) => CodexAnswer::Allow,
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
            Decision::Allow | Decision::AllowAlways | Decision::ApprovePlan(_) => !asks,
            Decision::Deny => true,
        };
        if !fits || !inner.engine.holds(request, session) {
            return false;
        }
        // A mode goes only to a plan that offers it.
        if let Decision::ApprovePlan(mode) = &decision {
            let offered = inner
                .pending
                .get(request)
                .is_some_and(|p| p.approval_input.is_some() && p.plan_modes.contains(mode));
            if !offered {
                return false;
            }
        }
        let Some(pending) = inner.pending.remove(request) else {
            return false;
        };
        // A plain approval of a plan, from a shortcut or the list, picks the
        // mode the card puts first.
        let decision = match decision {
            Decision::Allow | Decision::AllowAlways if pending.approval_input.is_some() => pending
                .plan_modes
                .first()
                .map_or(decision, |first| Decision::ApprovePlan(*first)),
            other => other,
        };
        let body = hook_message::reply(
            &decision,
            pending.suggestions.as_ref(),
            pending.question_input.as_ref(),
            pending.approval_input.as_ref(),
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
        inner.hook_agents.acknowledge(session, t);
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
        if !on {
            inner.hook_agents.purge_content();
        }
        if on {
            // Read everything again to fill in what was skipped.
            inner.tails.clear();
            inner.metas_read.clear();
            inner.task_stamps.clear();
            inner.remote.tails.clear();
            for link in inner.remote.links.values() {
                link.reread_transcripts();
            }
        }
        let t = now();
        self.publish(&mut inner, t);
    }

    /// Never types while the session waits on a prompt: the registry is read
    /// again right before, as a dialog may be open before it says so.
    fn reply(&self, session: &str, text: &str, images: &[String]) -> Option<&'static str> {
        if let Some(refusal) = self.reply_remote(session, text) {
            return refusal;
        }
        if let Some(codex) = self.codex().filter(|c| c.owns(session)) {
            // Codex takes replies through its own server: no terminal typing, no Esc risk.
            if codex.reply(session, text, images) {
                self.acknowledge(session);
                return None;
            }
            let inner = self.inner.lock().unwrap();
            return Some(codex_refusal(&inner.sessions, session));
        }
        #[cfg(windows)]
        {
            let pid = self
                .inner
                .lock()
                .unwrap()
                .local_codex
                .iter()
                .find(|s| s.id == session)
                .map(|s| s.pid);
            if let Some(pid) = pid {
                // Re-read the rollout and process just before typing. A turn may
                // have started since the last published snapshot.
                let current = super::codex::local::discover()
                    .into_iter()
                    .find(|s| s.id == session && s.pid == pid);
                let Some(current) = current else {
                    return Some("unreachable");
                };
                if super::codex::local::as_session(&current, ContentPolicy::default()).phase
                    != models::SessionPhase::Idle
                {
                    return Some("waiting");
                }
                let Some(located) = hosts::locate(pid) else {
                    return Some("unreachable");
                };
                if !matches!(
                    located.host,
                    hosts::Host::Conhost | hosts::Host::WindowsTerminal { .. }
                ) {
                    return Some("unreachable");
                }
                if hosts::deliver_codex(pid, &located, text, images).is_ok() {
                    self.acknowledge(session);
                    return None;
                }
                return Some("unreachable");
            }
        }
        if let Some(refusal) = self.reply_to_agent(session, text) {
            return refusal;
        }
        let job = self.background_of(session);
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
            let located = inner.hosts.get(&current.pid).cloned();
            if job.is_none() && located.is_none() {
                return Some("unreachable");
            }
            (current.pid, located)
        };
        if let Some(home) = hooks::installer::home_dir() {
            let files = registry_files(&home);
            let fresh = registry::scan(&files, pid_domains(), process_start);
            match fresh.records.iter().find(|r| r.pid == pid) {
                Some(r) if r.session_id != session => return Some("unreachable"),
                Some(r) if r.status == Some(models::RegistryStatus::Waiting) => {
                    return Some("waiting");
                }
                _ => {}
            }
        }
        if let Some(details) = job {
            // Through `claude attach`, which counts as the person's own typing.
            if background::reply(text, images, &details.job, details.config_home.as_deref()) {
                self.acknowledge(session);
                return None;
            }
            return Some("unreachable");
        }
        let Some(located) = located else {
            return Some("unreachable");
        };
        match hosts::deliver(pid as u32, &located, text, images) {
            Ok(()) => {
                self.acknowledge(session);
                None
            }
            Err(_) => Some("unreachable"),
        }
    }

    /// A reply to OpenCode goes to its listening plugin; one to Gemini CLI,
    /// Copilot CLI or `cursor-agent` is typed into its terminal and sent by
    /// a return of its own a moment later, as their prompts take a return
    /// that comes with a paste as a line break. None when the session is
    /// not one of theirs.
    fn reply_to_agent(&self, session: &str, text: &str) -> Option<Option<&'static str>> {
        let mut inner = self.inner.lock().unwrap();
        let entry = inner.hook_agents.entry(session)?.clone();
        let current = inner.sessions.iter().find(|s| s.id == session)?.clone();
        if !models::can_reply(&current) {
            return Some(Some(
                if current.phase.is_waiting() || current.waiting_for.is_some() {
                    "waiting"
                } else {
                    "unreachable"
                },
            ));
        }
        if entry.provider == models::Provider::Opencode {
            let Some(listener) = inner.listeners.remove(&entry.pid) else {
                return Some(Some("unreachable"));
            };
            let sent = listener
                .reply(&hook_agents::opencode_prompt(&entry.session_id, text))
                .is_ok();
            drop(inner);
            if sent {
                self.acknowledge(session);
            }
            return Some((!sent).then_some("unreachable"));
        }
        let Some(located) = inner.hosts.get(&entry.pid).cloned() else {
            return Some(Some("unreachable"));
        };
        drop(inner);
        let typed = hosts::deliver(entry.pid as u32, &located, text, &[]).is_ok() && {
            std::thread::sleep(Duration::from_millis(450));
            hosts::deliver(entry.pid as u32, &located, "", &[]).is_ok()
        };
        if typed {
            self.acknowledge(session);
        }
        Some((!typed).then_some("unreachable"))
    }

    /// Hides or shows each hook agent's sessions, as Settings say.
    pub fn set_hidden_agents(&self, hidden: HashSet<models::Provider>) {
        let mut inner = self.inner.lock().unwrap();
        if inner.hidden_agents == hidden {
            return;
        }
        inner.hidden_agents = hidden.clone();
        let t = now();
        self.publish(&mut inner, t);
        drop(inner);
        // A switch turned on or off puts its agent's hooks in or takes them out.
        thread::spawn(move || sync_agent_hooks(&hidden));
    }

    /// Whether the session's own tab or pane is in front in its terminal
    /// (#210). None for a session no local terminal shows, or one whose
    /// terminal can't tell: Codex found by folder, remote and background ones.
    fn terminal_shown(&self, session: &str) -> Option<bool> {
        let (pid, located) = {
            let inner = self.inner.lock().unwrap();
            let found = inner.sessions.iter().find(|s| s.id == session)?;
            if found.pid <= 1 {
                return None;
            }
            (found.pid, inner.hosts.get(&found.pid).cloned()?)
        };
        if self.remote_target(session).is_some() || self.background_of(session).is_some() {
            return None;
        }
        hosts::shown(pid as u32, &located)
    }

    fn open(&self, session: &str) -> bool {
        if let Some((link, pid)) = self.remote_target(session) {
            // Its pane comes forward in the host's tmux; no window here shows it.
            let ok = link.focus(pid, session);
            if ok {
                self.acknowledge(session);
            }
            return ok;
        }
        if let Some(details) = self.background_of(session) {
            // Attaching also brings back a session that has stopped.
            let arguments = ["attach".to_owned(), details.job];
            let ok = background::in_terminal(&arguments, details.config_home.as_deref(), None);
            if ok {
                self.acknowledge(session);
            }
            return ok;
        }
        let codex_target = {
            let inner = self.inner.lock().unwrap();
            inner
                .sessions
                .iter()
                .find(|s| s.id == session && s.provider == models::Provider::Codex)
                .map(|s| {
                    (
                        s.name.clone(),
                        s.cwd.clone(),
                        inner
                            .sessions
                            .iter()
                            .filter(|other| {
                                other.id != session && other.provider == models::Provider::Codex
                            })
                            .map(|other| other.cwd.clone())
                            .collect::<Vec<_>>(),
                    )
                })
        };
        if let Some((name, cwd, others)) = codex_target {
            // Codex's terminal titles carry the thread's own name; with
            // previews off the list shows the folder's instead, which can't
            // tell two terminals in one folder apart.
            let name = self
                .codex()
                .and_then(|c| c.thread_name(session))
                .unwrap_or(name);
            let ok = hosts::focus_codex(session, &cwd, &name, &others);
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
                .and_then(|s| {
                    inner
                        .hosts
                        .get(&s.pid)
                        .cloned()
                        .map(|l| (s.pid, l, s.cwd.clone()))
                })
        };
        let Some((pid, located, cwd)) = target else {
            return false;
        };
        let ok = hosts::focus(pid as u32, &located, &cwd);
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
        hosts::Host::Wezterm { .. } => HostKind::Wezterm,
        hosts::Host::Tmux { .. } => HostKind::Tmux,
        hosts::Host::Kitty { .. } => HostKind::Kitty,
        hosts::Host::App { .. } => HostKind::App,
        hosts::Host::Unknown => HostKind::Unknown,
    }
}

/// The Claude Code config folder whose account the limits are read for;
/// None until a session has run, which means `%USERPROFILE%\.claude`.
static ACCOUNT_FOLDER: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn account_folder() -> Option<PathBuf> {
    ACCOUNT_FOLDER.lock().ok().and_then(|folder| folder.clone())
}

/// Follows the account of the session that changed last; with none running,
/// the one before stays.
fn note_account_folder(by_home: &[(PathBuf, Vec<RegistryFile>)]) {
    let records: Vec<_> = by_home
        .iter()
        .map(|(claude, files)| {
            (
                claude.clone(),
                registry::scan(files, pid_domains(), process_start).records,
            )
        })
        .collect();
    if let (Some(folder), Ok(mut current)) =
        (registry::latest_home(&records), ACCOUNT_FOLDER.lock())
    {
        *current = Some(folder);
    }
}

/// Every registry folder's files, with their contents, by config folder.
fn registry_files_by_home(home: &Path) -> Vec<(PathBuf, Vec<RegistryFile>)> {
    registry::homes(home)
        .into_iter()
        .map(|claude| {
            let files = folder_files(&claude.join("sessions"));
            (claude, files)
        })
        .collect()
}

/// Every file of every registry folder, with its contents.
fn registry_files(home: &Path) -> Vec<RegistryFile> {
    registry_files_by_home(home)
        .into_iter()
        .flat_map(|(_, files)| files)
        .collect()
}

/// The record files of one registry folder.
fn folder_files(folder: &Path) -> Vec<RegistryFile> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files = Vec::new();
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
    files
}

/// When the process with `pid` started, or None when none runs.
fn process_start(pid: i32) -> Option<Time> {
    if pid <= 0 {
        return None;
    }
    #[cfg(any(windows, target_os = "linux"))]
    {
        hosts::process::started_at(pid as u32)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
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

// MARK: Remote hosts

impl Service {
    /// Starts a connection for each host listed while the switch is on, and
    /// ends the others (sessions::remote, docs/remote-hosts.md).
    pub fn set_remote_hosts(self: &Arc<Self>, wanted: Vec<String>) {
        let mut inner = self.inner.lock().unwrap();
        let gone: Vec<String> = inner
            .remote
            .links
            .keys()
            .filter(|a| !wanted.contains(a))
            .cloned()
            .collect();
        for alias in gone {
            if let Some(link) = inner.remote.links.remove(&alias) {
                link.stop();
            }
            inner.remote.snapshots.remove(&alias);
            inner.remote.failures.remove(&alias);
            inner.remote.tails.retain(|(a, _), _| a != &alias);
        }
        if wanted.is_empty() {
            return;
        }
        let Some(target) = self.remote_target_address(&mut inner) else {
            return;
        };
        for alias in wanted {
            if inner.remote.links.contains_key(&alias) {
                continue;
            }
            let snapshots = Arc::downgrade(self);
            let states = Arc::downgrade(self);
            let link = remote::Link::start(
                &alias,
                hooks::APP_ID,
                target.clone(),
                move |alias, snapshot| {
                    if let Some(service) = snapshots.upgrade() {
                        service.ingest_remote(alias, snapshot);
                    }
                },
                move |alias, state| {
                    // A host that went away takes its sessions with it, and
                    // nothing else waits on it.
                    if let Some(service) = states.upgrade() {
                        if !matches!(state, remote::State::Connected { .. }) {
                            let mut inner = service.inner.lock().unwrap();
                            if inner.remote.snapshots.remove(alias).is_some() {
                                drop(inner);
                                service.tick();
                            }
                        }
                        let _ = service.app.emit("sessions-remote-hosts", ());
                    }
                },
            );
            inner.remote.links.insert(alias, link);
        }
    }

    /// Where the hosts' sockets are forwarded to: on Windows a loopback port
    /// that takes only these links' connections; elsewhere the island's socket.
    fn remote_target_address(self: &Arc<Self>, inner: &mut Inner) -> Option<String> {
        #[cfg(windows)]
        {
            if inner.remote.bridge.is_none() {
                let allowed = Arc::downgrade(self);
                let handler = Arc::downgrade(self);
                let port = remote::bridge::serve(
                    move |pid| {
                        allowed.upgrade().is_some_and(|service| {
                            let inner = service.inner.lock().unwrap();
                            inner.remote.is_peer(Some(pid))
                        })
                    },
                    move |connection| {
                        if let Some(service) = handler.upgrade() {
                            service.on_hook(connection);
                        }
                    },
                )
                .ok()?;
                inner.remote.bridge = Some(port);
            }
            inner.remote.bridge.map(|port| format!("127.0.0.1:{port}"))
        }
        #[cfg(not(windows))]
        {
            let _ = inner;
            Some(hooks::transport::endpoint(hooks::APP_ID))
        }
    }

    /// One look at a host: its sessions for the next tick, and the new lines
    /// of their transcripts now.
    fn ingest_remote(&self, alias: &str, snapshot: remote::Snapshot) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.remote.links.contains_key(alias) {
            return;
        }
        let t = now();
        let changed = inner
            .remote
            .snapshots
            .get(alias)
            .map(|s| s.records.iter().map(|r| &r.session_id).collect::<Vec<_>>())
            != Some(snapshot.records.iter().map(|r| &r.session_id).collect());
        let owners: HashMap<&String, &String> = snapshot
            .transcripts
            .iter()
            .map(|(id, path)| (path, id))
            .collect();
        let policy = inner.engine.policy();
        let mut facts_by_session: Vec<(String, Vec<models::TranscriptFact>)> = Vec::new();
        for chunk in &snapshot.chunks {
            let Some(session) = owners.get(&chunk.path) else {
                continue;
            };
            let key = (alias.to_string(), chunk.path.clone());
            let entry = inner.remote.tails.entry(key).or_insert_with(|| {
                (
                    remote::Tail::default(),
                    TranscriptReader::new(policy, &chunk.path),
                )
            });
            let (lines, restarted) = entry.0.take(chunk);
            if restarted && chunk.offset == 0 {
                entry.1 = TranscriptReader::new(policy, &chunk.path);
            }
            let mut facts = Vec::new();
            for line in lines {
                let line = remote::shift_line(&line, snapshot.clock_offset);
                facts.extend(entry.1.parse(&line, t, &NoPricing));
            }
            if !facts.is_empty() {
                facts_by_session.push(((*session).clone(), facts));
            }
        }
        let live: HashSet<String> = snapshot.transcripts.values().cloned().collect();
        inner
            .remote
            .tails
            .retain(|(a, path), _| a != alias || live.contains(path));
        for (session, facts) in facts_by_session {
            inner.engine.ingest_facts(&session, &facts, t);
        }
        inner.remote.snapshots.insert(alias.to_string(), snapshot);
        drop(inner);
        // A session that came or went shows at once, not at the next tick.
        if changed {
            self.tick();
        }
    }

    /// The connection and the host's process number of a remote session.
    fn remote_target(&self, session: &str) -> Option<(Arc<remote::Link>, i32)> {
        let inner = self.inner.lock().unwrap();
        let current = inner.sessions.iter().find(|s| s.id == session)?;
        let alias = current.remote.as_ref()?;
        Some((inner.remote.links.get(alias)?.clone(), current.pid))
    }

    /// None when the session is not on a remote host.
    fn reply_remote(&self, session: &str, text: &str) -> Option<Option<&'static str>> {
        let (link, pid) = {
            let inner = self.inner.lock().unwrap();
            let current = inner.sessions.iter().find(|s| s.id == session)?;
            let alias = current.remote.clone()?;
            if !models::can_reply(current) {
                return Some(Some(
                    if current.phase.is_waiting() || current.waiting_for.is_some() {
                        "waiting"
                    } else {
                        "unreachable"
                    },
                ));
            }
            if inner.engine.permission(session).is_some()
                || inner.pending.values().any(|p| p.session == session)
            {
                return Some(Some("waiting"));
            }
            match inner.remote.links.get(&alias) {
                Some(link) => (link.clone(), current.pid),
                None => return Some(Some("unreachable")),
            }
        };
        if text.trim().is_empty() {
            return Some(Some("unreachable"));
        }
        let refusal = link.type_reply(pid, session, text);
        if refusal.is_none() {
            self.acknowledge(session);
        }
        Some(refusal)
    }

    fn remote_statuses(&self, hosts: &[String]) -> Vec<RemoteHostStatus> {
        let inner = self.inner.lock().unwrap();
        hosts
            .iter()
            .map(|alias| {
                let link = inner.remote.links.get(alias);
                RemoteHostStatus {
                    alias: alias.clone(),
                    state: link.map(|l| l.state.lock().unwrap().clone()),
                    busy: inner.remote.busy.contains(alias),
                    failure: inner.remote.failures.get(alias).cloned(),
                }
            })
            .collect()
    }

    fn change_remote_hooks(&self, alias: &str, install: bool) -> Result<(), String> {
        let link = {
            let mut inner = self.inner.lock().unwrap();
            let link = inner
                .remote
                .links
                .get(alias)
                .cloned()
                .ok_or("The host isn’t connected")?;
            if !inner.remote.busy.insert(alias.to_string()) {
                return Err("Already working on it".into());
            }
            inner.remote.failures.remove(alias);
            link
        };
        let _ = self.app.emit("sessions-remote-hosts", ());
        let result = if install {
            let info = link.info.lock().unwrap().clone();
            let info = match info {
                Some(info) => Ok(info),
                None => remote::one_shot(
                    alias,
                    &format!(
                        "probe {} {}",
                        remote::socket_name(hooks::APP_ID),
                        hooks::APP_ID
                    ),
                    &[],
                )
                .and_then(|lines| {
                    remote::probe(&lines).ok_or_else(|| "The host can’t be reached".to_string())
                }),
            };
            info.and_then(|info| remote::install(alias, &info, hooks::APP_ID))
        } else {
            remote::remove(alias, hooks::APP_ID)
        };
        if result.is_ok() {
            link.mark_hooks(install);
            // The hooks write to the socket the connection forwards; a fresh
            // one is sure to have it.
            if install {
                link.reconnect();
            }
        }
        {
            let mut inner = self.inner.lock().unwrap();
            inner.remote.busy.remove(alias);
            if let Err(reason) = &result {
                inner
                    .remote
                    .failures
                    .insert(alias.to_string(), reason.clone());
            }
        }
        let _ = self.app.emit("sessions-remote-hosts", ());
        result
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHostStatus {
    alias: String,
    /// None while the switch is off or the host was just added.
    state: Option<remote::State>,
    busy: bool,
    failure: Option<String>,
}

#[tauri::command]
pub fn sessions_remote_hosts_status(
    service: tauri::State<'_, Shared>,
    hosts: Vec<String>,
) -> Vec<RemoteHostStatus> {
    service.remote_statuses(&hosts)
}

/// Adds the island's hooks to a host's Claude Code settings, or takes them out.
#[tauri::command]
pub async fn sessions_remote_hosts_change(
    service: tauri::State<'_, Shared>,
    alias: String,
    install: bool,
) -> Result<(), String> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.change_remote_hooks(&alias, install))
        .await
        .map_err(|e| e.to_string())?
}

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
    plan_mode: Option<PlanMode>,
) -> bool {
    let decision = match (decision, plan_mode) {
        (UiDecision::Allow, Some(mode)) => Decision::ApprovePlan(mode),
        (UiDecision::Allow, None) => Decision::Allow,
        (UiDecision::Always, _) => Decision::AllowAlways,
        (UiDecision::Deny, _) => Decision::Deny,
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

/// Why the last change to each agent's hooks failed, for Settings.
static AGENT_HOOK_FAILURES: Mutex<BTreeMap<&'static str, String>> = Mutex::new(BTreeMap::new());
/// One sync at a time: the ticker's and a switch's may meet.
static AGENT_HOOK_SYNC: Mutex<()> = Mutex::new(());

/// Keeps every agent's hooks in step, with no button to press
/// (ClaudeSessionService.syncAgentHooks).
fn sync_agent_hooks(hidden: &HashSet<models::Provider>) {
    let _one = AGENT_HOOK_SYNC.lock().unwrap();
    let (Some(home), Ok(exe)) = (hooks::installer::home_dir(), hooks::current_program()) else {
        return;
    };
    let endpoint = hooks::transport::endpoint(hooks::APP_ID);
    let found: Vec<models::Provider> = models::Provider::HOOK_AGENTS
        .into_iter()
        .filter(|p| hook_agents::installer::is_present(*p, &home))
        .collect();
    let (install, remove) = hook_agents::installer::plan(&found, hidden, |p| {
        hook_agents::installer::state(p, &home, &exe, &endpoint, hooks::APP_ID)
    });
    for (provider, on) in install
        .into_iter()
        .map(|p| (p, true))
        .chain(remove.into_iter().map(|p| (p, false)))
    {
        let result = if on {
            hook_agents::installer::install(provider, &home, &exe, &endpoint, hooks::APP_ID)
        } else {
            hook_agents::installer::remove(provider, &home, hooks::APP_ID)
        };
        let mut failures = AGENT_HOOK_FAILURES.lock().unwrap();
        match result {
            Ok(()) => failures.remove(provider.key()),
            Err(e) => failures.insert(provider.key(), e),
        };
    }
}

/// For Settings › Content › Sessions: each hook agent's hooks or plugin.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHooksStatus {
    agent: &'static str,
    state: hook_agents::installer::State,
    present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<String>,
}

#[tauri::command]
pub fn sessions_agent_hooks_status() -> Vec<AgentHooksStatus> {
    let (Some(home), Ok(exe)) = (hooks::installer::home_dir(), hooks::current_program()) else {
        return Vec::new();
    };
    let endpoint = hooks::transport::endpoint(hooks::APP_ID);
    models::Provider::HOOK_AGENTS
        .iter()
        .map(|&provider| {
            let state =
                hook_agents::installer::state(provider, &home, &exe, &endpoint, hooks::APP_ID);
            AgentHooksStatus {
                agent: provider.key(),
                state,
                present: hook_agents::installer::is_present(provider, &home),
                failure: AGENT_HOOK_FAILURES
                    .lock()
                    .unwrap()
                    .get(provider.key())
                    .cloned(),
            }
        })
        .collect()
}

#[tauri::command]
pub fn sessions_agent_hooks_install(agent: String, install: bool) -> Result<(), String> {
    let provider = models::Provider::from_key(&agent)
        .filter(|p| p.reports_through_hooks())
        .ok_or("unknown agent")?;
    let home = hooks::installer::home_dir().ok_or("no home folder")?;
    let result = if install {
        let exe = hooks::current_program()?;
        let endpoint = hooks::transport::endpoint(hooks::APP_ID);
        hook_agents::installer::install(provider, &home, &exe, &endpoint, hooks::APP_ID)
    } else {
        hook_agents::installer::remove(provider, &home, hooks::APP_ID)
    };
    let mut failures = AGENT_HOOK_FAILURES.lock().unwrap();
    match &result {
        Ok(()) => failures.remove(provider.key()),
        Err(e) => failures.insert(provider.key(), e.clone()),
    };
    result
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

/// What a `claude` command answered, as the failure to show; None when it worked.
fn command_failure(output: Option<background::Output>) -> Option<String> {
    match output {
        Some(output) if output.ok => None,
        Some(output) if !output.error.is_empty() => Some(output.error),
        _ => Some("unavailable".to_owned()),
    }
}

/// `claude stop` or `claude rm` on a background session. What went wrong, if anything.
fn run_on_job(service: &Shared, session: &str, verb: &str) -> Option<String> {
    let Some(details) = service.background_of(session) else {
        return Some("unavailable".to_owned());
    };
    let arguments = [verb.to_owned(), details.job];
    let output = background::claude(
        &arguments,
        details.config_home.as_deref(),
        None,
        Duration::from_secs(15),
    );
    Service::list_background_jobs(service, true);
    command_failure(output)
}

#[tauri::command]
pub async fn sessions_background_stop(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<Option<String>, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_on_job(&service, &session_id, "stop"))
        .await
        .map_err(|_| ())
}

/// Deletes a background session for good, and the worktree Claude Code made
/// for it. A worktree with changes found nowhere else is kept, and the
/// session with it, unless `anyway` says to lose them.
#[tauri::command]
pub async fn sessions_background_remove(
    service: tauri::State<'_, Shared>,
    session_id: String,
    anyway: Option<bool>,
) -> Result<super::worktree::Removal, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        remove_job(&service, &session_id, anyway.unwrap_or(false))
    })
    .await
    .map_err(|_| ())
}

fn remove_job(service: &Shared, session: &str, anyway: bool) -> super::worktree::Removal {
    use super::worktree::{Removal, discard_changes};
    let Some(details) = service.background_of(session) else {
        return Removal::Failed {
            message: "unavailable".to_owned(),
        };
    };
    let cwd = {
        let inner = service.inner.lock().unwrap();
        inner
            .sessions
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.cwd.clone())
    };
    let outcome = super::worktree::remove_claude_job(
        |arguments| {
            let mut all = vec!["rm".to_owned(), details.job.clone()];
            all.extend_from_slice(arguments);
            background::claude(
                &all,
                details.config_home.as_deref(),
                None,
                Duration::from_secs(15),
            )
        },
        cwd.as_deref().unwrap_or(""),
        anyway,
        discard_changes,
    );
    Service::list_background_jobs(service, true);
    outcome
}

/// Its recent output, read off what `claude logs` prints. None when there is
/// none to read: a stopped session keeps no screen.
#[tauri::command]
pub async fn sessions_background_logs(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<Option<String>, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let details = service.background_of(&session_id)?;
        let arguments = ["logs".to_owned(), details.job.clone()];
        let home = details.config_home.as_deref();
        let mut text = background::claude(&arguments, home, None, Duration::from_secs(15))
            .filter(|output| output.ok)
            .map(|output| background::screen_text(&output.out));
        // Its last output may be nothing but a spinner; the screen itself still reads.
        if !details.ended && !text.as_deref().is_some_and(background::is_readable) {
            text = background::screen(&details.job, home).or(text);
        }
        text
    })
    .await
    .map_err(|_| ())
}

#[tauri::command]
pub async fn sessions_recent_folders() -> Result<Vec<background::RecentFolder>, ()> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(home) = hooks::installer::home_dir() else {
            return Vec::new();
        };
        background::recent_folders(&registry::homes(&home), &home)
    })
    .await
    .map_err(|_| ())
}

#[tauri::command]
pub async fn sessions_pick_folder(app: AppHandle<Wry>) -> Result<Option<String>, ()> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        use tauri_plugin_dialog::DialogExt;
        let mut builder = app.dialog().file();
        if let Some(window) = app.get_webview_window(crate::island::LABEL) {
            builder = builder.set_parent(&window);
        }
        builder
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|_| ())
}

/// The models each agent can start with, for the New session page: the
/// default first (the island words it), then each model.
#[derive(Serialize)]
pub struct NewSessionModels {
    claude: Vec<background::NewSessionModel>,
    codex: Vec<background::NewSessionModel>,
}

#[tauri::command]
pub async fn sessions_new_models(config_home: Option<String>) -> Result<NewSessionModels, ()> {
    tauri::async_runtime::spawn_blocking(move || {
        let claude_folder = config_home
            .map(std::path::PathBuf::from)
            .or_else(|| hooks::installer::home_dir().map(|h| h.join(".claude")));
        let setting = claude_folder
            .and_then(|f| std::fs::read(f.join("settings.json")).ok())
            .and_then(|data| background::claude_setting(&data));
        let codex_home = crate::sessions::codex::client::codex_home();
        let cache = codex_home
            .as_ref()
            .and_then(|h| std::fs::read(h.join("models_cache.json")).ok());
        let config = codex_home
            .as_ref()
            .and_then(|h| std::fs::read_to_string(h.join("config.toml")).ok());
        NewSessionModels {
            claude: background::claude_models(setting.as_deref()),
            codex: background::codex_models(cache.as_deref(), config.as_deref()),
        }
    })
    .await
    .map_err(|_| ())
}

/// Starts Claude Code, Codex or another agent in a folder, in the background
/// or in the person's terminal, with a model or the agent's default. With
/// `worktree`, a folder in a git repository gets the session its own
/// worktree: Claude Code makes it with `--worktree`, the island makes it for
/// the rest. "notTrusted", "needsPrompt", "codexNotStarted", "unavailable",
/// "worktreeFailed[: what git said]" or the agent's own words when it didn't
/// start.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn sessions_start(
    service: tauri::State<'_, Shared>,
    folder: String,
    config_home: Option<String>,
    prompt: String,
    background: bool,
    agent: Option<String>,
    model: Option<String>,
    worktree: Option<bool>,
    worktree_name: Option<String>,
) -> Result<Option<String>, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let model = model.as_deref().filter(|m| !m.is_empty());
        let hook_agent = agent
            .as_deref()
            .and_then(models::Provider::from_key)
            .filter(|p| p.reports_through_hooks());
        let codex = agent.as_deref() == Some("codex");
        // A background session has nobody to type to it.
        if background && hook_agent.is_none() && prompt.trim().is_empty() {
            return Some("needsPrompt".to_owned());
        }
        let mut folder = folder;
        let mut claude_worktree = None;
        let root = worktree
            .unwrap_or(false)
            .then(|| super::worktree::state(&folder).root().map(str::to_owned))
            .flatten();
        if let Some(root) = root {
            let name =
                super::worktree::reserve_name(worktree_name.as_deref(), prompt.trim(), &root);
            if codex || hook_agent.is_some() {
                match super::worktree::create(&root, &name) {
                    Ok(path) => folder = super::worktree::inside(&path, &folder, &root),
                    Err(said) if said.is_empty() => return Some("worktreeFailed".to_owned()),
                    Err(said) => return Some(format!("worktreeFailed: {said}")),
                }
            } else {
                claude_worktree = Some(name);
            }
        }
        if codex {
            return start_codex(&service, &folder, &prompt, background, model);
        }
        // Gemini CLI, Copilot CLI, OpenCode and cursor-agent run only in a
        // console, where their hooks or plugin report them.
        if let Some(provider) = hook_agent {
            let started = hook_agents::command_name(provider)
                .and_then(crate::agent_usage::agent_executable)
                .is_some_and(|exe| {
                    self::background::agent_in_terminal(
                        &exe,
                        &hook_agents::start_arguments(provider, &prompt),
                        Some(&folder),
                    )
                });
            return (!started).then(|| "unavailable".to_owned());
        }
        let arguments = self::background::start_arguments(
            &prompt,
            background,
            model,
            claude_worktree.as_deref(),
        );
        let home = config_home.as_deref();
        let failure = if background {
            let output =
                self::background::claude(&arguments, home, Some(&folder), Duration::from_secs(30));
            // Claude Code asks only in a terminal whether a folder can be trusted.
            let untrusted = output
                .as_ref()
                .is_some_and(|o| !o.ok && o.error.contains("not trusted"));
            if untrusted {
                Some("notTrusted".to_owned())
            } else {
                command_failure(output)
            }
        } else if self::background::in_terminal(&arguments, home, Some(&folder)) {
            None
        } else {
            Some("unavailable".to_owned())
        };
        Service::list_background_jobs(&service, true);
        failure
    })
    .await
    .map_err(|_| ())
}

/// Whether a folder is in a git repository, and whether its checkout has
/// changes a new worktree won't start with, for the New session page.
#[tauri::command]
pub async fn sessions_repo_state(folder: String) -> Result<super::worktree::RepoState, ()> {
    tauri::async_runtime::spawn_blocking(move || super::worktree::state(&folder))
        .await
        .map_err(|_| ())
}

/// The folder of the session's worktree, when it works in one.
fn worktree_of(service: &Shared, session: &str) -> Option<String> {
    let inner = service.inner.lock().unwrap();
    let cwd = &inner.sessions.iter().find(|s| s.id == session)?.cwd;
    super::worktree::worktree_folder(cwd)
}

/// Removes the worktree the island made for a session, keeping its branch.
/// One with changes stays unless `anyway` says to lose them.
#[tauri::command]
pub async fn sessions_worktree_remove(
    service: tauri::State<'_, Shared>,
    session_id: String,
    anyway: bool,
) -> Result<super::worktree::Removal, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || match worktree_of(&service, &session_id) {
        Some(folder) => super::worktree::remove(&folder, anyway),
        None => super::worktree::Removal::Failed {
            message: String::new(),
        },
    })
    .await
    .map_err(|_| ())
}

/// The branch a session's worktree is on, for Copy branch.
#[tauri::command]
pub async fn sessions_worktree_branch(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<Option<String>, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let folder = worktree_of(&service, &session_id)?;
        std::path::Path::new(&folder)
            .exists()
            .then(|| super::worktree::branch(&folder))
            .flatten()
    })
    .await
    .map_err(|_| ())
}

/// Opens the session's worktree in Explorer.
#[tauri::command]
pub async fn sessions_worktree_open(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<bool, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = worktree_of(&service, &session_id) else {
            return false;
        };
        if !std::path::Path::new(&folder).is_dir() {
            return false;
        }
        #[cfg(windows)]
        {
            std::process::Command::new("explorer.exe")
                .arg(&folder)
                .spawn()
                .is_ok()
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("open")
                .arg(&folder)
                .spawn()
                .is_ok()
        }
    })
    .await
    .map_err(|_| ())
}

/// Codex in a console, or as a thread its shared app-server runs with no
/// terminal, started first when it isn't running.
fn start_codex(
    service: &Shared,
    folder: &str,
    prompt: &str,
    background: bool,
    model: Option<&str>,
) -> Option<String> {
    if !background {
        let arguments = self::background::codex_start_arguments(prompt, model);
        return (!self::background::codex_in_terminal(&arguments, Some(folder)))
            .then(|| "unavailable".to_owned());
    }
    if prompt.trim().is_empty() {
        return Some("needsPrompt".to_owned());
    }
    let client = service.codex()?;
    if !client.connected() {
        let arguments = self::background::CODEX_DAEMON_START.map(str::to_owned);
        let started = self::background::codex(&arguments, Duration::from_secs(30));
        if !started.is_some_and(|o| o.ok) {
            return Some("codexNotStarted".to_owned());
        }
        // The client looks for the daemon's socket every few seconds.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !client.connected() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let Some(answer) = client.start_thread(folder, model, prompt.trim()) else {
        return Some("codexNotStarted".to_owned());
    };
    match answer.recv_timeout(Duration::from_secs(30)) {
        Ok(None) => None,
        Ok(Some(words)) if !words.is_empty() => Some(words),
        _ => Some("unavailable".to_owned()),
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

/// Whether every one of these sessions shows in front in its own terminal
/// tab or pane, so the waiting sound keeps quiet (#210). It asks the
/// terminals, which takes a moment, so it runs off the main thread.
#[tauri::command]
pub async fn sessions_terminal_shown(
    service: tauri::State<'_, Shared>,
    session_ids: Vec<String>,
) -> Result<bool, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A burst of many requests at once is not worth asking every terminal for.
        (1..=16).contains(&session_ids.len())
            && session_ids
                .iter()
                .all(|id| service.terminal_shown(id) == Some(true))
    })
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
