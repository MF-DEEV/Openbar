// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Factory's Droid (`droid`, #242), as the Mac's AgentDroid.swift. Its hooks
//! take Claude Code's format, in `~/.factory/settings.json` under `hooks` or
//! in `~/.factory/hooks.json` keyed by event (`FACTORY_HOME_OVERRIDE` moves
//! the home the folder sits in). An event `hooks.json` names replaces the
//! same event in `settings.json`, so each of the island's entries goes where
//! its event is read: beside the person's own in `hooks.json` when that file
//! has the event, else in `settings.json`.
//!
//! Droid has no permission hook: a call it asks about is told by the tool's
//! `PreToolUse`, then a `permission_prompt` notification naming no tool. Its
//! prompt takes Return for "Yes, allow" and, with "No, cancel" last, arrows
//! down to that and Return for no. A question (`AskUser`) shows an
//! `elicitation_dialog` whose choices take their number, and a spec
//! (`ExitSpecMode`) a dialog whose first choice proceeds, then asks the
//! autonomy to go on with.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::hook_message::Decision;
use super::models::{Permission, Question, QuestionOption, Time, tool_summary};
use super::plan_modes::PlanMode;
use crate::hooks::installer as claude;
use crate::hosts::Key;

/// Every event the island hears. Droid has no `PostToolUseFailure`, and a
/// turn the person stops ends with an `idle_prompt` notification, no `Stop`.
pub const EVENTS: [&str; 7] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
];
const TOOL_EVENTS: [&str; 2] = ["PreToolUse", "PostToolUse"];

pub const QUESTION_TOOL: &str = "AskUser";
pub const PLAN_TOOL: &str = "ExitSpecMode";

/// The autonomy a spec is approved into, in the order of Droid's second
/// dialog: Manual approvals (1), Low (2, edits), Medium (3). High, which
/// runs anything, is never picked from the island.
pub const PLAN_MODES: [PlanMode; 3] = [PlanMode::Auto, PlanMode::AcceptEdits, PlanMode::Manual];

/// Enough downs to reach the last of Droid's choices from the first.
pub const DENY_DOWNS: usize = 5;

// MARK: Places

/// Droid's folder: `.factory` in `FACTORY_HOME_OVERRIDE`, or in the home folder.
pub fn folder(home: &Path) -> PathBuf {
    std::env::var_os("FACTORY_HOME_OVERRIDE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.to_path_buf())
        .join(".factory")
}

pub fn settings(home: &Path) -> PathBuf {
    folder(home).join("settings.json")
}

/// The hooks file Droid reads in a folder: `hooks.json`, or the older
/// `hooks/hooks.json` while there is no `hooks.json`.
pub fn hooks_file(folder: &Path) -> PathBuf {
    let file = folder.join("hooks.json");
    let legacy = folder.join("hooks").join("hooks.json");
    if !file.exists() && legacy.exists() {
        legacy
    } else {
        file
    }
}

// MARK: Hooks

pub fn entry(event: &str, exe: &Path, app: &str) -> Value {
    json!({
        "type": "command",
        "timeout": super::hook_agents::installer::EVENT_TIMEOUT,
        "command": super::hook_agents::installer::portable_command(super::models::Provider::Droid, event, exe, app),
    })
}

fn holds(groups: Option<&Value>, wanted: &Value) -> bool {
    groups.and_then(Value::as_array).is_some_and(|groups| {
        groups.iter().any(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hooks| hooks.contains(wanted))
        })
    })
}

/// The state of the island's hooks in a Droid folder.
pub fn state(
    folder: &Path,
    exe: &Path,
    app: &str,
) -> Result<super::hook_agents::installer::State, claude::Failure> {
    use super::hook_agents::installer::State;
    let settings = claude::read(&folder.join("settings.json"))?;
    let hooks = claude::read(&hooks_file(folder))?;
    let current = EVENTS.iter().all(|event| {
        let groups = if hooks.get(*event).is_some() {
            hooks.get(*event)
        } else {
            settings.pointer(&format!("/hooks/{event}"))
        };
        holds(groups, &entry(event, exe, app))
    });
    if current {
        return Ok(State::Installed);
    }
    let marker = claude::marker(app);
    let any = claude::has_any_marked(&settings, &marker)
        || claude::has_any_marked(&json!({ "hooks": hooks }), &marker);
    Ok(if any { State::Outdated } else { State::Missing })
}

/// Puts the island's entries in, each where its event is read, after taking
/// any older ones out of both files. An unreadable file leaves both alone.
pub fn install(folder: &Path, exe: &Path, app: &str) -> Result<(), claude::Failure> {
    let hooks_path = hooks_file(folder);
    let settings_path = folder.join("settings.json");
    let hooks = claude::read(&hooks_path)?;
    claude::read(&settings_path)?;
    let marker = claude::marker(app);
    let kept = removing(&hooks, &marker);
    let in_hooks: Vec<&str> = EVENTS
        .iter()
        .copied()
        .filter(|e| kept.get(*e).is_some())
        .collect();
    let in_settings: Vec<&str> = EVENTS
        .iter()
        .copied()
        .filter(|e| !in_hooks.contains(e))
        .collect();
    claude::update(&hooks_path, |file| {
        adding(&removing(file, &marker), &in_hooks, exe, app)
    })?;
    claude::update(&settings_path, |settings| {
        let mut settings = settings.clone();
        let own = removing(
            settings.get("hooks").unwrap_or(&Value::Object(Map::new())),
            &marker,
        );
        let added = adding(&own, &in_settings, exe, app);
        if let Some(object) = settings.as_object_mut() {
            if added.as_object().is_some_and(Map::is_empty) {
                object.remove("hooks");
            } else {
                object.insert("hooks".into(), added);
            }
        }
        settings
    })
}

/// Takes the island's entries out of both files, a copy of each kept.
pub fn remove(folder: &Path, app: &str) -> Result<(), claude::Failure> {
    let marker = claude::marker(app);
    let hooks_path = hooks_file(folder);
    if claude::contents(&hooks_path)?.is_some() {
        claude::update(&hooks_path, |file| removing(file, &marker))?;
    }
    let settings_path = folder.join("settings.json");
    if claude::contents(&settings_path)?.is_some() {
        claude::update(&settings_path, |settings| {
            let Some(hooks) = settings.get("hooks").filter(|h| h.is_object()) else {
                return settings.clone();
            };
            let mut settings = settings.clone();
            let kept = removing(hooks, &marker);
            if let Some(object) = settings.as_object_mut() {
                if kept.as_object().is_some_and(Map::is_empty) {
                    object.remove("hooks");
                } else {
                    object.insert("hooks".into(), kept);
                }
            }
            settings
        })?;
    }
    Ok(())
}

/// An event map without the hooks whose command carries `marker`. An event
/// the island's removal empties goes; one the person left empty stays,
/// since an empty list in `hooks.json` clears the event in `settings.json`.
/// Keys that aren't events, as `hooksDisabled`, stay.
pub fn removing(hooks: &Value, marker: &str) -> Value {
    let mut hooks = hooks.clone();
    let Some(object) = hooks.as_object_mut() else {
        return hooks;
    };
    let events: Vec<String> = object.keys().cloned().collect();
    for event in events {
        let Some(groups) = object.get(&event).and_then(Value::as_array).cloned() else {
            continue;
        };
        if groups.is_empty() {
            continue;
        }
        let mut removed = false;
        let kept: Vec<Value> = groups
            .into_iter()
            .filter_map(|group| {
                let Some(entries) = group.get("hooks").and_then(Value::as_array) else {
                    return Some(group);
                };
                let remaining: Vec<Value> = entries
                    .iter()
                    .filter(|e| {
                        !e.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| c.contains(marker))
                    })
                    .cloned()
                    .collect();
                if remaining.len() == entries.len() {
                    return Some(group);
                }
                removed = true;
                if remaining.is_empty() {
                    return None;
                }
                let mut group = group;
                group["hooks"] = Value::Array(remaining);
                Some(group)
            })
            .collect();
        if !removed {
            continue;
        }
        if kept.is_empty() {
            object.remove(&event);
        } else {
            object.insert(event, Value::Array(kept));
        }
    }
    hooks
}

/// The island's groups after the person's own; tool events match every tool.
pub fn adding(hooks: &Value, events: &[&str], exe: &Path, app: &str) -> Value {
    let mut hooks = hooks.clone();
    if !hooks.is_object() {
        hooks = json!({});
    }
    let object = hooks.as_object_mut().expect("an object");
    for event in events {
        let mut group = json!({ "hooks": [entry(event, exe, app)] });
        if TOOL_EVENTS.contains(event) {
            group["matcher"] = json!("*");
        }
        let mut list = object
            .get(*event)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        list.push(group);
        object.insert((*event).to_owned(), Value::Array(list));
    }
    hooks
}

// MARK: Questions

/// The questions in an `AskUser` call's questionnaire: numbered `[question]`
/// lines, `(multi)` at the end of one for several picks, `[topic]` its
/// short label, `[option]` each choice. None when one has no question or
/// fewer than two choices.
pub fn questions_in(questionnaire: &str) -> Option<Vec<Question>> {
    let mut questions = Vec::new();
    let mut current: Option<Question> = None;
    fn close(current: &mut Option<Question>, questions: &mut Vec<Question>) -> bool {
        let Some(open) = current.take() else {
            return true;
        };
        if open.question.is_empty() || open.options.len() < 2 {
            return false;
        }
        questions.push(open);
        true
    }
    for line in questionnaire.lines().map(str::trim) {
        if let Some(at) = line.find("[question]") {
            if !close(&mut current, &mut questions) {
                return None;
            }
            let mut text = line[at + "[question]".len()..].trim().to_owned();
            let multi = text.to_lowercase().ends_with("(multi)");
            if multi {
                text = text[..text.len() - "(multi)".len()].trim().to_owned();
            }
            current = Some(Question {
                question: text,
                header: None,
                options: Vec::new(),
                multi_select: multi,
            });
        } else if let (Some(topic), Some(open)) = (line.strip_prefix("[topic]"), current.as_mut()) {
            let topic = topic.trim();
            open.header = (!topic.is_empty()).then(|| topic.to_owned());
        } else if let (Some(label), Some(open)) = (line.strip_prefix("[option]"), current.as_mut())
        {
            let label = label.trim();
            if !label.is_empty() {
                open.options.push(QuestionOption {
                    label: label.to_owned(),
                    ..Default::default()
                });
            }
        }
    }
    if !close(&mut current, &mut questions) || questions.is_empty() {
        return None;
    }
    Some(questions)
}

// MARK: Asks

/// What Droid's prompt would ask about the tool its `PreToolUse` named: the
/// command or file change, a spec, or the questions it asks. None for a
/// question the island can't show.
pub fn ask(
    tool: &str,
    raw: &str,
    input: &Map<String, Value>,
    cwd: Option<&str>,
    at: Time,
) -> Option<Permission> {
    if raw == QUESTION_TOOL {
        let questions = questions_in(input.get("questionnaire")?.as_str()?)?;
        let mut permission = Permission::new(
            &format!("{:x}", super::hook_agents::rand_id()),
            Question::TOOL,
            Some(tool_summary::clip(&questions[0].question)).as_deref(),
            false,
            at,
        );
        permission.questions = questions;
        return Some(permission);
    }
    let mut permission = super::hook_agents::grok_ask(tool, input, cwd, false, at);
    if raw == PLAN_TOOL {
        permission.plan_modes = PLAN_MODES.to_vec();
    }
    Some(permission)
}

// MARK: Keys

/// The keys that answer Droid's prompt, None for an answer it can't be
/// given by keys.
pub fn keys(decision: &Decision, asked: &Permission) -> Option<Vec<Key>> {
    if asked.tool == tool_summary::PLAN_TOOL {
        let mode = match decision {
            Decision::Allow | Decision::AllowAlways => PLAN_MODES[0],
            Decision::ApprovePlan(mode) if PLAN_MODES.contains(mode) => *mode,
            // Escape cancels the spec and keeps planning.
            Decision::Deny => return Some(vec![Key::Escape]),
            _ => return None,
        };
        let digit = match mode {
            PlanMode::Manual => 1,
            PlanMode::AcceptEdits => 2,
            _ => 3,
        };
        return Some(vec![Key::Digit(1), Key::Digit(digit)]);
    }
    if !asked.questions.is_empty() {
        return match decision {
            // Escape stops the agent rather than skipping the question.
            Decision::Deny => Some(vec![Key::Escape]),
            Decision::Answer(answers) => question_keys(answers, &asked.questions),
            _ => None,
        };
    }
    match decision {
        Decision::Allow => Some(vec![Key::Enter]),
        // "No, cancel" is last; the arrows stop there.
        Decision::Deny => {
            let mut keys = vec![Key::Down; DENY_DOWNS];
            keys.push(Key::Enter);
            Some(keys)
        }
        _ => None,
    }
}

/// Each question's answer in the dialog's order: a single choice is its
/// number, which moves on by itself; a multiple choice ticks each number,
/// then Tab moves on. Several questions, or a multiple choice, end on a
/// review whose Return submits them. Only listed choices, nine at most.
pub fn question_keys(
    answers: &std::collections::BTreeMap<String, Vec<String>>,
    questions: &[Question],
) -> Option<Vec<Key>> {
    if questions.is_empty() {
        return None;
    }
    let mut keys = Vec::new();
    for question in questions {
        let given = answers.get(&question.question).or_else(|| {
            (questions.len() == 1 && answers.len() == 1)
                .then(|| answers.values().next())
                .flatten()
        })?;
        let picked: Vec<&String> = given.iter().filter(|p| !p.is_empty()).collect();
        if picked.is_empty() {
            return None;
        }
        let mut numbers = Vec::new();
        for label in &picked {
            let index = question.options.iter().position(|o| &o.label == *label)?;
            if index >= 9 {
                return None;
            }
            numbers.push(index as u8 + 1);
        }
        if question.multi_select {
            numbers.sort_unstable();
            numbers.dedup();
            keys.extend(numbers.into_iter().map(Key::Digit));
            keys.push(Key::Tab);
        } else {
            if numbers.len() != 1 {
                return None;
            }
            keys.push(Key::Digit(numbers[0]));
        }
    }
    if questions.len() > 1 || questions.iter().any(|q| q.multi_select) {
        keys.push(Key::Enter);
    }
    Some(keys)
}

// MARK: Log

/// A session's own settings beside its transcript
/// (`sessions/<folder>/<id>.settings.json`) name its model.
pub fn model(transcript: &Path, home: &Path) -> Option<String> {
    let name = transcript.to_str()?.strip_suffix(".jsonl")?;
    let data = small(Path::new(&format!("{name}.settings.json")))?;
    let json: Value = serde_json::from_slice(&data).ok()?;
    let model = json.get("model")?.as_str().filter(|m| !m.is_empty())?;
    if !model.starts_with("custom:") {
        return Some(model.to_owned());
    }
    Some(custom_name(model, small(&settings(home)).as_deref()).unwrap_or_else(|| model.to_owned()))
}

/// The name a custom model is shown by: Droid calls the `n`th of
/// `customModels` `custom:<its displayName, dashed>-<n>`.
pub fn custom_name(id: &str, settings: Option<&[u8]>) -> Option<String> {
    let json: Value = serde_json::from_slice(settings?).ok()?;
    let models = json.get("customModels")?.as_array()?;
    let index: usize = id.rsplit('-').next()?.parse().ok()?;
    let model = models.get(index)?;
    model
        .get("displayName")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .or_else(|| model.get("model").and_then(Value::as_str))
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

/// The text of the reply a `Stop` names by its message id, from the end of
/// the transcript. Read only with previews on.
pub fn reply(transcript: &Path, message: &str) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(transcript).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(524_288)))
        .ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;
    reply_in(&String::from_utf8_lossy(&data), message)
}

pub fn reply_in(text: &str, message: &str) -> Option<String> {
    for line in text.lines().rev().filter(|l| l.contains(message)) {
        let Ok(json) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if json.get("id").and_then(Value::as_str) != Some(message)
            || json.pointer("/message/role").and_then(Value::as_str) != Some("assistant")
        {
            continue;
        }
        let parts = json.pointer("/message/content").and_then(Value::as_array)?;
        let text: Vec<&str> = parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect();
        return super::hook_agents::reply_preview(Some(&Value::String(text.join("\n"))));
    }
    None
}

fn small(path: &Path) -> Option<Vec<u8>> {
    if std::fs::metadata(path).ok()?.len() > 1_048_576 {
        return None;
    }
    std::fs::read(path).ok()
}

/// Whether a tool's response says it failed: a command that exited with a
/// code other than 0, or an error.
pub fn failed(response: Option<&Value>) -> bool {
    match response {
        Some(Value::Object(object)) => object.get("error").is_some_and(|e| !e.is_null()),
        Some(Value::String(text)) => {
            let Some(at) = text.rfind("[Process exited with code ") else {
                return false;
            };
            let code: String = text[at + "[Process exited with code ".len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .collect();
            code.parse::<i64>().is_ok_and(|c| c != 0)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const QUESTIONS: &str = "1. [question] Which database?\n[topic] DB\n[option] Postgres\n[option] SQLite\n\n\
        2. [question] Which extras do you want? (multi)\n[topic] Extras\n[option] Tests\n[option] Docs\n[option] CI";

    #[test]
    fn questionnaires_are_read() {
        let questions = questions_in(QUESTIONS).unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].header.as_deref(), Some("DB"));
        assert!(questions[1].multi_select);
        assert_eq!(questions[1].question, "Which extras do you want?");
        assert_eq!(questions[1].options.len(), 3);
        assert!(questions_in("1. [question] Pick?\n[option] Only").is_none());
        assert!(questions_in("nothing").is_none());
    }

    #[test]
    fn answers_are_keys() {
        let questions = questions_in(QUESTIONS).unwrap();
        let mut answers = BTreeMap::new();
        answers.insert("Which database?".to_owned(), vec!["SQLite".to_owned()]);
        answers.insert(
            "Which extras do you want?".to_owned(),
            vec!["CI".to_owned(), "Tests".to_owned()],
        );
        assert_eq!(
            question_keys(&answers, &questions),
            Some(vec![
                Key::Digit(2),
                Key::Digit(1),
                Key::Digit(3),
                Key::Tab,
                Key::Enter
            ])
        );
        let one = questions_in("1. [question] Color?\n[option] Red\n[option] Blue").unwrap();
        let mut picked = BTreeMap::new();
        picked.insert("Color?".to_owned(), vec!["Blue".to_owned()]);
        assert_eq!(question_keys(&picked, &one), Some(vec![Key::Digit(2)]));
        picked.insert("Color?".to_owned(), vec!["Purple".to_owned()]);
        assert_eq!(question_keys(&picked, &one), None);
    }

    #[test]
    fn prompts_and_specs_are_keys() {
        let command = super::super::hook_agents::grok_ask(
            "Bash",
            json!({"command": "rm -rf x"}).as_object().unwrap(),
            None,
            false,
            0.0,
        );
        assert_eq!(keys(&Decision::Allow, &command), Some(vec![Key::Enter]));
        let mut deny = vec![Key::Down; DENY_DOWNS];
        deny.push(Key::Enter);
        assert_eq!(keys(&Decision::Deny, &command), Some(deny));
        assert_eq!(keys(&Decision::AllowAlways, &command), None);
        let spec = ask(
            "ExitPlanMode",
            PLAN_TOOL,
            json!({"plan": "1. Add a button."}).as_object().unwrap(),
            None,
            0.0,
        )
        .unwrap();
        assert_eq!(spec.plan_modes, PLAN_MODES.to_vec());
        assert_eq!(
            keys(&Decision::Allow, &spec),
            Some(vec![Key::Digit(1), Key::Digit(3)])
        );
        assert_eq!(
            keys(&Decision::ApprovePlan(PlanMode::Manual), &spec),
            Some(vec![Key::Digit(1), Key::Digit(1)])
        );
        assert_eq!(keys(&Decision::Deny, &spec), Some(vec![Key::Escape]));
    }

    #[test]
    fn hooks_go_where_their_event_is_read() {
        let dir = std::env::temp_dir().join(format!("ob-droid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = dir.join(".factory");
        std::fs::create_dir_all(&folder).unwrap();
        let exe = Path::new("C:/Openbar/Openbar.exe");
        std::fs::write(folder.join("settings.json"), br#"{"logoAnimation":"off"}"#).unwrap();
        use super::super::hook_agents::installer::State;
        assert_eq!(state(&folder, exe, "app").unwrap(), State::Missing);
        install(&folder, exe, "app").unwrap();
        assert_eq!(state(&folder, exe, "app").unwrap(), State::Installed);
        assert!(!folder.join("hooks.json").exists());
        std::fs::write(
            folder.join("hooks.json"),
            br#"{"hooksDisabled":false,"Stop":[{"hooks":[{"type":"command","command":"say done"}]}],"PreCompact":[]}"#,
        )
        .unwrap();
        assert_eq!(state(&folder, exe, "app").unwrap(), State::Outdated);
        install(&folder, exe, "app").unwrap();
        assert_eq!(state(&folder, exe, "app").unwrap(), State::Installed);
        let hooks = claude::read(&folder.join("hooks.json")).unwrap();
        let settings = claude::read(&folder.join("settings.json")).unwrap();
        assert_eq!(hooks["Stop"].as_array().unwrap().len(), 2);
        assert!(settings.pointer("/hooks/Stop").is_none());
        assert_eq!(settings["logoAnimation"], "off");
        remove(&folder, "app").unwrap();
        let hooks = claude::read(&folder.join("hooks.json")).unwrap();
        let settings = claude::read(&folder.join("settings.json")).unwrap();
        assert_eq!(hooks["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(hooks["PreCompact"], json!([]));
        assert_eq!(hooks["hooksDisabled"], json!(false));
        assert!(settings.get("hooks").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn logs_name_the_model_and_the_reply() {
        assert_eq!(
            custom_name(
                "custom:Mock-Coder-0",
                Some(br#"{"customModels":[{"model":"m","displayName":"Mock Coder"}]}"#)
            )
            .as_deref(),
            Some("Mock Coder")
        );
        assert_eq!(
            custom_name("custom:X-3", Some(br#"{"customModels":[]}"#)),
            None
        );
        let log = r#"{"type":"message","id":"a","message":{"role":"user","content":[{"type":"text","text":"hi"}]}}
{"type":"message","id":"b","message":{"role":"assistant","content":[{"type":"text","text":"Mock reply"}]}}"#;
        assert_eq!(reply_in(log, "b").as_deref(), Some("Mock reply"));
        assert_eq!(reply_in(log, "a"), None);
        assert!(failed(Some(&json!("x [Process exited with code 2]"))));
        assert!(!failed(Some(&json!("x [Process exited with code 0]"))));
        assert!(failed(Some(&json!({"error": "no"}))));
    }
}
