// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Sessions the island lets run without asking (#234): one started in its
//! agent's no-ask mode (Claude Code's bypass permissions, Codex with
//! approvals off) and, for whoever opts in, every session of an agent. A
//! permission such a session asks for is allowed at once, with no card,
//! sound or wait, and listed on its page; questions still come to the island.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use super::models::{Permission, Provider, Session, Time};

/// Claude Code's mode that skips its permission prompts. It still asks for
/// questions and for commands a settings `ask` rule names.
pub const CLAUDE_BYPASS: &str = "bypassPermissions";
/// Codex's approval policy that never asks.
pub const CODEX_NEVER: &str = "never";

/// Whether Claude Code's `permission_mode` is its no-ask mode. Its auto mode
/// is not: it asks on purpose when its classifier holds back.
pub fn is_auto_claude(mode: Option<&str>) -> bool {
    mode == Some(CLAUDE_BYPASS)
}

/// Whether a Codex thread's approval policy never asks.
pub fn is_auto_codex(policy: Option<&str>) -> bool {
    policy == Some(CODEX_NEVER)
}

/// The agents whose no-ask mode the island can tell.
pub const MODE_AGENTS: [Provider; 2] = [Provider::Claude, Provider::Codex];
/// The agents whose permissions the island answers itself.
pub const PROCEED_AGENTS: [Provider; 4] = [
    Provider::Claude,
    Provider::Codex,
    Provider::Opencode,
    Provider::Vibe,
];

/// Why a request went through without the island.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The session runs in its agent's no-ask mode.
    AutoMode,
    /// The person chose Always proceed for the agent.
    AlwaysProceed,
}

/// One permission the island allowed without asking, as the session's page lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LetThrough {
    pub id: String,
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub at: Time,
    pub reason: Reason,
}

impl LetThrough {
    pub fn of(permission: &Permission, at: Time, reason: Reason) -> Self {
        Self {
            id: permission.request_id.clone(),
            tool: permission.tool.clone(),
            summary: permission.summary.clone(),
            at,
            reason,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Agents whose no-ask sessions run without the island; on by default.
    pub auto_run: HashSet<Provider>,
    /// Agents whose every permission is allowed; off unless chosen.
    pub always_proceed: HashSet<Provider>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_run: MODE_AGENTS.into_iter().collect(),
            always_proceed: HashSet::new(),
        }
    }
}

impl Settings {
    pub fn none() -> Self {
        Self {
            auto_run: HashSet::new(),
            always_proceed: HashSet::new(),
        }
    }

    /// Why a permission of this agent's session goes through without the
    /// island, or None when the island asks.
    pub fn passes(&self, provider: Provider, auto_mode: bool) -> Option<Reason> {
        if auto_mode && self.auto_run.contains(&provider) {
            return Some(Reason::AutoMode);
        }
        self.always_proceed
            .contains(&provider)
            .then_some(Reason::AlwaysProceed)
    }
}

/// The prefs key of an agent's switch: `autoRunClaude`, `alwaysProceedOpencode`.
fn key(prefix: &str, provider: Provider) -> String {
    let name = provider.key();
    format!("{prefix}{}{}", name[..1].to_uppercase(), &name[1..])
}

/// The choices as the prefs keep them under `/sessions`.
pub fn settings_from_prefs(value: &Value) -> Settings {
    let flag = |name: String, default: bool| {
        value
            .pointer(&format!("/sessions/{name}"))
            .and_then(Value::as_bool)
            .unwrap_or(default)
    };
    Settings {
        auto_run: MODE_AGENTS
            .into_iter()
            .filter(|p| flag(key("autoRun", *p), true))
            .collect(),
        always_proceed: PROCEED_AGENTS
            .into_iter()
            .filter(|p| flag(key("alwaysProceed", *p), false))
            .collect(),
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

/// Prefs changed: requests from now on follow the new choices.
pub fn set_settings(settings: Settings) {
    if let Ok(mut current) = SETTINGS.lock() {
        *current = Some(settings);
    }
}

pub fn settings() -> Settings {
    SETTINGS
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_default()
}

/// What the island let through, by session, newest last; written from the
/// hook threads and Codex's connections alike.
#[derive(Default)]
pub struct Log {
    entries: Mutex<BTreeMap<String, Vec<LetThrough>>>,
}

impl Log {
    pub const KEPT: usize = 30;

    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn add(&self, entry: LetThrough, session: &str) {
        let mut entries = self.entries.lock().unwrap();
        let list = entries.entry(session.to_owned()).or_default();
        list.push(entry);
        if list.len() > Self::KEPT {
            let extra = list.len() - Self::KEPT;
            list.drain(..extra);
        }
    }

    pub fn entries(&self, session: &str) -> Vec<LetThrough> {
        self.entries
            .lock()
            .unwrap()
            .get(session)
            .cloned()
            .unwrap_or_default()
    }

    /// Gives each session what was let through for it.
    pub fn decorate(&self, sessions: &mut [Session]) {
        let entries = self.entries.lock().unwrap();
        for session in sessions {
            if let Some(list) = entries.get(&session.id) {
                session.let_through = list.clone();
            }
        }
    }

    /// Forgets sessions no longer listed, once a while passed since their
    /// last entry: a session may be heard before it is listed.
    pub fn prune(&self, listed: &HashSet<String>, now: Time) {
        self.entries.lock().unwrap().retain(|id, list| {
            listed.contains(id) || list.last().is_some_and(|e| now - e.at < 600.0)
        });
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

pub static LOG: Log = Log::new();

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn modes() {
        assert!(is_auto_claude(Some("bypassPermissions")));
        for mode in ["default", "auto", "acceptEdits", "plan", "dontAsk"] {
            assert!(!is_auto_claude(Some(mode)), "{mode} still asks");
        }
        assert!(!is_auto_claude(None));
        assert!(is_auto_codex(Some("never")));
        for policy in ["on-request", "untrusted", "granular"] {
            assert!(!is_auto_codex(Some(policy)));
        }
        assert!(!is_auto_codex(None));
    }

    #[test]
    fn settings_read_from_prefs() {
        let fresh = settings_from_prefs(&json!({}));
        assert_eq!(fresh, Settings::default());
        assert_eq!(fresh.passes(Provider::Claude, true), Some(Reason::AutoMode));
        assert_eq!(fresh.passes(Provider::Codex, true), Some(Reason::AutoMode));
        assert_eq!(
            fresh.passes(Provider::Claude, false),
            None,
            "a normal session asks"
        );
        assert_eq!(
            fresh.passes(Provider::Opencode, true),
            None,
            "its mode is not read"
        );

        let changed = settings_from_prefs(&json!({"sessions": {
            "autoRunClaude": false, "alwaysProceedOpencode": true, "alwaysProceedGemini": true,
        }}));
        assert_eq!(
            changed.passes(Provider::Claude, true),
            None,
            "the switch is off"
        );
        assert_eq!(
            changed.passes(Provider::Codex, true),
            Some(Reason::AutoMode),
            "per agent"
        );
        assert_eq!(
            changed.passes(Provider::Opencode, false),
            Some(Reason::AlwaysProceed)
        );
        assert_eq!(changed.passes(Provider::Vibe, false), None);
        assert!(
            !changed.always_proceed.contains(&Provider::Gemini),
            "the island cannot answer Gemini"
        );

        let both = settings_from_prefs(
            &json!({"sessions": {"autoRunClaude": false, "alwaysProceedClaude": true}}),
        );
        assert_eq!(
            both.passes(Provider::Claude, true),
            Some(Reason::AlwaysProceed)
        );
    }

    #[test]
    fn log_keeps_the_newest() {
        let log = Log::new();
        for i in 0..(Log::KEPT + 5) {
            log.add(
                LetThrough {
                    id: format!("r{i}"),
                    tool: "Bash".into(),
                    summary: None,
                    at: i as f64,
                    reason: Reason::AutoMode,
                },
                "s1",
            );
        }
        let kept = log.entries("s1");
        assert_eq!(kept.len(), Log::KEPT);
        assert_eq!(kept.first().unwrap().id, "r5");
        assert_eq!(kept.last().unwrap().id, "r34");
        log.add(
            LetThrough {
                id: "x".into(),
                tool: "Bash".into(),
                summary: None,
                at: 1000.0,
                reason: Reason::AutoMode,
            },
            "gone",
        );
        let listed: HashSet<String> = ["s1".to_owned()].into();
        log.prune(&listed, 1100.0);
        assert_eq!(
            log.entries("gone").len(),
            1,
            "heard before listed, kept a while"
        );
        log.prune(&listed, 2000.0);
        assert!(log.entries("gone").is_empty());
        assert!(!log.entries("s1").is_empty());
        log.clear();
        assert!(log.entries("s1").is_empty());
    }

    #[test]
    fn let_through_serializes_for_the_page() {
        let entry = LetThrough {
            id: "r1".into(),
            tool: "Bash".into(),
            summary: Some("ls".into()),
            at: 5.0,
            reason: Reason::AlwaysProceed,
        };
        assert_eq!(
            serde_json::to_value(entry).unwrap(),
            json!({"id": "r1", "tool": "Bash", "summary": "ls", "at": 5.0, "reason": "alwaysProceed"})
        );
    }
}
