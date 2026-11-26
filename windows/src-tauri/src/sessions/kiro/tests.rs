// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Kiro CLI (#238): payloads copied from Kiro CLI 3.6.2 runs.

use serde_json::{Value, json};

use super::*;
use crate::hosts::Key;
use crate::sessions::hook_agents::{self, Kind, Tracker};
use crate::sessions::hook_message::Decision;
use crate::sessions::models::ContentPolicy;

const SESSION: &str = "sess_d01ac6eb-aece-4bcf-84cb-51b9c20d3dfe";
const ON: ContentPolicy = ContentPolicy { previews: true };
const NOW: f64 = 1_795_681_369.0;

fn envelope(hook: &str, extra: Value) -> Vec<u8> {
    let mut event =
        json!({"session_id": SESSION, "hook_event_name": hook, "cwd": "/private/tmp/ob238/rocket"});
    for (k, v) in extra.as_object().unwrap() {
        event[k] = v.clone();
    }
    serde_json::to_vec(&json!({"openbar": "kiro", "hook": hook, "pid": 35932, "event": event}))
        .unwrap()
}

fn shell() -> Value {
    json!({"tool_name": "execute_bash", "tool_input": {
        "command": "touch hello.txt", "description": "Create hello.txt",
        "cwd": "/private/tmp/ob238/rocket", "run_in_background": false, "timeout": null,
    }})
}

fn feed(tracker: &mut Tracker, data: &[u8]) {
    let decoded = hook_agents::decode(data, NOW, ON).unwrap();
    for event in decoded.events {
        tracker.ingest(event, NOW);
    }
}

#[test]
fn its_permission_hook_is_held_and_answered() {
    let request = envelope(
        "PermissionRequest",
        json!({"tool_name": "execute_bash", "tool_input": {"command": "touch hello.txt"},
               "permission": {"capability": "shell", "resource": "touch hello.txt", "command": "touch hello.txt"}}),
    );
    assert!(hook_agents::waits(&request));
    assert!(!hook_agents::waits(&envelope("PreToolUse", shell())));
    assert!(!hook_agents::waits(&envelope("Stop", json!({}))));
    let decoded = hook_agents::decode(&request, NOW, ContentPolicy { previews: false }).unwrap();
    let Some(Kind::HeldPermission(asked)) = decoded.events.first().map(|e| e.kind.clone()) else {
        panic!("held: {:?}", decoded.events);
    };
    assert_eq!(asked.tool, "Bash");
    assert_eq!(
        decoded.permission,
        Some((SESSION.to_owned(), asked.request_id.clone()))
    );
    let body: Value = serde_json::from_slice(&reply(&Decision::Allow).unwrap()).unwrap();
    assert_eq!(body["hookSpecificOutput"]["decision"]["behavior"], "allow");
    let body: Value = serde_json::from_slice(&reply(&Decision::Deny).unwrap()).unwrap();
    assert_eq!(body["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert_eq!(
        body["hookSpecificOutput"]["decision"]["message"],
        crate::sessions::hook_message::DENY_MESSAGE
    );
    assert!(reply(&Decision::Answer(Default::default())).is_none());
    assert_eq!(keys(&Decision::Allow), Some(vec![Key::Enter]));
    assert_eq!(keys(&Decision::Deny), Some(vec![Key::Down, Key::Enter]));
    assert_eq!(keys(&Decision::AllowAlways), None);
    // An edit's request reads as Claude Code's, its diff with it.
    let edit = hook_agents::decode(
        &envelope(
            "PermissionRequest",
            json!({"tool_name": "str_replace", "tool_input": {
                "path": "/private/tmp/ob238/rocket/notes.txt", "oldStr": "beta", "newStr": "gamma", "replace_all": false}}),
        ),
        NOW,
        ON,
    )
    .unwrap();
    let Some(Kind::HeldPermission(change)) = edit.events.first().map(|e| e.kind.clone()) else {
        panic!("edit held");
    };
    assert_eq!(change.tool, "Edit");
    assert!(!change.diffs.is_empty());
}

#[test]
fn a_turn_cancelled_at_its_dialog_ends() {
    let mut tracker = Tracker::default();
    feed(
        &mut tracker,
        &envelope("UserPromptSubmit", json!({"prompt": "go"})),
    );
    feed(&mut tracker, &envelope("PreToolUse", shell()));
    let mut cancelled = shell();
    cancelled["tool_response"] =
        json!("Output:\nThe tool call was cancelled before it was approved.\n\nExit Code: -1");
    feed(&mut tracker, &envelope("PostToolUse", cancelled));
    let entry = tracker.entry(&format!("kiro:{SESSION}")).unwrap();
    assert!(entry.turn_open_since().is_none());
}

#[test]
fn its_tools_read_as_claude_codes() {
    let input = claude_input(
        "fs_write",
        json!({"path": "/a/notes.txt", "text": "alpha\n"})
            .as_object()
            .unwrap(),
    );
    assert_eq!(input["file_path"], "/a/notes.txt");
    assert_eq!(input["content"], "alpha\n");
    let input = claude_input(
        "str_replace",
        json!({"path": "/a/b", "oldStr": "x", "newStr": "y"})
            .as_object()
            .unwrap(),
    );
    assert_eq!(input["old_string"], "x");
    assert_eq!(input["new_string"], "y");
    assert_eq!(
        hook_agents::name(crate::sessions::models::Provider::Kiro, "execute_pwsh"),
        "Bash"
    );
    assert_eq!(
        hook_agents::name(crate::sessions::models::Provider::Kiro, "read_file"),
        "Read"
    );
}

#[test]
fn the_session_is_its_terminal_ui() {
    // The hook's shell, node (the agent server), bun (the terminal UI), kiro-cli.
    let chain = vec![
        (49107, "bash".to_owned()),
        (48969, "node".to_owned()),
        (48952, "bun".to_owned()),
        (48917, "kiro-cli".to_owned()),
    ];
    assert_eq!(terminal_process(&chain, 48969), 48952);
    let windows = vec![
        (7, "cmd.exe".to_owned()),
        (8, "node.exe".to_owned()),
        (9, "bun.exe".to_owned()),
    ];
    assert_eq!(terminal_process(&windows, 8), 9);
    assert_eq!(
        terminal_process(&[(1, "node".to_owned()), (2, "zsh".to_owned())], 1),
        1
    );
    assert_eq!(terminal_process(&[], 5), 5);
}

#[test]
fn its_log_tells_the_last_turn() {
    let lines = [
        r#"{"id":"d97b","timestamp":"2026-11-26T08:22:48.828Z","payload":{"type":"user","content":"Run it"}}"#,
        r#"{"id":"t","timestamp":"2026-11-26T08:22:48.849Z","payload":{"type":"turn_start","turnId":"988f"}}"#,
        r#"{"id":"r","timestamp":"2026-11-26T08:22:51.334Z","payload":{"type":"assistant","content":"...","operationType":"Reasoning"}}"#,
        r#"{"id":"s","timestamp":"2026-11-26T08:23:24.750Z","payload":{"type":"assistant","content":"Done.","operationType":"Say"}}"#,
        r#"{"id":"e","timestamp":"2026-11-26T08:23:24.810Z","payload":{"type":"turn_end","stopReason":"end_turn"}}"#,
        r##"{"id":"x","timestamp":"2026-11-26T08:23:24.820Z","payload":{"type":"session_start","content":"# Kiro"}}"##,
    ];
    let turn = last_turn_in(&lines.join("\n"), true);
    assert!(turn.ended && !turn.cancelled);
    assert_eq!(turn.reply.as_deref(), Some("Done."));
    assert!(turn.at.is_some());
    assert_eq!(last_turn_in(&lines.join("\n"), false).reply, None);
    assert!(!last_turn_in(&lines[..4].join("\n"), true).ended);
    let cancelled = format!(
        "{}\n{}",
        lines[..4].join("\n"),
        r#"{"id":"c","timestamp":"2026-11-26T08:29:48.016Z","payload":{"type":"turn_end","stopReason":"cancelled"}}"#
    );
    assert!(last_turn_in(&cancelled, true).cancelled);

    let kiro = std::env::temp_dir().join(format!("ob-kiro-{}", std::process::id()));
    let folder = kiro.join("sessions/1881f463eb2dbfa5").join(SESSION);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("messages.jsonl"), lines.join("\n")).unwrap();
    std::fs::write(
        folder.join("session.json"),
        format!(
            r#"{{"id":"{SESSION}","title":"Create File With Touch Command","modelId":"auto"}}"#
        ),
    )
    .unwrap();
    assert_eq!(session_folder(SESSION, &kiro), Some(folder.clone()));
    assert_eq!(session_folder("../x", &kiro), None);
    assert_eq!(session_folder("sess_missing", &kiro), None);
    assert_eq!(
        details(&folder),
        (
            Some("Create File With Touch Command".to_owned()),
            Some("auto".to_owned())
        )
    );
    assert_eq!(
        last_turn(&folder.join("messages.jsonl"), true)
            .reply
            .as_deref(),
        Some("Done.")
    );
    let _ = std::fs::remove_dir_all(&kiro);
}
