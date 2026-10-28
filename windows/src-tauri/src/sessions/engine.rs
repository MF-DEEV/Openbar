// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Joins what Claude Code's process registry, its transcripts and its hooks
//! say about each running session into the state the island shows, ported
//! from `ClaudeSessionEngine.swift`. Kept free of files, threads and clocks
//! so every rule can be tested; the owner calls it from one thread and
//! passes the time in.
//!
//! The registry is written by the session itself as its state changes, so it
//! decides whether a session waits on the person and otherwise whether it
//! works. A hook or transcript line newer than the registry's last change
//! wins, since the registry can trail it by a moment, and only those say
//! which tool runs.

use std::collections::{BTreeMap, HashMap};

use super::models::{
    ChatMessage, ChatRole, ContentPolicy, DISTANT_PAST, HookEvent, HookFact, Permission, Provider,
    RegistryStatus, Session, SessionEvent, SessionPhase, SessionRecord, Subagent, TaskProgress,
    Time, Tokens, ToolUse, TranscriptFact, project_name,
};

/// How long an unread finished turn stays marked before it reads as idle.
pub const FINISHED_FOR: f64 = 10.0 * 60.0;
/// Hook events can arrive before the registry lists their session.
pub const UNLISTED_GRACE: f64 = 120.0;
pub const TIMELINE_LENGTH: usize = 12;
/// The notice and the registry's change are written apart.
pub const ASKED_SLACK: f64 = 5.0;
/// Background commands and subagents kept per session, the ended ones going first.
pub const BACKGROUND_KEPT: usize = 40;
/// The conversation keeps its latest messages.
pub const CHAT_LENGTH: usize = 150;

/// Notices that do not ask the person anything: permissions have their
/// own page, and an idle reminder or a sign-in is not a question.
pub const QUIET_NOTICES: [&str; 12] = [
    "idle_prompt",
    "permission_prompt",
    "worker_permission_prompt",
    "auth_success",
    "auth_storage_failure",
    "computer_use_enter",
    "computer_use_exit",
    "elicitation_complete",
    "elicitation_response",
    "agent_completed",
    "plugin_notification",
    "push_notification",
];

pub fn asks(kind: &str) -> bool {
    !QUIET_NOTICES.contains(&kind)
}

#[derive(Clone, Debug)]
struct Background {
    task: Option<String>,
    service: bool,
    label: Option<String>,
    started: Time,
    ended: Option<Time>,
}

#[derive(Clone, Debug)]
struct Activity {
    turn_open: bool,
    turn_started: Option<Time>,
    /// The open turn is known only from the registry saying busy, with
    /// nothing typed, said or run since; and when the turn before began.
    registry_only: bool,
    started_before: Option<Time>,
    turn_ended: Option<Time>,
    completed: bool,
    /// The newest moment a hook or transcript line told of.
    last_fact: Option<Time>,
    tools: Vec<ToolUse>,
    subagents: BTreeMap<String, Subagent>,
    /// Subagents reported done before anything else was known of them;
    /// the end applies once they are.
    ended_agents: HashMap<String, Time>,
    /// Commands and subagents left running in the background, by the call
    /// that started them.
    background: BTreeMap<String, Background>,
    tasks: Option<TaskProgress>,
    usage: BTreeMap<String, (Tokens, f64)>,
    title: Option<String>,
    last_text: Option<String>,
    chat: Vec<ChatMessage>,
    /// Messages said so far, which numbers the next one.
    said: i64,
    branch: Option<String>,
    model: Option<String>,
    /// Permissions the hooks hold, oldest first: subagents can ask at once.
    permissions: Vec<Permission>,
    hooked: bool,
    /// When Claude Code last asked the person for something by a notice.
    asked: Option<Time>,
    acknowledged: Option<Time>,
    first_seen: Time,
}

impl Activity {
    fn new(first_seen: Time) -> Self {
        Self {
            turn_open: false,
            turn_started: None,
            registry_only: false,
            started_before: None,
            turn_ended: None,
            completed: true,
            last_fact: None,
            tools: Vec::new(),
            subagents: BTreeMap::new(),
            ended_agents: HashMap::new(),
            background: BTreeMap::new(),
            tasks: None,
            usage: BTreeMap::new(),
            title: None,
            last_text: None,
            chat: Vec::new(),
            said: 0,
            branch: None,
            model: None,
            permissions: Vec::new(),
            hooked: false,
            asked: None,
            acknowledged: None,
            first_seen,
        }
    }

    fn open_tool(&self) -> Option<&ToolUse> {
        self.tools.iter().rev().find(|tool| tool.is_running())
    }

    /// Jobs still running, oldest first.
    fn running_background(&self) -> Vec<&Background> {
        let mut running: Vec<&Background> = self
            .background
            .values()
            .filter(|job| job.ended.is_none())
            .collect();
        running.sort_by(|a, b| a.started.total_cmp(&b.started));
        running
    }
}

/// What one snapshot shows, and the changes worth a notice since the last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// In no particular order; `models::sorted` gives the list's.
    pub sessions: Vec<Session>,
    pub events: Vec<SessionEvent>,
}

pub struct Engine {
    policy: ContentPolicy,
    records: BTreeMap<String, SessionRecord>,
    activity: HashMap<String, Activity>,
    phases: HashMap<String, (SessionPhase, Time)>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(ContentPolicy::default())
    }
}

impl Engine {
    pub fn new(policy: ContentPolicy) -> Self {
        Self {
            policy,
            records: BTreeMap::new(),
            activity: HashMap::new(),
            phases: HashMap::new(),
        }
    }

    pub fn policy(&self) -> ContentPolicy {
        self.policy
    }

    /// Turning previews off forgets every text read so far. Turning them on
    /// brings nothing back by itself: the service reads the transcripts again.
    pub fn set_policy(&mut self, policy: ContentPolicy) {
        self.policy = policy;
        if !policy.previews {
            self.purge_content();
        }
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.records.keys().cloned().collect()
    }

    pub fn record(&self, id: &str) -> Option<&SessionRecord> {
        self.records.get(id)
    }

    // MARK: Input

    /// Replaces the list of running sessions.
    pub fn ingest_records(&mut self, list: Vec<SessionRecord>, now: Time) {
        let mut records = BTreeMap::new();
        for record in list {
            records.entry(record.session_id.clone()).or_insert(record);
        }
        self.records = records;
        for (id, record) in &self.records {
            // The registry left the prompt behind, so it was answered in the
            // terminal or went away with its turn.
            if record.status != Some(RegistryStatus::Waiting)
                && let Some(changed) = record.status_updated
                && let Some(entry) = self.activity.get_mut(id)
            {
                entry.permissions.retain(|p| p.since >= changed);
            }
            // The session went idle after the last line read, so the turn
            // ended even when its last lines were not read yet.
            if matches!(
                record.status,
                Some(RegistryStatus::Idle | RegistryStatus::Shell)
            ) && let Some(changed) = record.status_updated
                && let Some(entry) = self.activity.get_mut(id)
                && entry.turn_open
                && changed > entry.last_fact.unwrap_or(DISTANT_PAST)
            {
                if entry.registry_only {
                    // Busy a moment with nothing typed, said or run, as while
                    // Claude Code writes a recap of the session: no turn ran,
                    // and nothing finished.
                    entry.turn_open = false;
                    entry.turn_started = entry.started_before;
                    entry.registry_only = false;
                } else {
                    end(changed, true, entry);
                }
            }
            // And it went to work before its prompt was read.
            if record.status == Some(RegistryStatus::Busy)
                && let Some(changed) = record.status_updated
            {
                let (open, last_fact) = self
                    .activity
                    .get(id)
                    .map_or((false, None), |entry| (entry.turn_open, entry.last_fact));
                if !open && changed > last_fact.unwrap_or(DISTANT_PAST) {
                    let entry = self
                        .activity
                        .entry(id.clone())
                        .or_insert_with(|| Activity::new(now));
                    let before = entry.turn_started;
                    begin(changed, entry);
                    entry.registry_only = true;
                    entry.started_before = before;
                }
            }
        }
        let records = &self.records;
        self.activity.retain(|id, entry| {
            records.contains_key(id)
                || now - entry.last_fact.unwrap_or(entry.first_seen) < UNLISTED_GRACE
        });
        self.phases.retain(|id, _| records.contains_key(id));
    }

    pub fn ingest_facts(&mut self, session_id: &str, facts: &[TranscriptFact], now: Time) {
        if facts.is_empty() {
            return;
        }
        let previews = self.policy.previews;
        let entry = self
            .activity
            .entry(session_id.to_owned())
            .or_insert_with(|| Activity::new(now));
        for fact in facts {
            apply_fact(fact, previews, entry);
        }
    }

    pub fn ingest_hook(&mut self, fact: HookFact, now: Time) {
        let previews = self.policy.previews;
        let entry = self
            .activity
            .entry(fact.session_id.clone())
            .or_insert_with(|| Activity::new(now));
        entry.hooked = true;
        match &fact.agent_id {
            Some(agent) => apply_subagent(fact.event, agent, fact.date, previews, entry),
            None => apply_hook(fact.event, fact.date, previews, entry),
        }
    }

    /// The person answered, or the hook stopped waiting for an answer.
    pub fn resolve_permission(&mut self, request_id: &str) {
        for entry in self.activity.values_mut() {
            entry.permissions.retain(|p| p.request_id != request_id);
        }
    }

    /// The permission the island shows for a session: the oldest one asked.
    pub fn permission(&self, id: &str) -> Option<&Permission> {
        self.activity.get(id)?.permissions.first()
    }

    pub fn holds(&self, request_id: &str, id: &str) -> bool {
        self.activity
            .get(id)
            .is_some_and(|entry| entry.permissions.iter().any(|p| p.request_id == request_id))
    }

    /// The person saw the session's finished turn.
    pub fn acknowledge(&mut self, id: &str, now: Time) {
        if !self.activity.contains_key(id) && !self.records.contains_key(id) {
            return;
        }
        self.activity
            .entry(id.to_owned())
            .or_insert_with(|| Activity::new(now))
            .acknowledged = Some(now);
    }

    /// Forgets every text, title and argument read so far.
    pub fn purge_content(&mut self) {
        for entry in self.activity.values_mut() {
            entry.title = None;
            entry.last_text = None;
            entry.chat.clear();
            for job in entry.background.values_mut() {
                job.label = None;
            }
            if let Some(tasks) = &mut entry.tasks {
                tasks.current = None;
            }
            for tool in &mut entry.tools {
                tool.summary = None;
            }
            for agent in entry.subagents.values_mut() {
                agent.description = None;
            }
        }
    }

    // MARK: Output

    pub fn snapshot(&mut self, now: Time) -> Snapshot {
        let mut snapshot = Snapshot::default();
        let fresh = Activity::new(now);
        for (id, record) in &self.records {
            let entry = self.activity.get(id).unwrap_or(&fresh);
            let phase = phase(record, entry, now);
            let previous = self.phases.get(id).copied();
            let since = match previous {
                Some((was, since)) if was == phase => since,
                _ => now,
            };
            self.phases.insert(id.clone(), (phase, since));
            if let Some((was, was_since)) = previous
                && was != phase
            {
                match phase {
                    SessionPhase::WaitingPermission => {
                        snapshot.events.push(SessionEvent::NeedsPermission {
                            id: id.clone(),
                            tool: entry
                                .permissions
                                .first()
                                .map(|p| p.tool.clone())
                                .or_else(|| entry.open_tool().map(|t| t.name.clone()))
                                .unwrap_or_default(),
                        })
                    }
                    SessionPhase::WaitingInput => snapshot
                        .events
                        .push(SessionEvent::NeedsInput { id: id.clone() }),
                    // Only a turn that ended after the work was seen begin is
                    // news; going back to a result already shown, as after a
                    // recap, is not.
                    SessionPhase::Finished
                        if (was.is_active() || was.is_waiting())
                            && entry.turn_ended.unwrap_or(DISTANT_PAST) >= was_since =>
                    {
                        let duration = entry
                            .turn_started
                            .zip(entry.turn_ended)
                            .map(|(start, end)| end - start);
                        snapshot.events.push(SessionEvent::Finished {
                            id: id.clone(),
                            duration,
                        });
                    }
                    _ => {}
                }
            }
            snapshot.sessions.push(session(
                id,
                record,
                entry,
                phase,
                since,
                self.policy.previews,
            ));
        }
        snapshot
    }
}

/// The phase the rules give one session.
fn phase(record: &SessionRecord, entry: &Activity, now: Time) -> SessionPhase {
    // A question Claude asks waits on the person's answer, not a permission.
    if let Some(first) = entry.permissions.first() {
        return if first.asks_questions() {
            SessionPhase::WaitingInput
        } else {
            SessionPhase::WaitingPermission
        };
    }
    if record.status == Some(RegistryStatus::Waiting) {
        if record.waiting_for.as_deref() != Some(SessionRecord::DIALOG_OPEN) {
            return if record.waiting_for.as_deref() == Some(SessionRecord::PERMISSION_PROMPT) {
                SessionPhase::WaitingPermission
            } else {
                SessionPhase::WaitingInput
            };
        }
        // A dialog the person opened waits on nobody. One Claude Code
        // raises to ask something sends a notice as it opens.
        if let Some(asked) = entry.asked
            && asked >= record.status_updated.unwrap_or(DISTANT_PAST) - ASKED_SLACK
        {
            return SessionPhase::WaitingInput;
        }
    }
    let working = match entry.last_fact {
        Some(fact)
            if record.status.is_none() || fact > record.status_updated.unwrap_or(DISTANT_PAST) =>
        {
            entry.turn_open
        }
        _ => record.status == Some(RegistryStatus::Busy),
    };
    if working {
        return if entry.turn_open && entry.open_tool().is_some() {
            SessionPhase::RunningTool
        } else {
            SessionPhase::Generating
        };
    }
    // Claude Code says "shell" for a session at rest with a command of its
    // own still running in the background. The work is not done while one
    // it waits for runs; a dev server or an emulator left running is not
    // work, and when nothing was seen start, it counts as work. While a
    // subagent it sent to the background works, it says "busy" though
    // the turn ended.
    let running = entry.running_background();
    if record.status == Some(RegistryStatus::Shell)
        && (running.is_empty() || running.iter().any(|job| !job.service))
    {
        return SessionPhase::Background;
    }
    if record.status == Some(RegistryStatus::Busy)
        && running.iter().any(|job| !job.service && job.task.is_some())
    {
        return SessionPhase::Background;
    }
    if let Some(ended) = entry.turn_ended
        && entry.completed
        && now - ended < FINISHED_FOR
        && entry.acknowledged.unwrap_or(DISTANT_PAST) < ended
    {
        return SessionPhase::Finished;
    }
    SessionPhase::Idle
}

fn session(
    id: &str,
    record: &SessionRecord,
    entry: &Activity,
    phase: SessionPhase,
    since: Time,
    previews: bool,
) -> Session {
    let gated = |tool: &ToolUse| {
        let mut tool = tool.clone();
        if !previews {
            tool.summary = None;
        }
        tool
    };
    let mut tokens = Tokens::default();
    let mut cost = 0.0;
    for (used, priced) in entry.usage.values() {
        tokens += *used;
        cost += priced;
    }
    let subagents = ordered(entry.subagents.values().cloned().collect())
        .into_iter()
        .map(|mut agent| {
            if !previews {
                agent.description = None;
            }
            agent
        })
        .collect();
    let mut tasks = entry.tasks.clone();
    if !previews && let Some(tasks) = &mut tasks {
        tasks.current = None;
    }
    let running = entry.running_background();
    let start = entry.tools.len().saturating_sub(TIMELINE_LENGTH);
    Session {
        provider: Provider::Claude,
        id: id.to_owned(),
        pid: record.pid,
        cwd: record.cwd.clone(),
        project: project_name(&record.cwd),
        name: record.name.clone(),
        name_source: record.name_source.clone(),
        branch: entry.branch.clone(),
        model: entry.model.clone(),
        model_name: None,
        host: record.host,
        accepts_replies: record.accepts_replies,
        phase,
        phase_since: since,
        turn_started: entry.turn_started,
        last_active: [entry.turn_ended, entry.last_fact, record.status_updated]
            .into_iter()
            .flatten()
            .reduce(f64::max),
        looked_at: entry.acknowledged,
        waiting_for: if record.status == Some(RegistryStatus::Waiting) {
            record.waiting_for.clone()
        } else {
            None
        },
        current_tool: if matches!(
            phase,
            SessionPhase::RunningTool | SessionPhase::WaitingPermission
        ) {
            entry.open_tool().map(gated)
        } else {
            None
        },
        permission: entry.permissions.first().cloned(),
        timeline: entry.tools[start..].iter().map(gated).collect(),
        subagents,
        tasks,
        tokens,
        cost,
        title: if previews { entry.title.clone() } else { None },
        last_text: if previews {
            entry.last_text.clone()
        } else {
            None
        },
        hooked: entry.hooked,
        chat: if previews {
            entry.chat.clone()
        } else {
            Vec::new()
        },
        background_label: if previews {
            running
                .iter()
                .rev()
                .find(|job| !job.service)
                .and_then(|job| job.label.clone())
        } else {
            None
        },
        services: if record.status == Some(RegistryStatus::Shell)
            || phase == SessionPhase::Background
        {
            running.iter().filter(|job| job.service).count() as i64
        } else {
            0
        },
    }
}

/// Working subagents first, by when they began, then the ones done, the
/// latest first.
pub fn ordered(mut agents: Vec<Subagent>) -> Vec<Subagent> {
    agents.sort_by(|lhs, rhs| match (lhs.finished, rhs.finished) {
        (None, None) => lhs.started.total_cmp(&rhs.started),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(left), Some(right)) => right.total_cmp(&left),
    });
    agents
}

// MARK: Rules

fn apply_fact(fact: &TranscriptFact, previews: bool, entry: &mut Activity) {
    match fact {
        TranscriptFact::TurnBegan(date) => begin(*date, entry),
        TranscriptFact::TurnEnded { at, completed } => end(*at, *completed, entry),
        TranscriptFact::ToolStarted(tool) => start(tool.clone(), previews, entry),
        TranscriptFact::ToolFinished { id, at, failed } => finish(id, *at, *failed, entry),
        TranscriptFact::Text {
            text,
            at,
            ends_turn,
        } => {
            if previews {
                entry.last_text = Some(text.clone());
            }
            say(text, ChatRole::Assistant, *at, *ends_turn, previews, entry);
            touch(*at, entry);
        }
        TranscriptFact::Prompt { text, at } => {
            say(text, ChatRole::User, *at, false, previews, entry)
        }
        TranscriptFact::PromptImages { images, at } => {
            if previews {
                if !entry
                    .chat
                    .last()
                    .is_some_and(|m| m.role == ChatRole::User && m.at == *at)
                {
                    say("", ChatRole::User, *at, false, previews, entry);
                }
                if let Some(message) = entry.chat.last_mut() {
                    message.images = images.clone();
                }
            }
        }
        TranscriptFact::Title(title) => {
            if previews {
                entry.title = Some(title.clone());
            }
        }
        TranscriptFact::Branch(branch) => entry.branch = Some(branch.clone()),
        TranscriptFact::Model(model) => entry.model = Some(model.clone()),
        TranscriptFact::Usage { key, tokens, cost } => {
            let merged = match entry.usage.get(key) {
                Some((known, priced)) => (known.merged(tokens), priced.max(*cost)),
                None => (*tokens, cost.max(0.0)),
            };
            entry.usage.insert(key.clone(), merged);
        }
        TranscriptFact::SubagentActivity { id, at } => {
            let ended = entry.ended_agents.get(id).copied();
            match entry.subagents.get_mut(id) {
                None => {
                    let mut agent = Subagent::new(id, Subagent::UNTYPED, *at);
                    if let Some(ended) = ended
                        && ended >= *at
                    {
                        agent.finished = Some(ended);
                    }
                    entry.subagents.insert(id.clone(), agent);
                }
                Some(agent) => {
                    if agent.finished.is_some_and(|finished| finished < *at)
                        && ended.unwrap_or(DISTANT_PAST) < *at
                    {
                        agent.finished = None;
                    }
                }
            }
        }
        TranscriptFact::SubagentEnded { id, at } => {
            end_subagent(id, *at, entry);
            for job in entry.background.values_mut() {
                if job.task.as_deref() == Some(id.as_str()) && job.ended.is_none() {
                    job.ended = Some(*at);
                }
            }
        }
        TranscriptFact::BackgroundStarted {
            tool_id,
            service,
            label,
            at,
        } => {
            entry
                .background
                .entry(tool_id.clone())
                .or_insert_with(|| Background {
                    task: None,
                    service: *service,
                    label: if previews { label.clone() } else { None },
                    started: *at,
                    ended: None,
                });
            if entry.background.len() > BACKGROUND_KEPT {
                let mut ended: Vec<(String, Time)> = entry
                    .background
                    .iter()
                    .filter(|(_, job)| job.ended.is_some())
                    .map(|(key, job)| (key.clone(), job.started))
                    .collect();
                ended.sort_by(|a, b| a.1.total_cmp(&b.1));
                let excess = entry.background.len() - BACKGROUND_KEPT;
                for (key, _) in ended.into_iter().take(excess) {
                    entry.background.remove(&key);
                }
            }
        }
        TranscriptFact::BackgroundTask { tool_id, task } => {
            let ended = entry.ended_agents.get(task).copied();
            if let Some(job) = entry.background.get_mut(tool_id) {
                job.task = Some(task.clone());
                // The end may have been read first.
                if let Some(ended) = ended {
                    job.ended = Some(ended);
                }
            }
        }
        TranscriptFact::SubagentInfo {
            id,
            agent_type,
            description,
        } => {
            let started = entry.last_fact.unwrap_or(DISTANT_PAST);
            let agent = entry
                .subagents
                .entry(id.clone())
                .or_insert_with(|| Subagent::new(id, agent_type, started));
            agent.agent_type = agent_type.clone();
            if previews {
                agent.description = description.clone();
            }
        }
        TranscriptFact::Tasks(tasks) => {
            let mut tasks = tasks.clone();
            if !previews {
                tasks.current = None;
            }
            entry.tasks = (tasks.total > 0).then_some(tasks);
        }
    }
}

fn say(
    text: &str,
    role: ChatRole,
    at: Time,
    ends_turn: bool,
    previews: bool,
    entry: &mut Activity,
) {
    if !previews {
        return;
    }
    if let Some(last) = entry.chat.last_mut()
        && last.role == role
        && last.text == text
        && (role != ChatRole::User || last.at == at)
    {
        if ends_turn {
            last.ends_turn = true;
        }
        return;
    }
    entry.said += 1;
    entry.chat.push(ChatMessage {
        id: entry.said,
        role,
        text: text.to_owned(),
        images: Vec::new(),
        at,
        ends_turn,
    });
    if entry.chat.len() > CHAT_LENGTH {
        let excess = entry.chat.len() - CHAT_LENGTH;
        entry.chat.drain(..excess);
    }
}

fn apply_hook(event: HookEvent, date: Time, previews: bool, entry: &mut Activity) {
    match event {
        HookEvent::SessionStarted | HookEvent::Other(_) => {}
        HookEvent::SessionEnded => end(date, false, entry),
        HookEvent::PromptSubmitted => begin(date, entry),
        HookEvent::ToolStarted(tool) => start(tool, previews, entry),
        HookEvent::ToolFinished { id, failed } => finish(&id, date, failed, entry),
        HookEvent::PermissionRequested(mut permission) => {
            if !entry.turn_open {
                begin(date, entry);
            }
            // The tool that asked is the main agent's running call with the
            // same input, or the only running call by that name; its result
            // answers the request.
            if permission.tool_id.is_none() {
                let candidates: Vec<&ToolUse> = entry
                    .tools
                    .iter()
                    .filter(|t| t.is_running() && t.name == permission.tool)
                    .collect();
                permission.tool_id = candidates
                    .iter()
                    .rev()
                    .find(|t| t.input_key.is_some() && t.input_key == permission.input_key)
                    .or(if candidates.len() == 1 {
                        candidates.first()
                    } else {
                        None
                    })
                    .map(|t| t.id.clone());
            }
            entry
                .permissions
                .retain(|p| p.request_id != permission.request_id);
            entry.permissions.push(permission);
        }
        HookEvent::Notification { kind } => {
            if asks(&kind) {
                entry.asked = Some(date);
            }
        }
        HookEvent::Stopped { last_text } => {
            if previews && let Some(text) = last_text {
                entry.last_text = Some(text);
            }
            end(date, true, entry);
        }
        HookEvent::SubagentStarted { id, agent_type } => {
            // The hook adds to what the transcript already told, never resets it.
            match entry.subagents.get_mut(&id) {
                Some(known) => {
                    known.started = known.started.min(date);
                    if known.agent_type == Subagent::UNTYPED && !agent_type.is_empty() {
                        known.agent_type = agent_type;
                    }
                }
                None => {
                    let kind = if agent_type.is_empty() {
                        Subagent::UNTYPED
                    } else {
                        agent_type.as_str()
                    };
                    entry
                        .subagents
                        .insert(id.clone(), Subagent::new(&id, kind, date));
                }
            }
        }
        HookEvent::SubagentStopped { id } => end_subagent(&id, date, entry),
    }
}

/// A subagent's own tools tell that it works, not what the session runs.
fn apply_subagent(event: HookEvent, agent: &str, date: Time, previews: bool, entry: &mut Activity) {
    match event {
        HookEvent::SubagentStopped { .. } => apply_hook(event, date, previews, entry),
        HookEvent::PermissionRequested(mut permission) => {
            // Its permission still waits on the person; its tools are not
            // followed, so only a later prompt, the end or the registry
            // settle it.
            permission.tool_id.get_or_insert_with(String::new);
            apply_hook(
                HookEvent::PermissionRequested(permission),
                date,
                previews,
                entry,
            );
        }
        _ => {
            entry
                .subagents
                .entry(agent.to_owned())
                .or_insert_with(|| Subagent::new(agent, Subagent::UNTYPED, date));
            entry.last_fact = Some(entry.last_fact.map_or(date, |last| last.max(date)));
        }
    }
}

/// A subagent known to have run ends; one not seen yet ends as soon as it is.
fn end_subagent(id: &str, date: Time, entry: &mut Activity) {
    let ended = entry
        .ended_agents
        .get(id)
        .map_or(date, |known| known.max(date));
    entry.ended_agents.insert(id.to_owned(), ended);
    if let Some(known) = entry.subagents.get_mut(id) {
        known.finished = Some(known.finished.unwrap_or(date).max(date).max(known.started));
    }
}

fn begin(date: Time, entry: &mut Activity) {
    // A prompt read late cannot open a turn that already ended after it.
    if entry.turn_ended.is_some_and(|ended| date < ended) {
        return;
    }
    // A prompt queued while the session works joins the turn under way.
    if !entry.turn_open {
        // Whatever still looked open stopped with the turn it served.
        close_tools(date, entry);
        entry.turn_open = true;
        entry.turn_started = Some(date);
    }
    // A prompt can only be typed once the dialogs before it closed.
    entry.permissions.retain(|p| p.since >= date);
    touch(date, entry);
}

fn end(date: Time, completed: bool, entry: &mut Activity) {
    // An older line read late cannot end a turn that began after it.
    if entry.turn_open && entry.turn_started.is_some_and(|started| date < started) {
        return;
    }
    entry.turn_open = false;
    entry.turn_ended = Some(date);
    entry.completed = completed;
    entry.permissions.retain(|p| p.since > date);
    close_tools(date, entry);
    touch(date, entry);
}

fn start(mut tool: ToolUse, previews: bool, entry: &mut Activity) {
    let started = tool.started;
    if let Some(known) = entry.tools.iter_mut().find(|known| known.id == tool.id) {
        if known.summary.is_none() && previews {
            known.summary = tool.summary;
        }
        if known.input_key.is_none() {
            known.input_key = tool.input_key;
        }
    } else {
        if !previews {
            tool.summary = None;
        }
        entry.tools.push(tool);
        if entry.tools.len() > TIMELINE_LENGTH * 2 {
            let excess = entry.tools.len() - TIMELINE_LENGTH;
            entry.tools.drain(..excess);
        }
    }
    // A tool after the turn ended means a turn whose prompt was not seen.
    if !entry.turn_open && entry.turn_ended.unwrap_or(DISTANT_PAST) <= started {
        entry.turn_open = true;
        entry.turn_started = Some(started);
    }
    touch(started, entry);
}

fn finish(id: &str, date: Time, failed: bool, entry: &mut Activity) {
    if let Some(tool) = entry.tools.iter_mut().find(|tool| tool.id == id)
        && tool.is_running()
    {
        tool.finished = Some(date.max(tool.started));
        tool.failed = failed;
    }
    // The tool that asked ran or was refused, here or in the terminal.
    entry
        .permissions
        .retain(|p| p.tool_id.as_deref() != Some(id));
    // A background command refused before it ran got no id and never ends.
    if failed
        && let Some(job) = entry.background.get_mut(id)
        && job.task.is_none()
        && job.ended.is_none()
    {
        job.ended = Some(date);
    }
    touch(date, entry);
}

fn close_tools(date: Time, entry: &mut Activity) {
    for tool in entry.tools.iter_mut().filter(|tool| tool.is_running()) {
        tool.finished = Some(date.max(tool.started));
    }
}

fn touch(date: Time, entry: &mut Activity) {
    entry.last_fact = Some(entry.last_fact.map_or(date, |last| last.max(date)));
    // Something was typed, said or run: the turn is real.
    entry.registry_only = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::models::{
        DOZE_AFTER, HostKind, Question, QuestionOption, SessionCounts, can_reply, dozes,
        tool_summary,
    };
    use serde_json::json;

    const T0: f64 = 1_792_600_000.0;

    fn at(seconds: f64) -> f64 {
        T0 + seconds
    }

    fn record_full(
        id: &str,
        status: Option<RegistryStatus>,
        waiting_for: Option<&str>,
        changed: f64,
    ) -> SessionRecord {
        SessionRecord {
            pid: 100,
            session_id: id.into(),
            cwd: "/Users/me/code/app".into(),
            name: "app-1".into(),
            status,
            waiting_for: waiting_for.map(str::to_owned),
            status_updated: Some(at(changed)),
            started: Some(T0),
            version: "2.1.306".into(),
            host: HostKind::WindowsTerminal,
            accepts_replies: true,
            name_source: None,
        }
    }

    fn record(status: Option<RegistryStatus>, changed: f64) -> SessionRecord {
        record_full("s1", status, None, changed)
    }

    fn waiting(why: &str, changed: f64) -> SessionRecord {
        record_full("s1", Some(RegistryStatus::Waiting), Some(why), changed)
    }

    const IDLE: Option<RegistryStatus> = Some(RegistryStatus::Idle);
    const BUSY: Option<RegistryStatus> = Some(RegistryStatus::Busy);
    const SHELL: Option<RegistryStatus> = Some(RegistryStatus::Shell);

    fn hook(event: HookEvent, seconds: f64) -> HookFact {
        HookFact {
            session_id: "s1".into(),
            date: at(seconds),
            event,
            agent_id: None,
        }
    }

    fn hook_in(event: HookEvent, seconds: f64, id: &str, agent: Option<&str>) -> HookFact {
        HookFact {
            session_id: id.into(),
            date: at(seconds),
            event,
            agent_id: agent.map(str::to_owned),
        }
    }

    fn tool(id: &str, seconds: f64, name: &str, summary: Option<&str>) -> ToolUse {
        ToolUse::new(id, name, summary.map(str::to_owned), at(seconds))
    }

    fn bash(id: &str, seconds: f64) -> ToolUse {
        tool(id, seconds, "Bash", Some("npm test"))
    }

    fn only(engine: &mut Engine, seconds: f64) -> Option<Session> {
        engine.snapshot(at(seconds)).sessions.into_iter().next()
    }

    fn phase_of(engine: &mut Engine, seconds: f64) -> Option<SessionPhase> {
        only(engine, seconds).map(|s| s.phase)
    }

    fn finished_event(events: &[SessionEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::Finished { .. }))
    }

    fn previewing() -> Engine {
        Engine::new(ContentPolicy { previews: true })
    }

    fn began(seconds: f64) -> TranscriptFact {
        TranscriptFact::TurnBegan(at(seconds))
    }

    fn ended(seconds: f64, completed: bool) -> TranscriptFact {
        TranscriptFact::TurnEnded {
            at: at(seconds),
            completed,
        }
    }

    fn text(words: &str, seconds: f64) -> TranscriptFact {
        TranscriptFact::Text {
            text: words.into(),
            at: at(seconds),
            ends_turn: false,
        }
    }

    fn job(tool_id: &str, service: bool, label: Option<&str>, seconds: f64) -> TranscriptFact {
        TranscriptFact::BackgroundStarted {
            tool_id: tool_id.into(),
            service,
            label: label.map(str::to_owned),
            at: at(seconds),
        }
    }

    fn task(tool_id: &str, task: &str) -> TranscriptFact {
        TranscriptFact::BackgroundTask {
            tool_id: tool_id.into(),
            task: task.into(),
        }
    }

    fn agent_ended(id: &str, seconds: f64) -> TranscriptFact {
        TranscriptFact::SubagentEnded {
            id: id.into(),
            at: at(seconds),
        }
    }

    fn ask(request: &str, summary: &str, offers_always: bool, since: f64) -> Permission {
        Permission::new(request, "Bash", Some(summary), offers_always, at(since))
    }

    // ClaudeSessionEngineTests.swift phases.
    #[test]
    fn phases() {
        let mut engine = Engine::default();
        engine.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        assert_eq!(
            phase_of(&mut engine, 1.0),
            Some(SessionPhase::Idle),
            "a listed session with nothing else known is idle"
        );

        engine.ingest_records(vec![record(BUSY, 5.0)], at(5.0));
        assert_eq!(
            phase_of(&mut engine, 6.0),
            Some(SessionPhase::Generating),
            "a busy registry reads as working"
        );
        assert_eq!(
            only(&mut engine, 6.0).unwrap().turn_started,
            Some(at(5.0)),
            "the busy change starts the turn clock"
        );

        engine.ingest_hook(hook(HookEvent::ToolStarted(bash("t1", 7.0)), 7.0), at(7.0));
        assert_eq!(
            phase_of(&mut engine, 8.0),
            Some(SessionPhase::RunningTool),
            "a tool reported after the registry change runs"
        );
        assert_eq!(
            only(&mut engine, 8.0).unwrap().current_tool.unwrap().name,
            "Bash",
            "the running tool is named"
        );

        engine.ingest_hook(
            hook(
                HookEvent::ToolFinished {
                    id: "t1".into(),
                    failed: false,
                },
                9.0,
            ),
            at(9.0),
        );
        assert_eq!(
            phase_of(&mut engine, 10.0),
            Some(SessionPhase::Generating),
            "after the tool the model writes again"
        );

        engine.ingest_hook(hook(HookEvent::Stopped { last_text: None }, 12.0), at(12.0));
        assert_eq!(
            phase_of(&mut engine, 13.0),
            Some(SessionPhase::Finished),
            "a stop newer than the registry finishes the turn"
        );

        engine.ingest_records(vec![record(IDLE, 12.2)], at(13.0));
        assert_eq!(
            phase_of(&mut engine, 14.0),
            Some(SessionPhase::Finished),
            "the registry agreeing keeps it finished"
        );
        assert_eq!(
            phase_of(&mut engine, 12.0 + FINISHED_FOR + 1.0),
            Some(SessionPhase::Idle),
            "an unread result fades to idle after a while"
        );

        engine.acknowledge("s1", at(20.0));
        assert_eq!(
            phase_of(&mut engine, 21.0),
            Some(SessionPhase::Idle),
            "looking at a finished session makes it idle"
        );

        // A result read long after the turn ended: the Clawd wakes for the
        // reader rather than sleeping on the page they just opened.
        let mut late_read = Engine::default();
        late_read.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        late_read.ingest_facts("s1", &[began(1.0), ended(3.0, true)], at(3.0));
        late_read.ingest_records(vec![record(IDLE, 3.0)], at(3.0));
        let read = 3.0 + DOZE_AFTER + 60.0;
        late_read.acknowledge("s1", at(read));
        let just_read = only(&mut late_read, read + 1.0).unwrap();
        assert!(
            just_read.phase == SessionPhase::Idle && !dozes(&just_read, at(read + 1.0)),
            "a session read long after it finished stays awake while the person reads it"
        );
        let left = only(&mut late_read, read + DOZE_AFTER + 1.0).unwrap();
        assert!(
            dozes(&left, at(read + DOZE_AFTER + 1.0)),
            "and dozes off once it has been left alone as long again"
        );

        let mut wait = Engine::default();
        wait.ingest_records(vec![waiting("permission prompt", 3.0)], at(3.0));
        assert_eq!(
            phase_of(&mut wait, 4.0),
            Some(SessionPhase::WaitingPermission),
            "the registry's permission prompt is a permission wait"
        );
        wait.ingest_records(vec![waiting("input needed", 5.0)], at(5.0));
        assert_eq!(
            phase_of(&mut wait, 6.0),
            Some(SessionPhase::WaitingInput),
            "other prompts wait for input"
        );
        assert_eq!(
            only(&mut wait, 6.0).unwrap().waiting_for.as_deref(),
            Some("input needed"),
            "the reason is kept while it waits"
        );

        let mut recap = Engine::default();
        recap.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        recap.ingest_facts(
            "s1",
            &[began(1.0), text("Done.", 2.0), ended(3.0, true)],
            at(3.0),
        );
        recap.ingest_records(vec![record(IDLE, 3.0)], at(3.0));
        recap.acknowledge("s1", at(4.0));
        recap.snapshot(at(700.0));
        recap.ingest_records(vec![record(BUSY, 900.0)], at(900.0));
        recap.snapshot(at(901.0));
        recap.ingest_records(vec![record(IDLE, 902.0)], at(902.0));
        let recapped = recap.snapshot(at(903.0));
        assert!(
            recapped.sessions[0].phase == SessionPhase::Idle
                && recapped.sessions[0].turn_started == Some(at(0.0))
                && !finished_event(&recapped.events),
            "busy a moment with nothing typed or said, as while a recap is written, finishes nothing"
        );
        let mut fresh = Engine::default();
        fresh.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        fresh.ingest_facts("s1", &[began(1.0), ended(3.0, true)], at(3.0));
        fresh.ingest_records(vec![record(IDLE, 3.0)], at(3.0));
        fresh.snapshot(at(4.0));
        fresh.ingest_records(vec![record(BUSY, 20.0)], at(20.0));
        fresh.snapshot(at(21.0));
        fresh.ingest_records(vec![record(IDLE, 22.0)], at(22.0));
        let back = fresh.snapshot(at(23.0));
        assert!(
            back.sessions[0].phase == SessionPhase::Finished && back.events.is_empty(),
            "a recap over a result not yet looked at shows it again without a second notice"
        );
        recap.ingest_records(vec![record(BUSY, 1000.0)], at(1000.0));
        recap.snapshot(at(1001.0));
        recap.ingest_facts("s1", &[began(1000.0), text("Fixed.", 1001.0)], at(1001.0));
        recap.ingest_records(vec![record(IDLE, 1003.0)], at(1003.0));
        assert!(
            finished_event(&recap.snapshot(at(1004.0)).events),
            "a turn the transcript shows still finishes when the registry goes idle"
        );

        let mut render = Engine::default();
        render.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        render.ingest_facts("s1", &[began(1.0)], at(1.0));
        render.snapshot(at(2.0));
        render.ingest_facts("s1", &[ended(5.0, true)], at(5.0));
        render.ingest_records(vec![record(SHELL, 5.0)], at(5.0));
        let behind = render.snapshot(at(6.0));
        assert_eq!(
            behind.sessions[0].phase,
            SessionPhase::Background,
            "a turn that left a command running in the background, like a render, is not done yet"
        );
        assert!(
            !finished_event(&behind.events),
            "so no notice says it finished"
        );
        assert_eq!(
            SessionCounts::of(&behind.sessions).working,
            1,
            "it counts as work, though no turn runs"
        );
        render.ingest_records(vec![record(BUSY, 30.0)], at(30.0));
        render.ingest_facts("s1", &[began(30.0)], at(30.0));
        assert_eq!(
            phase_of(&mut render, 31.0),
            Some(SessionPhase::Generating),
            "when the command ends Claude Code takes the turn up again"
        );

        let mut services = previewing();
        services.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        services.ingest_facts(
            "s1",
            &[
                began(1.0),
                job("t1", true, Some("Run own Metro"), 2.0),
                task("t1", "b1"),
                job("t2", true, Some("Run Convex"), 3.0),
                task("t2", "b2"),
                ended(5.0, true),
            ],
            at(5.0),
        );
        services.ingest_records(vec![record(SHELL, 5.0)], at(5.0));
        let left = only(&mut services, 6.0).unwrap();
        assert!(
            left.phase == SessionPhase::Finished && left.services == 2,
            "a turn that only left a dev server and a backend running is done, and says what runs"
        );
        services.ingest_facts(
            "s1",
            &[
                job("t3", false, Some("Render the videos"), 7.0),
                task("t3", "b3"),
            ],
            at(7.0),
        );
        let rendering = only(&mut services, 8.0).unwrap();
        assert!(
            rendering.phase == SessionPhase::Background
                && rendering.background_label.as_deref() == Some("Render the videos"),
            "work it waits for keeps it in the background, saying what"
        );
        services.ingest_facts("s1", &[agent_ended("b3", 9.0)], at(9.0));
        assert_ne!(
            phase_of(&mut services, 10.0),
            Some(SessionPhase::Background),
            "once that work ends, the services alone do not hold it there"
        );
        // Claude Code stays "busy" while subagents it sent to the background
        // work, and the turn that sent them still ends with its Stop hook.
        let mut explore = previewing();
        explore.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        explore.ingest_facts(
            "s1",
            &[
                began(1.0),
                job("t1", false, Some("Map friends tab"), 2.0),
                task("t1", "a1"),
                job("t2", false, Some("Map poll design"), 3.0),
                task("t2", "a2"),
            ],
            at(2.0),
        );
        explore.snapshot(at(3.0));
        explore.ingest_facts("s1", &[ended(10.0, true)], at(10.0));
        explore.ingest_hook(
            hook(
                HookEvent::Stopped {
                    last_text: Some("Waiting for both agents".into()),
                },
                10.0,
            ),
            at(10.0),
        );
        let sent_off = explore.snapshot(at(11.0));
        assert_eq!(
            sent_off.sessions[0].phase,
            SessionPhase::Background,
            "a turn that left subagents working in the background is not done"
        );
        assert_eq!(
            sent_off.sessions[0].background_label.as_deref(),
            Some("Map poll design"),
            "and says what it waits for"
        );
        assert!(sent_off.events.is_empty(), "so no notice says it finished");
        explore.ingest_facts("s1", &[agent_ended("a1", 100.0), began(100.0)], at(100.0));
        assert_eq!(
            phase_of(&mut explore, 101.0),
            Some(SessionPhase::Generating),
            "the first one's end takes the turn up again"
        );
        explore.ingest_facts("s1", &[ended(105.0, true)], at(105.0));
        let second = explore.snapshot(at(106.0));
        assert!(
            second.sessions[0].phase == SessionPhase::Background && second.events.is_empty(),
            "while the other still works it stays in the background"
        );
        explore.ingest_facts("s1", &[agent_ended("a2", 200.0), began(200.0)], at(200.0));
        explore.snapshot(at(201.0));
        explore.ingest_facts("s1", &[ended(230.0, true)], at(230.0));
        explore.ingest_records(vec![record(IDLE, 230.0)], at(230.0));
        let done = explore.snapshot(at(231.0));
        assert!(
            done.sessions[0].phase == SessionPhase::Finished && finished_event(&done.events),
            "once both are done and the turn ends, it finished, with a notice"
        );
        let mut served = Engine::default();
        served.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        served.ingest_facts(
            "s1",
            &[
                began(1.0),
                job("t1", true, Some("Run own Metro"), 2.0),
                task("t1", "b1"),
                ended(10.0, true),
            ],
            at(10.0),
        );
        assert_eq!(
            phase_of(&mut served, 11.0),
            Some(SessionPhase::Finished),
            "while the registry trails the end, a service alone does not hold it"
        );
        served.ingest_facts(
            "s1",
            &[
                job("t2", false, Some("Map the app"), 12.0),
                task("t2", "a1"),
            ],
            at(12.0),
        );
        assert_eq!(
            only(&mut served, 13.0).unwrap().services,
            1,
            "beside a subagent in the background it says what else runs"
        );
        // A background command refused before it ran gets no id and never ends.
        let mut refused = Engine::default();
        refused.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        refused.ingest_facts(
            "s1",
            &[
                began(1.0),
                TranscriptFact::ToolStarted(bash("b1", 2.0)),
                job("b1", false, Some("Submit the build"), 2.0),
                TranscriptFact::ToolFinished {
                    id: "b1".into(),
                    at: at(3.0),
                    failed: true,
                },
            ],
            at(3.0),
        );
        refused.snapshot(at(5.0));
        refused.ingest_facts("s1", &[ended(10.0, true)], at(10.0));
        let trailing = refused.snapshot(at(10.5));
        refused.ingest_records(vec![record(IDLE, 11.0)], at(11.0));
        let later = refused.snapshot(at(11.5));
        assert!(
            trailing.sessions[0].phase != SessionPhase::Background
                && (finished_event(&trailing.events) || finished_event(&later.events)),
            "a background command refused before it ran holds nothing, and the turn's end is still told"
        );
        let mut lost_end = Engine::default();
        lost_end.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        lost_end.ingest_facts(
            "s1",
            &[
                began(1.0),
                job("t1", false, None, 2.0),
                task("t1", "a1"),
                ended(10.0, true),
            ],
            at(10.0),
        );
        lost_end.ingest_records(vec![record(IDLE, 20.0)], at(20.0));
        assert_eq!(
            phase_of(&mut lost_end, 21.0),
            Some(SessionPhase::Finished),
            "an end not read does not hold it once the registry goes idle"
        );

        let mut unseen = Engine::default();
        unseen.ingest_records(vec![record(SHELL, 0.0)], at(0.0));
        assert_eq!(
            phase_of(&mut unseen, 1.0),
            Some(SessionPhase::Background),
            "a command running that was never seen start counts as work"
        );

        let mut dialog = Engine::default();
        dialog.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        dialog.ingest_hook(
            hook(
                HookEvent::Notification {
                    kind: "idle_prompt".into(),
                },
                9.0,
            ),
            at(9.0),
        );
        dialog.ingest_records(vec![waiting("dialog open", 10.0)], at(10.0));
        let opened = only(&mut dialog, 11.0).unwrap();
        assert!(
            opened.phase == SessionPhase::Idle
                && opened.waiting_for.as_deref() == Some("dialog open"),
            "a dialog the person opened, like /usage, waits on nobody but is still known"
        );
        assert!(
            opened.accepts_replies && !can_reply(&opened),
            "and nothing is typed into it"
        );
        dialog.ingest_hook(
            hook(
                HookEvent::Notification {
                    kind: "agent_needs_input".into(),
                },
                12.0,
            ),
            at(12.0),
        );
        assert_eq!(
            phase_of(&mut dialog, 13.0),
            Some(SessionPhase::WaitingInput),
            "a dialog Claude Code raised with a notice waits for the person"
        );
        dialog.ingest_records(vec![record(IDLE, 20.0)], at(20.0));
        dialog.ingest_records(vec![waiting("dialog open", 30.0)], at(30.0));
        assert_eq!(
            phase_of(&mut dialog, 31.0),
            Some(SessionPhase::Idle),
            "an earlier notice does not carry over to the next dialog"
        );
        dialog.ingest_hook(
            hook(
                HookEvent::Notification {
                    kind: String::new(),
                },
                28.0,
            ),
            at(31.0),
        );
        assert_eq!(
            phase_of(&mut dialog, 32.0),
            Some(SessionPhase::WaitingInput),
            "a notice written just before the registry changes still counts"
        );

        // The registry going idle after the last line read ends the turn.
        let mut missed = Engine::default();
        missed.ingest_records(vec![record(BUSY, 1.0)], at(1.0));
        missed.ingest_facts(
            "s1",
            &[began(1.0), TranscriptFact::ToolStarted(bash("t1", 2.0))],
            at(2.0),
        );
        missed.ingest_records(vec![record(IDLE, 30.0)], at(30.0));
        let finished = only(&mut missed, 31.0).unwrap();
        assert!(
            finished.phase == SessionPhase::Finished && finished.current_tool.is_none(),
            "a newer idle registry finishes a turn whose end was not read"
        );
        assert!(
            !finished.timeline[0].is_running(),
            "and closes the tools it left open"
        );

        // A busy registry that is older than the last line defers to it.
        let mut stale = Engine::default();
        stale.ingest_records(vec![record(BUSY, 1.0)], at(1.0));
        stale.ingest_facts("s1", &[began(1.0), ended(4.0, true)], at(4.0));
        assert_eq!(
            phase_of(&mut stale, 5.0),
            Some(SessionPhase::Finished),
            "a later end_turn wins over an older busy registry"
        );

        let mut interrupted = Engine::default();
        interrupted.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        interrupted.ingest_facts("s1", &[began(1.0), ended(2.0, false)], at(2.0));
        assert_eq!(
            phase_of(&mut interrupted, 3.0),
            Some(SessionPhase::Idle),
            "an interrupted turn is not a result to read"
        );

        let mut unknown = Engine::default();
        unknown.ingest_records(vec![record(None, 0.0)], at(0.0));
        unknown.ingest_facts("s1", &[began(1.0)], at(1.0));
        assert_eq!(
            phase_of(&mut unknown, 2.0),
            Some(SessionPhase::Generating),
            "without a registry status the transcript decides"
        );

        let mut late = Engine::default();
        late.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        late.ingest_facts("s1", &[began(10.0), ended(20.0, true)], at(20.0));
        late.ingest_facts("s1", &[began(15.0)], at(21.0));
        assert_eq!(
            phase_of(&mut late, 22.0),
            Some(SessionPhase::Finished),
            "a prompt read after its turn ended does not reopen it"
        );

        let mut queued = Engine::default();
        queued.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        queued.ingest_hook(hook(HookEvent::PromptSubmitted, 1.0), at(1.0));
        queued.ingest_hook(hook(HookEvent::ToolStarted(bash("t1", 2.0)), 2.0), at(2.0));
        queued.ingest_hook(hook(HookEvent::PromptSubmitted, 3.0), at(3.0));
        let busy = only(&mut queued, 4.0).unwrap();
        assert!(
            busy.turn_started == Some(at(1.0)) && busy.phase == SessionPhase::RunningTool,
            "a prompt queued while working joins the turn and leaves its tool running"
        );
    }

    // ClaudeSessionEngineTests.swift events.
    #[test]
    fn events() {
        let mut engine = Engine::default();
        engine.ingest_records(vec![waiting("permission prompt", 0.0)], at(0.0));
        assert!(
            engine.snapshot(at(1.0)).events.is_empty(),
            "a session seen for the first time raises nothing"
        );

        engine.ingest_records(vec![record(BUSY, 2.0)], at(2.0));
        assert!(
            engine.snapshot(at(3.0)).events.is_empty(),
            "going back to work raises nothing"
        );

        engine.ingest_hook(hook(HookEvent::ToolStarted(bash("t1", 4.0)), 4.0), at(4.0));
        engine.ingest_records(vec![waiting("permission prompt", 5.0)], at(5.0));
        assert_eq!(
            engine.snapshot(at(6.0)).events,
            [SessionEvent::NeedsPermission {
                id: "s1".into(),
                tool: "Bash".into()
            }],
            "a permission wait names the tool"
        );
        assert!(
            engine.snapshot(at(7.0)).events.is_empty(),
            "and is raised once"
        );

        engine.ingest_records(vec![waiting("input needed", 8.0)], at(8.0));
        assert_eq!(
            engine.snapshot(at(9.0)).events,
            [SessionEvent::NeedsInput { id: "s1".into() }],
            "a question raises a wait for input"
        );

        engine.ingest_records(vec![record(BUSY, 10.0)], at(10.0));
        engine.snapshot(at(11.0));
        engine.ingest_hook(hook(HookEvent::Stopped { last_text: None }, 20.0), at(20.0));
        assert_eq!(
            engine.snapshot(at(21.0)).events,
            [SessionEvent::Finished {
                id: "s1".into(),
                duration: Some(18.0)
            }],
            "a finished turn reports how long it took"
        );

        engine.ingest_records(vec![], at(30.0));
        assert!(
            engine.snapshot(at(31.0)).sessions.is_empty(),
            "a session gone from the registry leaves"
        );
    }

    // ClaudeSessionEngineTests.swift permissions.
    #[test]
    fn permissions() {
        let mut engine = Engine::default();
        engine.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        let asked = ask("r1", "rm -rf build", true, 2.0);
        engine.ingest_hook(
            hook(HookEvent::PermissionRequested(asked.clone()), 2.0),
            at(2.0),
        );
        let state = only(&mut engine, 3.0).unwrap();
        assert!(
            state.phase == SessionPhase::WaitingPermission
                && state.permission.as_ref() == Some(&asked),
            "a hook's permission request waits on the person before the registry says so"
        );
        assert!(
            state.hooked,
            "a session the hooks report can be answered here"
        );

        engine.ingest_records(vec![record(BUSY, 1.0)], at(3.0));
        assert_eq!(
            engine.permission("s1"),
            Some(&asked),
            "an older registry change does not drop the request"
        );

        engine.ingest_records(vec![record(BUSY, 4.0)], at(4.0));
        assert_eq!(
            engine.permission("s1"),
            None,
            "the registry leaving the prompt means it was answered"
        );

        engine.ingest_hook(
            hook(HookEvent::PermissionRequested(asked.clone()), 5.0),
            at(5.0),
        );
        engine.resolve_permission("other");
        assert!(
            engine.permission("s1").is_some(),
            "resolving another request leaves this one"
        );
        engine.resolve_permission("r1");
        assert_eq!(
            engine.permission("s1"),
            None,
            "resolving the request clears it"
        );

        // The request belongs to the tool that asked: another tool finishing,
        // as one run alongside it, leaves it waiting.
        engine.ingest_hook(hook(HookEvent::ToolStarted(bash("t9", 6.0)), 6.0), at(6.0));
        engine.ingest_hook(
            hook(
                HookEvent::ToolStarted(tool("t8", 6.0, "Read", Some("npm test"))),
                6.0,
            ),
            at(6.0),
        );
        engine.ingest_hook(
            hook(HookEvent::PermissionRequested(asked.clone()), 6.5),
            at(6.5),
        );
        assert_eq!(
            engine.permission("s1").and_then(|p| p.tool_id.as_deref()),
            Some("t9"),
            "a request is tied to the running tool of that name"
        );
        engine.ingest_hook(
            hook(
                HookEvent::ToolFinished {
                    id: "t8".into(),
                    failed: false,
                },
                7.0,
            ),
            at(7.0),
        );
        assert!(
            engine.permission("s1").is_some(),
            "another tool finishing leaves the request"
        );
        engine.ingest_facts(
            "s1",
            &[
                began(1.0),
                TranscriptFact::ToolFinished {
                    id: "t1".into(),
                    at: at(6.2),
                    failed: false,
                },
            ],
            at(7.0),
        );
        assert!(
            engine.permission("s1").is_some(),
            "older transcript lines read late leave the request"
        );
        engine.ingest_hook(
            hook(
                HookEvent::ToolFinished {
                    id: "t9".into(),
                    failed: false,
                },
                7.5,
            ),
            at(7.5),
        );
        assert_eq!(
            engine.permission("s1"),
            None,
            "its own tool finishing means it was answered"
        );

        // Two subagents can ask at once; both stay answerable, oldest first.
        let first = ask("r1", "ls", false, 20.0);
        let second = ask("r2", "rm", false, 21.0);
        engine.ingest_hook(
            hook_in(
                HookEvent::PermissionRequested(first),
                20.0,
                "s1",
                Some("a1"),
            ),
            at(20.0),
        );
        engine.ingest_hook(
            hook_in(
                HookEvent::PermissionRequested(second),
                21.0,
                "s1",
                Some("a2"),
            ),
            at(21.0),
        );
        assert!(
            engine.permission("s1").map(|p| p.request_id.as_str()) == Some("r1")
                && engine.holds("r2", "s1"),
            "a newer request waits behind the one on screen instead of replacing it"
        );
        assert_eq!(
            engine.permission("s1").and_then(|p| p.tool_id.as_deref()),
            Some(""),
            "a subagent's is tied to no tool"
        );
        engine.resolve_permission("r1");
        assert_eq!(
            engine.permission("s1").map(|p| p.request_id.as_str()),
            Some("r2"),
            "and shows once the first is answered"
        );
        assert!(
            !engine.holds("r1", "s1") && !engine.holds("r2", "other"),
            "answered or foreign requests are not held"
        );
        engine.ingest_hook(hook(HookEvent::PromptSubmitted, 22.0), at(22.0));
        assert_eq!(
            engine.permission("s1"),
            None,
            "a prompt typed after the requests means they were answered"
        );
        let third = ask("r3", "ls", false, 23.0);
        engine.ingest_hook(hook(HookEvent::PermissionRequested(third), 23.0), at(23.0));
        engine.ingest_hook(hook(HookEvent::PromptSubmitted, 22.5), at(24.0));
        assert!(
            engine.permission("s1").is_some(),
            "a prompt from before the request does not settle it"
        );
        engine.resolve_permission("r3");

        engine.ingest_hook(
            hook(HookEvent::PermissionRequested(asked.clone()), 8.0),
            at(8.0),
        );
        engine.ingest_hook(hook(HookEvent::Stopped { last_text: None }, 9.0), at(9.0));
        assert_eq!(
            engine.permission("s1"),
            None,
            "a turn ending drops its request"
        );

        // A question Claude asks waits on an answer, not a permission.
        let mut quiz = Engine::default();
        quiz.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        quiz.snapshot(at(1.0));
        let mut question =
            Permission::new("q1", Question::TOOL, Some("Which color?"), false, at(2.0));
        question.questions = vec![Question {
            question: "Which color?".into(),
            options: vec![QuestionOption::new("Red"), QuestionOption::new("Blue")],
            ..Question::default()
        }];
        quiz.ingest_hook(hook(HookEvent::PermissionRequested(question), 2.0), at(2.0));
        let quizzed = quiz.snapshot(at(3.0));
        assert!(
            quizzed.sessions[0].phase == SessionPhase::WaitingInput
                && quizzed.events == [SessionEvent::NeedsInput { id: "s1".into() }]
                && quizzed.sessions[0]
                    .permission
                    .as_ref()
                    .is_some_and(Permission::asks_questions),
            "a question waits for the person's answer and says so once"
        );

        // Two calls of one tool run side by side: the request belongs to the
        // one whose input it carries, not the newest.
        let mut twin = Engine::default();
        twin.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        let key_a = tool_summary::input_key(
            "WebFetch",
            json!({"url": "https://a.example"}).as_object().unwrap(),
        );
        let key_b = tool_summary::input_key(
            "WebFetch",
            json!({"url": "https://b.example"}).as_object().unwrap(),
        );
        let mut w1 = tool("w1", 1.0, "WebFetch", None);
        w1.input_key = Some(key_a);
        let mut w2 = tool("w2", 1.0, "WebFetch", None);
        w2.input_key = Some(key_b);
        twin.ingest_hook(hook(HookEvent::ToolStarted(w1), 1.0), at(1.0));
        twin.ingest_hook(hook(HookEvent::ToolStarted(w2), 1.0), at(1.0));
        let mut fetch = Permission::new("rw1", "WebFetch", Some("a.example"), false, at(2.0));
        fetch.input_key = Some(key_a);
        twin.ingest_hook(hook(HookEvent::PermissionRequested(fetch), 2.0), at(2.0));
        assert_eq!(
            twin.permission("s1").and_then(|p| p.tool_id.as_deref()),
            Some("w1"),
            "a request is tied to the call with its input"
        );
        twin.ingest_hook(
            hook(
                HookEvent::ToolFinished {
                    id: "w2".into(),
                    failed: false,
                },
                3.0,
            ),
            at(3.0),
        );
        assert!(
            twin.holds("rw1", "s1"),
            "the other call finishing leaves it waiting"
        );
        let unknown = Permission::new("rw3", "WebFetch", None, false, at(4.0));
        twin.ingest_hook(hook(HookEvent::PermissionRequested(unknown), 4.0), at(4.0));
        assert!(
            twin.permission("s1").and_then(|p| p.tool_id.as_deref()) == Some("w1")
                && twin.holds("rw3", "s1"),
            "a request that matches no single call is tied to none"
        );
        let mut double = Engine::default();
        double.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        double.ingest_hook(hook(HookEvent::ToolStarted(bash("x1", 1.0)), 1.0), at(1.0));
        double.ingest_hook(hook(HookEvent::ToolStarted(bash("x2", 1.0)), 1.0), at(1.0));
        double.ingest_hook(
            hook(
                HookEvent::PermissionRequested(ask("rx", "ls", false, 2.0)),
                2.0,
            ),
            at(2.0),
        );
        assert_eq!(
            double.permission("s1").and_then(|p| p.tool_id.clone()),
            None,
            "two calls of a name and no input match: none"
        );

        let mut sub = Engine::default();
        sub.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        sub.ingest_hook(
            hook(
                HookEvent::SubagentStarted {
                    id: "a1".into(),
                    agent_type: "Explore".into(),
                },
                1.0,
            ),
            at(1.0),
        );
        sub.ingest_hook(
            hook_in(
                HookEvent::ToolStarted(tool("t2", 2.0, "Grep", Some("x"))),
                2.0,
                "s1",
                Some("a1"),
            ),
            at(2.0),
        );
        sub.ingest_hook(
            hook_in(HookEvent::PermissionRequested(asked), 3.0, "s1", Some("a1")),
            at(3.0),
        );
        let state = only(&mut sub, 4.0).unwrap();
        assert!(
            state.timeline.is_empty(),
            "a subagent's tools stay out of the session's timeline"
        );
        assert_eq!(
            state.phase,
            SessionPhase::WaitingPermission,
            "a subagent's permission still waits on the person"
        );
        assert!(
            state
                .subagents
                .iter()
                .map(|a| a.agent_type.as_str())
                .eq(["Explore"])
                && state.subagents[0].is_running(),
            "the subagent is listed as running"
        );
        sub.ingest_hook(
            hook_in(
                HookEvent::SubagentStopped { id: "a1".into() },
                5.0,
                "s1",
                Some("a1"),
            ),
            at(5.0),
        );
        assert!(
            !only(&mut sub, 6.0).unwrap().subagents[0].is_running(),
            "and as done once it stops"
        );
    }

    // ClaudeSessionEngineTests.swift content.
    #[test]
    fn content() {
        let secret = "SENTINEL-7f3a";
        let mut closed = Engine::default();
        closed.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        closed.ingest_facts(
            "s1",
            &[
                began(1.0),
                TranscriptFact::Title(secret.into()),
                text(secret, 2.0),
                TranscriptFact::ToolStarted(tool("t1", 3.0, "Bash", Some(secret))),
                TranscriptFact::SubagentInfo {
                    id: "a1".into(),
                    agent_type: "Plan".into(),
                    description: Some(secret.into()),
                },
                TranscriptFact::Tasks(TaskProgress {
                    done: 1,
                    total: 3,
                    current: Some(secret.into()),
                }),
                TranscriptFact::Prompt {
                    text: secret.into(),
                    at: at(2.0),
                },
                text(secret, 2.0),
                job("b1", false, Some(secret), 3.0),
            ],
            at(3.0),
        );
        let mut quiet = Engine::default();
        quiet.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        quiet.ingest_hook(
            hook(
                HookEvent::ToolStarted(tool("t1", 1.0, "Bash", Some(secret))),
                1.0,
            ),
            at(1.0),
        );
        quiet.ingest_hook(
            hook(
                HookEvent::Stopped {
                    last_text: Some(secret.into()),
                },
                2.0,
            ),
            at(2.0),
        );
        for (engine, label) in [(&mut closed, "transcript"), (&mut quiet, "hooks")] {
            let state = only(engine, 4.0).unwrap();
            assert!(
                !format!("{state:?}").contains(secret),
                "with previews off nothing a session wrote reaches the island ({label})"
            );
            assert!(
                !serde_json::to_string(&state).unwrap().contains(secret),
                "nor its JSON ({label})"
            );
        }
        let state = only(&mut closed, 4.0).unwrap();
        assert!(
            state
                .tasks
                .as_ref()
                .is_some_and(|t| t.done == 1 && t.total == 3),
            "task progress counts are kept without their names"
        );
        assert_eq!(
            state.subagents[0].agent_type, "Plan",
            "a subagent's type is kept without its task"
        );

        let asked = Permission::new("r1", "Bash", Some(secret), false, at(5.0));
        quiet.ingest_hook(hook(HookEvent::PermissionRequested(asked), 5.0), at(5.0));
        assert_eq!(
            only(&mut quiet, 6.0)
                .unwrap()
                .permission
                .and_then(|p| p.summary)
                .as_deref(),
            Some(secret),
            "a pending permission always shows what it would run"
        );

        let mut open = previewing();
        open.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        open.ingest_facts(
            "s1",
            &[
                began(1.0),
                TranscriptFact::Title("Fix the build".into()),
                text("Done, tests pass.", 2.0),
                TranscriptFact::ToolStarted(bash("t1", 3.0)),
                TranscriptFact::SubagentInfo {
                    id: "a1".into(),
                    agent_type: "Plan".into(),
                    description: Some("Plan it".into()),
                },
                TranscriptFact::Tasks(TaskProgress {
                    done: 0,
                    total: 1,
                    current: Some("Doing".into()),
                }),
                job("b1", false, Some("Render"), 3.0),
            ],
            at(3.0),
        );
        open.ingest_records(vec![record(SHELL, 3.5)], at(3.5));
        let shown = only(&mut open, 4.0).unwrap();
        assert!(
            shown.title.as_deref() == Some("Fix the build")
                && shown.last_text.as_deref() == Some("Done, tests pass.")
                && shown.timeline[0].summary.as_deref() == Some("npm test"),
            "with previews on titles, replies and commands show"
        );
        assert_eq!(shown.subagents[0].description.as_deref(), Some("Plan it"));
        assert_eq!(
            shown.tasks.and_then(|t| t.current).as_deref(),
            Some("Doing")
        );
        assert_eq!(shown.background_label.as_deref(), Some("Render"));
        open.set_policy(ContentPolicy { previews: false });
        let purged = only(&mut open, 5.0).unwrap();
        assert!(
            purged.title.is_none()
                && purged.last_text.is_none()
                && purged.timeline[0].summary.is_none(),
            "turning previews off clears what was read"
        );
        open.set_policy(ContentPolicy { previews: true });
        let back = only(&mut open, 6.0).unwrap();
        assert!(
            back.title.is_none(),
            "and turning them back on does not bring it back by itself"
        );
        assert!(
            back.subagents[0].description.is_none()
                && back.background_label.is_none()
                && back.tasks.unwrap().current.is_none(),
            "nor the tasks, subagents and jobs it named"
        );

        let mut talk = previewing();
        talk.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        let said = |words: &str, seconds: f64| TranscriptFact::Prompt {
            text: words.into(),
            at: at(seconds),
        };
        talk.ingest_facts(
            "s1",
            &[
                said("Fix the build", 1.0),
                text("Looking.", 2.0),
                text("Looking.", 2.0),
                said("Also run lint", 3.0),
                text("Both pass.", 4.0),
            ],
            at(4.0),
        );
        let chat = only(&mut talk, 5.0).unwrap().chat;
        assert!(
            chat.iter().map(|m| m.text.as_str()).eq([
                "Fix the build",
                "Looking.",
                "Also run lint",
                "Both pass."
            ]) && chat.iter().map(|m| m.role).eq([
                ChatRole::User,
                ChatRole::Assistant,
                ChatRole::User,
                ChatRole::Assistant
            ]),
            "the conversation keeps prompts and replies in order, once each"
        );
        let ids: std::collections::HashSet<i64> = chat.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), chat.len(), "each message has its own id");
        assert!(
            chat.iter().all(|m| !m.ends_turn),
            "a reply ends no turn unless the transcript says so"
        );
        let ending = |words: &str, seconds: f64| TranscriptFact::Text {
            text: words.into(),
            at: at(seconds),
            ends_turn: true,
        };
        talk.ingest_facts("s1", &[ending("Both pass.", 4.0)], at(4.0));
        let marked = only(&mut talk, 5.0).unwrap().chat;
        assert!(
            marked.len() == 4 && marked.last().unwrap().ends_turn,
            "a reply read again as the one its turn ended on is marked, not repeated"
        );
        talk.ingest_facts("s1", &[text("Both pass.", 4.0)], at(4.0));
        assert!(
            only(&mut talk, 5.0).unwrap().chat.last().unwrap().ends_turn,
            "and reading it again plainly keeps the mark"
        );
        talk.ingest_facts(
            "s1",
            &[
                said("Ship it", 5.0),
                text("Shipping.", 6.0),
                ending("Shipped.", 7.0),
            ],
            at(7.0),
        );
        let shipped = only(&mut talk, 8.0).unwrap().chat;
        let tail = &shipped[shipped.len() - 3..];
        assert!(
            tail.iter()
                .map(|m| m.text.as_str())
                .eq(["Ship it", "Shipping.", "Shipped."])
                && tail.iter().map(|m| m.ends_turn).eq([false, false, true]),
            "a new reply its turn ended on joins the conversation marked"
        );
        // `NotchClaudeSessionsSupport.answers(in:)`, which splits the chat into
        // answers and replies on the way, is the UI's and not ported here.
        let many: Vec<TranscriptFact> = (0..CHAT_LENGTH + 10)
            .map(|n| text(&format!("m{n}"), 10.0))
            .collect();
        talk.ingest_facts("s1", &many, at(10.0));
        let long = only(&mut talk, 11.0).unwrap().chat;
        assert!(
            long.len() == CHAT_LENGTH
                && long.last().unwrap().text == format!("m{}", CHAT_LENGTH + 9),
            "a long conversation keeps its latest messages"
        );
        talk.set_policy(ContentPolicy { previews: false });
        assert!(
            only(&mut talk, 12.0).unwrap().chat.is_empty(),
            "turning previews off clears the conversation"
        );
    }

    // ClaudeSessionEngineTests.swift bookkeeping (the summary and detail
    // checks live with `tool_summary` in models.rs).
    #[test]
    fn bookkeeping() {
        let mut engine = Engine::default();
        engine.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        let first = Tokens {
            input: 10,
            cache_write: 0,
            cache_read: 100,
            output: 5,
            reasoning: 0,
        };
        let streamed = Tokens {
            output: 40,
            ..first
        };
        let usage = |key: &str, tokens: Tokens, cost: f64| TranscriptFact::Usage {
            key: key.into(),
            tokens,
            cost,
        };
        engine.ingest_facts(
            "s1",
            &[
                usage("m1", first, 0.01),
                usage("m1", streamed, 0.02),
                usage("m2", first, 0.01),
                TranscriptFact::Branch("feat/x".into()),
                TranscriptFact::Model("claude-opus-5-5".into()),
            ],
            at(1.0),
        );
        let state = only(&mut engine, 2.0).unwrap();
        assert_eq!(
            state.tokens.output, 45,
            "streamed blocks of one reply count once at their largest"
        );
        assert!(
            (state.cost - 0.03).abs() < 1e-9,
            "cost adds up across replies"
        );
        assert!(
            state.branch.as_deref() == Some("feat/x")
                && state.model.as_deref() == Some("claude-opus-5-5"),
            "branch and model are kept"
        );
        assert_eq!(state.project, "app", "the project is the folder's name");

        let mut tools = vec![began(1.0)];
        for index in 0..40 {
            tools.push(TranscriptFact::ToolStarted(bash(
                &format!("t{index}"),
                f64::from(index) + 2.0,
            )));
            tools.push(TranscriptFact::ToolFinished {
                id: format!("t{index}"),
                at: at(f64::from(index) + 2.5),
                failed: index == 39,
            });
        }
        engine.ingest_facts("s1", &tools, at(50.0));
        let timeline = only(&mut engine, 51.0).unwrap().timeline;
        assert!(
            timeline.len() == TIMELINE_LENGTH
                && timeline.last().unwrap().id == "t39"
                && timeline.last().unwrap().failed,
            "the timeline keeps the latest tools in order"
        );

        // Subagents: the transcript's start and type stay; a hook adds to them.
        let mut team = Engine::default();
        team.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        team.ingest_facts(
            "s1",
            &[
                TranscriptFact::SubagentActivity {
                    id: "a1".into(),
                    at: at(10.0),
                },
                TranscriptFact::SubagentInfo {
                    id: "a1".into(),
                    agent_type: "general-purpose".into(),
                    description: None,
                },
            ],
            at(10.0),
        );
        team.ingest_hook(
            hook(
                HookEvent::SubagentStarted {
                    id: "a1".into(),
                    agent_type: String::new(),
                },
                40.0,
            ),
            at(40.0),
        );
        let agent = &only(&mut team, 41.0).unwrap().subagents[0];
        assert!(
            agent.agent_type == "general-purpose" && agent.started == at(10.0),
            "a late start hook keeps the subagent's real start and type"
        );
        team.ingest_hook(
            hook(HookEvent::SubagentStopped { id: "ghost".into() }, 42.0),
            at(42.0),
        );
        assert_eq!(
            only(&mut team, 43.0).unwrap().subagents.len(),
            1,
            "a stop for a subagent not seen adds no blank row"
        );
        team.ingest_facts(
            "s1",
            &[TranscriptFact::SubagentActivity {
                id: "ghost".into(),
                at: at(30.0),
            }],
            at(44.0),
        );
        let ghost = only(&mut team, 45.0)
            .unwrap()
            .subagents
            .into_iter()
            .find(|a| a.id == "ghost")
            .unwrap();
        assert_eq!(
            ghost.finished,
            Some(at(42.0)),
            "and ends it once its transcript is read"
        );
        team.ingest_facts(
            "s1",
            &[
                TranscriptFact::SubagentActivity {
                    id: "a2".into(),
                    at: at(20.0),
                },
                agent_ended("a1", 50.0),
            ],
            at(50.0),
        );
        let agents = only(&mut team, 51.0).unwrap().subagents;
        assert!(
            agents
                .iter()
                .map(|a| a.id.as_str())
                .eq(["a2", "a1", "ghost"]),
            "working subagents come first, then the latest done"
        );
        let a1 = agents.iter().find(|a| a.id == "a1").unwrap();
        assert!(
            a1.finished == Some(at(50.0)) && a1.started == at(10.0),
            "a background subagent's end comes from the session's task notification"
        );
        // A subagent that works again after its end is running again.
        team.ingest_facts(
            "s1",
            &[TranscriptFact::SubagentActivity {
                id: "a1".into(),
                at: at(60.0),
            }],
            at(60.0),
        );
        assert!(
            only(&mut team, 61.0)
                .unwrap()
                .subagents
                .iter()
                .find(|a| a.id == "a1")
                .unwrap()
                .is_running()
        );

        let mut early = Engine::default();
        early.ingest_hook(
            hook_in(HookEvent::PromptSubmitted, 0.0, "s2", None),
            at(0.0),
        );
        early.ingest_records(vec![], at(10.0));
        early.ingest_records(vec![record_full("s2", BUSY, None, 0.0)], at(11.0));
        assert_eq!(
            only(&mut early, 12.0).unwrap().turn_started,
            Some(at(0.0)),
            "hook events before the registry lists a session are kept"
        );
        let mut gone = Engine::default();
        gone.ingest_hook(
            hook_in(HookEvent::PromptSubmitted, 0.0, "s3", None),
            at(0.0),
        );
        gone.ingest_records(vec![], at(UNLISTED_GRACE + 1.0));
        gone.ingest_records(
            vec![record_full("s3", IDLE, None, 0.0)],
            at(UNLISTED_GRACE + 2.0),
        );
        assert_eq!(
            only(&mut gone, UNLISTED_GRACE + 3.0).unwrap().turn_started,
            None,
            "but not for long when it never appears"
        );

        engine.ingest_facts(
            "s1",
            &[TranscriptFact::Tasks(TaskProgress {
                done: 1,
                total: 2,
                current: None,
            })],
            at(52.0),
        );
        assert!(only(&mut engine, 53.0).unwrap().tasks.is_some());
        engine.ingest_facts(
            "s1",
            &[TranscriptFact::Tasks(TaskProgress::default())],
            at(53.0),
        );
        assert!(
            only(&mut engine, 54.0).unwrap().tasks.is_none(),
            "a cleared task list stops showing progress"
        );
    }

    #[test]
    fn housekeeping() {
        // The first record per session wins.
        let mut engine = Engine::default();
        let mut twin = record(BUSY, 0.0);
        twin.pid = 200;
        engine.ingest_records(vec![record(IDLE, 0.0), twin], at(0.0));
        assert_eq!(engine.session_ids(), ["s1"]);
        assert_eq!(engine.record("s1").unwrap().pid, 100);
        // Acknowledging a session nobody knows does nothing.
        engine.acknowledge("nobody", at(1.0));
        assert!(!engine.activity.contains_key("nobody"));
        // No facts make no activity.
        engine.ingest_facts("s9", &[], at(1.0));
        assert!(!engine.activity.contains_key("s9"));
        // The phase clock keeps running while the phase stays.
        engine.ingest_records(vec![record(BUSY, 5.0)], at(5.0));
        let first = only(&mut engine, 6.0).unwrap();
        let later = only(&mut engine, 9.0).unwrap();
        assert!(first.phase_since == at(6.0) && later.phase_since == at(6.0));
        // A finished turn's service jobs are counted only at rest in a shell.
        assert_eq!(later.services, 0);
        // Older background jobs that ended go first past the limit.
        let mut jobs = Engine::default();
        jobs.ingest_records(vec![record(SHELL, 0.0)], at(0.0));
        let mut facts = Vec::new();
        for n in 0..BACKGROUND_KEPT + 5 {
            facts.push(job(&format!("j{n}"), true, None, n as f64));
            facts.push(task(&format!("j{n}"), &format!("k{n}")));
            if n < 10 {
                facts.push(agent_ended(&format!("k{n}"), 100.0));
            }
        }
        jobs.ingest_facts("s1", &facts, at(200.0));
        let kept = &jobs.activity["s1"].background;
        assert_eq!(kept.len(), BACKGROUND_KEPT);
        assert!(!kept.contains_key("j0") && kept.contains_key("j9") && kept.contains_key("j44"));
        assert_eq!(
            only(&mut jobs, 201.0).unwrap().services,
            (BACKGROUND_KEPT + 5 - 10) as i64
        );
        // An end read before the job's id applies once the id is known.
        let mut early_end = Engine::default();
        early_end.ingest_records(vec![record(BUSY, 0.0)], at(0.0));
        early_end.ingest_facts(
            "s1",
            &[
                agent_ended("a9", 3.0),
                job("t", false, None, 1.0),
                task("t", "a9"),
            ],
            at(3.0),
        );
        assert!(early_end.activity["s1"].running_background().is_empty());
        // Usage keeps the higher cost of a reply read twice.
        let mut priced = Engine::default();
        let tokens = Tokens::default();
        priced.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        priced.ingest_facts(
            "s1",
            &[
                TranscriptFact::Usage {
                    key: "k".into(),
                    tokens,
                    cost: 0.5,
                },
                TranscriptFact::Usage {
                    key: "k".into(),
                    tokens,
                    cost: 0.2,
                },
            ],
            at(1.0),
        );
        assert_eq!(only(&mut priced, 2.0).unwrap().cost, 0.5);
        // A session-end hook ends the turn unfinished.
        let mut quit = Engine::default();
        quit.ingest_records(vec![record(IDLE, 0.0)], at(0.0));
        quit.ingest_hook(hook(HookEvent::PromptSubmitted, 1.0), at(1.0));
        quit.ingest_hook(hook(HookEvent::SessionEnded, 2.0), at(2.0));
        assert_eq!(phase_of(&mut quit, 3.0), Some(SessionPhase::Idle));
        // A tool seen after the turn ended opens one whose prompt was missed.
        let mut unseen = Engine::default();
        unseen.ingest_records(vec![record(None, 0.0)], at(0.0));
        unseen.ingest_facts(
            "s1",
            &[
                began(1.0),
                ended(2.0, true),
                TranscriptFact::ToolStarted(bash("t", 3.0)),
            ],
            at(3.0),
        );
        let reopened = only(&mut unseen, 4.0).unwrap();
        assert!(
            reopened.phase == SessionPhase::RunningTool && reopened.turn_started == Some(at(3.0))
        );
        assert_eq!(
            reopened.current_tool.unwrap().summary,
            None,
            "with previews off its command is not kept"
        );
        // A waiting session's reason only shows while it waits.
        let mut why = Engine::default();
        why.ingest_records(
            vec![record_full("s1", IDLE, Some("permission prompt"), 0.0)],
            at(0.0),
        );
        assert_eq!(only(&mut why, 1.0).unwrap().waiting_for, None);
        // Quiet notices ask nothing.
        assert!(
            !asks("idle_prompt")
                && !asks("permission_prompt")
                && asks("agent_needs_input")
                && asks("")
        );
    }
}
