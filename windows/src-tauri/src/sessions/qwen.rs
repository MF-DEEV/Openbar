// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Alibaba's Qwen Code (`qwen`, #239), as the Mac's AgentQwen.swift. Its
//! hooks take Claude Code's format in `~/.qwen/settings.json` (`QWEN_HOME`
//! moves the folder). Its permission hook runs before its own dialog shows,
//! and the dialog only opens once the hook answered nothing: the island
//! holds a tool's request and answers it, and lets it go unanswered when
//! the person opens the session to answer in Qwen Code. A question
//! (`ask_user_question`) and a plan (`exit_plan_mode`) are never allowed by
//! a hook: Qwen Code's dialog takes them, and the island presses its keys.

use std::collections::{HashMap, HashSet};

use serde_json::json;

use super::hook_message::{DENY_MESSAGE, Decision};
use super::models::{Permission, Question, Time, tool_summary};
use super::plan_modes::PlanMode;
use crate::hosts::Key;

/// Every event the island hears. `Notification` carries `permission_prompt`
/// and `idle_prompt`; `StopFailure` ends a turn an error ended.
pub const EVENTS: [&str; 10] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
];

pub const QUESTION_TOOL: &str = "ask_user_question";
pub const PLAN_TOOL: &str = "exit_plan_mode";

/// Whether Qwen Code's own dialog takes the tool, whatever a hook answers.
pub fn asked_in_qwen(raw: &str) -> bool {
    raw == QUESTION_TOOL || raw == PLAN_TOOL
}

/// The modes Qwen Code's plan dialog offers after "restore previous mode".
pub const PLAN_MODES: [PlanMode; 2] = [PlanMode::AcceptEdits, PlanMode::Manual];

/// What the permission hook prints: allow or deny. A question or a plan is
/// never answered there.
pub fn reply(decision: &Decision) -> Option<Vec<u8>> {
    let body = match decision {
        Decision::Allow | Decision::AllowAlways => json!({"behavior": "allow"}),
        Decision::Deny => json!({"behavior": "deny", "message": DENY_MESSAGE}),
        Decision::Answer(_) | Decision::ApprovePlan(_) => return None,
    };
    serde_json::to_vec(
        &json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": body}}),
    )
    .ok()
}

// MARK: Keys

/// One step of an answer typed into Qwen Code's dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Key(Key),
    /// Text typed on "Type something…", then sent with one Return.
    Text(String),
    /// A pause for the dialog to move on.
    Wait(u64),
}

/// Whether the request is answered in Qwen Code's own dialog, by keys: a
/// question or a plan; a tool's too once its hook was let go (`in_dialog`).
pub fn answered_by_keys(asked: &Permission) -> bool {
    !asked.questions.is_empty() || asked.tool == tool_summary::PLAN_TOOL
}

/// How long Qwen Code waits for its permission hook: its message bus gives
/// up on a request after 60 seconds and shows its own dialog. The island
/// lets the hook go a little before, and answers that dialog.
pub const HOLD_LIMIT: f64 = 55.0;

/// The keys that answer Qwen Code's dialog for a question or a plan:
/// arrows and Return, never a choice's number, which picks at once (the
/// Mac's Terminal sends a return after every key). Each question opens on
/// its first choice; a multiple choice is ticked with Space, then Return.
/// "Type something…" follows the choices. With more than one question, a
/// last tab asks to submit, Submit already picked. A plan's dialog always
/// offers four choices and its arrows wrap around them. Escape skips a
/// question. Two returns at Qwen Code's empty prompt would send the reply
/// it suggests, so no answer ends on a spare one.
pub fn keys(decision: &Decision, asked: &Permission, in_dialog: bool) -> Option<Vec<Step>> {
    if in_dialog && asked.questions.is_empty() && asked.tool != tool_summary::PLAN_TOOL {
        // A tool's dialog: "Yes, allow once" is picked first, Escape says no.
        return match decision {
            Decision::Allow => Some(vec![Step::Key(Key::Enter)]),
            Decision::Deny => Some(vec![Step::Key(Key::Escape)]),
            _ => None,
        };
    }
    if asked.tool == tool_summary::PLAN_TOOL {
        let downs = match decision {
            Decision::Allow | Decision::AllowAlways => 1,
            Decision::ApprovePlan(PlanMode::AcceptEdits) => 1,
            Decision::ApprovePlan(PlanMode::Manual) => 2,
            // "No, keep planning", the last of four.
            Decision::Deny => 3,
            _ => return None,
        };
        let mut steps = vec![Step::Key(Key::Down); downs];
        steps.push(Step::Key(Key::Enter));
        return Some(steps);
    }
    if asked.questions.is_empty() {
        return None;
    }
    match decision {
        Decision::Deny => Some(vec![Step::Key(Key::Escape)]),
        Decision::Answer(answers) => question_keys(answers, &asked.questions),
        _ => None,
    }
}

/// Each question's answer as keys, in the dialog's order.
pub fn question_keys(
    answers: &std::collections::BTreeMap<String, Vec<String>>,
    questions: &[Question],
) -> Option<Vec<Step>> {
    if questions.is_empty() {
        return None;
    }
    let mut steps = Vec::new();
    for question in questions {
        let given = answers.get(&question.question).or_else(|| {
            (questions.len() == 1 && answers.len() == 1)
                .then(|| answers.values().next())
                .flatten()
        });
        let picked: Vec<&String> = given?.iter().filter(|p| !p.is_empty()).collect();
        let first = *picked.first()?;
        let labels: Vec<&str> = question.options.iter().map(|o| o.label.as_str()).collect();
        if question.multi_select {
            // Only its choices: a typed answer beside ticks isn't one the keys can give.
            let mut indices = Vec::new();
            for label in &picked {
                indices.push(labels.iter().position(|l| l == label)?);
            }
            indices.sort_unstable();
            indices.dedup();
            let mut at = 0;
            for index in indices {
                steps.extend(std::iter::repeat_n(Step::Key(Key::Down), index - at));
                steps.push(Step::Key(Key::Space));
                at = index;
            }
            steps.push(Step::Key(Key::Enter));
        } else if let Some(index) = labels.iter().position(|l| l == first) {
            steps.extend(std::iter::repeat_n(Step::Key(Key::Down), index));
            steps.push(Step::Key(Key::Enter));
        } else {
            // Typed on "Type something…", which follows the choices.
            steps.extend(std::iter::repeat_n(Step::Key(Key::Down), labels.len()));
            steps.push(Step::Text(first.clone()));
        }
        steps.push(Step::Wait(300));
    }
    // Several questions end on a tab that asks to submit them.
    if questions.len() > 1 {
        steps.push(Step::Key(Key::Enter));
    }
    Some(steps)
}

// MARK: Log

/// The model of the session's last reply, from its chat log
/// (`projects/<project>/chats/<session>.jsonl`), for a session first heard
/// after its start (AgentQwen.model). Only the log's end is read.
pub fn model(path: &std::path::Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(262_144)))
        .ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    model_in(&String::from_utf8_lossy(&data))
}

pub fn model_in(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .filter(|line| line.contains(r#""type":"assistant""#))
        .find_map(|line| {
            let json: serde_json::Value = serde_json::from_str(line).ok()?;
            json.get("model")
                .and_then(serde_json::Value::as_str)
                .filter(|m| !m.is_empty())
                .map(str::to_owned)
        })
}

// MARK: Turns

/// What an event says of Qwen Code's turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fact {
    Prompt,
    Tool,
    Asked,
    Stopped,
    Idle,
}

/// What tells Qwen Code's turns apart from what it does after them
/// (AgentQwen.Turns). Once a turn stops, its memory helper may run tools
/// under the same session and prompt: tools of a prompt that already
/// stopped are dropped. `idle_prompt` comes each time the model stops
/// answering, also just before a tool of the same turn runs, so it ends the
/// turn only when no tool started just before it, or when a dialog waited:
/// then the person cancelled it with Escape, which brings no `Stop`.
#[derive(Debug, Default)]
pub struct Turns {
    stopped: HashMap<String, String>,
    tool_started: HashMap<String, Time>,
    asked: HashSet<String>,
}

pub const TOOL_GRACE: f64 = 2.0;

impl Turns {
    /// Whether an event of the session's goes on to the island.
    pub fn keep(&mut self, session: &str, turn: Option<&str>, fact: Fact, now: Time) -> bool {
        let done = turn.is_some_and(|t| self.stopped.get(session).map(String::as_str) == Some(t));
        match fact {
            Fact::Prompt => {
                self.tool_started.remove(session);
                self.asked.remove(session);
                true
            }
            Fact::Tool => {
                if done {
                    return false;
                }
                // A tool that starts or ends means no dialog waits any more.
                self.tool_started.insert(session.to_owned(), now);
                self.asked.remove(session);
                true
            }
            Fact::Asked => {
                if done {
                    return false;
                }
                self.asked.insert(session.to_owned());
                true
            }
            Fact::Stopped => {
                if let Some(turn) = turn {
                    self.stopped.insert(session.to_owned(), turn.to_owned());
                }
                self.tool_started.remove(session);
                self.asked.remove(session);
                true
            }
            Fact::Idle => {
                if self.asked.remove(session) {
                    self.tool_started.remove(session);
                    return true;
                }
                if self
                    .tool_started
                    .get(session)
                    .is_some_and(|started| now - started < TOOL_GRACE)
                {
                    return false;
                }
                self.tool_started.remove(session);
                true
            }
        }
    }

    pub fn forget(&mut self, session: &str) {
        self.stopped.remove(session);
        self.tool_started.remove(session);
        self.asked.remove(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::models::QuestionOption;
    use std::collections::BTreeMap;

    fn option(label: &str) -> QuestionOption {
        QuestionOption {
            label: label.into(),
            ..Default::default()
        }
    }

    fn question(text: &str, labels: &[&str], multi: bool) -> Question {
        Question {
            question: text.into(),
            header: None,
            options: labels.iter().map(|l| option(l)).collect(),
            multi_select: multi,
        }
    }

    fn asked(tool: &str, questions: Vec<Question>) -> Permission {
        Permission {
            request_id: "r".into(),
            tool: tool.into(),
            summary: None,
            detail: String::new(),
            content: None,
            offers_always: false,
            since: 0.0,
            tool_id: None,
            input_key: None,
            questions,
            diffs: Vec::new(),
            plan_modes: Vec::new(),
            risk: crate::sessions::risk::Risk::Unassessed,
        }
    }

    fn answers(pairs: &[(&str, &[&str])]) -> Decision {
        Decision::Answer(
            pairs
                .iter()
                .map(|(q, a)| ((*q).to_owned(), a.iter().map(|s| (*s).to_owned()).collect()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    #[test]
    fn reads_the_model_from_the_log() {
        let log = "{\"type\":\"user\"}\n{\"type\":\"assistant\",\"model\":\"qwen3-coder-plus\"}\n{\"type\":\"system\"}\n";
        assert_eq!(model_in(log).as_deref(), Some("qwen3-coder-plus"));
        assert_eq!(model_in("{\"type\":\"user\"}"), None);
        assert_eq!(model(std::path::Path::new("/nope")), None);
    }

    #[test]
    fn replies_allow_and_deny_only() {
        let allow: serde_json::Value =
            serde_json::from_slice(&reply(&Decision::Allow).unwrap()).unwrap();
        assert_eq!(
            allow["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let deny: serde_json::Value =
            serde_json::from_slice(&reply(&Decision::Deny).unwrap()).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(
            deny["hookSpecificOutput"]["decision"]["message"],
            DENY_MESSAGE
        );
        assert!(reply(&Decision::ApprovePlan(PlanMode::Manual)).is_none());
        assert!(reply(&answers(&[("q", &["a"])])).is_none());
    }

    #[test]
    fn answers_questions_with_arrows() {
        let color = asked(
            "AskUserQuestion",
            vec![question("Which color?", &["Red", "Blue", "Green"], false)],
        );
        assert_eq!(
            keys(&answers(&[("Which color?", &["Green"])]), &color, false),
            Some(vec![
                Step::Key(Key::Down),
                Step::Key(Key::Down),
                Step::Key(Key::Enter),
                Step::Wait(300)
            ])
        );
        assert_eq!(
            keys(&answers(&[("Which color?", &["Purple"])]), &color, false),
            Some(vec![
                Step::Key(Key::Down),
                Step::Key(Key::Down),
                Step::Key(Key::Down),
                Step::Text("Purple".into()),
                Step::Wait(300)
            ])
        );
        assert_eq!(
            keys(&Decision::Deny, &color, false),
            Some(vec![Step::Key(Key::Escape)])
        );
        assert_eq!(keys(&Decision::Allow, &color, false), None);
        let two = asked(
            "AskUserQuestion",
            vec![
                question("Which database?", &["Postgres", "SQLite"], false),
                question("Which extras?", &["Tests", "Docs", "CI"], true),
            ],
        );
        assert_eq!(
            keys(
                &answers(&[
                    ("Which database?", &["SQLite"]),
                    ("Which extras?", &["CI", "Tests"])
                ]),
                &two,
                false
            ),
            Some(vec![
                Step::Key(Key::Down),
                Step::Key(Key::Enter),
                Step::Wait(300),
                Step::Key(Key::Space),
                Step::Key(Key::Down),
                Step::Key(Key::Down),
                Step::Key(Key::Space),
                Step::Key(Key::Enter),
                Step::Wait(300),
                Step::Key(Key::Enter),
            ])
        );
        assert_eq!(
            keys(
                &answers(&[
                    ("Which database?", &["SQLite"]),
                    ("Which extras?", &["Mine"])
                ]),
                &two,
                false
            ),
            None
        );
        assert_eq!(
            keys(&answers(&[("Which database?", &["SQLite"])]), &two, false),
            None
        );
    }

    #[test]
    fn answers_plans_with_arrows() {
        let plan = asked(tool_summary::PLAN_TOOL, Vec::new());
        let downs = |n: usize| {
            let mut v = vec![Step::Key(Key::Down); n];
            v.push(Step::Key(Key::Enter));
            Some(v)
        };
        assert_eq!(
            keys(&Decision::ApprovePlan(PlanMode::AcceptEdits), &plan, false),
            downs(1)
        );
        assert_eq!(
            keys(&Decision::ApprovePlan(PlanMode::Manual), &plan, false),
            downs(2)
        );
        assert_eq!(keys(&Decision::Allow, &plan, false), downs(1));
        assert_eq!(keys(&Decision::Deny, &plan, false), downs(3));
        assert_eq!(
            keys(&Decision::ApprovePlan(PlanMode::Auto), &plan, false),
            None
        );
        assert!(answered_by_keys(&plan));
        // A tool's own dialog, once its hook was let go.
        let tool = asked("Bash", Vec::new());
        assert_eq!(
            keys(&Decision::Allow, &tool, true),
            Some(vec![Step::Key(Key::Enter)])
        );
        assert_eq!(
            keys(&Decision::Deny, &tool, true),
            Some(vec![Step::Key(Key::Escape)])
        );
        assert_eq!(keys(&Decision::AllowAlways, &tool, true), None);
        assert!(HOLD_LIMIT < 60.0);
        assert!(!answered_by_keys(&asked("Bash", Vec::new())));
        assert_eq!(
            keys(&Decision::Allow, &asked("Bash", Vec::new()), false),
            None
        );
    }

    #[test]
    fn tells_turns_from_what_runs_after() {
        let mut turns = Turns::default();
        let s = "s";
        assert!(turns.keep(s, Some("p1"), Fact::Prompt, 0.0));
        assert!(turns.keep(s, Some("p1"), Fact::Tool, 0.0));
        assert!(
            !turns.keep(s, Some("p1"), Fact::Idle, 0.01),
            "idle as a tool starts"
        );
        assert!(turns.keep(s, Some("p1"), Fact::Tool, 8.0));
        assert!(turns.keep(s, Some("p1"), Fact::Stopped, 9.0));
        assert!(turns.keep(s, Some("p1"), Fact::Idle, 9.0));
        assert!(
            !turns.keep(s, Some("p1"), Fact::Tool, 9.1),
            "the memory helper's tool"
        );
        assert!(turns.keep(s, Some("p2"), Fact::Prompt, 20.0));
        assert!(turns.keep(s, Some("p2"), Fact::Asked, 20.05));
        assert!(
            turns.keep(s, Some("p2"), Fact::Idle, 21.0),
            "Escape at a dialog"
        );
        assert!(turns.keep(s, Some("p3"), Fact::Tool, 30.0));
        assert!(
            turns.keep(s, Some("p3"), Fact::Idle, 40.0),
            "Escape during a long tool"
        );
        assert!(turns.keep(s, Some("p4"), Fact::Asked, 50.0));
        assert!(turns.keep(s, Some("p4"), Fact::Tool, 51.0));
        assert!(turns.keep(s, Some("p4"), Fact::Tool, 55.0));
        assert!(!turns.keep(s, Some("p4"), Fact::Idle, 55.01));
        turns.forget(s);
        assert!(turns.keep(s, Some("p1"), Fact::Tool, 60.0));
    }
}
