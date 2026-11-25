// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Kimi Code in the Sessions list (#240), as KimiAgentTests.swift: what its
//! hooks send (payloads copied from Kimi Code CLI 2.8.0 runs), the sessions
//! kept from them, the keys and server bodies that answer it, the island's
//! tables in its config.toml, and its session log.

use super::*;
use crate::sessions::hook_agents::{Place, Tracker, decode, installer, waits};
use crate::sessions::models::{ContentPolicy, HostKind, Session, SessionPhase};
use std::collections::BTreeMap;

const NOW: Time = 1_800_000_000.0;
const ON: ContentPolicy = ContentPolicy { previews: true };
const OFF: ContentPolicy = ContentPolicy { previews: false };
const SESSION: &str = "session_f6fd8114-c8d8-4ae0-9158-5b3bd8c4a9e7";

fn envelope(hook: &str, extra: Value, client: &str) -> Vec<u8> {
    let mut event = json!({
        "hook_event_name": hook, "session_id": SESSION, "cwd": "/w/rocket",
        "client_type": client, "session_title": "tool echo hi",
    });
    for (k, v) in extra.as_object().unwrap() {
        event[k] = v.clone();
    }
    serde_json::to_vec(&json!({"openbar": "kimi", "hook": hook, "pid": 59299, "event": event}))
        .unwrap()
}

fn feed_as(tracker: &mut Tracker, hook: &str, extra: Value, client: &str, policy: ContentPolicy) {
    for event in decode(&envelope(hook, extra, client), NOW, policy)
        .map(|d| d.events)
        .unwrap_or_default()
    {
        tracker.ingest(event, NOW);
    }
}

fn feed(tracker: &mut Tracker, hook: &str, extra: Value) {
    feed_as(tracker, hook, extra, "kimi_code_cli", ON);
}

fn snap_at(tracker: &mut Tracker, takes_keys: bool, previews: bool) -> Vec<Session> {
    tracker
        .snapshot(
            NOW,
            ContentPolicy { previews },
            |_| Place {
                host: HostKind::WindowsTerminal,
                takes_keys,
                accepts_replies: true,
                host_name: None,
            },
            |_| Some("main".into()),
        )
        .0
}

fn snap(tracker: &mut Tracker) -> Vec<Session> {
    snap_at(tracker, true, true)
}

fn danger() -> Value {
    json!({
        "id": "approval_ad3eed77", "agent_id": "main", "turn_id": 3,
        "tool_call_id": "call_df5a55a4", "tool_name": "Bash", "action": "Running: rm -rf ./scratch-dir",
        "display": {"kind": "command", "command": "rm -rf ./scratch-dir", "cwd": "/w/rocket", "language": "bash"},
        "tool_input": {"command": "rm -rf ./scratch-dir", "description": "Remove scratch"},
    })
}

fn planned() -> Value {
    json!({
        "id": "approval_5e585559", "tool_call_id": "call_e56075f5", "tool_name": "ExitPlanMode",
        "action": "Presenting plan and exiting plan mode",
        "display": {"kind": "plan_review", "plan": "## Plan\n\n1. Add a button.\n2. Test it.\n"},
        "tool_input": {},
    })
}

fn asking_two() -> Value {
    json!({
        "tool_name": "AskUserQuestion", "tool_call_id": "call_f587936f",
        "tool_input": {"questions": [
            {"question": "Which database?", "header": "DB", "multi_select": false,
             "options": [{"label": "Postgres", "description": "SQL"}, {"label": "SQLite", "description": "File"}]},
            {"question": "Which extras do you want?", "header": "Extras", "multi_select": true,
             "options": [{"label": "Tests"}, {"label": "Docs"}, {"label": "CI"}]},
        ]},
    })
}

#[test]
fn sessions_turns_and_ends() {
    let mut tracker = Tracker::default();
    feed(
        &mut tracker,
        "SessionStart",
        json!({"source": "startup", "model": "kimi-code/k3"}),
    );
    let sessions = snap(&mut tracker);
    assert_eq!(sessions[0].id, format!("kimi:{SESSION}"));
    assert_eq!(sessions[0].provider, Provider::Kimi);
    assert_eq!(sessions[0].model.as_deref(), Some("k3"));
    assert_eq!(sessions[0].phase, SessionPhase::Idle);
    feed(
        &mut tracker,
        "UserPromptSubmit",
        json!({"prompt": [{"type": "text", "text": "tool echo hi"}], "is_steer": false}),
    );
    let sessions = snap(&mut tracker);
    assert_eq!(sessions[0].phase, SessionPhase::Generating);
    assert_eq!(sessions[0].chat.len(), 1);
    let echo = json!({"tool_name": "Bash", "tool_call_id": "call_e", "tool_input": {"command": "echo hi"}});
    feed(&mut tracker, "PreToolUse", echo.clone());
    assert_eq!(snap(&mut tracker)[0].phase, SessionPhase::RunningTool);
    feed(&mut tracker, "PostToolUse", echo);
    let write = json!({"tool_name": "Write", "tool_call_id": "call_w", "tool_input": {"path": "a.txt", "content": "x"}});
    feed(&mut tracker, "PreToolUse", write.clone());
    feed(&mut tracker, "PostToolUseFailure", write);
    let sessions = snap(&mut tracker);
    assert_eq!(sessions[0].timeline.len(), 2);
    assert!(!sessions[0].timeline[0].failed && sessions[0].timeline[1].failed);
    feed(
        &mut tracker,
        "Stop",
        json!({"turn_id": 0, "finish_reason": "completed"}),
    );
    assert_eq!(snap(&mut tracker)[0].phase, SessionPhase::Finished);
    feed(
        &mut tracker,
        "UserPromptSubmit",
        json!({"prompt": [{"type": "text", "text": "slow"}]}),
    );
    feed(&mut tracker, "Interrupt", json!({"reason": "cancelled"}));
    assert_eq!(snap(&mut tracker)[0].phase, SessionPhase::Idle);
    feed(&mut tracker, "TurnStarted", json!({"origin_kind": "task"}));
    assert_eq!(snap(&mut tracker)[0].phase, SessionPhase::Generating);
    feed(&mut tracker, "SessionEnd", json!({"reason": "exit"}));
    assert!(snap(&mut tracker).is_empty());

    // No prompt or title without previews.
    let mut hidden = Tracker::default();
    feed_as(
        &mut hidden,
        "UserPromptSubmit",
        json!({"prompt": [{"type": "text", "text": "secret"}]}),
        "kimi_code_cli",
        OFF,
    );
    let sessions = snap_at(&mut hidden, true, false);
    assert!(sessions[0].chat.is_empty() && sessions[0].title.is_none());

    // A session gone with its process; one a server holds stays until quiet.
    let mut gone = Tracker::default();
    feed(&mut gone, "UserPromptSubmit", json!({"prompt": []}));
    gone.prune(NOW, |_| false);
    assert!(snap(&mut gone).is_empty());
    let mut served = Tracker::default();
    feed_as(
        &mut served,
        "SessionStart",
        json!({}),
        "kimi_code_desktop",
        ON,
    );
    served.prune(NOW + 60.0, |_| true);
    assert_eq!(snap(&mut served).len(), 1);
    served.prune(NOW + 31.0 * 60.0, |_| true);
    assert!(snap(&mut served).is_empty());
    assert!(!waits(&envelope(
        "PermissionRequest",
        danger(),
        "kimi_code_cli"
    )));
}

#[test]
fn requests_and_where_they_are_answered() {
    let mut tracker = Tracker::default();
    feed(&mut tracker, "PermissionRequest", danger());
    let sessions = snap(&mut tracker);
    let asked = sessions[0].permission.as_ref().expect("a card");
    assert_eq!(sessions[0].phase, SessionPhase::WaitingPermission);
    assert_eq!(asked.tool, "Bash");
    assert_eq!(asked.detail, "rm -rf ./scratch-dir");
    assert_eq!(asked.request_id, "approval_ad3eed77");
    assert!(asked.offers_always);
    // Where keys can't go, it is answered in Kimi.
    let sessions = snap_at(&mut tracker, false, true);
    assert!(sessions[0].permission.is_none() && sessions[0].waiting_for.is_some());
    // Another request's result settles nothing; its own does.
    let mut other = danger();
    other["id"] = json!("approval_older");
    feed(&mut tracker, "PermissionResult", other);
    assert!(snap(&mut tracker)[0].permission.is_some());
    let mut result = danger();
    result["decision"] = json!("rejected");
    feed(&mut tracker, "PermissionResult", result);
    assert!(snap(&mut tracker)[0].permission.is_none());

    // A write shows its file, named by Kimi's `path`.
    let mut write = Tracker::default();
    feed(
        &mut write,
        "PermissionRequest",
        json!({"id": "approval_w", "tool_name": "Write", "tool_call_id": "call_w",
               "tool_input": {"path": "/w/rocket/note.txt", "content": "hello\n"}}),
    );
    let asked = snap(&mut write)[0].permission.clone().unwrap();
    assert_eq!(asked.tool, "Write");
    assert!(asked.detail.contains("note.txt"));

    // VS Code's extension asks in its panel; the app's server answers anywhere.
    let mut vscode = Tracker::default();
    feed_as(
        &mut vscode,
        "PermissionRequest",
        danger(),
        "kimi_code_vscode",
        ON,
    );
    let sessions = snap(&mut vscode);
    assert!(sessions[0].permission.is_none() && !sessions[0].served);
    let mut app = Tracker::default();
    feed_as(
        &mut app,
        "PermissionRequest",
        danger(),
        "kimi_code_desktop",
        ON,
    );
    let sessions = snap_at(&mut app, false, true);
    assert!(sessions[0].permission.is_some() && sessions[0].served);
}

#[test]
fn questions_and_plans() {
    let mut tracker = Tracker::default();
    feed(&mut tracker, "PreToolUse", asking_two());
    let sessions = snap(&mut tracker);
    let asked = sessions[0].permission.as_ref().expect("a question");
    assert_eq!(asked.request_id, "call_f587936f");
    assert_eq!(asked.questions.len(), 2);
    assert!(asked.questions[1].multi_select);
    assert!(sessions[0].timeline.is_empty());
    feed(&mut tracker, "PostToolUse", asking_two());
    assert!(snap(&mut tracker)[0].permission.is_none());

    let mut plan = Tracker::default();
    feed(
        &mut plan,
        "PreToolUse",
        json!({"tool_name": "ExitPlanMode", "tool_call_id": "call_e56075f5", "tool_input": {}}),
    );
    assert!(snap(&mut plan)[0].timeline.is_empty());
    feed(&mut plan, "PermissionRequest", planned());
    let asked = snap(&mut plan)[0].permission.clone().unwrap();
    assert_eq!(asked.tool, tool_summary::PLAN_TOOL);
    assert!(asked.plan_modes.is_empty() && !asked.offers_always);
    assert!(asked.detail.contains("Add a button"));
}

#[test]
fn keys_answer_the_terminal() {
    let tool = Permission::new("a", "Bash", None, true, NOW);
    assert_eq!(keys(&Decision::Allow, &tool), Some(vec![Key::Digit(1)]));
    assert_eq!(
        keys(&Decision::AllowAlways, &tool),
        Some(vec![Key::Digit(2)])
    );
    assert_eq!(keys(&Decision::Deny, &tool), Some(vec![Key::Digit(3)]));
    let plan = ask(
        "ExitPlanMode",
        PLAN_TOOL,
        "p",
        &Map::new(),
        planned()["display"].as_object(),
        None,
        NOW,
    );
    assert_eq!(keys(&Decision::Allow, &plan), Some(vec![Key::Digit(1)]));
    assert_eq!(keys(&Decision::Deny, &plan), Some(vec![Key::Digit(2)]));
    let two = question("c", asking_two()["tool_input"].as_object().unwrap(), NOW).unwrap();
    let mut answers = BTreeMap::new();
    answers.insert("Which database?".to_owned(), vec!["SQLite".to_owned()]);
    answers.insert(
        "Which extras do you want?".to_owned(),
        vec!["CI".to_owned(), "Tests".to_owned()],
    );
    assert_eq!(
        keys(&Decision::Answer(answers.clone()), &two),
        Some(vec![
            Key::Digit(2),
            Key::Digit(1),
            Key::Digit(3),
            Key::Tab,
            Key::Digit(1)
        ])
    );
    answers.insert("Which database?".to_owned(), vec!["MySQL".to_owned()]);
    assert_eq!(keys(&Decision::Answer(answers), &two), None);
    assert_eq!(keys(&Decision::Deny, &two), Some(vec![Key::Escape]));
}

#[test]
fn server_bodies_and_places() {
    let found =
        instance(br#"{"pid":60964,"host":"127.0.0.1","port":58627,"product":"kimi-code-cli"}"#)
            .unwrap();
    assert_eq!(found.address(), "127.0.0.1:58627");
    assert_eq!(
        page(&found, SESSION, Some("t-1")),
        format!("http://127.0.0.1:58627/sessions/{SESSION}#token=t-1")
    );
    assert!(instance(br#"{"pid":0,"port":1}"#).is_none());
    assert_eq!(
        Instance {
            pid: 1,
            host: "::".into(),
            port: 9,
            product: String::new()
        }
        .address(),
        "[::1]:9"
    );
    assert!(
        !Instance {
            pid: 1,
            host: "10.0.0.2".into(),
            port: 9,
            product: String::new()
        }
        .is_local()
    );

    assert_eq!(
        approval(&Decision::AllowAlways, false),
        Some(json!({"kind": "approval", "decision": "approved", "scope": "session"}))
    );
    assert_eq!(
        approval(&Decision::Deny, true).unwrap()["feedback"],
        json!(PLAN_DENY_MESSAGE)
    );
    let interaction: Value = serde_json::from_str(r#"{"interaction_id":"question_77b9","tool_call_id":"call_f587936f","kind":"question","request":{"questions":[
        {"id":"q_0","question":"Which database?","options":[{"id":"opt_0_0","label":"Postgres"},{"id":"opt_0_1","label":"SQLite"}],"multi_select":false,"allow_other":true},
        {"id":"q_1","question":"Which extras do you want?","options":[{"id":"opt_1_0","label":"Tests"},{"id":"opt_1_1","label":"Docs"},{"id":"opt_1_2","label":"CI"}],"multi_select":true,"allow_other":true}]}}"#).unwrap();
    let mut picked = BTreeMap::new();
    picked.insert("Which database?".to_owned(), vec!["SQLite".to_owned()]);
    picked.insert(
        "Which extras do you want?".to_owned(),
        vec!["Tests".to_owned(), "CI".to_owned()],
    );
    assert_eq!(
        answers(&Decision::Answer(picked.clone()), &interaction),
        Some(json!({"kind": "question", "answers": {
            "q_0": {"kind": "single", "option_id": "opt_0_1"},
            "q_1": {"kind": "multi", "option_ids": ["opt_1_0", "opt_1_2"]}}}))
    );
    picked.insert("Which database?".to_owned(), vec!["MySQL".to_owned()]);
    assert_eq!(
        answers(&Decision::Answer(picked), &interaction).unwrap()["answers"]["q_0"],
        json!({"kind": "other", "text": "MySQL"})
    );
    assert_eq!(
        answers(&Decision::Deny, &interaction),
        Some(json!({"kind": "question", "dismissed": true}))
    );

    let folder = std::env::temp_dir().join(format!(
        "vs-kimi-{:x}",
        super::super::hook_agents::rand_id()
    ));
    let instances_dir = folder.join("server").join("instances");
    std::fs::create_dir_all(&instances_dir).unwrap();
    std::fs::write(
        instances_dir.join("01A.json"),
        r#"{"pid":60964,"host":"127.0.0.1","port":58627,"product":"kimi-code-cli"}"#,
    )
    .unwrap();
    std::fs::write(folder.join("server.token"), "  tok-1\n").unwrap();
    assert_eq!(
        serving(60964, &folder, &|_| true).map(|i| i.port),
        Some(58627)
    );
    assert!(serving(59299, &folder, &|_| true).is_none());
    assert!(serving(60964, &folder, &|_| false).is_none());
    assert_eq!(token(&folder).as_deref(), Some("tok-1"));
    std::fs::remove_dir_all(&folder).ok();

    assert_eq!(
        response_body(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{\"code\":0}").as_deref(),
        Some(&b"{\"code\":0"[..])
    );
    assert_eq!(
        response_body(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"co\r\n6\r\nde\":0}\r\n0\r\n\r\n")
            .as_deref(),
        Some(&b"{\"code\":0}"[..])
    );
}

#[test]
fn config_tables_go_in_and_out() {
    let exe = Path::new("/opt/openbar/openbar");
    let own = "default_model = \"kimi-code/k3\"\n# my notes stay\n\n[[hooks]]\nevent = \"Notification\"\ncommand = \"say done\"\n";
    let installed = installing(Some(own), exe, "app");
    assert!(installed.starts_with(&format!("{own}\n{}", installer::vibe_start("app"))));
    let added = &installed[own.len()..];
    assert_eq!(added.matches("[[hooks]]").count(), EVENTS.len());
    let keys: std::collections::BTreeSet<&str> = added
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(" = ").map(|(k, _)| k))
        .collect();
    assert_eq!(keys, ["command", "event", "timeout"].into_iter().collect());
    assert!(added.contains("timeout = 5\n") && !added.contains("timeout = 5.0"));
    assert!(added.contains("--agent kimi --event PermissionRequest"));
    assert_eq!(
        state(Some(&installed), exe, "app"),
        installer::State::Installed
    );
    assert_eq!(
        state(Some(&installed), Path::new("/elsewhere/openbar"), "app"),
        installer::State::Outdated
    );
    assert_eq!(installing(Some(&installed), exe, "app"), installed);
    assert_eq!(
        installer::vibe_removing(&installed, "app").as_deref(),
        Some(own)
    );
    assert_eq!(state(None, exe, "app"), installer::State::Missing);
    assert_eq!(
        state(Some("hooks = [{ event = \"Stop\" }]\n"), exe, "app"),
        installer::State::Unreadable
    );
    let home = Path::new("/home/a");
    assert_eq!(folder_with(home, &|_| None), home.join(".kimi-code"));
    assert_eq!(
        folder_with(home, &|_| Some("/k".into())),
        PathBuf::from("/k")
    );
    assert_eq!(folder_with(home, &|_| Some("~/k".into())), home.join("k"));
}

#[test]
fn the_log_names_the_model_and_answer() {
    let wire = [
        r#"{"type":"turn.prompt","input":[{"type":"text","text":"tool echo hi"}],"turnId":1}"#,
        r#"{"type":"context.append_loop_event","event":{"type":"content.part","turnId":"1","stepUuid":"s1","part":{"type":"text","text":"Let me run it."}}}"#,
        r#"{"type":"context.append_loop_event","event":{"type":"content.part","turnId":"1","stepUuid":"s2","part":{"type":"text","text":"Done: "}}}"#,
        r#"{"type":"context.append_loop_event","event":{"type":"content.part","turnId":"1","stepUuid":"s2","part":{"type":"text","text":"hi"}}}"#,
        r#"{"type":"turn.ended","turnId":1,"reason":"completed","replyModel":{"provider":"openai","model":"kimi-for-coding"}}"#,
        r#"{"type":"prompt.completed"}"#,
    ]
    .join("\n");
    assert_eq!(
        last_turn_in(&wire, true),
        (Some("kimi-for-coding".into()), Some("Done: hi".into()))
    );
    assert_eq!(
        last_turn_in(&wire, false),
        (Some("kimi-for-coding".into()), None)
    );
    let folder = std::env::temp_dir().join(format!(
        "vs-kimi-log-{:x}",
        super::super::hook_agents::rand_id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("session_index.jsonl"),
        format!("{{\"sessionId\":\"other\",\"sessionDir\":\"/x\"}}\n{{\"sessionId\":\"{SESSION}\",\"sessionDir\":\"/s/{SESSION}\"}}\n"),
    )
    .unwrap();
    assert_eq!(
        session_folder_in(SESSION, &folder),
        Some(PathBuf::from(format!("/s/{SESSION}")))
    );
    assert!(session_folder_in("none", &folder).is_none());
    std::fs::remove_dir_all(&folder).ok();
    assert_eq!(model_name("kimi-code/kimi-for-coding"), "kimi-for-coding");
}

use crate::sessions::models::Provider;
