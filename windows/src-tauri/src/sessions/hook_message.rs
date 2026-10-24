// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! One hook event as Claude Code writes it to the hook's input, and the
//! answer a permission request takes back. Ported from
//! `ClaudeHookMessage.swift`, with the question form's answer rules from
//! `NotchClaudeSessionsSupport.swift`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value, json};
use unicode_segmentation::UnicodeSegmentation;

use super::models::{
    ContentPolicy, HookEvent, HookFact, Permission, Question, Time, ToolUse, canonical_json,
    preview, str_of, tool_summary, trim,
};
use super::transcript::REPLY_LIMIT;

#[derive(Clone, Debug, PartialEq)]
pub struct HookMessage {
    pub fact: HookFact,
    pub transcript: Option<String>,
    /// The rules Claude Code offered to stop asking, handed back as they came
    /// when the person picks Always.
    pub suggestions: Option<Value>,
    /// The input of a question, handed back whole with the answers.
    pub question_input: Option<Map<String, Value>>,
}

impl HookMessage {
    pub fn is_permission_request(&self) -> bool {
        matches!(self.fact.event, HookEvent::PermissionRequested(_))
    }

    /// Nil for input that is not a hook event. Text and arguments are kept
    /// only as the policy allows, except what a permission would run.
    pub fn decode(
        data: &[u8],
        request_id: &str,
        now: Time,
        policy: ContentPolicy,
    ) -> Option<HookMessage> {
        let value: Value = serde_json::from_slice(data).ok()?;
        let json = value.as_object()?;
        let name = str_of(json.get("hook_event_name"))?;
        let session = str_of(json.get("session_id")).filter(|s| !s.is_empty())?;
        let tool = str_of(json.get("tool_name")).unwrap_or("");
        let empty = Map::new();
        let input = json
            .get("tool_input")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let tool_id = str_of(json.get("tool_use_id")).unwrap_or("");
        let agent = str_of(json.get("agent_id"))
            .filter(|a| !a.is_empty())
            .map(str::to_owned);
        let mut suggestions = None;
        let mut question_input = None;
        // Subagent events name the subagent they are about; any other event
        // carrying an agent comes from inside that subagent.
        let mut from = agent.clone();
        let event = match name {
            "SessionStart" => HookEvent::SessionStarted,
            "SessionEnd" => HookEvent::SessionEnded,
            "UserPromptSubmit" => HookEvent::PromptSubmitted,
            "PreToolUse" => {
                if tool_id.is_empty() {
                    return None;
                }
                let summary = if policy.previews {
                    tool_summary::summary(tool, input)
                } else {
                    None
                };
                let mut started = ToolUse::new(tool_id, tool, summary, now);
                started.input_key = Some(tool_summary::input_key(tool, input));
                HookEvent::ToolStarted(started)
            }
            "PostToolUse" | "PostToolUseFailure" => {
                if tool_id.is_empty() {
                    return None;
                }
                HookEvent::ToolFinished {
                    id: tool_id.to_owned(),
                    failed: name == "PostToolUseFailure",
                }
            }
            // A question the island cannot show is asked in the terminal,
            // never allowed without its answers.
            "PermissionRequest"
                if tool == Question::TOOL && Question::questions_in(input).is_none() =>
            {
                HookEvent::Other(name.to_owned())
            }
            "PermissionRequest" => {
                let offered = json
                    .get("permission_suggestions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let offers = !offered.is_empty();
                if offers {
                    suggestions = Some(Value::Array(offered));
                }
                // A question Claude asks comes as a permission for its tool;
                // the answers go back in the tool's input.
                let questions = if tool == Question::TOOL {
                    Question::questions_in(input).unwrap_or_default()
                } else {
                    Vec::new()
                };
                if !questions.is_empty() {
                    question_input = Some(input.clone());
                }
                HookEvent::PermissionRequested(Permission {
                    request_id: request_id.to_owned(),
                    tool: if tool.is_empty() {
                        "Tool".to_owned()
                    } else {
                        tool.to_owned()
                    },
                    summary: questions
                        .first()
                        .map(|q| tool_summary::clip(&q.question))
                        .or_else(|| tool_summary::summary(tool, input)),
                    detail: tool_summary::permission_detail(tool, input),
                    offers_always: offers && questions.is_empty(),
                    since: now,
                    tool_id: None,
                    input_key: Some(tool_summary::input_key(tool, input)),
                    questions,
                })
            }
            "Notification" => HookEvent::Notification {
                kind: str_of(json.get("notification_type"))
                    .unwrap_or("")
                    .to_owned(),
            },
            "Stop" => {
                let text = if policy.previews {
                    preview(str_of(json.get("last_assistant_message")), REPLY_LIMIT)
                } else {
                    None
                };
                HookEvent::Stopped { last_text: text }
            }
            "SubagentStart" => {
                let id = agent.clone()?;
                from = None;
                HookEvent::SubagentStarted {
                    id,
                    agent_type: str_of(json.get("agent_type")).unwrap_or("Agent").to_owned(),
                }
            }
            "SubagentStop" => {
                let id = agent.clone()?;
                from = None;
                HookEvent::SubagentStopped { id }
            }
            other => HookEvent::Other(other.to_owned()),
        };
        Some(HookMessage {
            fact: HookFact {
                session_id: session.to_owned(),
                date: now,
                event,
                agent_id: from,
            },
            transcript: str_of(json.get("transcript_path")).map(str::to_owned),
            suggestions,
            question_input,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Allow,
    AllowAlways,
    Deny,
    /// What the person picked or typed for each question, by its text.
    Answer(BTreeMap<String, Vec<String>>),
}

pub const DENY_MESSAGE: &str = "Denied from the Dynamic Island.";
pub const SKIP_MESSAGE: &str =
    "The user skipped the question from the Dynamic Island without answering.";

/// What the hook prints for Claude Code, with sorted keys. Always adds the
/// rules Claude Code offered, so it stops asking for the same thing. Answers
/// go back in the question's own input, unchanged but for them. Empty when
/// there is no question input to answer, which leaves the terminal to ask.
pub fn reply(
    decision: &Decision,
    suggestions: Option<&Value>,
    input: Option<&Map<String, Value>>,
) -> Vec<u8> {
    let body = match decision {
        Decision::Allow => json!({"behavior": "allow"}),
        Decision::AllowAlways => {
            let mut body = Map::new();
            body.insert("behavior".into(), json!("allow"));
            if let Some(suggestions) = suggestions {
                body.insert("updatedPermissions".into(), suggestions.clone());
            }
            Value::Object(body)
        }
        Decision::Deny => {
            json!({"behavior": "deny", "message": if input.is_none() { DENY_MESSAGE } else { SKIP_MESSAGE }})
        }
        Decision::Answer(answers) => {
            let Some(input) = input else {
                return Vec::new();
            };
            let mut updated = input.clone();
            // One answer goes as text; several, for a question that takes
            // more than one, as a list.
            let answers: Map<String, Value> = answers
                .iter()
                .map(|(question, picked)| {
                    let value = if picked.len() == 1 {
                        json!(picked[0])
                    } else {
                        json!(picked)
                    };
                    (question.clone(), value)
                })
                .collect();
            updated.insert("answers".into(), Value::Object(answers));
            json!({"behavior": "allow", "updatedInput": Value::Object(updated)})
        }
    };
    let output =
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": body}});
    canonical_json(&output).into_bytes()
}

/// Whether the input is a permission request, told without decoding the rest.
pub fn is_permission_request(data: &[u8]) -> bool {
    memchr::memmem::find(data, br#""hook_event_name":"PermissionRequest""#).is_some()
        || serde_json::from_slice::<Value>(data)
            .ok()
            .is_some_and(|v| str_of(v.get("hook_event_name")) == Some("PermissionRequest"))
}

/// The input is complete once it parses: the hook sends one JSON object
/// and keeps the connection open for the answer.
pub fn is_complete(buffer: &[u8]) -> bool {
    let last = buffer
        .iter()
        .rev()
        .find(|b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'));
    last == Some(&b'}') && serde_json::from_slice::<Value>(buffer).is_ok()
}

/// What goes back for Claude's questions: for each, the options picked,
/// in the order Claude wrote them, and the person's own words, which
/// take the place of a single choice. Nil until every question has an
/// answer. `picked` and `typed` are keyed by the question's index.
pub fn answers(
    questions: &[Question],
    picked: &HashMap<usize, HashSet<String>>,
    typed: &HashMap<usize, String>,
) -> Option<BTreeMap<String, Vec<String>>> {
    if questions.is_empty() {
        return None;
    }
    let mut answers = BTreeMap::new();
    for (index, question) in questions.iter().enumerate() {
        let mut chosen: Vec<String> = question
            .options
            .iter()
            .map(|option| option.label.clone())
            .filter(|label| picked.get(&index).is_some_and(|set| set.contains(label)))
            .collect();
        let own = clipped_utf16(
            typed.get(&index).map_or("", |text| trim(text)),
            Question::OWN_ANSWER_LIMIT,
        );
        if !own.is_empty() {
            if question.multi_select {
                chosen.push(own);
            } else {
                chosen = vec![own];
            }
        }
        if !question.multi_select {
            chosen.truncate(1);
        }
        if chosen.is_empty() {
            return None;
        }
        answers.insert(question.question.clone(), chosen);
    }
    Some(answers)
}

/// The text cut to whole characters within a count of UTF-16 units, the
/// length Claude Code measures.
pub fn clipped_utf16(text: &str, limit: usize) -> String {
    if text.encode_utf16().count() <= limit {
        return text.to_owned();
    }
    let mut used = 0;
    let mut end = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        let size = grapheme.encode_utf16().count();
        if used + size > limit {
            break;
        }
        used += size;
        end = index + grapheme.len();
    }
    text[..end].to_owned()
}

/// Ctrl+Y allows and Ctrl+N denies (⌘Y and ⌘N on the Mac), as in the
/// dialogs that ask the same; `modifier_only` is that modifier alone held.
pub fn decision_for_key(key: Option<&str>, modifier_only: bool) -> Option<Decision> {
    if !modifier_only {
        return None;
    }
    match key.map(str::to_lowercase).as_deref() {
        Some("y") => Some(Decision::Allow),
        Some("n") => Some(Decision::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // Skipped from ClaudeHookTests.swift: `server` (the AF_UNIX socket and
    // `nc`), `installer` (settings.json edits) and `replies` (AppleScript,
    // the Antigravity bridge, pasteboard images). The Windows transport,
    // installer and hosts replace them and carry their own tests.
    use super::super::models::QuestionOption;
    use super::*;

    const NOW: f64 = 1_792_600_000.0;
    const BASE: &str = r#""session_id":"s1","transcript_path":"/x/s1.jsonl","cwd":"/a""#;

    fn decode(json: &str, previews: bool) -> Option<HookMessage> {
        HookMessage::decode(json.as_bytes(), "r1", NOW, ContentPolicy { previews })
    }

    fn output(data: &[u8]) -> Map<String, Value> {
        let value: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
        value
            .get("hookSpecificOutput")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    const QUESTIONS: &str = r#"{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"Warm"},{"label":"Blue","description":"","preview":"[ blue ]"}],"multiSelect":false},{"question":"Which sizes?","header":"","options":[{"label":"S"},{"label":"M"},{"label":"L"}],"multiSelect":true}]}"#;

    fn question_message() -> HookMessage {
        decode(
            &format!(
                r#"{{{BASE},"hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{QUESTIONS},"permission_suggestions":[{{"type":"addRules"}}]}}"#
            ),
            false,
        )
        .expect("a question decodes")
    }

    // ClaudeHookTests.swift messages.
    #[test]
    fn messages() {
        let permission = decode(
            &format!(
                r#"{{{BASE},"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{{"command":"rm -rf build","description":"Clean"}},"permission_suggestions":[{{"type":"addRules","rules":[{{"toolName":"Bash"}}]}}]}}"#
            ),
            false,
        )
        .unwrap();
        assert!(
            permission.is_permission_request()
                && permission.transcript.as_deref() == Some("/x/s1.jsonl")
        );
        let HookEvent::PermissionRequested(ask) = &permission.fact.event else {
            panic!("a permission request decodes as one")
        };
        assert!(
            ask.tool == "Bash"
                && ask.summary.as_deref() == Some("rm -rf build")
                && ask.offers_always
                && ask.request_id == "r1",
            "it shows the command even with previews off, and offers Always when Claude Code does"
        );
        assert_eq!(ask.detail, "rm -rf build");
        assert_eq!(ask.since, NOW);
        assert!(
            permission
                .suggestions
                .as_ref()
                .is_some_and(|s| s[0]["type"] == "addRules")
        );

        let pre = format!(
            r#"{{{BASE},"hook_event_name":"PreToolUse","tool_name":"Edit","tool_use_id":"t1","tool_input":{{"file_path":"/a/b.swift"}}}}"#
        );
        let edit_key = tool_summary::input_key(
            "Edit",
            json!({"file_path": "/a/b.swift"}).as_object().unwrap(),
        );
        let mut expected = ToolUse::new("t1", "Edit", None, NOW);
        expected.input_key = Some(edit_key);
        assert_eq!(
            decode(&pre, false).unwrap().fact.event,
            HookEvent::ToolStarted(expected.clone()),
            "a tool start keeps its name but not its file while previews are off"
        );
        expected.summary = Some("b.swift".into());
        assert_eq!(
            decode(&pre, true).unwrap().fact.event,
            HookEvent::ToolStarted(expected),
            "and its file with previews on"
        );
        assert_eq!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"PostToolUseFailure","tool_use_id":"t1"}}"#),
                false
            )
            .unwrap()
            .fact
            .event,
            HookEvent::ToolFinished {
                id: "t1".into(),
                failed: true
            },
            "a failed tool finishes as failed"
        );
        assert_eq!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"PostToolUse","tool_use_id":"t1"}}"#),
                false
            )
            .unwrap()
            .fact
            .event,
            HookEvent::ToolFinished {
                id: "t1".into(),
                failed: false
            }
        );
        assert!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"PreToolUse","tool_name":"Bash"}}"#),
                false
            )
            .is_none()
        );
        assert!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"PostToolUse"}}"#),
                false
            )
            .is_none()
        );
        assert_eq!(
            decode(
                &format!(
                    r#"{{{BASE},"hook_event_name":"Stop","last_assistant_message":"SECRET"}}"#
                ),
                false
            )
            .unwrap()
            .fact
            .event,
            HookEvent::Stopped { last_text: None },
            "a stop drops the reply while previews are off"
        );
        assert_eq!(
            decode(
                &format!(
                    r#"{{{BASE},"hook_event_name":"Stop","last_assistant_message":"  Done.  "}}"#
                ),
                true
            )
            .unwrap()
            .fact
            .event,
            HookEvent::Stopped {
                last_text: Some("Done.".into())
            },
            "and keeps it trimmed with previews on"
        );
        let start = decode(&format!(r#"{{{BASE},"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"Explore"}}"#), false)
            .unwrap();
        assert!(
            start.fact.event
                == HookEvent::SubagentStarted {
                    id: "a1".into(),
                    agent_type: "Explore".into()
                }
                && start.fact.agent_id.is_none(),
            "a subagent starting is about that subagent, not from it"
        );
        let untyped = decode(
            &format!(r#"{{{BASE},"hook_event_name":"SubagentStart","agent_id":"a1"}}"#),
            false,
        )
        .unwrap();
        assert_eq!(
            untyped.fact.event,
            HookEvent::SubagentStarted {
                id: "a1".into(),
                agent_type: "Agent".into()
            }
        );
        assert!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"SubagentStop","agent_id":""}}"#),
                false
            )
            .is_none()
        );
        let stop = decode(
            &format!(r#"{{{BASE},"hook_event_name":"SubagentStop","agent_id":"a1"}}"#),
            false,
        )
        .unwrap();
        assert!(
            stop.fact.event == HookEvent::SubagentStopped { id: "a1".into() }
                && stop.fact.agent_id.is_none()
        );
        let inside =
            decode(&format!(r#"{{{BASE},"hook_event_name":"PreToolUse","tool_name":"Grep","tool_use_id":"t2","agent_id":"a1"}}"#), false)
                .unwrap();
        assert_eq!(
            inside.fact.agent_id.as_deref(),
            Some("a1"),
            "a tool a subagent runs is marked as its own"
        );
        assert_eq!(
            decode(&format!(r#"{{{BASE},"hook_event_name":"Notification","notification_type":"idle_prompt"}}"#), false)
                .unwrap()
                .fact
                .event,
            HookEvent::Notification { kind: "idle_prompt".into() },
            "notifications keep their type"
        );
        assert_eq!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"SessionStart"}}"#),
                false
            )
            .unwrap()
            .fact
            .event,
            HookEvent::SessionStarted
        );
        assert_eq!(
            decode(
                &format!(r#"{{{BASE},"hook_event_name":"Elicitation"}}"#),
                false
            )
            .unwrap()
            .fact
            .event,
            HookEvent::Other("Elicitation".into())
        );
        assert!(
            decode(r#"{"hook_event_name":"Stop"}"#, false).is_none()
                && decode("not json", false).is_none(),
            "input without a session or not JSON is ignored"
        );
        assert!(decode(r#"{"hook_event_name":"Stop","session_id":""}"#, false).is_none());
        let unnamed = decode(
            &format!(r#"{{{BASE},"hook_event_name":"PermissionRequest"}}"#),
            false,
        )
        .unwrap();
        let HookEvent::PermissionRequested(unnamed) = unnamed.fact.event else {
            panic!()
        };
        assert!(unnamed.tool == "Tool" && !unnamed.offers_always && unnamed.summary.is_none());

        let allow = output(&reply(&Decision::Allow, None, None));
        assert!(
            allow["hookEventName"] == "PermissionRequest"
                && allow["decision"]["behavior"] == "allow"
        );
        let deny = output(&reply(&Decision::Deny, None, None));
        assert!(
            deny["decision"]["behavior"] == "deny" && deny["decision"]["message"] == DENY_MESSAGE
        );
        let offered = json!([{"type": "addRules"}]);
        let always = output(&reply(&Decision::AllowAlways, Some(&offered), None));
        assert_eq!(
            always["decision"]["updatedPermissions"][0]["type"], "addRules",
            "always hands back the rules offered"
        );
        assert!(
            output(&reply(&Decision::AllowAlways, None, None))["decision"]
                .get("updatedPermissions")
                .is_none()
        );
        assert_eq!(
            String::from_utf8(reply(&Decision::Allow, None, None)).unwrap(),
            r#"{"hookSpecificOutput":{"decision":{"behavior":"allow"},"hookEventName":"PermissionRequest"}}"#,
            "keys go out sorted"
        );

        assert!(
            is_complete(br#"{"a":{"b":1}}  "#),
            "a whole object is complete"
        );
        assert!(is_complete(b"{\"a\":1}\r\n\t"));
        assert!(
            !is_complete(br#"{"a":{"b":1}"#) && !is_complete(br#"{"a":"}"#),
            "a cut one is not"
        );
        assert!(!is_complete(b"") && !is_complete(b"[1]"));

        assert!(is_permission_request(
            br#"{"hook_event_name":"PermissionRequest"}"#
        ));
        assert!(
            is_permission_request(br#"{"hook_event_name": "PermissionRequest"}"#),
            "spacing falls back to parsing"
        );
        assert!(
            !is_permission_request(br#"{"hook_event_name":"Stop"}"#)
                && !is_permission_request(b"x")
        );

        assert_eq!(decision_for_key(Some("y"), true), Some(Decision::Allow));
        assert_eq!(decision_for_key(Some("N"), true), Some(Decision::Deny));
        assert_eq!(decision_for_key(Some("y"), false), None);
        assert_eq!(decision_for_key(Some("k"), true), None);
        assert_eq!(decision_for_key(None, true), None);
    }

    // ClaudeHookTests.swift questions.
    #[test]
    fn questions() {
        let message = question_message();
        let HookEvent::PermissionRequested(ask) = &message.fact.event else {
            panic!("a question decodes as a request")
        };
        assert!(
            ask.asks_questions()
                && ask.questions.len() == 2
                && !ask.fits_on_a_line()
                && !ask.offers_always,
            "a question is a request answered on its card, never allowed for good"
        );
        assert!(
            ask.summary.as_deref() == Some("Which color?")
                && ask.questions[0].header.as_deref() == Some("Color")
                && ask.questions[1].header.is_none()
                && ask.questions[0].options
                    == vec![
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
                && ask.questions[1].multi_select,
            "its questions, options, descriptions and previews are read even with previews off"
        );
        assert!(
            message
                .question_input
                .as_ref()
                .is_some_and(|i| i.contains_key("questions")),
            "the question's input is kept to hand back"
        );
        assert!(
            message.suggestions.is_some(),
            "the offered rules still travel, though Always is not offered"
        );

        let bash = decode(&format!(r#"{{{BASE},"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{{"command":"ls"}}}}"#), false)
            .unwrap();
        assert!(
            bash.question_input.is_none(),
            "a tool's request keeps no input"
        );
        let empty = decode(
            &format!(r#"{{{BASE},"hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{{"questions":[{{"header":"x"}}]}}}}"#),
            false,
        )
        .unwrap();
        assert!(
            empty.fact.event == HookEvent::Other("PermissionRequest".into())
                && !empty.is_permission_request()
                && empty.question_input.is_none(),
            "a question that can't be read is left to the terminal, never allowed unanswered"
        );

        let original = message.question_input.clone().unwrap_or_default();
        let picked: BTreeMap<String, Vec<String>> = [
            ("Which color?".to_owned(), vec!["Blue".to_owned()]),
            (
                "Which sizes?".to_owned(),
                vec!["S".to_owned(), "L".to_owned()],
            ),
        ]
        .into();
        let answered = output(&reply(&Decision::Answer(picked), None, Some(&original)));
        let decision = &answered["decision"];
        let updated = &decision["updatedInput"];
        assert!(
            decision["behavior"] == "allow"
                && updated["answers"]["Which color?"] == "Blue"
                && updated["answers"]["Which sizes?"] == json!(["S", "L"]),
            "answers go back by question, one as text and several as a list"
        );
        assert_eq!(
            canonical_json(&updated["questions"]),
            canonical_json(&original["questions"]),
            "the questions go back exactly as Claude asked them"
        );
        let skipped = output(&reply(&Decision::Deny, None, Some(&original)));
        assert!(
            skipped["decision"]["behavior"] == "deny"
                && skipped["decision"]["message"] == SKIP_MESSAGE,
            "skipping tells Claude the question went unanswered"
        );
        let lone: BTreeMap<String, Vec<String>> = [("x".to_owned(), vec!["y".to_owned()])].into();
        assert!(
            reply(&Decision::Answer(lone), None, None).is_empty(),
            "answers with no question to hand back leave the terminal to ask"
        );

        let shown = &ask.questions;
        let set = |labels: &[&str]| {
            labels
                .iter()
                .map(|l| l.to_string())
                .collect::<HashSet<String>>()
        };
        let map = |pairs: &[(&str, &[&str])]| -> BTreeMap<String, Vec<String>> {
            pairs
                .iter()
                .map(|(q, a)| (q.to_string(), a.iter().map(|s| s.to_string()).collect()))
                .collect()
        };
        assert_eq!(
            answers(shown, &[(0, set(&["Blue"]))].into(), &HashMap::new()),
            None,
            "nothing goes until every question has an answer"
        );
        assert_eq!(
            answers(
                shown,
                &[(0, set(&["Blue"])), (1, set(&["L", "S"]))].into(),
                &HashMap::new()
            ),
            Some(map(&[
                ("Which color?", &["Blue"]),
                ("Which sizes?", &["S", "L"])
            ])),
            "choices keep the order Claude wrote them in"
        );
        assert_eq!(
            answers(
                shown,
                &[(0, set(&["Red"])), (1, set(&["M"]))].into(),
                &[(0, "  Green ".to_owned()), (1, "XL".to_owned())].into()
            ),
            Some(map(&[
                ("Which color?", &["Green"]),
                ("Which sizes?", &["M", "XL"])
            ])),
            "an answer written by hand replaces a single choice and joins several"
        );
        assert_eq!(
            answers(
                shown,
                &[(0, set(&["Red", "Blue"])), (1, set(&["M"]))].into(),
                &[(1, "   ".to_owned())].into()
            ),
            Some(map(&[("Which color?", &["Red"]), ("Which sizes?", &["M"])])),
            "a single choice sends one answer, and blank words none"
        );
        assert_eq!(
            answers(&[], &HashMap::new(), &HashMap::new()),
            None,
            "no questions, no answers"
        );
        let long = "😀".repeat(6_000);
        let clipped = answers(
            shown,
            &[(1, set(&["S"]))].into(),
            &[(0, long.clone()), (1, long)].into(),
        )
        .unwrap();
        let units: usize = clipped
            .values()
            .flatten()
            .map(|a| a.encode_utf16().count())
            .sum();
        assert!(
            units > 0 && units <= 32_768 && clipped["Which color?"][0].chars().all(|c| c == '😀'),
            "answers written by hand stay within what Claude Code takes in all, cut at whole characters"
        );
        assert_eq!(clipped["Which color?"][0].encode_utf16().count(), 7_500);
    }

    #[test]
    fn utf16_clip() {
        assert_eq!(clipped_utf16("abc", 3), "abc");
        assert_eq!(clipped_utf16("abcd", 3), "abc");
        assert_eq!(
            clipped_utf16("a😀", 2),
            "a",
            "a surrogate pair is never split"
        );
        assert_eq!(
            clipped_utf16("👍🏽👍🏽", 5),
            "👍🏽",
            "nor a character of several scalars"
        );
    }

    #[test]
    fn zero_width_space_trims() {
        let stop = decode(&format!("{{{BASE},\"hook_event_name\":\"Stop\",\"last_assistant_message\":\"Done.\u{200B}\"}}"), true).unwrap();
        assert_eq!(
            stop.fact.event,
            HookEvent::Stopped {
                last_text: Some("Done.".into())
            },
            "as Foundation trims it"
        );
        let blank = decode(
            &format!("{{{BASE},\"hook_event_name\":\"PermissionRequest\",\"tool_name\":\"AskUserQuestion\",\"tool_input\":{{\"questions\":[{{\"question\":\"\u{200B}\"}}]}}}}"),
            false,
        )
        .unwrap();
        assert_eq!(
            blank.fact.event,
            HookEvent::Other("PermissionRequest".into()),
            "a blank-looking question is left to the terminal"
        );
    }
}
