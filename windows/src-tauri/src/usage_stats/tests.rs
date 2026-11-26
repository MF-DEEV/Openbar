// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

use super::*;

fn context() -> Context {
    Context {
        platform: "windows",
        os_version: "10.0.26100".into(),
        arch: "x86_64".into(),
        app_version: "1.3.0".into(),
        hooks: BTreeMap::from([("claude".into(), true), ("gemini".into(), false)]),
        enabled: vec!["sessions".into(), "mixer".into()],
    }
}

fn note(id: &str, provider: &str, terminal: Option<&str>) -> Note {
    Note {
        id: id.into(),
        provider: provider.into(),
        terminal: terminal.map(String::from),
    }
}

const NOW: f64 = 1_800_000_000.0;
const ID: &str = "3f2b8c4e-9a1d-4e7b-8c2f-1a2b3c4d5e6f";

#[test]
fn the_payload_has_exactly_the_schema_fields() {
    let mut state = State::default();
    state.sessions.insert("claude".into(), 12);
    state.sessions.insert("codex".into(), 3);
    state.terminals.insert("windows-terminal".into(), 9);
    state.opened.insert("sessions".into(), 41);
    let body = payload(ID, &context(), &state);
    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "agents",
            "appVersion",
            "arch",
            "features",
            "installId",
            "osVersion",
            "platform",
            "terminals",
            "v"
        ]
    );
    assert_eq!(body["v"], 1);
    assert_eq!(body["platform"], "windows");
    assert_eq!(
        body["agents"],
        json!({"claude": {"sessions": 12, "hooks": true}, "codex": {"sessions": 3, "hooks": false}})
    );
    assert_eq!(body["terminals"], json!({"windows-terminal": 9}));
    assert_eq!(
        body["features"],
        json!({"enabled": ["sessions", "mixer"], "opened": {"sessions": 41}})
    );
    assert!(
        body.get("rosetta").is_none() && body.get("build").is_none(),
        "Mac-only fields stay out"
    );
}

#[test]
fn the_schedule_waits_twenty_or_twenty_four_hours_and_six_after_a_failure() {
    let mut state = State::default();
    assert!(is_due(NOW, &state, true));
    state.last_success = Some(NOW - 19.0 * 3600.0);
    assert!(!is_due(NOW, &state, true));
    state.last_success = Some(NOW - 21.0 * 3600.0);
    assert!(is_due(NOW, &state, true));
    assert!(!is_due(NOW, &state, false));
    state.last_success = Some(NOW - 24.0 * 3600.0);
    assert!(is_due(NOW, &state, false));
    state.last_failure = Some(NOW - 5.0 * 3600.0);
    assert!(!is_due(NOW, &state, true));
    state.last_failure = Some(NOW - 6.0 * 3600.0);
    assert!(is_due(NOW, &state, true));
    let future = State {
        last_success: Some(NOW + 86_400.0 * 30.0),
        ..State::default()
    };
    assert!(
        is_due(NOW, &future, true),
        "a send dated in the future counts as never"
    );
}

#[test]
fn the_endpoint_takes_https_or_local_http_only() {
    let default = endpoint(None).unwrap();
    assert_eq!(
        default,
        Target {
            secure: true,
            host: "openbar-stats.vercel.app".into(),
            port: 443,
            path: "/api/v1/ping".into()
        }
    );
    let local = endpoint(Some("http://10.0.2.2:3000/api/v1/ping")).unwrap();
    assert_eq!(
        (
            local.secure,
            local.host.as_str(),
            local.port,
            local.path.as_str()
        ),
        (false, "10.0.2.2", 3000, "/api/v1/ping")
    );
    assert!(endpoint(Some("http://host.lima.internal:3000/x")).is_some());
    assert!(endpoint(Some("https://preview.example.dev")).is_some_and(|t| t.path == "/"));
    assert!(
        endpoint(Some("http://example.com/api")).is_none(),
        "plain http elsewhere sends nothing"
    );
    assert!(endpoint(Some("ftp://127.0.0.1/")).is_none());
    assert!(endpoint(Some("https://user:pw@example.com/")).is_none());
    assert!(endpoint(Some("https://example.com:notaport/")).is_none());
    assert!(endpoint(Some("   ")).is_some_and(|t| t.host == "openbar-stats.vercel.app"));
}

#[test]
fn switched_off_dnt_and_debug_builds_are_quiet() {
    assert!(allowed(true, false, false));
    assert!(!allowed(false, false, false));
    assert!(!allowed(true, true, false));
    assert!(!allowed(true, false, true));
    assert!(do_not_track(Some("1")) && do_not_track(Some("true")) && do_not_track(Some(" YES ")));
    assert!(!do_not_track(Some("0")) && !do_not_track(None));
    assert!(setting_on(&json!({})), "on by default");
    assert!(!setting_on(&json!({"island": {"shareUsageStats": false}})));
}

#[test]
fn sessions_and_terminals_are_counted_once() {
    let mut state = State::default();
    record(
        &mut state,
        &[
            note("a", "claude", None),
            note("b", "codex", Some("wezterm")),
            note(crate::sessions::sample::ID, "claude", Some("conhost")),
        ],
        NOW,
    );
    assert_eq!(
        state.sessions,
        BTreeMap::from([("claude".into(), 1), ("codex".into(), 1)])
    );
    assert_eq!(state.terminals, BTreeMap::from([("wezterm".into(), 1)]));
    record(
        &mut state,
        &[
            note("a", "claude", Some("vscode")),
            note("b", "codex", Some("wezterm")),
        ],
        NOW + 60.0,
    );
    assert_eq!(
        state.sessions["claude"], 1,
        "listed again, not counted again"
    );
    assert_eq!(
        state.terminals,
        BTreeMap::from([("vscode".into(), 1), ("wezterm".into(), 1)])
    );
    record(
        &mut state,
        &[note("a", "claude", None)],
        NOW + 2.0 * 86_400.0,
    );
    assert_eq!(state.sessions["claude"], 1, "a restart within three days");
    record(
        &mut state,
        &[note("a", "claude", None)],
        NOW + 4.0 * 86_400.0,
    );
    assert_eq!(state.sessions["claude"], 2, "forgotten after three days");
    record(
        &mut state,
        &[note("x", "Not An Id", Some("C:\\Users\\me"))],
        NOW,
    );
    assert!(
        !state.sessions.contains_key("Not An Id") && !state.terminals.contains_key("C:\\Users\\me")
    );
    let mut many = State::default();
    let notes: Vec<Note> = (0..SEEN_LIMIT + 10)
        .map(|i| note(&format!("s{i}"), "claude", None))
        .collect();
    record(&mut many, &notes, NOW);
    assert_eq!(many.seen.len(), SEEN_LIMIT);
}

#[test]
fn terminals_map_to_fixed_ids() {
    assert_eq!(
        terminal_id(HostKind::WindowsTerminal, None),
        Some("windows-terminal")
    );
    assert_eq!(
        terminal_id(HostKind::Vscode, Some("Cursor")),
        Some("cursor")
    );
    assert_eq!(
        terminal_id(HostKind::Vscode, Some("Visual Studio Code")),
        Some("vscode")
    );
    assert_eq!(terminal_id(HostKind::App, Some("Ghostty")), Some("ghostty"));
    assert_eq!(
        terminal_id(HostKind::App, Some("GNOME Terminal")),
        Some("gnome-terminal")
    );
    assert_eq!(
        terminal_id(HostKind::App, Some("My Private Tool")),
        Some("other")
    );
    assert_eq!(
        terminal_id(HostKind::Remote, Some("my-server")),
        Some("remote")
    );
    assert_eq!(terminal_id(HostKind::Unknown, None), None);
}

#[test]
fn a_delivery_keeps_only_what_came_in_meanwhile() {
    let mut state = State::default();
    record_open(&mut state, "sessions");
    record(&mut state, &[note("a", "claude", Some("conhost"))], NOW);
    let sent = state.clone();
    record_open(&mut state, "sessions");
    settle(&mut state, &sent, Outcome::Delivered, NOW);
    assert_eq!(state.opened, BTreeMap::from([("sessions".into(), 1)]));
    assert!(state.sessions.is_empty() && state.terminals.is_empty());
    assert_eq!((state.last_success, state.last_failure), (Some(NOW), None));
    let sent = state.clone();
    settle(&mut state, &sent, Outcome::Failed, NOW + 1.0);
    assert_eq!(state.opened["sessions"], 1);
    assert_eq!(state.last_failure, Some(NOW + 1.0));
    assert_eq!(outcome(Some(204)), Outcome::Delivered);
    assert_eq!(outcome(Some(429)), Outcome::Delivered);
    assert_eq!(outcome(Some(400)), Outcome::Delivered);
    assert_eq!(outcome(Some(503)), Outcome::Failed);
    assert_eq!(outcome(Some(404)), Outcome::Failed);
    assert_eq!(outcome(None), Outcome::Failed);
}

#[test]
fn the_body_stays_under_four_kilobytes() {
    let mut state = State::default();
    for i in 0..400 {
        state.terminals.insert(format!("t{i}"), i + 1);
    }
    for i in 0..40 {
        state.opened.insert(format!("m{i}"), 1);
    }
    let body = encode(payload(ID, &context(), &state)).unwrap();
    assert!(body.len() <= MAX_BYTES, "{}", body.len());
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["terminals"]["t399"], 400);
    assert!(value["terminals"].get("t0").is_none());
}

#[test]
fn os_versions_and_sections() {
    if cfg!(windows) {
        assert_eq!(
            os_version("Windows 11 Pro", "24H2", Some("26100.1234")),
            "10.0.26100"
        );
        assert_eq!(os_version("Windows", "", None), "10.0");
    } else {
        assert_eq!(os_version("Ubuntu", "24.04", None), "Ubuntu 24.04");
        assert_eq!(
            os_version("Debian GNU/Linux", "12", None),
            "Debian GNU/Linux 12"
        );
        assert_eq!(os_version("Weird\"<OS>", "1;2", None), "Weird OS 1 2");
        assert_eq!(os_version("", "", None), "Linux");
    }
    assert_eq!(
        arch(),
        if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        }
    );
    let prefs = json!({"island": {"moduleOrder": ["sessions", "music"], "hiddenModules": ["mixer"],
        "sections": {"camera": false}}});
    let enabled = enabled_sections(&prefs);
    assert_eq!(&enabled[..3], ["sessions", "music", "controls"]);
    assert!(!enabled.contains(&"mixer".to_string()) && !enabled.contains(&"camera".to_string()));
    assert_eq!(enabled.len(), 15);
    assert!(enabled_sections(&json!({"island": {"enabled": false}})).is_empty());
    let id = new_id().unwrap();
    assert_eq!((id.len(), &id[14..15]), (36, "4"));
    assert_ne!(new_id(), new_id());
}

fn temp_store(name: &str) -> (Store, PathBuf) {
    let path =
        std::env::temp_dir().join(format!("openbar-usage-{name}-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    (Store::open(Some(path.clone())), path)
}

#[test]
fn the_store_sends_settles_and_persists() {
    let (store, path) = temp_store("send");
    store.set_active(true);
    store.note(&[note("a", "claude", Some("kitty"))], NOW);
    store.note_open("sessions");
    let mut ids = 0;
    let mut install_id = || {
        ids += 1;
        Some(ID.to_string())
    };
    let mut bodies: Vec<Value> = Vec::new();
    let mut fail = |body: &[u8]| {
        bodies.push(serde_json::from_slice(body).unwrap());
        Some(503)
    };
    assert_eq!(
        store.send_if_due(true, NOW, &context(), &mut install_id, &mut fail),
        Some(Outcome::Failed)
    );
    assert_eq!(
        store.state().sessions["claude"],
        1,
        "a failure keeps the counts"
    );
    assert_eq!(
        store.send_if_due(false, NOW + 3600.0, &context(), &mut install_id, &mut fail),
        None,
        "no retry before 6 h"
    );
    let mut ok = |body: &[u8]| {
        bodies.push(serde_json::from_slice(body).unwrap());
        Some(204)
    };
    assert_eq!(
        store.send_if_due(
            false,
            NOW + 6.0 * 3600.0,
            &context(),
            &mut install_id,
            &mut ok
        ),
        Some(Outcome::Delivered)
    );
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[1]["installId"], ID);
    assert_eq!(bodies[1]["agents"]["claude"]["sessions"], 1);
    assert_eq!(bodies[1]["terminals"], json!({"kitty": 1}));
    let reopened = Store::open(Some(path.clone()));
    assert!(
        reopened.state().sessions.is_empty()
            && reopened.state().last_success == Some(NOW + 6.0 * 3600.0)
    );
    assert!(
        reopened.state().seen.contains_key("a"),
        "the counted sessions are kept on disk"
    );
    store.clear();
    assert!(!path.exists() && store.state() == State::default() && !store.has_data());
}

#[test]
fn an_inactive_store_counts_and_sends_nothing() {
    let (store, path) = temp_store("quiet");
    store.note(&[note("a", "claude", None)], NOW);
    store.note_open("mixer");
    assert!(!path.exists() && store.state() == State::default());
    let mut asked = false;
    let mut install_id = || {
        asked = true;
        Some(ID.to_string())
    };
    let sent = std::cell::Cell::new(false);
    let mut send = |_: &[u8]| {
        sent.set(true);
        Some(204)
    };
    assert_eq!(
        store.send_if_due(true, NOW, &context(), &mut install_id, &mut send),
        None
    );
    assert!(!asked && !sent.get(), "no id is made and nothing is sent");
    let (on, _) = temp_store("noid");
    on.set_active(true);
    let mut none = || None;
    assert_eq!(
        on.send_if_due(true, NOW, &context(), &mut none, &mut send),
        None,
        "no id, no send"
    );
    assert!(!sent.get());
}
