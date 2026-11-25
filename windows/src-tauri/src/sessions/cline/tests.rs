// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Cline in the Sessions list (#241), as ClineAgentTests.swift: its store
//! and conversations (shapes copied from Cline CLI 3.0.75 and the VS Code
//! extension 4.1.27), the sessions kept from them, the keys and hub frames
//! that answer it, and that nothing is installed into it.

use super::*;
use crate::sessions::hook_agents::{Place, Tracker};
use crate::sessions::models::{HostKind, Session, SessionPhase};
use std::collections::BTreeMap;

const NOW: Time = 1_800_000_000.0;
const ON: ContentPolicy = ContentPolicy { previews: true };
const OFF: ContentPolicy = ContentPolicy { previews: false };
const SESSION: &str = "1795622999301_iedbg";

fn message(role: &str, content: Value, model: Option<&str>) -> Value {
    let mut json = json!({"role": role, "content": content, "ts": 1_795_622_999_400_i64});
    if let Some(model) = model {
        json["modelInfo"] = json!({"id": model, "provider": "openai-compatible"});
    }
    json
}

fn typed_text(text: &str) -> Value {
    json!({"type": "text", "text": format!("<user_input mode=\"act\">{text}</user_input>")})
}

fn tool_call(id: &str, name: &str, input: Value) -> Value {
    json!({"type": "tool_use", "id": id, "name": name, "input": input})
}

fn result(id: &str, name: &str, content: Value) -> Value {
    json!({"type": "tool_result", "tool_use_id": id, "name": name, "content": content})
}

fn approval() -> Vec<Value> {
    vec![
        message("user", json!([typed_text("tool echo two")]), None),
        message(
            "assistant",
            json!([tool_call(
                "call_da0cd2e3",
                "run_commands",
                json!({"commands": ["echo two"]})
            )]),
            Some("mock-coder"),
        ),
    ]
}

fn approved() -> Vec<Value> {
    let mut list = approval();
    list.push(message(
        "user",
        json!([result(
            "call_da0cd2e3",
            "run_commands",
            json!([{"query": "echo two", "result": "two\n", "success": true}])
        )]),
        None,
    ));
    list.push(message(
        "assistant",
        json!([{"type": "text", "text": "Done: two"}]),
        Some("mock-coder"),
    ));
    list
}

fn question() -> Vec<Value> {
    let mut list = approved();
    list.push(message("user", json!([typed_text("ask")]), None));
    list.push(message(
        "assistant",
        json!([tool_call(
            "call_79e96d2d",
            "ask_question",
            json!({"question": "Which color should the button be?", "options": ["Red", "Blue", "Green"]})
        )]),
        Some("mock-coder"),
    ));
    list
}

fn talk(messages: &[Value]) -> Transcript {
    let file = json!({"version": 1, "sessionId": SESSION, "messages": messages, "system_prompt": "You are Cline"});
    transcript_of(file.to_string().as_bytes()).unwrap()
}

fn record(status: &str) -> Record {
    Record {
        id: SESSION.into(),
        source: "cli".into(),
        pid: 97843,
        status: status.into(),
        cwd: "/w/proj".into(),
        model: Some("mock-coder".into()),
        title: Some("tool echo two".into()),
        messages: Some("/m/cli.json".into()),
    }
}

fn scan_with(
    watcher: &mut Watcher,
    records: &[Record],
    messages: Vec<Value>,
    policy: ContentPolicy,
    client: &'static str,
    alive: bool,
) -> Vec<AgentEvent> {
    let transcript = talk(&messages);
    watcher.scan(
        records,
        NOW,
        policy,
        &|r| Some(r.pid),
        &|_| client,
        &|_| alive,
        &|_| Some(transcript.clone()),
    )
}

fn scan(watcher: &mut Watcher, records: &[Record], messages: Vec<Value>) -> Vec<AgentEvent> {
    scan_with(watcher, records, messages, ON, CLI_CLIENT, true)
}

fn kinds(events: &[AgentEvent]) -> Vec<&'static str> {
    events
        .iter()
        .map(|e| match &e.kind {
            Kind::Started => "started",
            Kind::Ended => "ended",
            Kind::Prompt(_) => "prompt",
            Kind::ToolStarted(_) => "tool",
            Kind::ToolFinished { .. } => "toolDone",
            Kind::Permission(p) if p.asks_questions() => "question",
            Kind::Permission(_) => "permission",
            Kind::PermissionSettled(_) => "settled",
            Kind::Stopped { .. } => "stopped",
            Kind::Model(_) => "model",
            Kind::Title(_) => "title",
            Kind::Client(_) => "client",
            _ => "other",
        })
        .collect()
}

fn call(name: &str, input: Value) -> Call {
    Call {
        id: "c".into(),
        name: name.into(),
        input: input.as_object().unwrap().clone(),
        at: None,
    }
}

#[test]
fn places_follow_clines_variables() {
    let home = Path::new("/Users/a");
    let none = |_: &str| None;
    assert_eq!(
        data_folder(home, &none),
        PathBuf::from("/Users/a/.cline/data")
    );
    let dir = |k: &str| (k == "CLINE_DIR").then(|| "/x/c".to_owned());
    assert_eq!(data_folder(home, &dir), PathBuf::from("/x/c/data"));
    let both = |k: &str| match k {
        "CLINE_DIR" => Some("/x/c".to_owned()),
        "CLINE_DATA_DIR" => Some("/d".to_owned()),
        _ => None,
    };
    assert_eq!(data_folder(home, &both), PathBuf::from("/d"));
    assert_eq!(
        database(home, &none),
        PathBuf::from("/Users/a/.cline/data/db/sessions.db")
    );
    let db = |k: &str| (k == "CLINE_DB_DATA_DIR").then(|| "/db".to_owned());
    assert_eq!(database(home, &db), PathBuf::from("/db/sessions.db"));
    assert!(records(Path::new("/nonexistent/sessions.db")).is_empty());
}

#[test]
fn conversation_reads_calls_results_and_words() {
    let waiting = talk(&approval());
    assert_eq!(waiting.open().map(|c| c.id.as_str()), Some("call_da0cd2e3"));
    assert_eq!(waiting.last_user.as_deref(), Some("tool echo two"));
    assert_eq!(waiting.model.as_deref(), Some("mock-coder"));
    let done = talk(&approved());
    assert!(done.open().is_none());
    assert_eq!(done.last_text.as_deref(), Some("Done: two"));
    assert_eq!(
        typed(Some(
            "<hook_context source=\"ToolSuccess\">x</hook_context>"
        )),
        None
    );
    assert_eq!(typed(Some("plain words")).as_deref(), Some("plain words"));
    assert!(transcript_of(b"[]").is_none());
}

#[test]
fn tools_read_as_claude_codes() {
    let (name, input) = claude_tool(&call(
        "run_commands",
        json!({"commands": ["npm test", "ls"]}),
    ));
    assert_eq!(name, "Bash");
    assert_eq!(input["command"], "npm test\nls");
    assert_eq!(
        commands(
            json!({"commands": [{"command": "git", "args": ["status"]}]})
                .as_object()
                .unwrap()
        ),
        vec!["git status"]
    );
    let (name, input) = claude_tool(&call(
        "editor",
        json!({"path": "/w/a", "old_text": "a", "new_text": "b"}),
    ));
    assert_eq!(
        (name.as_str(), input["old_string"].as_str()),
        ("Edit", Some("a"))
    );
    let (name, input) = claude_tool(&call("editor", json!({"path": "/w/n", "new_text": "hi"})));
    assert_eq!(
        (name.as_str(), input["content"].as_str()),
        ("Write", Some("hi"))
    );
    assert_eq!(
        patch_file("*** Begin Patch\n*** Update File: src/a.ts\n@@\n").as_deref(),
        Some("src/a.ts")
    );
    let asked = ask(
        &call(
            "run_commands",
            json!({"commands": ["rm -rf ./scratch-dir"]}),
        ),
        Some("/w"),
        NOW,
    )
    .unwrap();
    assert_eq!(asked.request_id, "c");
    assert_eq!(asked.tool, "Bash");
    assert!(asked.detail.contains("rm -rf ./scratch-dir"));
    let q = ask(
        &call(
            "ask_question",
            json!({"question": "Which color?", "options": ["Red", "Blue"]}),
        ),
        None,
        NOW,
    )
    .unwrap();
    assert_eq!(q.questions[0].options.len(), 2);
    assert!(ask(&call("ask_question", json!({"question": ""})), None, NOW).is_none());
}

#[test]
fn the_watcher_tells_what_hooks_would() {
    let mut watcher = Watcher::default();
    let events = scan(&mut watcher, &[record("pending")], approval());
    assert_eq!(
        kinds(&events),
        [
            "started",
            "client",
            "model",
            "title",
            "prompt",
            "tool",
            "permission"
        ]
    );
    assert!(scan(&mut watcher, &[record("pending")], approval()).is_empty());
    assert_eq!(
        kinds(&scan(&mut watcher, &[record("running")], approved())),
        ["toolDone", "settled"]
    );
    let stopped = scan(&mut watcher, &[record("idle")], approved());
    assert_eq!(kinds(&stopped), ["stopped"]);
    assert!(
        matches!(&stopped[0].kind, Kind::Stopped { last_text: Some(t), failed: false } if t == "Done: two")
    );
    assert_eq!(
        kinds(&scan(&mut watcher, &[record("running")], question())),
        ["prompt", "tool", "question"]
    );
    let mut answered = question();
    answered.push(message(
        "user",
        json!([result("call_79e96d2d", "ask_question", json!("Blue"))]),
        None,
    ));
    answered.push(message(
        "assistant",
        json!([{"type": "text", "text": "Blue it is."}]),
        Some("mock-coder"),
    ));
    assert_eq!(
        kinds(&scan(&mut watcher, &[record("idle")], answered.clone())),
        ["toolDone", "settled", "stopped"]
    );
    // A whole turn between two readings.
    answered.push(message(
        "user",
        json!([typed_text("hello from the island")]),
        None,
    ));
    answered.push(message(
        "assistant",
        json!([{"type": "text", "text": "Hi there."}]),
        Some("mock-coder"),
    ));
    assert_eq!(
        kinds(&scan(&mut watcher, &[record("idle")], answered.clone())),
        ["prompt", "stopped"]
    );
    assert_eq!(kinds(&scan(&mut watcher, &[], answered)), ["ended"]);

    // Previews off: no prompt text and no title.
    let mut quiet = Watcher::default();
    let events = scan_with(
        &mut quiet,
        &[record("running")],
        approval(),
        OFF,
        CLI_CLIENT,
        true,
    );
    assert!(!kinds(&events).contains(&"title"));
    assert!(events.iter().any(|e| matches!(e.kind, Kind::Prompt(None))));
    // A process that quit ends its session; a hub session no CLI shows isn't listed.
    let mut gone = Watcher::default();
    scan(&mut gone, &[record("idle")], approved());
    assert_eq!(
        kinds(&scan_with(
            &mut gone,
            &[record("idle")],
            approved(),
            ON,
            CLI_CLIENT,
            false
        )),
        ["ended"]
    );
    let mut orphan = Watcher::default();
    let t = talk(&approved());
    assert!(
        orphan
            .scan(
                &[record("idle")],
                NOW,
                ON,
                &|_| None,
                &|_| HUB_CLIENT,
                &|_| true,
                &|_| Some(t.clone())
            )
            .is_empty()
    );
    // History isn't replayed.
    let mut late = Watcher::default();
    let events = scan(&mut late, &[record("idle")], approved());
    assert!(!kinds(&events).contains(&"tool") && !kinds(&events).contains(&"stopped"));
}

fn snapshot(
    client: &'static str,
    host: HostKind,
    takes_keys: bool,
    host_name: Option<&str>,
    messages: Vec<Value>,
    status: &str,
) -> Session {
    let mut tracker = Tracker::default();
    let mut watcher = Watcher::default();
    for event in scan_with(&mut watcher, &[record(status)], messages, ON, client, true) {
        tracker.ingest(event, NOW);
    }
    let name = host_name.map(str::to_owned);
    let (sessions, _) = tracker.snapshot(
        NOW,
        ON,
        |_| Place {
            host,
            takes_keys,
            accepts_replies: true,
            host_name: name.clone(),
        },
        |_| Some("main".into()),
    );
    sessions.into_iter().next().unwrap()
}

#[test]
fn answers_where_cline_takes_them() {
    let cli = snapshot(
        CLI_CLIENT,
        HostKind::Tmux,
        true,
        None,
        approval(),
        "pending",
    );
    assert_eq!(cli.phase, SessionPhase::WaitingPermission);
    assert_eq!(
        cli.permission.as_ref().map(|p| p.request_id.as_str()),
        Some("call_da0cd2e3")
    );
    assert_eq!(cli.provider, Provider::Cline);
    assert!(cli.editor.is_none());
    let vscode = snapshot(
        EXTENSION_CLIENT,
        HostKind::Vscode,
        false,
        Some("Visual Studio Code"),
        approval(),
        "pending",
    );
    assert!(vscode.permission.is_none());
    assert_eq!(
        vscode.waiting_for.as_deref(),
        Some(super::super::hook_agents::ANSWER_IN_TERMINAL)
    );
    assert_eq!(vscode.editor.as_deref(), Some("VS Code"));
    let cursor = snapshot(
        EXTENSION_CLIENT,
        HostKind::Vscode,
        false,
        Some("Cursor"),
        approval(),
        "pending",
    );
    assert_eq!(cursor.editor.as_deref(), Some("Cursor"));
    let hub = snapshot(
        HUB_CLIENT,
        HostKind::Vscode,
        false,
        None,
        approval(),
        "pending",
    );
    assert!(hub.permission.is_some(), "an approval through the hub");
    let hub_question = snapshot(
        HUB_CLIENT,
        HostKind::Vscode,
        false,
        None,
        question(),
        "running",
    );
    assert!(
        hub_question.permission.is_none(),
        "a question the hub can't take"
    );
    let keyed = snapshot(
        CLI_CLIENT,
        HostKind::Tmux,
        true,
        None,
        question(),
        "running",
    );
    assert!(
        keyed
            .permission
            .as_ref()
            .is_some_and(|p| p.asks_questions())
    );
}

#[test]
fn keys_answer_the_clis_dialog() {
    let approval = ask(
        &call("run_commands", json!({"commands": ["ls"]})),
        None,
        NOW,
    )
    .unwrap();
    assert_eq!(
        keys(&Decision::Allow, &approval).unwrap().keys,
        vec![Key::Letter('y')]
    );
    assert_eq!(
        keys(&Decision::Deny, &approval).unwrap().keys,
        vec![Key::Letter('n')]
    );
    assert!(keys(&Decision::AllowAlways, &approval).is_none());
    let asked = ask(
        &call(
            "ask_question",
            json!({"question": "Which color?", "options": ["Red", "Blue", "Green"]}),
        ),
        None,
        NOW,
    )
    .unwrap();
    let answer = |text: &str| {
        let mut answers = BTreeMap::new();
        answers.insert("Which color?".to_owned(), vec![text.to_owned()]);
        Decision::Answer(answers)
    };
    assert_eq!(
        keys(&answer("Blue"), &asked).unwrap().keys,
        vec![Key::Digit(2)]
    );
    let own = keys(&answer("Teal please"), &asked).unwrap();
    assert_eq!(own.keys, vec![Key::Down; 3]);
    assert_eq!(own.typed.as_deref(), Some("Teal please"));
    assert_eq!(
        keys(&Decision::Deny, &asked).unwrap().keys,
        vec![Key::Escape]
    );
    assert!(keys(&Decision::Allow, &asked).is_none());
    assert_eq!(Key::Letter('n').bytes(), "n");
    assert_eq!(Key::Letter('n').multiplexer_name(), "n");
}

#[test]
fn the_hub_is_found_and_read() {
    let file =
        br#"{"hubId":"hub_1","authToken":"0b12e9abc","url":"ws://127.0.0.1:25463/hub","pid":97843}"#;
    let found = hub::parse(file).unwrap();
    assert_eq!(found.url, "ws://127.0.0.1:25463/hub");
    assert_eq!(found.token, "0b12e9abc");
    assert_eq!(found.pid, 97843);
    assert!(
        hub::parse(br#"{"authToken":"t","url":"ws://example.com:25463/hub","pid":3}"#).is_none()
    );
    assert!(hub::parse(br#"{"url":"ws://127.0.0.1:1/hub","pid":3}"#).is_none());
    let sessions = vec![
        json!({"sessionId": "s1", "participants": [{"clientId": "core-ijkcpmrb", "role": "creator"}]}),
        json!({"sessionId": "s2", "participants": []}),
    ];
    let clients = vec![
        json!({"clientId": "core-ijkcpmrb", "clientType": "cli", "metadata": {"version": "0.2.1", "pid": 98185}}),
        json!({"clientId": "openbar-1", "clientType": "openbar"}),
    ];
    let shown = hub::clients(&sessions, &clients);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown.get("s1"), Some(&98185));
    let folder = std::env::temp_dir().join(format!("ob-cline-hub-{}", std::process::id()));
    std::fs::create_dir_all(folder.join("locks").join("hub")).unwrap();
    std::fs::write(
        folder.join("locks").join("hub").join("production.json"),
        file,
    )
    .unwrap();
    assert_eq!(
        hub::discovery(&folder, &|p| p == 97843).map(|d| d.pid),
        Some(97843)
    );
    assert!(hub::discovery(&folder, &|_| false).is_none());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn nothing_is_installed_into_cline() {
    assert!(Provider::HOOK_AGENTS.contains(&Provider::Cline));
    assert!(
        !Provider::INSTALLED_AGENTS.contains(&Provider::Cline) && !Provider::Cline.installs_hooks()
    );
    assert!(Provider::Cline.answers_permissions());
    assert_eq!(Provider::from_key("cline"), Some(Provider::Cline));
    assert_eq!(
        super::super::hook_agents::command_name(Provider::Cline),
        Some("cline")
    );
    assert_eq!(
        super::super::hook_agents::start_arguments(Provider::Cline, "hi"),
        vec!["--tui".to_owned(), "hi".to_owned()]
    );
    assert!(
        crate::diagnostics::hook_records::hook_file(Provider::Cline, Path::new("/h"), "app")
            .is_none()
    );
    assert_eq!(editor_name(Some("Visual Studio Code")), "VS Code");
    assert_eq!(editor_name(None), "VS Code");
    // A second return after a reply would approve the tool Cline asks about next.
    assert!(super::super::hook_agents::takes_one_return(Provider::Cline));
}

/// Reads a live Cline store: `OB_CLINE_DB=<path to sessions.db> cargo test
/// -- --ignored reads_a_live_store --nocapture`.
#[test]
#[ignore]
fn reads_a_live_store() {
    let path = std::env::var("OB_CLINE_DB").expect("OB_CLINE_DB");
    let found = records(Path::new(&path));
    println!("{found:#?}");
    assert!(!found.is_empty());
    if let Some(messages) = found.iter().find_map(|r| r.messages.clone()) {
        println!(
            "{:#?}",
            transcript(Path::new(&messages)).map(|t| (t.calls.len(), t.open().cloned()))
        );
    }
}

/// Asks a live Cline hub which CLI shows each session: `OB_CLINE_DATA=<data
/// folder> cargo test -- --ignored asks_a_live_hub --nocapture`.
#[test]
#[ignore]
fn asks_a_live_hub() {
    let data = std::env::var("OB_CLINE_DATA").expect("OB_CLINE_DATA");
    let found = hub::discovery(Path::new(&data), &|_| true);
    println!("{found:?}");
    let shown = hub::shown(&found.expect("a hub"), std::time::Duration::from_secs(3));
    println!("{shown:?}");
    assert!(shown.is_some());
}
