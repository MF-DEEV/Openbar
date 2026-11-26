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
use super::sample;
use super::transcript::{self, NoPricing, TranscriptReader};
use crate::hooks::{self, transport::HookConnection};
use crate::hosts::{self, Located};

/// Ticks while anything is going on, and when all is quiet.
const ACTIVE_TICK: Duration = Duration::from_secs(1);
const QUIET_TICK: Duration = Duration::from_secs(3);
/// A permission hook is let go a little before Claude Code's 600 s timeout.
/// How long a Kiro CLI turn is quiet before its log is read for an end (#238).
const KIRO_QUIET: Time = 3.0;
const HOOK_PATIENCE: f64 = 590.0;
/// A line longer than this is not one worth reading.
const MAX_LINE: usize = 32 * 1024 * 1024;
const CHUNK: usize = 4 * 1024 * 1024;

#[cfg(not(any(windows, target_os = "linux")))]
const PID_DOMAINS: &[&str] = registry::MAC_PID_DOMAINS;

/// The `pidDomain`s of records written on this machine.
fn pid_domains() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    return crate::linux::sessions::pid_domains();
    #[cfg(windows)]
    return windows_pid_domains();
    #[cfg(not(any(windows, target_os = "linux")))]
    PID_DOMAINS
}

/// "win32" and "win32:<this machine's host name>", the name Node's
/// `os.hostname()` gives (GetComputerNameExW's DNS host name).
#[cfg(windows)]
fn windows_pid_domains() -> &'static [&'static str] {
    static DOMAINS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    DOMAINS.get_or_init(|| {
        let host = unsafe {
            use windows::Win32::System::SystemInformation::{
                ComputerNameDnsHostname, GetComputerNameExW,
            };
            let mut size = 0u32;
            let _ = GetComputerNameExW(ComputerNameDnsHostname, None, &mut size);
            let mut buffer = vec![0u16; size as usize + 1];
            GetComputerNameExW(
                ComputerNameDnsHostname,
                Some(windows::core::PWSTR(buffer.as_mut_ptr())),
                &mut size,
            )
            .ok()
            .map(|_| String::from_utf16_lossy(&buffer[..size as usize]))
        };
        registry::windows_pid_domains(host.as_deref())
            .into_iter()
            .map(|domain| &*Box::leak(domain.into_boxed_str()))
            .collect()
    })
}

/// How often Agent View is asked: more often while it has sessions.
const JOBS_EVERY: Time = 10.0;
const JOBS_EVERY_QUIETLY: Time = 60.0;
/// How long a session's checkout is trusted before HEAD is read again (#233).
const HEAD_FRESH: Time = 3.0;

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
    /// The input of a ZCode question, handed back with the answers.
    input: Option<serde_json::Map<String, serde_json::Value>>,
}

struct Inner {
    /// The config folder each local Claude session's record lives in, by
    /// session id, and the accounts on this PC (#216).
    session_homes: HashMap<String, String>,
    accounts: Vec<super::accounts::Account>,
    #[cfg(any(windows, target_os = "linux"))]
    local_codex: Vec<super::codex::local::LocalSession>,
    /// Gemini CLI's, Copilot CLI's, OpenCode's and Cursor's sessions.
    hook_agents: HookAgents,
    /// Where each Hermes session runs: its hooks name only its gateway (#244).
    hermes_places: super::hermes::Places,
    /// Which of Qwen Code's events end or carry on its turns (#239).
    qwen_turns: super::qwen::Turns,
    /// Cline's sessions, read from its own store (#241).
    cline: super::cline::Watcher,
    /// When Cline's store last changed, so it is read again only then.
    cline_stamp: Vec<Option<std::time::SystemTime>>,
    /// The CLI that shows each session Cline's hub runs, as the hub last said.
    cline_shown: Arc<Mutex<HashMap<String, i32>>>,
    cline_asking: Arc<std::sync::atomic::AtomicBool>,
    agent_pending: HashMap<String, AgentPending>,
    inspection: super::inspection::Gate,
    /// What each ask looks like it does, for the queue's Allow all (#269).
    risks: super::risk::Cache,
    /// Each OpenCode's plugin listening for replies, by its process.
    listeners: HashMap<i32, HookConnection>,
    /// The Mistral Vibe sessions a New session runs through the island's
    /// app-server proxy, by the proxy's process. Those approvals and
    /// questions are answered here; Vibe's hooks only hear of them, so
    /// theirs are dropped.
    proxied: HashMap<String, u32>,
    /// The agents whose sessions the list shows.
    hidden_agents: HashSet<models::Provider>,
    /// The Claude app's switches: its Code tab's sessions shown, Cowork's
    /// files read, and what Cowork last listed (#246).
    claude_desktop_shown: bool,
    cowork_read: bool,
    cowork: super::claude_desktop::Cowork,
    cowork_rows: Vec<models::Session>,
    cowork_events: Vec<models::SessionEvent>,
    branches: HashMap<String, (Option<String>, Time)>,
    /// Each local session's checkout, read again every few seconds (#233).
    heads: HashMap<String, (hook_agents::Head, Time)>,
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
    /// The sample request onboarding shows (#168), which no agent asked, and
    /// what it has to tell the island next.
    sample: Option<Session>,
    sample_events: Vec<SessionEvent>,
    sample_round: u64,
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
    /// Each remote host's Codex, by alias (issue #200).
    remote_codex: Mutex<HashMap<String, Arc<CodexClient>>>,
    /// Wakes the ticker for an early publish (Codex says something changed).
    wake: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl Service {
    pub fn start(app: AppHandle<Wry>) -> Arc<Self> {
        let service = Arc::new(Service {
            app,
            inner: Mutex::new(Inner {
                session_homes: HashMap::new(),
                accounts: Vec::new(),
                #[cfg(any(windows, target_os = "linux"))]
                local_codex: Vec::new(),
                hook_agents: HookAgents::default(),
                hermes_places: Default::default(),
                cline: Default::default(),
                cline_stamp: Vec::new(),
                cline_shown: Default::default(),
                cline_asking: Default::default(),
                qwen_turns: Default::default(),
                agent_pending: HashMap::new(),
                inspection: Default::default(),
                risks: {
                    let home = hooks::installer::home_dir()
                        .map(|h| h.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let managed = super::plan_modes::managed_settings();
                    let reader_home = home.clone();
                    super::risk::Cache::new(
                        &home,
                        Box::new(move |config, cwd| {
                            super::risk::Rules::read(config, Some(cwd), &managed, &reader_home)
                        }),
                    )
                },
                listeners: HashMap::new(),
                proxied: HashMap::new(),
                hidden_agents: HashSet::new(),
                claude_desktop_shown: true,
                cowork_read: false,
                cowork: Default::default(),
                cowork_rows: Vec::new(),
                cowork_events: Vec::new(),
                branches: HashMap::new(),
                heads: HashMap::new(),
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
                sample: None,
                sample_events: Vec::new(),
                sample_round: 0,
            }),
            codex: Mutex::new(None),
            remote_codex: Mutex::new(HashMap::new()),
            wake: Mutex::new(None),
        });
        let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
        *service.wake.lock().unwrap() = Some(wake_tx.clone());
        let codex = CodexClient::start(move || {
            let _ = wake_tx.send(());
        });
        *service.codex.lock().unwrap() = Some(codex);
        hooks::page_service::set_app(service.app.clone());
        let hook_side = service.clone();
        if let Err(e) = hooks::transport::serve(hooks::APP_ID, move |connection| {
            hook_side.on_hook(connection)
        }) {
            eprintln!("openbar: Claude Code hooks unavailable: {e}");
        } else {
            crate::diagnostics::log::info("sessions", "listening for hooks");
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
                            // Hooks lost at launch or since come back (#236).
                            hooks::page_service::tick();
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
        #[cfg(any(windows, target_os = "linux"))]
        let local_codex = super::codex::local::discover();
        let home = hooks::installer::home_dir();
        let by_home = home
            .as_deref()
            .map(registry_files_by_home)
            .unwrap_or_default();
        let per_home = records_by_home(&by_home);
        note_account_folder(&per_home, home.as_deref());
        let home_of: HashMap<i32, String> = per_home
            .iter()
            .flat_map(|(folder, records)| {
                records
                    .iter()
                    .map(move |r| (r.pid, folder.to_string_lossy().into_owned()))
            })
            .collect();
        let accounts = home
            .as_deref()
            .map(|home| {
                let found: Vec<PathBuf> =
                    by_home.iter().map(|(folder, _)| folder.clone()).collect();
                super::accounts::all(&found, &super::accounts::settings(), home)
            })
            .unwrap_or_default();
        let files: Vec<RegistryFile> = by_home.into_iter().flat_map(|(_, files)| files).collect();
        let scan = registry::scan(&files, pid_domains(), process_start);
        let mut inner = self.inner.lock().unwrap();
        #[cfg(any(windows, target_os = "linux"))]
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
            // The Claude app's Code tab runs it: the app shows it, and has no
            // terminal to type a reply into (#246).
            if super::claude_desktop::is_code_tab(record.entrypoint.as_deref()) {
                record.host = HostKind::App;
                record.accepts_replies = false;
                record.host_name = Some(super::claude_desktop::NAME.to_owned());
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
        inner.session_homes = records
            .iter()
            .filter_map(|r| {
                home_of
                    .get(&r.pid)
                    .map(|h| (r.session_id.clone(), h.clone()))
            })
            .collect();
        inner.accounts = accounts;
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
        Self::read_cowork(&mut inner, t);
        Self::read_cline(&mut inner, home.as_deref(), t);
        self.release_stale_hooks(&mut inner, t);
        self.publish(&mut inner, t)
    }

    /// Reads Cline's session store while its switch is on (#241): every tick
    /// while one of its sessions is listed, else when the store changed or
    /// every few ticks. Cline's hub is asked, off this thread, which CLI
    /// shows each session it runs.
    fn read_cline(inner: &mut Inner, home: Option<&Path>, t: Time) {
        let policy = inner.engine.policy();
        let alive = |pid: i32| hosts::process::is_running(pid as u32);
        let read = |path: &Path| super::cline::transcript(path);
        let (Some(home), false) = (home, inner.hidden_agents.contains(&models::Provider::Cline))
        else {
            if !inner.cline.is_empty() {
                let events = inner
                    .cline
                    .scan(&[], t, policy, &|_| None, &|_| "", &alive, &read);
                for event in events {
                    inner.hook_agents.ingest(event, t);
                }
            }
            return;
        };
        let database = super::cline::database(home, &super::cline::process_env);
        let stamp: Vec<Option<std::time::SystemTime>> = [
            database.clone(),
            PathBuf::from(format!("{}-wal", database.to_string_lossy())),
        ]
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect();
        if inner.cline.is_empty() && stamp == inner.cline_stamp && inner.ticks % 3 != 0 {
            return;
        }
        inner.cline_stamp = stamp;
        let records = super::cline::records(&database);
        let hub = (!records.is_empty())
            .then(|| super::cline::hub(home))
            .flatten();
        let on_hub: HashSet<String> = records
            .iter()
            .filter(|r| hub.as_ref().is_some_and(|h| h.pid == r.pid))
            .map(|r| r.id.clone())
            .collect();
        // A hub session's CLI is asked for when it is new, then every few ticks: it
        // may have quit. A new one waits a moment for the answer, so its row shows now.
        let mut shown = inner.cline_shown.lock().unwrap().clone();
        let new = on_hub.iter().any(|id| !shown.contains_key(id));
        if let Some(hub) = hub
            && !on_hub.is_empty()
            && (new || inner.ticks % 3 == 0)
            && !inner
                .cline_asking
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            let into = inner.cline_shown.clone();
            let asking = inner.cline_asking.clone();
            let (done, answered) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                if let Some(found) = super::cline::hub::shown(&hub, Duration::from_secs(2)) {
                    *into.lock().unwrap() = found;
                }
                asking.store(false, std::sync::atomic::Ordering::SeqCst);
                let _ = done.send(());
            });
            if new && answered.recv_timeout(Duration::from_millis(500)).is_ok() {
                shown = inner.cline_shown.lock().unwrap().clone();
            }
        }
        let events = inner.cline.scan(
            &records,
            t,
            policy,
            &|r| {
                if on_hub.contains(&r.id) {
                    shown.get(&r.id).copied()
                } else {
                    Some(r.pid)
                }
            },
            &|r| {
                if r.is_extension() {
                    super::cline::EXTENSION_CLIENT
                } else if on_hub.contains(&r.id) {
                    super::cline::HUB_CLIENT
                } else {
                    super::cline::CLI_CLIENT
                }
            },
            &alive,
            &read,
        );
        for event in events {
            inner.hook_agents.ingest(event, t);
        }
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
        let hermes: std::collections::HashSet<String> = inner
            .hook_agents
            .entries()
            .filter(|e| e.provider == models::Provider::Hermes)
            .map(|e| e.session_id.clone())
            .collect();
        inner.hermes_places.keep(&hermes);
        self.end_cancelled_kiro_turns(inner, t);
        // Qwen Code stops waiting for its hook after a minute and asks in its
        // own dialog, which the island then answers with keys (#239).
        let dialogs: Vec<(String, String)> = inner
            .agent_pending
            .iter()
            .filter(|(request, p)| {
                t - p.since > super::qwen::HOLD_LIMIT
                    && inner.hook_agents.entry(&p.session).is_some_and(|e| {
                        e.provider == models::Provider::Qwen
                            && e.permission
                                .as_ref()
                                .is_some_and(|q| &q.request_id == *request)
                    })
            })
            .map(|(r, p)| (r.clone(), p.session.clone()))
            .collect();
        for (request, session) in dialogs {
            inner.agent_pending.remove(&request);
            inner.hook_agents.move_to_dialog(&session, &request);
        }
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
        inner
            .proxied
            .retain(|_, proxy| hosts::process::is_running(*proxy));
    }

    /// One event of Gemini CLI, Copilot CLI, Cursor, OpenCode, Pi, Mistral Vibe,
    /// Devin CLI or Google Antigravity.
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
        // Qwen Code's helpers after a turn, and its idle notes inside one,
        // aren't the session's (#239).
        if provider == models::Provider::Qwen
            && let Some(session) = decoded.events.first().map(|e| e.session_id.clone())
        {
            if decoded
                .events
                .iter()
                .any(|e| matches!(e.kind, hook_agents::Kind::Ended))
            {
                inner.qwen_turns.forget(&session);
            } else if let Some(fact) = decoded.qwen
                && !inner
                    .qwen_turns
                    .keep(&session, decoded.turn.as_deref(), fact, t)
            {
                return;
            }
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
        let mut pid = if decoded.from_proxy {
            0
        } else {
            hook_agents::agent_process(&chain, decoded.pid)
        };
        // Kiro CLI's hooks run in its agent server, which has no terminal or
        // console: its terminal UI above it has (#238).
        if provider == models::Provider::Kiro && pid > 0 {
            pid = super::kiro::terminal_process(&chain, pid as u32) as i32;
        }
        // Grok CLI's hook may outlive the shell that ran it (Windows'
        // PowerShell doesn't wait for this app): Grok names its own process.
        if provider == models::Provider::Grok
            && pid == decoded.pid
            && let Some(path) = decoded.transcript.as_deref()
            && let Some(session) = decoded.events.first().map(|e| e.session_id.clone())
            && let Some(own) = hook_agents::grok_session_pid(Path::new(path), &session)
        {
            pid = own;
        }
        if let Some(attached) = &decoded.attached {
            if let Some(proxy) = connection.peer_pid {
                inner
                    .proxied
                    .insert(hook_agents::id(provider, attached), proxy);
            }
            return;
        }
        if decoded.listens {
            inner.listeners.insert(pid, connection);
            return;
        }
        let mut connection = Some(connection);
        // A request the person chose Always proceed for goes ahead now (#234).
        let mut proceeded: Option<String> = None;
        if let Some((session, request)) = &decoded.permission
            && let Some(reason) = super::auto_mode::settings().passes(provider, false)
            && let Some(asked) = allowable(&decoded.events, request)
            && let Some(body) = match provider {
                models::Provider::Vibe => hook_agents::vibe_reply(&Decision::Allow, &[]),
                models::Provider::Zcode => hook_agents::zcode_reply(&Decision::Allow, None),
                models::Provider::Qwen => super::qwen::reply(&Decision::Allow),
                models::Provider::Kiro => super::kiro::reply(&Decision::Allow),
                _ => hook_agents::opencode_reply(&Decision::Allow),
            }
            && let Some(connection) = connection.take()
        {
            if let Some(old) = inner.agent_pending.remove(request) {
                drop(old);
            }
            let _ = connection.reply(&body);
            super::auto_mode::LOG.add(
                super::auto_mode::LetThrough::of(&asked, t, reason),
                &hook_agents::id(provider, session),
            );
            proceeded = Some(request.clone());
        }
        if let Some((session, request)) = &decoded.permission
            && let Some(connection) = connection.take()
        {
            inner
                .inspection
                .invalidate(&hook_agents::id(provider, session));
            inner.agent_pending.insert(
                request.clone(),
                AgentPending {
                    connection,
                    session: hook_agents::id(provider, session),
                    since: t,
                    input: decoded.input.clone(),
                },
            );
        }
        drop(connection);
        let previews = inner.engine.policy().previews;
        // For the diagnostics journal (#230): each session's state before these events.
        let mut heard: Vec<HeardEvent> = Vec::new();
        let mut before: HashMap<String, Option<models::SessionPhase>> = HashMap::new();
        for event in &decoded.events {
            let id = event.id();
            let phase = phase_of(&inner, &id);
            before.entry(id).or_insert(phase);
        }
        for mut event in decoded.events {
            // Let through: nothing waits on the person (#234).
            if let hook_agents::Kind::Permission(p) | hook_agents::Kind::HeldPermission(p) =
                &event.kind
                && proceeded.as_deref() == Some(p.request_id.as_str())
            {
                continue;
            }
            // Qwen Code's dialog for a question or plan the island shows a card for (#239).
            if provider == models::Provider::Qwen
                && matches!(event.kind, hook_agents::Kind::Question)
                && inner
                    .hook_agents
                    .entry(&event.id())
                    .is_some_and(|e| e.permission.is_some())
            {
                continue;
            }
            // A session run through the proxy is answered there: Vibe's hooks only hear of its requests.
            if provider == models::Provider::Vibe
                && !decoded.from_proxy
                && inner.proxied.contains_key(&event.id())
                && matches!(
                    event.kind,
                    hook_agents::Kind::Permission(_) | hook_agents::Kind::Question
                )
            {
                continue;
            }
            event.pid = if pid > 0 { pid } else { event.pid };
            // Hermes's gateway runs its hooks: the session's client and folder
            // come from Hermes's own notes, and a chat it answers isn't listed (#244).
            if provider == models::Provider::Hermes {
                let hermes_home = hooks::installer::home_dir()
                    .map(|h| super::hermes::home(&h))
                    .unwrap_or_default();
                let fresh = matches!(
                    event.kind,
                    hook_agents::Kind::Prompt(_)
                        | hook_agents::Kind::Stopped { .. }
                        | hook_agents::Kind::Started
                );
                if event.kind == hook_agents::Kind::Ended {
                    inner.hermes_places.forget(&event.session_id);
                }
                let lookup = super::hermes::Lookup {
                    record: &|s| super::hermes::record(s, &hermes_home),
                    notes: &|| super::hermes::notes(&hermes_home),
                    running: &super::hermes::running,
                    app: &super::hermes::app,
                    alive: &|p| hosts::process::is_running(p as u32),
                };
                let Some(place) = inner
                    .hermes_places
                    .place(&event.session_id, t, fresh, &lookup)
                else {
                    continue;
                };
                event.pid = place.pid;
                event.cwd = place.record.as_ref().and_then(|r| r.cwd.clone());
                // Where the session is shown, as Hermes recorded it when it was made.
                if let Some(client) = place
                    .record
                    .as_ref()
                    .and_then(|r| r.client())
                    .filter(|c| super::hermes::CLIENTS.contains(c))
                    && inner
                        .hook_agents
                        .entry(&event.id())
                        .is_none_or(|e| e.client.as_deref() != Some(client))
                {
                    let mut shown = event.clone();
                    shown.kind = hook_agents::Kind::Client(client.to_owned());
                    inner.hook_agents.ingest(shown, t);
                }
                // A title is made from the prompt: only read with previews on.
                if previews && let Some(title) = place.record.as_ref().and_then(|r| r.title.clone())
                {
                    let mut named = event.clone();
                    named.kind = hook_agents::Kind::Title(title);
                    inner.hook_agents.ingest(named, t);
                }
                // Hermes's hooks carry no reply: with previews on, its state.db tells the last one.
                if previews
                    && let hook_agents::Kind::Stopped {
                        last_text: None,
                        failed,
                    } = event.kind
                {
                    event.kind = hook_agents::Kind::Stopped {
                        last_text: super::hermes::last_reply(&event.session_id, &hermes_home),
                        failed,
                    };
                }
            }
            // Devin CLI names no folder: the agent's own is the session's.
            if event.cwd.is_none()
                && event.pid > 0
                && inner
                    .hook_agents
                    .entry(&event.id())
                    .is_none_or(|e| e.cwd.is_empty())
            {
                event.cwd = hosts::process::metadata(event.pid as u32)
                    .map(|m| m.cwd)
                    .filter(|c| !c.is_empty());
            }
            // A Qwen Code session heard first after its start: its log names the model (#239).
            if provider == models::Provider::Qwen
                && matches!(
                    event.kind,
                    hook_agents::Kind::Prompt(_) | hook_agents::Kind::Stopped { .. }
                )
                && inner
                    .hook_agents
                    .entry(&event.id())
                    .is_none_or(|e| e.model().is_none())
                && let Some(model) = decoded
                    .transcript
                    .as_deref()
                    .and_then(|path| super::qwen::model(Path::new(path)))
            {
                let mut named = event.clone();
                named.kind = hook_agents::Kind::Model(model);
                inner.hook_agents.ingest(named, t);
            }
            if provider == models::Provider::Kiro {
                self.kiro_details(&mut inner, &mut event, previews, t);
            }
            if provider == models::Provider::Kimi {
                // Kimi tells its client with every event: only a change is news.
                if let hook_agents::Kind::Client(client) = &event.kind
                    && inner
                        .hook_agents
                        .entry(&event.id())
                        .is_some_and(|e| e.client.as_deref() == Some(client.as_str()))
                {
                    continue;
                }
                let kimi = hooks::installer::home_dir().map(|home| super::kimi::folder(&home));
                // A session whose hooks Kimi Code's server runs is held there,
                // and answered there (#240).
                if let Some(kimi) = &kimi
                    && inner
                        .hook_agents
                        .entry(&event.id())
                        .is_none_or(|e| e.client.is_none())
                    && super::kimi::serving(event.pid.max(0) as u32, kimi, &|pid| {
                        hosts::process::is_running(pid)
                    })
                    .is_some()
                {
                    let mut held = event.clone();
                    held.kind = hook_agents::Kind::Client(super::kimi::KAP_CLIENT.into());
                    inner.hook_agents.ingest(held, t);
                }
                // A turn's end: the model that answered, and with previews on
                // its answer, from the session's log.
                if let hook_agents::Kind::Stopped {
                    last_text: None,
                    failed,
                } = event.kind
                    && let Some(folder) = kimi
                        .as_deref()
                        .and_then(|k| super::kimi::session_folder_in(&event.session_id, k))
                {
                    let (model, reply) =
                        super::kimi::last_turn(&super::kimi::wire(&folder), previews);
                    if let Some(model) = model
                        && inner
                            .hook_agents
                            .entry(&event.id())
                            .is_none_or(|e| e.model() != Some(model.as_str()))
                    {
                        let mut named = event.clone();
                        named.kind = hook_agents::Kind::Model(model);
                        inner.hook_agents.ingest(named, t);
                    }
                    if reply.is_some() {
                        event.kind = hook_agents::Kind::Stopped {
                            last_text: reply,
                            failed,
                        };
                    }
                }
            }
            if provider == models::Provider::Droid
                && let Some(path) = decoded.transcript.as_deref()
            {
                // The model can change at any prompt: the session's own settings name it (#242).
                if matches!(
                    event.kind,
                    hook_agents::Kind::Prompt(_)
                        | hook_agents::Kind::Stopped { .. }
                        | hook_agents::Kind::Started
                ) && let Some(home) = hooks::installer::home_dir()
                    && let Some(model) = super::droid::model(Path::new(path), &home)
                    && inner
                        .hook_agents
                        .entry(&event.id())
                        .is_none_or(|e| e.model() != Some(model.as_str()))
                {
                    let mut named = event.clone();
                    named.kind = hook_agents::Kind::Model(model);
                    inner.hook_agents.ingest(named, t);
                }
                // Droid's Stop names its reply: with previews on, its transcript holds the text.
                if previews
                    && let hook_agents::Kind::Stopped {
                        last_text: None,
                        failed,
                    } = event.kind
                    && let Some(message) = decoded.reply_id.as_deref()
                {
                    event.kind = hook_agents::Kind::Stopped {
                        last_text: super::droid::reply(Path::new(path), message),
                        failed,
                    };
                }
            }
            // Grok's hooks name the model only for xAI's own: its session's summary names any.
            if provider == models::Provider::Grok
                && matches!(
                    event.kind,
                    hook_agents::Kind::Prompt(_) | hook_agents::Kind::Stopped { .. }
                )
                && inner
                    .hook_agents
                    .entry(&event.id())
                    .is_none_or(|e| e.model().is_none())
                && let Some(model) = decoded
                    .transcript
                    .as_deref()
                    .and_then(|path| hook_agents::grok_model(Path::new(path)))
            {
                let mut named = event.clone();
                named.kind = hook_agents::Kind::Model(model);
                inner.hook_agents.ingest(named, t);
            }
            // Vibe's hooks carry no reply: with previews on, its session's log tells the last one.
            if provider == models::Provider::Vibe
                && previews
                && let hook_agents::Kind::Stopped {
                    last_text: None,
                    failed,
                } = event.kind
                && let Some(folder) = &decoded.transcript
            {
                event.kind = hook_agents::Kind::Stopped {
                    last_text: hook_agents::vibe_last_reply(Path::new(folder)),
                    failed,
                };
            }
            // Antigravity's hooks carry no reply: with previews on, its transcript tells the last one.
            if provider == models::Provider::Antigravity
                && previews
                && let hook_agents::Kind::Stopped {
                    last_text: None,
                    failed,
                } = event.kind
                && let Some(path) = &decoded.transcript
            {
                event.kind = hook_agents::Kind::Stopped {
                    last_text: hook_agents::antigravity_last_reply(Path::new(path)),
                    failed,
                };
            }
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
            heard.push((
                event.provider,
                event.id(),
                crate::diagnostics::journal::agent_label(&event.kind),
            ));
            inner.hook_agents.ingest(event, t);
        }
        self.publish(&mut inner, t);
        for (provider, id, label) in heard {
            crate::diagnostics::journal::record(crate::diagnostics::journal::make(
                provider,
                label,
                &id,
                before.get(&id).copied().flatten(),
                phase_of(&inner, &id),
                chrono::Utc::now(),
            ));
        }
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

    /// This PC's Codex and every host's.
    fn codex_clients(&self) -> Vec<Arc<CodexClient>> {
        let mut clients: Vec<Arc<CodexClient>> = self.codex().into_iter().collect();
        let remote = self.remote_codex.lock().unwrap();
        let mut aliases: Vec<&String> = remote.keys().collect();
        aliases.sort();
        clients.extend(aliases.into_iter().map(|a| remote[a].clone()));
        clients
    }

    /// The Codex client a session's thread belongs to.
    fn codex_owner(&self, session: &str) -> Option<Arc<CodexClient>> {
        self.codex_clients().into_iter().find(|c| c.owns(session))
    }

    /// Reads Cowork's session files while its switch is on: every tick
    /// while one of its sessions works or waits, otherwise every few (#246).
    fn read_cowork(inner: &mut Inner, t: Time) {
        if !inner.cowork_read || !inner.claude_desktop_shown {
            return;
        }
        let busy = inner
            .cowork_rows
            .iter()
            .any(|s| s.phase.is_active() || s.phase.is_waiting());
        if !busy && inner.ticks % 3 != 1 {
            return;
        }
        let policy = inner.engine.policy();
        let (rows, events) = inner.cowork.scan(
            &super::claude_desktop::cowork_folders(),
            t,
            policy,
            super::claude_desktop::is_running(),
        );
        inner.cowork_rows = rows;
        inner.cowork_events.extend(events);
    }

    fn publish(&self, inner: &mut Inner, t: Time) -> bool {
        let mut snapshot = inner.engine.snapshot(t);
        // The Code tab's sessions follow the Claude app's switch (#246).
        if !inner.claude_desktop_shown {
            let hidden: HashSet<String> = snapshot
                .sessions
                .iter()
                .filter(|s| s.origin == Some(super::claude_desktop::Origin::Desktop))
                .map(|s| s.id.clone())
                .collect();
            snapshot.sessions.retain(|s| !hidden.contains(&s.id));
            snapshot.events.retain(|e| !hidden.contains(event_id(e)));
        }
        snapshot.sessions.extend(inner.cowork_rows.iter().cloned());
        snapshot.events.append(&mut inner.cowork_events);
        if let Some(home) = hooks::installer::home_dir() {
            for session in &mut snapshot.sessions {
                inner.agent_colors.decorate(session, &home, t);
            }
        }
        for codex in self.codex_clients() {
            let (sessions, events) = codex.snapshot(t, inner.engine.policy());
            snapshot.sessions.extend(sessions);
            snapshot.events.extend(events);
        }
        #[cfg(any(windows, target_os = "linux"))]
        for local in &inner.local_codex {
            if !snapshot.sessions.iter().any(|s| s.id == local.id) {
                let mut session = super::codex::local::as_session(local, inner.engine.policy());
                if let Some(located) = hosts::locate(local.pid) {
                    session.host = host_kind(&located.host);
                    session.host_name = located.host.display_name();
                    session.accepts_replies = session.phase == models::SessionPhase::Idle
                        && hosts::types_codex(&located.host);
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
                    // OpenCode and Pi take replies through their plugin; Cursor's and
                    // Antigravity's apps keep their own chat, so only `cursor-agent`
                    // and `agy` in a terminal take one.
                    let accepts = match entry.provider {
                        models::Provider::Opencode | models::Provider::Pi => true,
                        // Cline's extension takes a reply only in its own panel;
                        // a session its hub runs takes one through the hub (#241).
                        models::Provider::Cline
                            if entry.client.as_deref() == Some(super::cline::EXTENSION_CLIENT) =>
                        {
                            false
                        }
                        models::Provider::Cline
                            if entry.client.as_deref() == Some(super::cline::HUB_CLIENT) =>
                        {
                            true
                        }
                        // Kimi Code's server takes them for `kimi web` and its
                        // app; its VS Code extension only in its own panel (#240).
                        models::Provider::Kimi
                            if entry.client.as_deref() == Some(super::kimi::KAP_CLIENT) =>
                        {
                            true
                        }
                        models::Provider::Kimi
                            if entry.client.as_deref() == Some(super::kimi::VSCODE_CLIENT) =>
                        {
                            false
                        }
                        models::Provider::Cursor | models::Provider::Antigravity => {
                            matches!(
                                kind,
                                HostKind::WindowsTerminal
                                    | HostKind::Conhost
                                    | HostKind::Wezterm
                                    | HostKind::Tmux
                                    | HostKind::Zellij
                                    | HostKind::Kitty
                            )
                        }
                        _ => host.is_some_and(|l| l.host.accepts_replies()),
                    };
                    hook_agents::Place {
                        host: kind,
                        takes_keys: host.is_some_and(|l| hosts::takes_keys(&l.host)),
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
        if let Some(sample) = &inner.sample {
            snapshot.sessions.push(sample.clone());
        }
        snapshot.events.append(&mut inner.sample_events);
        let mut sessions = models::sorted(self.with_background(inner, snapshot.sessions, t));
        // The branch as the checkout has it now, so a checkout inside a
        // session shows on its row; a host's sessions keep what it said.
        inner.heads.retain(|_, (_, at)| t - *at < HEAD_FRESH * 4.0);
        for session in &mut sessions {
            if session.remote.is_some() || session.cwd.is_empty() {
                continue;
            }
            let head = match inner.heads.get(&session.cwd) {
                Some((head, at)) if t - at < HEAD_FRESH => head.clone(),
                _ => {
                    let head = hook_agents::read_head(&session.cwd);
                    inner.heads.insert(session.cwd.clone(), (head.clone(), t));
                    head
                }
            };
            session.branch = hook_agents::live_branch(session.branch.take(), &head);
        }
        // The account each Claude session runs under, and its name while
        // more than one is in use (#216).
        for session in &mut sessions {
            if session.provider == models::Provider::Claude && session.remote.is_none() {
                session.account = inner.session_homes.get(&session.id).cloned().or_else(|| {
                    session
                        .background
                        .as_ref()
                        .and_then(|b| b.config_home.clone())
                });
            }
        }
        if let Some(home) = hooks::installer::home_dir() {
            let mut labels = super::accounts::labels(&sessions, &inner.accounts, &home);
            // A second Codex home and a tool added by hand say which they are (#236).
            let page = hooks::page::settings();
            for (id, label) in hooks::page::labels(&sessions, &page.folders, &page.tools) {
                labels.entry(id).or_insert(label);
            }
            for session in &mut sessions {
                session.account_label = labels.get(&session.id).cloned();
            }
        }
        for session in &mut sessions {
            session.account_id = session
                .account
                .as_deref()
                .map(|a| super::accounts::canonical(Path::new(a)));
        }
        super::accounts::note_in_use(&sessions, &inner.accounts);
        // What the island let through for each session (#234).
        let listed: HashSet<String> = sessions.iter().map(|s| s.id.clone()).collect();
        super::auto_mode::LOG.prune(&listed, t);
        super::auto_mode::LOG.decorate(&mut sessions);
        inner.risks.assess(&mut sessions, t);
        let live = sessions
            .iter()
            // Every ask a session holds keeps its inspection, not only the first (#269).
            .flat_map(|s| {
                s.permission.iter().chain(&s.later_asks).filter_map(|p| {
                    p.content
                        .as_ref()
                        .map(|c| (s.id.clone(), c.metadata(p).revision))
                })
            })
            .collect::<Vec<_>>();
        inner.inspection.retain(&live);
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
        // Codex is heard through its app-server, not hooks: what the
        // diagnostics journal keeps of it is each thread's change of state.
        for session in sessions.iter().filter(|s| s.codex_server()) {
            let before = inner
                .sessions
                .iter()
                .find(|s| s.id == session.id)
                .map(|s| s.phase);
            if before != Some(session.phase) {
                crate::diagnostics::journal::record(crate::diagnostics::journal::make(
                    models::Provider::Codex,
                    (if before.is_none() { "listed" } else { "update" }, None),
                    &session.id,
                    before,
                    Some(session.phase),
                    chrono::Utc::now(),
                ));
            }
        }
        inner.sessions = sessions;
        busy
    }

    /// Where each listed session runs, for a diagnostics file (#230).
    pub(crate) fn host_names(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .sessions
            .iter()
            .map(|s| s.host_name.clone().unwrap_or_else(|| "unknown".into()))
            .collect()
    }

    /// Each remote host's state in a word or two, for a diagnostics file (#230).
    pub(crate) fn remote_states(&self, aliases: &[String]) -> Vec<(String, String)> {
        self.remote_statuses(aliases)
            .into_iter()
            .map(|status| {
                let state = match &status.state {
                    None => "off".to_string(),
                    Some(remote::State::Connecting) => "connecting".into(),
                    Some(remote::State::Connected { .. }) => "connected".into(),
                    Some(remote::State::Disconnected { reason }) => {
                        format!("disconnected: {reason}")
                    }
                };
                (status.alias, state)
            })
            .collect()
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
                    hook_agents::starts_with_prompt(**p)
                        && hook_agents::command_name(**p).is_some_and(|name| {
                            crate::agent_usage::agent_executable(name).is_some()
                        })
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
        // Devin CLI and Grok CLI run the hooks in Claude Code's settings too;
        // their sessions are heard through hooks of their own, so these are
        // let go unanswered.
        if hook_agents::is_grok_payload(&connection.payload)
            || hook_agents::is_devin_payload(&connection.payload)
            || connection
                .peer_pid
                .and_then(hosts::process::ancestors)
                .is_some_and(|chain| {
                    let chain: Vec<(u32, String)> =
                        chain.into_iter().map(|e| (e.pid, e.name)).collect();
                    let agent = hook_agents::agent_process(&chain, 0);
                    chain.iter().any(|(pid, name)| {
                        *pid as i32 == agent && hook_agents::is_devin_process(name)
                    })
                })
        {
            return;
        }
        let request_id = format!("{:032x}", rand_id());
        let mut inner = self.inner.lock().unwrap();
        let policy = inner.engine.policy();
        let Some(mut message) =
            HookMessage::decode(&connection.payload, &request_id, now(), policy)
        else {
            return;
        };
        // With the Claude app's switch off, its Code tab's sessions are left
        // to the app, which asks there as it would without the island (#246).
        if !inner.claude_desktop_shown
            && super::claude_desktop::is_code_tab(
                inner
                    .engine
                    .record(&message.fact.session_id)
                    .and_then(|r| r.entrypoint.as_deref()),
            )
        {
            return;
        }
        let mut connection = Some(connection);
        // A session that runs without asking is never held (#234): its tool
        // goes ahead now, and its page lists it.
        if let Some(asked) = message.lets_through() {
            let mode = message.fact.mode.clone().or_else(|| {
                inner
                    .engine
                    .mode(&message.fact.session_id)
                    .map(str::to_owned)
            });
            if let Some(reason) = super::auto_mode::settings().passes(
                models::Provider::Claude,
                super::auto_mode::is_auto_claude(mode.as_deref()),
            ) {
                let body = hook_message::reply(
                    &Decision::Allow,
                    None,
                    None,
                    message.approval_input.as_ref(),
                );
                if let Some(connection) = connection.take() {
                    let _ = connection.reply(&body);
                }
                super::auto_mode::LOG.add(
                    super::auto_mode::LetThrough::of(asked, now(), reason),
                    &message.fact.session_id,
                );
                message = message.settled();
            }
        }
        let permission = message.is_permission_request();
        // A remote session's transcript is on its host, read through its connection.
        let remote = inner
            .remote
            .is_peer(connection.as_ref().and_then(|c| c.peer_pid));
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
        let label = crate::diagnostics::journal::claude_label(&message.fact.event);
        let before = phase_of(&inner, &session);
        let heard = session.clone();
        let modes = match &message.fact.event {
            HookEvent::PermissionRequested(asked) => asked.plan_modes.clone(),
            _ => Vec::new(),
        };
        inner.engine.ingest_hook(message.fact, now());
        if permission && let Some(connection) = connection {
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
        crate::diagnostics::journal::record(crate::diagnostics::journal::make(
            models::Provider::Claude,
            label,
            &heard,
            before,
            phase_of(&inner, &heard),
            chrono::Utc::now(),
        ));
    }

    // MARK: Actions

    fn answer(&self, session: &str, request: &str, decision: Decision) -> bool {
        self.answer_inspected(session, request, decision, None, None)
    }

    fn answer_inspected(
        &self,
        session: &str,
        request: &str,
        decision: Decision,
        revision: Option<&str>,
        token: Option<&str>,
    ) -> bool {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.hook_agents.owns(session) {
                let Some(p) = inner
                    .hook_agents
                    .entry(session)
                    .and_then(|e| e.permission.as_ref())
                else {
                    return false;
                };
                if p.request_id != request
                    || (!matches!(decision, Decision::Deny | Decision::Answer(_))
                        && !inner.inspection.allows(session, p, revision, token))
                {
                    return false;
                }
                // Hermes's classic prompt takes Return for the Allow once it
                // has picked and arrows down to Deny; anything else (a
                // question, its Ink UI, its app, a terminal that takes no
                // keys) goes through Hermes's gateway (#244).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Hermes)
                    .cloned()
                {
                    let keys = hook_agents::hermes_takes_keys(entry.client.as_deref())
                        .then(|| hermes_keys(&decision, p))
                        .flatten();
                    let located = inner.hosts.get(&entry.pid).cloned();
                    if let (Some(keys), Some(located)) =
                        (keys, located.filter(|l| hosts::takes_keys(&l.host)))
                    {
                        drop(inner);
                        if hosts::press_all(entry.pid as u32, &located, &keys).is_err() {
                            return false;
                        }
                    } else {
                        let Some(answer) = super::hermes::gateway::answer(&decision, p) else {
                            return false;
                        };
                        let Some(hermes) = crate::agent_usage::agent_executable("hermes") else {
                            return false;
                        };
                        drop(inner);
                        if !super::hermes::gateway::send(
                            &answer,
                            &entry.session_id,
                            &hermes,
                            std::time::Duration::from_secs(8),
                        ) {
                            crate::diagnostics::log::info(
                                "sessions",
                                "hermes: the gateway took no answer",
                            );
                            return false;
                        }
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Qwen Code's own dialog takes a question's or a plan's
                // answer, as keys where its terminal takes them (#239).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Qwen)
                    .cloned()
                    && (entry.in_dialog || super::qwen::answered_by_keys(p))
                {
                    let Some(steps) = super::qwen::keys(&decision, p, entry.in_dialog) else {
                        return false;
                    };
                    let Some(located) = inner
                        .hosts
                        .get(&entry.pid)
                        .cloned()
                        .filter(|l| hosts::takes_keys(&l.host))
                    else {
                        return false;
                    };
                    drop(inner);
                    if press_qwen(entry.pid as u32, &located, &steps).is_err() {
                        return false;
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Cline's CLI takes keys in its terminal: Return for Approve, `n`
                // for Deny, a question's number. Where they can't be pressed, an
                // approval goes through Cline's hub. Its store tells when it moves
                // on; its extension asks only in its own panel (#241).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Cline)
                    .cloned()
                {
                    if entry.client.as_deref() == Some(super::cline::EXTENSION_CLIENT) {
                        return false;
                    }
                    let located = inner
                        .hosts
                        .get(&entry.pid)
                        .cloned()
                        .filter(|l| hosts::takes_keys(&l.host));
                    let asked = p.clone();
                    drop(inner);
                    let answered = if let (Some(located), Some(keys)) =
                        (located, super::cline::keys(&decision, &asked))
                    {
                        press_droid(entry.pid as u32, &located, &keys.keys).is_ok()
                            && keys.typed.is_none_or(|text| {
                                std::thread::sleep(Duration::from_millis(200));
                                // The answer row reads keys, not a paste, in a Windows console.
                                #[cfg(windows)]
                                let typed =
                                    hosts::deliver_codex(entry.pid as u32, &located, &text, &[]);
                                #[cfg(not(windows))]
                                let typed = hosts::deliver(entry.pid as u32, &located, &text, &[]);
                                typed.is_ok()
                            })
                    } else {
                        let approved = match decision {
                            Decision::Allow => true,
                            Decision::Deny => false,
                            _ => return false,
                        };
                        entry.client.as_deref() == Some(super::cline::HUB_CLIENT)
                            && !asked.asks_questions()
                            && hooks::installer::home_dir()
                                .and_then(|home| super::cline::hub(&home))
                                .is_some_and(|hub| {
                                    super::cline::hub::approve(
                                        approved,
                                        &entry.session_id,
                                        request,
                                        &hub,
                                        Duration::from_secs(5),
                                    )
                                })
                    };
                    if !answered {
                        return false;
                    }
                    // Cline's store tells when it moved on: a card whose keys
                    // didn't take stays to be answered again.
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Kimi Code's server takes the answer for `kimi web` and its
                // app; its terminal UI takes a choice's number; its VS Code
                // extension asks in its own panel. Its hooks tell when it
                // moves on (#240).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Kimi)
                    .cloned()
                {
                    if entry.client.as_deref() == Some(super::kimi::VSCODE_CLIENT) {
                        return false;
                    }
                    let asked = p.clone();
                    let located = inner
                        .hosts
                        .get(&entry.pid)
                        .cloned()
                        .filter(|l| hosts::takes_keys(&l.host));
                    drop(inner);
                    let kimi = hooks::installer::home_dir().map(|home| super::kimi::folder(&home));
                    let server = kimi.as_deref().and_then(|k| {
                        super::kimi::server(entry.pid.max(0) as u32, entry.client.as_deref(), k)
                    });
                    let answered = match (server, kimi) {
                        (Some(server), Some(kimi)) => {
                            super::kimi::token(&kimi).is_some_and(|token| {
                                super::kimi::answer(
                                    &decision,
                                    &asked,
                                    &entry.session_id,
                                    &server,
                                    &token,
                                )
                            })
                        }
                        _ => {
                            entry.client.as_deref() != Some(super::kimi::KAP_CLIENT)
                                && match (located, super::kimi::keys(&decision, &asked)) {
                                    (Some(located), Some(keys)) => {
                                        press_droid(entry.pid as u32, &located, &keys).is_ok()
                                    }
                                    _ => false,
                                }
                        }
                    };
                    if !answered {
                        return false;
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Kiro CLI's own dialog, once the island let its hook go:
                // Return allows, Down and Return deny (#238).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Kiro && e.in_dialog)
                    .cloned()
                {
                    let Some(keys) = super::kiro::keys(&decision) else {
                        return false;
                    };
                    let Some(located) = inner
                        .hosts
                        .get(&entry.pid)
                        .cloned()
                        .filter(|l| hosts::takes_keys(&l.host))
                    else {
                        return false;
                    };
                    drop(inner);
                    if press_droid(entry.pid as u32, &located, &keys).is_err() {
                        return false;
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Droid's prompts take keys: Return for its yes, arrows to its
                // no, numbers for questions and specs; its hooks tell when it
                // moves on (#242).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Droid)
                    .cloned()
                {
                    let Some(keys) = super::droid::keys(&decision, p) else {
                        return false;
                    };
                    let Some(located) = inner
                        .hosts
                        .get(&entry.pid)
                        .cloned()
                        .filter(|l| hosts::takes_keys(&l.host))
                    else {
                        return false;
                    };
                    drop(inner);
                    if press_droid(entry.pid as u32, &located, &keys).is_err() {
                        return false;
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // Grok's prompt takes Return for the yes it has picked and
                // Ctrl-C for no; its hooks tell when it moves on (#243).
                if let Some(entry) = inner
                    .hook_agents
                    .entry(session)
                    .filter(|e| e.provider == models::Provider::Grok)
                    .cloned()
                {
                    let Some(keys) = grok_keys(&decision) else {
                        return false;
                    };
                    let Some(located) = inner.hosts.get(&entry.pid).cloned() else {
                        return false;
                    };
                    drop(inner);
                    if hosts::press_all(entry.pid as u32, &located, &keys).is_err() {
                        return false;
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let t = now();
                    inner.hook_agents.resolve(session, request);
                    inner.hook_agents.acknowledge(session, t);
                    self.publish(&mut inner, t);
                    return true;
                }
                // OpenCode's plugin and ZCode's hook wait for the answer; the other agents ask in their own terminal.
                if !matches!(decision, Decision::Deny)
                    && inner
                        .agent_pending
                        .get(request)
                        .is_none_or(|p| now() - p.since > HOOK_PATIENCE)
                {
                    return false;
                }
                let provider = inner.hook_agents.entry(session).map(|e| e.provider);
                let vibe = provider == Some(models::Provider::Vibe);
                let zcode = provider == Some(models::Provider::Zcode);
                let qwen = provider == Some(models::Provider::Qwen);
                let kiro = provider == Some(models::Provider::Kiro);
                if vibe || zcode {
                    // A question is answered or skipped, never allowed; answers go only to a question.
                    let asks = p.asks_questions();
                    match &decision {
                        Decision::Allow | Decision::AllowAlways | Decision::ApprovePlan(_)
                            if asks =>
                        {
                            return false;
                        }
                        Decision::Answer(_) if !asks => return false,
                        _ => {}
                    }
                }
                let reply = if vibe {
                    let questions = inner
                        .hook_agents
                        .entry(session)
                        .and_then(|e| e.permission.as_ref())
                        .map(|p| p.questions.clone())
                        .unwrap_or_default();
                    hook_agents::vibe_reply(&decision, &questions)
                } else if zcode {
                    let input = inner
                        .agent_pending
                        .get(request)
                        .and_then(|p| p.input.clone());
                    hook_agents::zcode_reply(&decision, input.as_ref())
                } else if qwen {
                    super::qwen::reply(&decision)
                } else if kiro {
                    super::kiro::reply(&decision)
                } else {
                    hook_agents::opencode_reply(&decision)
                };
                let Some(reply) = reply else {
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
        if let Some(codex) = self.codex_owner(session) {
            let answer = match decision {
                Decision::Allow | Decision::ApprovePlan(_) => CodexAnswer::Allow,
                Decision::AllowAlways => CodexAnswer::AllowForSession,
                Decision::Deny => CodexAnswer::Deny,
                Decision::Answer(map) => CodexAnswer::Questions(map.into_iter().collect()),
            };
            let ok = codex.answer_inspected(session, request, answer, revision, token);
            if ok {
                codex.acknowledge(session, now());
            }
            let mut inner = self.inner.lock().unwrap();
            let t = now();
            self.publish(&mut inner, t);
            return ok;
        }
        let mut inner = self.inner.lock().unwrap();
        // Any ask the session holds, not only the first: the queue answers
        // them in any order (#269).
        let Some(p) = inner.engine.permission_by_id(session, request) else {
            return false;
        };
        if p.request_id != request
            || (!matches!(decision, Decision::Deny | Decision::Answer(_))
                && !inner.inspection.allows(session, p, revision, token))
        {
            return false;
        }
        let asks = inner
            .engine
            .permission_by_id(session, request)
            .is_some_and(|p| !p.questions.is_empty());
        let fits = match &decision {
            Decision::Answer(_) => asks,
            Decision::Allow | Decision::AllowAlways | Decision::ApprovePlan(_) => !asks,
            Decision::Deny => true,
        };
        if !matches!(decision, Decision::Deny)
            && inner
                .pending
                .get(request)
                .is_none_or(|p| now() - p.since > HOOK_PATIENCE)
        {
            return false;
        }
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
        for codex in self.codex_clients() {
            codex.acknowledge(session, t);
        }
        inner.hook_agents.acknowledge(session, t);
        inner.cowork.acknowledge(session, t);
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
            inner.cline.forget_content();
            inner.cowork.purge_content();
            for row in &mut inner.cowork_rows {
                row.title = None;
                row.last_text = None;
                row.chat.clear();
                row.name = row.project.clone();
            }
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
        // A host's Codex takes replies through its own server too, before
        // the host's tmux is looked at.
        if let Some(codex) = self.codex_owner(session) {
            // Codex takes replies through its own server: no terminal typing, no Esc risk.
            if codex.reply(session, text, images) {
                self.acknowledge(session);
                return None;
            }
            let inner = self.inner.lock().unwrap();
            return Some(codex_refusal(&inner.sessions, session));
        }
        if let Some(refusal) = self.reply_remote(session, text) {
            return refusal;
        }
        #[cfg(any(windows, target_os = "linux"))]
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
                if !hosts::types_codex(&located.host) {
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

    /// A reply to OpenCode or Pi goes to its listening plugin; one to Gemini
    /// CLI, Copilot CLI, `cursor-agent`, Mistral Vibe or Devin CLI is typed
    /// into its terminal and sent by
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
        // A session Cline's hub runs takes the reply there, queued after the
        // turn under way; the CLI shows it as typed (#241).
        if entry.provider == models::Provider::Cline
            && entry.client.as_deref() == Some(super::cline::HUB_CLIENT)
        {
            drop(inner);
            let sent = hooks::installer::home_dir()
                .and_then(|home| super::cline::hub(&home))
                .is_some_and(|hub| {
                    super::cline::hub::send(text, &entry.session_id, &hub, Duration::from_secs(5))
                });
            if sent {
                self.acknowledge(session);
            }
            return Some((!sent).then_some("unreachable"));
        }
        // Kimi Code's server takes the reply for `kimi web` and its app,
        // queued after the turn under way; its VS Code extension only in its
        // own panel (#240).
        if entry.provider == models::Provider::Kimi {
            if entry.client.as_deref() == Some(super::kimi::VSCODE_CLIENT) {
                return Some(Some("unreachable"));
            }
            let kimi = hooks::installer::home_dir().map(|home| super::kimi::folder(&home));
            if let Some(kimi) = kimi
                && let Some(server) =
                    super::kimi::server(entry.pid.max(0) as u32, entry.client.as_deref(), &kimi)
            {
                drop(inner);
                let sent = super::kimi::token(&kimi).is_some_and(|token| {
                    super::kimi::send(text, &entry.session_id, &server, &token)
                });
                if sent {
                    self.acknowledge(session);
                }
                return Some((!sent).then_some("unreachable"));
            }
            if entry.client.as_deref() == Some(super::kimi::KAP_CLIENT) {
                return Some(Some("unreachable"));
            }
        }
        if entry.provider.reports_through_plugin() {
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
        // Mistral Vibe's, Devin CLI's and Grok CLI's terminal UIs read a console's keys,
        // as Codex's does, so a paste's markers would reach them as text:
        // their reply goes in as plain keys and one Return.
        // Codex heard through a second home's hooks runs its own terminal UI (#236).
        let plain = matches!(
            entry.provider,
            models::Provider::Vibe
                | models::Provider::Devin
                | models::Provider::Codex
                | models::Provider::Grok
        ) && matches!(
            located.host,
            hosts::Host::WindowsTerminal { .. } | hosts::Host::Conhost
        );
        #[cfg(windows)]
        let plain_typed =
            plain.then(|| hosts::deliver_codex(entry.pid as u32, &located, text, &[]).is_ok());
        #[cfg(not(windows))]
        let plain_typed: Option<bool> = plain.then_some(false);
        let one_return = hook_agents::takes_one_return(entry.provider);
        let typed = plain_typed.unwrap_or_else(|| {
            hosts::deliver(entry.pid as u32, &located, text, &[]).is_ok()
                && (one_return || {
                    std::thread::sleep(Duration::from_millis(450));
                    hosts::deliver(entry.pid as u32, &located, "", &[]).is_ok()
                })
        });
        if typed {
            self.acknowledge(session);
        }
        Some((!typed).then_some("unreachable"))
    }

    /// Hides or shows each hook agent's sessions, as Settings say.
    /// The Claude app's two switches (#246): its Code tab's sessions, and
    /// reading Cowork's files, off until turned on.
    pub fn set_claude_desktop(&self, shown: bool, cowork: bool) {
        let mut inner = self.inner.lock().unwrap();
        if inner.claude_desktop_shown == shown && inner.cowork_read == cowork {
            return;
        }
        inner.claude_desktop_shown = shown;
        inner.cowork_read = cowork;
        if !shown || !cowork {
            inner.cowork_rows.clear();
            inner.cowork_events.clear();
            let _ = inner.cowork.scan(&[], 0.0, ContentPolicy::default(), false);
        }
        let t = now();
        self.publish(&mut inner, t);
    }

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

    /// Open, telling a tmux or zellij with no terminal attached from a
    /// window not found (#235).
    fn open(&self, session: &str) -> OpenResult {
        if self.open_found(session) {
            OpenResult::Opened
        } else if self.detached_of(session).is_some() {
            OpenResult::Detached
        } else {
            OpenResult::Missing
        }
    }

    /// The session's own host, from its pid.
    fn located_of(&self, session: &str) -> Option<(u32, hosts::Located)> {
        let inner = self.inner.lock().unwrap();
        let s = inner.sessions.iter().find(|s| s.id == session)?;
        let located = inner.hosts.get(&s.pid)?.clone();
        Some((s.pid as u32, located))
    }

    /// What attaches the session's tmux or zellij, when no terminal shows it.
    fn detached_of(&self, session: &str) -> Option<(String, Vec<String>)> {
        let (pid, located) = self.located_of(session)?;
        hosts::detached(pid, &located)
    }

    /// Attaches the session's detached tmux or zellij in a new terminal
    /// window. The command line is worked out here again, never taken from
    /// the page.
    fn attach_detached(&self, session: &str) -> bool {
        let Some((program, arguments)) = self.detached_of(session) else {
            return false;
        };
        let ok = background::agent_in_terminal(std::path::Path::new(&program), &arguments, None);
        if ok {
            self.acknowledge(session);
        }
        ok
    }

    /// Lets Qwen Code's and Kiro CLI's held requests go unanswered, so they
    /// ask in their own dialog: the person went to answer there (#239, #238).
    fn release_held(&self, session: &str) {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .hook_agents
            .entry(session)
            .is_none_or(|e| !matches!(e.provider, models::Provider::Qwen | models::Provider::Kiro))
        {
            return;
        }
        let held: Vec<String> = inner
            .agent_pending
            .iter()
            .filter(|(_, p)| p.session == session)
            .map(|(r, _)| r.clone())
            .collect();
        for request in held {
            // Dropped unanswered: the hook prints nothing, and Qwen Code asks
            // in its own dialog, which the island answers with keys.
            inner.agent_pending.remove(&request);
            inner.hook_agents.move_to_dialog(session, &request);
        }
        let t = now();
        self.publish(&mut inner, t);
    }

    /// What Kiro CLI's session files add to an event (#238): at a start, a
    /// prompt or a turn's end, its model and, with previews on, its title
    /// from `session.json`; at a turn's end, with previews on, the reply its
    /// log holds, which its `Stop` doesn't carry.
    fn kiro_details(
        &self,
        inner: &mut Inner,
        event: &mut hook_agents::AgentEvent,
        previews: bool,
        t: Time,
    ) {
        if !matches!(
            event.kind,
            hook_agents::Kind::Started
                | hook_agents::Kind::Prompt(_)
                | hook_agents::Kind::Stopped { .. }
        ) {
            return;
        }
        let Some(folder) = hooks::installer::home_dir().and_then(|home| {
            super::kiro::session_folder(&event.session_id, &super::kiro::folder(&home))
        }) else {
            return;
        };
        let (title, model) = super::kiro::details(&folder);
        let entry = inner.hook_agents.entry(&event.id()).cloned();
        if let Some(model) = model
            && entry
                .as_ref()
                .is_none_or(|e| e.model() != Some(model.as_str()))
        {
            let mut named = event.clone();
            named.kind = hook_agents::Kind::Model(model);
            inner.hook_agents.ingest(named, t);
        }
        if previews
            && let Some(title) = title
            && entry
                .as_ref()
                .is_none_or(|e| e.title() != Some(title.as_str()))
        {
            let mut named = event.clone();
            named.kind = hook_agents::Kind::Title(title);
            inner.hook_agents.ingest(named, t);
        }
        if previews
            && let hook_agents::Kind::Stopped {
                last_text: None,
                failed,
            } = event.kind
            && let Some(reply) = super::kiro::last_turn(&folder.join("messages.jsonl"), true).reply
        {
            event.kind = hook_agents::Kind::Stopped {
                last_text: Some(reply),
                failed,
            };
        }
    }

    /// Kiro CLI's turns the person cancelled with Escape while it worked:
    /// Kiro runs no hook for that, its session's log tells (#238).
    fn end_cancelled_kiro_turns(&self, inner: &mut Inner, t: Time) {
        let Some(kiro) = hooks::installer::home_dir().map(|home| super::kiro::folder(&home)) else {
            return;
        };
        let previews = inner.engine.policy().previews;
        let open: Vec<hook_agents::Entry> = inner
            .hook_agents
            .entries()
            .filter(|e| {
                e.provider == models::Provider::Kiro
                    && e.permission.is_none()
                    && e.turn_open_since().is_some()
                    && t - e.heard > KIRO_QUIET
            })
            .cloned()
            .collect();
        for entry in open {
            let Some(started) = entry.turn_open_since() else {
                continue;
            };
            let Some(folder) = super::kiro::session_folder(&entry.session_id, &kiro) else {
                continue;
            };
            let last = super::kiro::last_turn(&folder.join("messages.jsonl"), previews);
            // Times here are seconds; the log's are milliseconds.
            if !last.ended
                || last
                    .at
                    .is_none_or(|at| (at as f64) / 1000.0 < started - 2.0)
            {
                continue;
            }
            inner.hook_agents.ingest(
                hook_agents::AgentEvent {
                    provider: models::Provider::Kiro,
                    session_id: entry.session_id.clone(),
                    pid: entry.pid,
                    cwd: Some(entry.cwd.clone()),
                    date: t,
                    kind: hook_agents::Kind::Stopped {
                        last_text: last.reply,
                        failed: false,
                    },
                },
                t,
            );
        }
    }

    /// A Kimi Code session its own server holds (#240).
    fn kimi_served(&self, session: &str) -> Option<hook_agents::Entry> {
        let inner = self.inner.lock().unwrap();
        inner
            .hook_agents
            .entry(session)
            .filter(|e| {
                e.provider == models::Provider::Kimi
                    && e.client.as_deref() == Some(super::kimi::KAP_CLIENT)
            })
            .cloned()
    }

    fn open_found(&self, session: &str) -> bool {
        self.release_held(session);
        let desktop = {
            let inner = self.inner.lock().unwrap();
            inner
                .sessions
                .iter()
                .find(|s| s.id == session && s.origin.is_some())
                .cloned()
        };
        if let Some(found) = desktop {
            // The Claude app shows it; no terminal does (#246).
            let link = super::claude_desktop::app_session_id(
                &found,
                &super::claude_desktop::data_folders_here(),
            )
            .and_then(|id| super::claude_desktop::link(&id));
            let ok = super::claude_desktop::open(link.as_deref());
            if ok {
                self.acknowledge(session);
            }
            return ok;
        }
        if let Some(entry) = self.kimi_served(session) {
            // `kimi web` shows it in the browser, at its own page; the Kimi
            // Code app in its window, by its own process (#240).
            let kimi = hooks::installer::home_dir().map(|home| super::kimi::folder(&home));
            if let Some(kimi) = kimi
                && let Some(server) =
                    super::kimi::server(entry.pid.max(0) as u32, entry.client.as_deref(), &kimi)
            {
                let ok = if server.product == super::kimi::APP_PRODUCT {
                    super::kimi::raise_app(server.pid)
                } else {
                    let page = super::kimi::page(
                        &server,
                        &entry.session_id,
                        super::kimi::token(&kimi).as_deref(),
                    );
                    tauri_plugin_opener::open_url(page, None::<&str>).is_ok()
                };
                if ok {
                    self.acknowledge(session);
                    return true;
                }
            }
        }
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
                .find(|s| s.id == session && s.codex_server())
                .map(|s| {
                    (
                        s.name.clone(),
                        s.cwd.clone(),
                        inner
                            .sessions
                            .iter()
                            .filter(|other| other.id != session && other.codex_server())
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
        hosts::Host::Zellij { .. } => HostKind::Zellij,
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

/// Each config folder's running sessions.
fn records_by_home(by_home: &[(PathBuf, Vec<RegistryFile>)]) -> Vec<(PathBuf, Vec<SessionRecord>)> {
    by_home
        .iter()
        .map(|(claude, files)| {
            (
                claude.clone(),
                registry::scan(files, pid_domains(), process_start).records,
            )
        })
        .collect()
}

/// Stays with the account in use while it runs a session, else follows the
/// session that changed last; with none running, the one before stays.
fn note_account_folder(records: &[(PathBuf, Vec<SessionRecord>)], home: Option<&Path>) {
    let Ok(mut current) = ACCOUNT_FOLDER.lock() else {
        return;
    };
    // Before any is chosen the limits read ~/.claude's account.
    let previous = current.clone().or_else(|| home.map(|h| h.join(".claude")));
    if let Some(folder) = registry::account_home(records, previous.as_deref()) {
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
            if let Some(codex) = self.remote_codex.lock().unwrap().remove(&alias) {
                codex.stop();
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
            // The host's Codex, joined through a pipe to its socket while
            // the link says its daemon runs (issue #200).
            let ready = Arc::downgrade(&link);
            let host = alias.clone();
            let connector: super::codex::client::Connector = Box::new(move || {
                let link = ready.upgrade()?;
                if !link.codex_ready() {
                    return None;
                }
                Some(
                    remote::CodexPipe::open(&host)
                        .map(|pipe| Box::new(pipe) as Box<dyn super::codex::client::Duplex>),
                )
            });
            let wake = self.wake.lock().unwrap().clone();
            let codex = CodexClient::start_remote(&alias, connector, move || {
                if let Some(wake) = &wake {
                    let _ = wake.send(());
                }
            });
            self.remote_codex
                .lock()
                .unwrap()
                .insert(alias.clone(), codex);
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
    revision: Option<String>,
    inspection_token: Option<String>,
) -> bool {
    let decision = match (decision, plan_mode) {
        (UiDecision::Allow, Some(mode)) => Decision::ApprovePlan(mode),
        (UiDecision::Allow, None) => Decision::Allow,
        (UiDecision::Always, _) => Decision::AllowAlways,
        (UiDecision::Deny, _) => Decision::Deny,
    };
    if session_id == sample::ID {
        return Service::answer_sample(
            service.inner(),
            &request_id,
            !matches!(decision, Decision::Deny),
        );
    }
    service.answer_inspected(
        &session_id,
        &request_id,
        decision,
        revision.as_deref(),
        inspection_token.as_deref(),
    )
}

/// Fetch only the authoritative pending request, never text supplied by the frontend.
#[tauri::command]
pub fn sessions_inspect_command(
    service: tauri::State<'_, Shared>,
    session_id: String,
    request_id: String,
    revision: String,
) -> Option<super::inspection::Inspection> {
    if let Some(codex) = service.codex_owner(&session_id) {
        return codex.inspect(&session_id, &request_id, &revision);
    }
    let mut inner = service.inner.lock().unwrap();
    let p = if inner.hook_agents.owns(&session_id) {
        inner.hook_agents.entry(&session_id)?.permission.clone()?
    } else {
        inner
            .engine
            .permission_by_id(&session_id, &request_id)?
            .clone()
    };
    if p.request_id != request_id {
        return None;
    }
    inner.inspection.open(&session_id, &p, &revision)
}

#[tauri::command]
pub fn sessions_close_inspection(service: tauri::State<'_, Shared>, token: String) {
    for codex in service.codex_clients() {
        codex.close_inspection(&token);
    }
    service.inner.lock().unwrap().inspection.close(&token);
}

// MARK: Sample request (#168)

impl Service {
    /// Shows a made-up permission request in the island, the way a real
    /// one arrives, so onboarding can show what will happen before any
    /// agent asks. Answering it goes nowhere.
    fn show_sample(&self) {
        let mut inner = self.inner.lock().unwrap();
        let t = now();
        inner.sample_round += 1;
        let home = hooks::installer::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default();
        let session = sample::waiting(inner.sample_round, &home, t);
        inner.sample_events.push(SessionEvent::NeedsPermission {
            id: session.id.clone(),
            tool: "Bash".to_owned(),
        });
        inner.sample = Some(session);
        self.publish(&mut inner, t);
    }

    fn dismiss_sample(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.sample.take().is_some() {
            inner.sample_round += 1;
            self.publish(&mut inner, now());
        }
    }

    /// The sample plays out the answer: an allowed command runs a moment
    /// and finishes, a denied one stops, then the sample leaves.
    fn answer_sample(service: &Shared, request: &str, allowed: bool) -> bool {
        let round = {
            let mut inner = service.inner.lock().unwrap();
            let Some(session) = inner.sample.take().filter(|s| {
                s.permission
                    .as_ref()
                    .is_some_and(|p| p.request_id == request)
            }) else {
                return false;
            };
            let t = now();
            inner.sample = Some(sample::answered(session, allowed, t));
            service.publish(&mut inner, t);
            inner.sample_round
        };
        let _ = service.app.emit("sessions://sample-answered", allowed);
        let later = move |delay: f64, service: Shared, change: fn(&mut Inner, Time)| {
            thread::spawn(move || {
                thread::sleep(Duration::from_secs_f64(delay));
                let mut inner = service.inner.lock().unwrap();
                if inner.sample_round != round || inner.sample.is_none() {
                    return;
                }
                let t = now();
                change(&mut inner, t);
                service.publish(&mut inner, t);
            });
        };
        if allowed {
            later(1.6, service.clone(), |inner, t| {
                if let Some(running) = inner.sample.take() {
                    let done = sample::finished(running, t);
                    inner.sample_events.push(SessionEvent::Finished {
                        id: done.id.clone(),
                        duration: Some(2.0),
                    });
                    inner.sample = Some(done);
                }
            });
        }
        later(
            if allowed { 7.0 } else { 2.5 },
            service.clone(),
            |inner, _| {
                inner.sample = None;
            },
        );
        true
    }
}

#[tauri::command]
pub fn sessions_show_sample(service: tauri::State<'_, Shared>) -> bool {
    service.show_sample();
    true
}

#[tauri::command]
pub fn sessions_dismiss_sample(service: tauri::State<'_, Shared>) {
    service.dismiss_sample();
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
    for codex in service.codex_clients() {
        codex.focus(session_id.as_deref());
    }
}

/// Why the last change to each agent's hooks failed, for Settings.
static AGENT_HOOK_FAILURES: Mutex<BTreeMap<&'static str, String>> = Mutex::new(BTreeMap::new());
/// One sync at a time: the ticker's and a switch's may meet.
static AGENT_HOOK_SYNC: Mutex<()> = Mutex::new(());

/// Keeps every agent's hooks in step, with no button to press
/// (ClaudeSessionService.syncAgentHooks). Hooks the island put in that
/// someone took out come back only while "Put lost hooks back" is on, hooks
/// changed by hand are the person's, and an agent the person pressed Remove
/// for on the Hooks page is left as it is (#236).
pub(crate) fn sync_agent_hooks(hidden: &HashSet<models::Provider>) {
    use crate::diagnostics::hook_records::{self, State as Derived};
    let _one = AGENT_HOOK_SYNC.lock().unwrap();
    let (Some(home), Ok(exe)) = (hooks::installer::home_dir(), hooks::current_program()) else {
        return;
    };
    let endpoint = hooks::transport::endpoint(hooks::APP_ID);
    let found: Vec<models::Provider> = models::Provider::INSTALLED_AGENTS
        .into_iter()
        .filter(|p| hook_agents::installer::is_present(*p, &home))
        .collect();
    let records = hook_records::all();
    let states: HashMap<models::Provider, Derived> = models::Provider::INSTALLED_AGENTS
        .into_iter()
        .map(|p| {
            let base = hook_agents::installer::state(p, &home, &exe, &endpoint, hooks::APP_ID);
            (
                p,
                hook_records::agent_state(p, base, &home, hooks::APP_ID, &records),
            )
        })
        .collect();
    let opted_out = hooks::page::opted_out_agents(&hooks::page::opt_out());
    let (install, remove) = hook_agents::installer::plan_with(
        &found,
        hidden,
        |p| states.get(&p).copied().unwrap_or(Derived::Missing),
        &opted_out,
        hooks::page::settings().auto_repair,
    );
    // Hooks someone took out come back a few times a run at most, and are said once.
    let lost = |p: &models::Provider| states.get(p) == Some(&Derived::RemovedByHand);
    let install: Vec<models::Provider> = install
        .into_iter()
        .filter(|p| !lost(p) || hooks::page_service::note_repair(&hook_records::agent_id(*p)))
        .collect();
    let mut put_back = Vec::new();
    for (provider, on) in install
        .into_iter()
        .map(|p| (p, true))
        .chain(remove.into_iter().map(|p| (p, false)))
    {
        let result = change_agent_hooks(provider, on, &home, &exe, &endpoint);
        if on && result.is_ok() && lost(&provider) {
            put_back.push(hooks::page::display_name(provider).to_owned());
        }
        let mut failures = AGENT_HOOK_FAILURES.lock().unwrap();
        match result {
            Ok(()) => failures.remove(provider.key()),
            Err(e) => failures.insert(provider.key(), e),
        };
    }
    hooks::page_service::announce(&put_back);
}

/// One agent's hooks put in or taken out, kept for a diagnostics file
/// (#230): what was written, and a line in the log.
fn change_agent_hooks(
    provider: models::Provider,
    install: bool,
    home: &Path,
    exe: &Path,
    endpoint: &str,
) -> Result<(), String> {
    use crate::diagnostics::{hook_records, log};
    let result = if install {
        hook_agents::installer::install(provider, home, exe, endpoint, hooks::APP_ID)
    } else {
        hook_agents::installer::remove(provider, home, hooks::APP_ID)
    };
    let change = if install { "installed" } else { "removed" };
    match &result {
        Ok(()) => {
            if install {
                hook_records::note_agent(provider, home, hooks::APP_ID);
            } else {
                hook_records::forget(&hook_records::agent_id(provider));
            }
            log::info("hooks", &format!("{} hooks {change}", provider.key()));
        }
        Err(e) => log::error(
            "hooks",
            &format!("{} hooks could not be {change}: {e}", provider.key()),
        ),
    }
    result
}

/// The agents' hook states as last logged, so only a change is written.
static AGENT_HOOK_SUMMARY: Mutex<String> = Mutex::new(String::new());

/// Whether the island hears Codex, in a word, for a diagnostics file (#230).
pub fn codex_status_name(service: &Service) -> &'static str {
    if service.codex().is_some_and(|c| c.connected()) {
        "connected"
    } else if crate::sessions::codex::client::codex_home().is_some_and(|h| h.is_dir()) {
        "notRunning"
    } else {
        "notFound"
    }
}

/// An agent's event as the diagnostics journal keeps it: whose, which
/// session, and its kind with the tool it names.
type HeardEvent = (models::Provider, String, (&'static str, Option<String>));

/// The state a session was last shown in; None when it isn't listed.
/// The request among an agent's events that a plain allow answers: a
/// tool's, never a question (#234).
fn allowable(events: &[hook_agents::AgentEvent], request: &str) -> Option<models::Permission> {
    events.iter().find_map(|event| match &event.kind {
        hook_agents::Kind::Permission(p) | hook_agents::Kind::HeldPermission(p)
            if p.request_id == request && p.questions.is_empty() =>
        {
            Some(p.clone())
        }
        _ => None,
    })
}

/// Types Qwen Code's answer into its dialog: keys one by one, a typed
/// answer pasted and sent with one Return, pauses between questions.
fn press_qwen(
    pid: u32,
    located: &hosts::Located,
    steps: &[super::qwen::Step],
) -> Result<(), hosts::DeliveryError> {
    use super::qwen::Step;
    for step in steps {
        match step {
            Step::Key(key) => hosts::press(pid, located, *key)?,
            Step::Text(text) => hosts::deliver(pid, located, text, &[])?,
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(*ms)),
        }
        // Each key in turn, as a person presses them.
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    Ok(())
}

/// Presses Droid's keys in turn: a dialog a number or Tab moves on from
/// gets a moment to open the next step before the key after it.
fn press_droid(
    pid: u32,
    located: &hosts::Located,
    keys: &[hosts::Key],
) -> Result<(), hosts::DeliveryError> {
    let mut previous: Option<hosts::Key> = None;
    for key in keys {
        let pause = match previous {
            Some(hosts::Key::Digit(_) | hosts::Key::Tab)
                if !matches!(key, hosts::Key::Digit(_) | hosts::Key::Tab) =>
            {
                400
            }
            Some(hosts::Key::Digit(_)) if keys.len() == 2 => 400,
            Some(_) => 60,
            None => 0,
        };
        std::thread::sleep(std::time::Duration::from_millis(pause));
        hosts::press(pid, located, *key)?;
        previous = Some(*key);
    }
    Ok(())
}

/// Grok's prompt: Return for the yes it has picked, Ctrl-C for no.
fn grok_keys(decision: &Decision) -> Option<Vec<hosts::Key>> {
    match decision {
        Decision::Allow => Some(vec![hosts::Key::Enter]),
        Decision::Deny => Some(vec![hosts::Key::Interrupt]),
        _ => None,
    }
}

/// Hermes's classic prompt (hermes_cli/cli_modal_mixin.py): Allow once is
/// picked first and Return confirms it; Deny is the last choice but View,
/// which shows a long command whole, and the arrows stop at the ends, so
/// three downs reach it whichever choices are offered. Questions go through
/// its gateway.
pub fn hermes_keys(decision: &Decision, asked: &models::Permission) -> Option<Vec<hosts::Key>> {
    use hosts::Key;
    let asks = !asked.questions.is_empty();
    match decision {
        Decision::Allow if !asks => Some(vec![Key::Enter]),
        Decision::Deny if !asks => Some(vec![Key::Down, Key::Down, Key::Down, Key::Enter]),
        _ => None,
    }
}

fn phase_of(inner: &Inner, id: &str) -> Option<models::SessionPhase> {
    inner.sessions.iter().find(|s| s.id == id).map(|s| s.phase)
}

/// For Settings › Content › Sessions: each hook agent's hooks or plugin.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHooksStatus {
    agent: &'static str,
    state: hook_agents::installer::State,
    present: bool,
    /// For Hermes, once its hooks are in: how many it hasn't been told to
    /// run yet, which it skips (#244).
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting_approval: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<String>,
}

#[tauri::command]
pub fn sessions_agent_hooks_status() -> Vec<AgentHooksStatus> {
    let (Some(home), Ok(exe)) = (hooks::installer::home_dir(), hooks::current_program()) else {
        return Vec::new();
    };
    let endpoint = hooks::transport::endpoint(hooks::APP_ID);
    let statuses: Vec<AgentHooksStatus> = models::Provider::INSTALLED_AGENTS
        .iter()
        .map(|&provider| {
            let state =
                hook_agents::installer::state(provider, &home, &exe, &endpoint, hooks::APP_ID);
            AgentHooksStatus {
                agent: provider.key(),
                state,
                present: hook_agents::installer::is_present(provider, &home),
                waiting_approval: (provider == models::Provider::Hermes
                    && state == hook_agents::installer::State::Installed)
                    .then(
                        || match super::hermes::approval(&home, &exe, hooks::APP_ID) {
                            super::hermes::Approval::Approved => 0,
                            super::hermes::Approval::Waiting(n) => n,
                        },
                    ),
                failure: AGENT_HOOK_FAILURES
                    .lock()
                    .unwrap()
                    .get(provider.key())
                    .cloned(),
            }
        })
        .collect();
    let summary = statuses
        .iter()
        .map(|s| {
            format!(
                "{}={:?}{}",
                s.agent,
                s.state,
                if s.present { "" } else { "(absent)" }
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut last = AGENT_HOOK_SUMMARY.lock().unwrap();
    if *last != summary {
        crate::diagnostics::log::info("hooks", &format!("agent hooks: {summary}"));
        *last = summary;
    }
    statuses
}

#[tauri::command]
pub fn sessions_agent_hooks_install(agent: String, install: bool) -> Result<(), String> {
    let provider = models::Provider::from_key(&agent)
        .filter(|p| p.reports_through_hooks())
        .ok_or("unknown agent")?;
    let home = hooks::installer::home_dir().ok_or("no home folder")?;
    let exe = hooks::current_program()?;
    let endpoint = hooks::transport::endpoint(hooks::APP_ID);
    let result = change_agent_hooks(provider, install, &home, &exe, &endpoint);
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
        let settings = claude_folder
            .as_ref()
            .and_then(|f| std::fs::read(f.join("settings.json")).ok());
        let setting = settings.as_deref().and_then(background::claude_setting);
        let efforts = settings.as_deref().map(background::claude_effort_setting);
        // Claude Code's cached model catalog says what each alias points to.
        let catalogs: Vec<Vec<u8>> = claude_folder
            .and_then(|f| std::fs::read_dir(f.join("cache").join("model-catalog")).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .filter_map(|path| std::fs::read(path).ok())
            .collect();
        let catalog = background::claude_catalog(&catalogs);
        let codex_home = crate::sessions::codex::client::codex_home();
        let cache = codex_home
            .as_ref()
            .and_then(|h| std::fs::read(h.join("models_cache.json")).ok());
        let config = codex_home
            .as_ref()
            .and_then(|h| std::fs::read_to_string(h.join("config.toml")).ok());
        NewSessionModels {
            claude: background::claude_models(
                setting.as_deref(),
                catalog.as_ref(),
                efforts.as_ref(),
            ),
            codex: background::codex_models(cache.as_deref(), config.as_deref()),
        }
    })
    .await
    .map_err(|_| ())
}

/// Starts Claude Code, Codex or another agent in a folder, in the background
/// or in the person's terminal, with a model or the agent's default, and an
/// effort or the model's default (Claude Code and Codex only, #289). With
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
    effort: Option<String>,
    worktree: Option<bool>,
    worktree_name: Option<String>,
    safe_commands: Option<Vec<String>>,
) -> Result<Option<String>, ()> {
    let service = service.inner().clone();
    // Only agents with a session allow list take one (#270).
    let safe_commands: Vec<String> = safe_commands
        .unwrap_or_default()
        .iter()
        .filter(|_| agent.as_deref().is_none_or(super::safe_commands::supports))
        .filter_map(|c| super::safe_commands::cleaned(c))
        .collect();
    tauri::async_runtime::spawn_blocking(move || {
        let model = model.as_deref().filter(|m| !m.is_empty());
        let effort = effort.as_deref().filter(|e| self::background::is_effort(e));
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
            return start_codex(
                &service,
                &folder,
                &prompt,
                background,
                model,
                effort,
                safe_commands,
            );
        }
        // Gemini CLI, Copilot CLI, OpenCode and cursor-agent run only in a
        // console, where their hooks or plugin report them.
        if let Some(provider) = hook_agent {
            let started = hook_agents::command_name(provider)
                .and_then(crate::agent_usage::agent_executable)
                .is_some_and(|exe| {
                    // Vibe's server runs through the island's proxy, which answers its approvals and questions.
                    if provider == models::Provider::Vibe
                        && let Some((python, arguments)) = super::vibe_proxy::launch(
                            Path::new(&exe),
                            crate::hooks::APP_ID,
                            &prompt,
                        )
                    {
                        return self::background::agent_in_terminal(
                            &python,
                            &arguments,
                            Some(&folder),
                        );
                    }
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
            effort,
            claude_worktree.as_deref(),
            &super::safe_commands::claude_rules(&safe_commands),
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

/// The safe commands a folder suggests for New session (#270).
#[tauri::command]
pub async fn sessions_safe_commands(
    folder: String,
) -> Result<Vec<super::safe_commands::Entry>, ()> {
    tauri::async_runtime::spawn_blocking(move || super::safe_commands::suggested(&folder))
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
    effort: Option<&str>,
    safe_commands: Vec<String>,
) -> Option<String> {
    if !background {
        // The console's thread is named only once it starts, in this folder.
        if !safe_commands.is_empty()
            && let Some(client) = service.codex()
        {
            client.expect_safe_commands(folder, safe_commands);
        }
        let arguments = self::background::codex_start_arguments(prompt, model, effort);
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
    let Some(answer) = client.start_thread(folder, model, effort, prompt.trim(), safe_commands)
    else {
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

/// What Open came to (#235).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenResult {
    Opened,
    Missing,
    /// The session's tmux or zellij has no terminal attached; `sessions_attach` attaches it.
    Detached,
}

#[tauri::command]
pub async fn sessions_open(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<OpenResult, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.open(&session_id))
        .await
        .map_err(|_| ())
}

/// Whether the Claude app is on this PC, for its switches in Settings (#246).
#[tauri::command]
pub fn sessions_claude_desktop_installed() -> bool {
    super::claude_desktop::is_installed()
}

/// Attaches a session's detached tmux or zellij in a new terminal window.
#[tauri::command]
pub async fn sessions_attach(
    service: tauri::State<'_, Shared>,
    session_id: String,
) -> Result<bool, ()> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || service.attach_detached(&session_id))
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

#[cfg(test)]
mod auto_mode_tests {
    use super::*;
    use hook_agents::{AgentEvent, Kind};
    use models::{Permission, Provider, Question};

    fn event(kind: Kind) -> AgentEvent {
        AgentEvent {
            provider: Provider::Opencode,
            session_id: "s1".into(),
            pid: 1,
            cwd: None,
            date: 1.0,
            kind,
        }
    }

    /// Always proceed answers a tool's request, never a question (#234).
    #[test]
    fn only_a_tools_request_is_allowable() {
        let asked = Permission::new("per_1", "Bash", Some("rm -rf build"), false, 1.0);
        let events = vec![
            event(Kind::Question),
            event(Kind::Permission(asked.clone())),
        ];
        assert_eq!(allowable(&events, "per_1"), Some(asked.clone()));
        assert_eq!(allowable(&events, "per_2"), None, "another request");
        let mut question = Permission::new("q_1", "AskUserQuestion", None, false, 1.0);
        question.questions = vec![Question {
            question: "Which?".into(),
            header: None,
            options: Vec::new(),
            multi_select: false,
        }];
        let held = vec![event(Kind::HeldPermission(question))];
        assert_eq!(
            allowable(&held, "q_1"),
            None,
            "a question waits on the person"
        );
        let vibe = vec![event(Kind::HeldPermission(asked))];
        assert!(
            allowable(&vibe, "per_1").is_some(),
            "Vibe's approvals held by its proxy"
        );
    }
}

/// The session an event is about.
fn event_id(event: &models::SessionEvent) -> &str {
    match event {
        models::SessionEvent::NeedsPermission { id, .. }
        | models::SessionEvent::NeedsInput { id }
        | models::SessionEvent::Finished { id, .. } => id,
    }
}
