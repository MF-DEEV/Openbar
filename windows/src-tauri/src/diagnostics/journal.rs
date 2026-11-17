// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The last events heard from sessions, in memory, for a diagnostics file
//! (#230): what kind of event, from which agent, when, and how the
//! session's state changed. Never what was said or run: a tool's name at
//! most. Each also goes into the day's log as a line. As the Mac app's
//! `DiagnosticsJournal.swift`.

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::sessions::hook_agents::Kind;
use crate::sessions::models::{HookEvent, Provider, SessionPhase};

pub const CAPACITY: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub time: String,
    pub agent: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub session: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

pub fn record(event: Event) {
    let mut line = format!("{} {}", event.agent, event.event);
    if let Some(detail) = &event.detail {
        line.push_str(&format!(" ({detail})"));
    }
    line.push_str(&format!(" session {}", event.session));
    if event.from != event.to {
        line.push_str(&format!(
            " {} -> {}",
            event.from.as_deref().unwrap_or("none"),
            event.to.as_deref().unwrap_or("none")
        ));
    }
    {
        let mut events = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
        events.push_back(event);
        while events.len() > CAPACITY {
            events.pop_front();
        }
    }
    super::log::info("events", &line);
}

/// The events kept, oldest first.
pub fn recent() -> Vec<Event> {
    EVENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .cloned()
        .collect()
}

pub fn make(
    agent: Provider,
    event: (&str, Option<String>),
    session: &str,
    from: Option<SessionPhase>,
    to: Option<SessionPhase>,
    at: DateTime<Utc>,
) -> Event {
    Event {
        time: super::log::timestamp(at),
        agent: agent.key().to_string(),
        event: event.0.to_string(),
        detail: event.1,
        session: short_session(session),
        from: from.map(phase_name),
        to: to.map(phase_name),
    }
}

/// The phase as the Mac app names it (`ClaudeSessionPhase`'s raw value).
pub fn phase_name(phase: SessionPhase) -> String {
    serde_json::to_value(phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

/// A Claude Code hook's kind, and the tool or notice it names.
pub fn claude_label(event: &HookEvent) -> (&'static str, Option<String>) {
    match event {
        HookEvent::SessionStarted => ("sessionStarted", None),
        HookEvent::SessionEnded => ("sessionEnded", None),
        HookEvent::PromptSubmitted => ("promptSubmitted", None),
        HookEvent::ToolStarted(tool) => ("toolStarted", safe(&tool.name)),
        HookEvent::ToolFinished { failed, .. } => {
            ("toolFinished", failed.then(|| "failed".to_string()))
        }
        HookEvent::PermissionRequested(permission) => {
            ("permissionRequested", safe(&permission.tool))
        }
        HookEvent::Notification { kind } => ("notification", safe(kind)),
        HookEvent::Stopped { .. } => ("stopped", None),
        HookEvent::SubagentStarted { agent_type, .. } => ("subagentStarted", safe(agent_type)),
        HookEvent::SubagentStopped { .. } => ("subagentStopped", None),
        HookEvent::Other(name) => ("other", safe(name)),
    }
}

/// Another agent's event kind, and the tool it names.
pub fn agent_label(kind: &Kind) -> (&'static str, Option<String>) {
    match kind {
        Kind::Started => ("sessionStarted", None),
        Kind::Ended => ("sessionEnded", None),
        Kind::Prompt(_) => ("promptSubmitted", None),
        Kind::Working => ("working", None),
        Kind::ToolStarted(tool) => ("toolStarted", safe(&tool.name)),
        Kind::ToolFinished { name, failed, .. } => (
            "toolFinished",
            safe(&if *failed {
                format!("{name}:failed")
            } else {
                name.clone()
            }),
        ),
        Kind::Permission(p) | Kind::HeldPermission(p) => ("permissionRequested", safe(&p.tool)),
        Kind::PermissionSettled(_) => ("permissionSettled", None),
        Kind::Question => ("question", None),
        Kind::Reply(_) => ("reply", None),
        Kind::Stopped { failed, .. } => ("stopped", failed.then(|| "failed".to_string())),
        Kind::Model(_) => ("model", None),
        Kind::Title(_) => ("title", None),
        Kind::Usage { .. } => ("usage", None),
    }
}

/// A name made only of what tool and event names are made of; anything
/// else (a sentence, a path) is left out.
pub fn safe(text: &str) -> Option<String> {
    (!text.is_empty()
        && text.chars().count() <= 64
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-')))
    .then(|| text.to_string())
}

/// The first characters of the agent's own id for a session: enough to
/// tell sessions apart in one file.
pub fn short_session(id: &str) -> String {
    let own = match id.split_once(':') {
        Some((agent, rest)) if Provider::from_key(agent).is_some() => rest,
        _ => id,
    };
    own.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::models::ToolUse;

    fn tool(name: &str, summary: &str) -> ToolUse {
        ToolUse {
            id: "t1".into(),
            name: name.into(),
            summary: Some(summary.into()),
            started: 0.0,
            finished: None,
            failed: false,
            input_key: None,
        }
    }

    #[test]
    fn labels_keep_names_never_content() {
        assert_eq!(
            agent_label(&Kind::ToolStarted(tool("Bash", "rm -rf ~/secret"))),
            ("toolStarted", Some("Bash".into()))
        );
        assert_eq!(
            agent_label(&Kind::Prompt(Some("CANARY-PROMPT".into()))),
            ("promptSubmitted", None)
        );
        assert_eq!(agent_label(&Kind::Reply("CANARY".into())), ("reply", None));
        assert_eq!(
            agent_label(&Kind::Stopped {
                last_text: Some("CANARY".into()),
                failed: true
            }),
            ("stopped", Some("failed".into()))
        );
        assert_eq!(
            claude_label(&HookEvent::Stopped {
                last_text: Some("CANARY".into())
            }),
            ("stopped", None)
        );
        assert_eq!(
            claude_label(&HookEvent::Other("PreCompact".into())),
            ("other", Some("PreCompact".into()))
        );
        assert_eq!(safe("rm -rf /tmp"), None);
        assert!(safe("mcp__github__create_issue").is_some());
        assert_eq!(safe(&"a".repeat(65)), None);
        assert_eq!(safe("zażółć"), None);
    }

    #[test]
    fn sessions_are_cut_short() {
        assert_eq!(short_session("gemini:abcdef12-3456-7890"), "abcdef12");
        assert_eq!(short_session("3f2504e0-4f89-11d3"), "3f2504e0");
        assert_eq!(short_session("host:a/b"), "hostab");
    }

    #[test]
    fn phases_are_named_as_on_the_mac() {
        assert_eq!(phase_name(SessionPhase::RunningTool), "runningTool");
        let event = make(
            Provider::Gemini,
            ("stopped", None),
            "gemini:x",
            Some(SessionPhase::Generating),
            Some(SessionPhase::Finished),
            DateTime::from_timestamp(0, 0).unwrap(),
        );
        assert_eq!(event.agent, "gemini");
        assert_eq!(event.from.as_deref(), Some("generating"));
        assert_eq!(event.to.as_deref(), Some("finished"));
        assert_eq!(event.time, "1970-01-01T00:00:00.000Z");
    }
}
