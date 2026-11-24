// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The Hooks page (#236), as Tests/HookPageTests.swift: each file's state and
//! why it is skipped, folders and tools added by hand, Codex's hook review,
//! Reinstall and Remove with copies kept, putting lost hooks back, and
//! Remove everything leaving every file as it was before Openbar.

use super::*;
use crate::sessions::hook_agents::installer as agents;
use crate::sessions::models::{ContentPolicy, HostKind};
use serde_json::json;

const APP: &str = "com.openbar.test";

/// A home of its own, with its own records and machine state.
struct Sandbox {
    home: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let home =
            std::env::temp_dir().join(format!("ob236-{tag}-{}-{}", std::process::id(), new_id()));
        std::fs::create_dir_all(&home).unwrap();
        hook_records::set_test_file(Some(home.join("records.json")));
        set_test_store(Some(home.join("machine.json")));
        Sandbox { home }
    }

    fn env(&self) -> Env {
        Env {
            home: self.home.clone(),
            exe: PathBuf::from("/opt/openbar/openbar"),
            endpoint: "/tmp/ob.sock".into(),
            app: APP.into(),
        }
    }

    fn folder(&self, path: &str) -> PathBuf {
        let folder = self.home.join(path);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn path(&self, path: &str) -> PathBuf {
        self.home.join(path)
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.home.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.home.join(path)).ok()
    }

    fn names(&self, path: &str) -> Vec<String> {
        std::fs::read_dir(self.home.join(path))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn context(
        &self,
        hidden: &[Provider],
        found: &[Provider],
        folders: &[FolderSetting],
        tools: &[Tool],
    ) -> Context {
        let env = self.env();
        Context {
            home: env.home,
            exe: env.exe,
            endpoint: env.endpoint,
            app: env.app,
            accounts: accounts::all(&[self.home.join(".claude")], &[], &self.home),
            hidden: hidden.iter().copied().collect(),
            found: found.iter().copied().collect(),
            folders: folders.to_vec(),
            tools: tools.to_vec(),
            opt_out: opt_out(),
            records: hook_records::all(),
        }
    }

    /// Only the rows whose files are in this home.
    fn list(&self, ctx: &Context) -> Vec<Target> {
        list(ctx)
            .into_iter()
            .filter(|t| t.file.as_ref().is_none_or(|f| f.starts_with(&self.home)))
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        hook_records::set_test_file(None);
        set_test_store(None);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn skipped(reason: Skip) -> Shown {
    Shown::Skipped { reason }
}

fn row<'a>(targets: &'a [Target], id: &str) -> &'a Target {
    targets
        .iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("no row {id}"))
}

fn claude_row(targets: &[Target]) -> &Target {
    targets
        .iter()
        .find(|t| t.provider == Provider::Claude)
        .unwrap()
}

// MARK: States

#[test]
fn states_and_reasons() {
    let state = shown_state;
    let plain = |d| state(d, true, true, true, false, false, false);
    assert_eq!(plain(Derived::Installed), Shown::Installed);
    assert_eq!(plain(Derived::Outdated), Shown::Outdated);
    assert_eq!(plain(Derived::ChangedByHand), Shown::ChangedByHand);
    assert_eq!(plain(Derived::Unreadable), Shown::Unreadable);
    // Hooks deleted by hand read as missing.
    assert_eq!(plain(Derived::RemovedByHand), Shown::Missing);
    assert_eq!(plain(Derived::Missing), Shown::Missing);
    // A file left without hooks says why.
    let missing = Derived::Missing;
    assert_eq!(
        state(missing, false, true, true, false, false, false),
        skipped(Skip::AgentNotInstalled)
    );
    assert_eq!(
        state(missing, true, false, true, false, false, false),
        skipped(Skip::FolderMissing)
    );
    assert_eq!(
        state(missing, true, true, false, false, false, false),
        skipped(Skip::SwitchedOff)
    );
    assert_eq!(
        state(missing, true, true, true, true, false, false),
        skipped(Skip::RemovedByYou)
    );
    assert_eq!(
        state(missing, true, true, true, false, true, false),
        skipped(Skip::AccountHidden)
    );
    // Hooks the agent still has to trust say so.
    assert_eq!(
        state(Derived::Installed, true, true, true, false, false, true),
        skipped(Skip::NeedsApproval)
    );
    // Hooks changed by hand show whatever the switch.
    assert_eq!(
        state(
            Derived::ChangedByHand,
            true,
            true,
            false,
            false,
            false,
            false
        ),
        Shown::ChangedByHand
    );
    // Only what the person can fix counts as needing a look.
    assert!(skipped(Skip::NeedsApproval).needs_attention() && Shown::Missing.needs_attention());
    assert!(!skipped(Skip::AgentNotInstalled).needs_attention());
    assert!(!Shown::Installed.needs_attention());
    // The page reads the state as `{kind, reason}`.
    assert_eq!(
        serde_json::to_value(skipped(Skip::FolderMissing)).unwrap(),
        json!({"kind": "skipped", "reason": "folderMissing"})
    );
    assert_eq!(
        serde_json::to_value(Shown::ChangedByHand).unwrap(),
        json!({"kind": "changedByHand"})
    );
}

// MARK: Folders added by hand

#[test]
fn extra_folders() {
    let box_ = Sandbox::new("folders");
    let gemini = box_.folder("work/.gemini");
    let picked = box_.path("work");
    // A folder holding .gemini stands for it, as GEMINI_CLI_HOME does.
    assert_eq!(config_folder(Provider::Gemini, &picked), gemini);
    assert_eq!(config_folder(Provider::Gemini, &gemini), gemini);
    let setting = add_folder(
        Provider::Gemini,
        &picked,
        Some(&box_.path(".gemini")),
        &box_.home,
        &[],
    )
    .unwrap();
    let listed = vec![setting.clone()];
    // A folder is listed once, and only a folder.
    assert_eq!(
        add_folder(Provider::Gemini, &gemini, None, &box_.home, &listed),
        Err(FolderFailure::AlreadyListed)
    );
    assert_eq!(
        add_folder(
            Provider::Gemini,
            &box_.path("nope"),
            None,
            &box_.home,
            &listed
        ),
        Err(FolderFailure::NotFolder)
    );
    // The agent's own folder needs no second row.
    let own = box_.folder(".codex");
    assert_eq!(
        add_folder(Provider::Codex, &own, Some(&own), &box_.home, &[]),
        Err(FolderFailure::AlreadyListed)
    );
    // The home folder or the disk is never an agent's config folder.
    assert_eq!(
        add_folder(Provider::Codex, &box_.home, None, &box_.home, &[]),
        Err(FolderFailure::TooBroad)
    );
    let root = if cfg!(windows) { "C:\\" } else { "/" };
    assert_eq!(
        add_folder(Provider::Codex, Path::new(root), None, &box_.home, &[]),
        Err(FolderFailure::TooBroad)
    );
    let many: Vec<FolderSetting> = (0..FOLDERS_PER_AGENT)
        .map(|i| FolderSetting {
            agent: "codex".into(),
            path: box_.folder(&format!("c{i}")).to_string_lossy().into_owned(),
            name: None,
        })
        .collect();
    assert_eq!(
        add_folder(
            Provider::Codex,
            &box_.folder("one-more"),
            None,
            &box_.home,
            &many
        ),
        Err(FolderFailure::TooMany)
    );
    // A hidden config folder goes by the folder it sits in.
    assert_eq!(folder_name(&gemini), "work");
    assert_eq!(folder_name(&box_.path("codex-2")), "codex-2");

    // Prefs keep only well-formed folders of agents that take them.
    let prefs = json!({"sessions": {"agentFolders": [
        {"agent": "gemini", "path": gemini},
        {"agent": "gemini", "path": gemini},
        {"agent": "cursor", "path": box_.home},
        {"agent": "codex", "path": "relative"},
    ]}});
    let read = settings_from_prefs(&prefs);
    assert_eq!(read.folders.len(), 1);
    assert!(read.auto_repair);
    assert!(
        !settings_from_prefs(&json!({"hooks": {"autoRepair": false}})).auto_repair,
        "the switch is read"
    );

    // The hooks go into the added folder's settings.json, beside the person's.
    let env = box_.env();
    box_.write("work/.gemini/settings.json", r#"{"theme":"dark"}"#);
    agents::install_in(
        Provider::Gemini,
        &box_.home,
        &env.exe,
        "",
        APP,
        Some(&gemini),
    )
    .unwrap();
    assert_eq!(
        agents::state_in(
            Provider::Gemini,
            &box_.home,
            &env.exe,
            "",
            APP,
            Some(&gemini)
        ),
        Base::Installed
    );
    let settings = installer::read(&gemini.join("settings.json")).unwrap();
    assert_eq!(settings["theme"], "dark", "the person's settings stay");

    // A file of the island's own in an added Copilot folder, with a copy kept when it changes.
    let copilot = box_.folder("copilot-2");
    agents::install_in(
        Provider::Copilot,
        &box_.home,
        &env.exe,
        "",
        APP,
        Some(&copilot),
    )
    .unwrap();
    let other = Path::new("/opt/other/openbar");
    agents::install_in(
        Provider::Copilot,
        &box_.home,
        other,
        "",
        APP,
        Some(&copilot),
    )
    .unwrap();
    let copies = |sandbox: &Sandbox| {
        sandbox
            .names("copilot-2/hooks")
            .into_iter()
            .filter(|n| n.ends_with(".bak") && !n.contains("original"))
            .count()
    };
    assert_eq!(
        agents::state_in(
            Provider::Copilot,
            &box_.home,
            other,
            "",
            APP,
            Some(&copilot)
        ),
        Base::Installed
    );
    assert_eq!(
        copies(&box_),
        1,
        "a file of the island's own is copied aside before it changes"
    );
    agents::remove_in(Provider::Copilot, &box_.home, APP, Some(&copilot)).unwrap();
    assert_eq!(
        agents::state_in(
            Provider::Copilot,
            &box_.home,
            other,
            "",
            APP,
            Some(&copilot)
        ),
        Base::Missing
    );
    assert_eq!(copies(&box_), 2, "and before it goes");

    // Taken off the page, a folder's file is as it was before the island.
    let codex_home = box_.folder("codex-x");
    let codex_setting = FolderSetting {
        agent: "codex".into(),
        path: codex_home.to_string_lossy().into_owned(),
        name: None,
    };
    let ctx = box_.context(
        &[],
        &[Provider::Codex],
        std::slice::from_ref(&codex_setting),
        &[],
    );
    let target = row(&box_.list(&ctx), &codex_setting.target_id()).clone();
    assert_eq!(target.state, Shown::Missing);
    reinstall(&target, &env).unwrap();
    assert!(
        box_.read("codex-x/hooks.json").is_some(),
        "the folder's hooks are in"
    );
    forget(&target, &env).unwrap();
    assert!(box_.read("codex-x/hooks.json").is_none());
    assert!(
        !box_.names("codex-x").iter().any(|n| n.contains("original")),
        "and gone with it once taken off the page"
    );
    // Only agents with a home of their own take folders by hand.
    assert!(agents::file_in(Provider::Opencode, &box_.home, APP).is_none());
    assert_eq!(
        agents::FOLDERS_BY_HAND,
        [
            Provider::Codex,
            Provider::Gemini,
            Provider::Copilot,
            Provider::Pi,
            Provider::Vibe,
            Provider::Grok,
            Provider::Qwen
        ]
    );
    assert_eq!(
        agents::file_in(Provider::Pi, &box_.home, APP).unwrap(),
        box_.home
            .join("extensions")
            .join(format!("openbar-{APP}.js"))
    );
    assert_eq!(
        agents::file_in(Provider::Vibe, &box_.home, APP).unwrap(),
        box_.home.join("hooks.toml")
    );
}

// MARK: Codex

#[test]
fn codex_hashes_are_codexs_own() {
    use codex_trust::*;
    // Codex's own trust hashes, read from `codex app-server` hooks/list (0.164).
    let command = "/bin/cat >> /tmp/ob236/hooklog.txt # openbar-hook test";
    assert_eq!(
        hash("PreToolUse", Some("*"), command, Some(5), false),
        "sha256:9306a10457ff8b14c7c628dd8734747317142cb0e96aebe2e5642673a1bf7e83"
    );
    assert_eq!(
        hash("Stop", None, command, None, false),
        "sha256:4f420d4bacda7a630e3bdc7e5164be1b8c068b64accb2fa7b1c5c81e322d4ddb"
    );
    assert_eq!(snake("PermissionRequest"), "permission_request");
    assert_eq!(snake("Stop"), "stop");
    assert_eq!(timeout("SessionEnd", Some(5)), 3);
    assert_eq!(timeout("SessionEnd", None), 1);
    assert_eq!(timeout("PreToolUse", None), 600);
    // Strings are written as serde_json does.
    assert_eq!(
        serde_json::to_string("a\"b\\c/d\u{1}é").unwrap(),
        "\"a\\\"b\\\\c/d\\u0001é\""
    );
}

#[test]
fn codex_trust_is_read_from_config_toml() {
    let toml = r#"model = "gpt-5"
[hooks.state."/x/hooks.json:stop:0:0"]
trusted_hash = "sha256:aa" # trusted in the review
[hooks.state]
"/x/hooks.json:pre_tool_use:1:0" = { enabled = true, trusted_hash = "sha256:bb" }
[projects."/x"]
trust_level = "trusted"
"#;
    let hashes = codex_trust::trusted_hashes(toml);
    assert_eq!(hashes.len(), 2, "{hashes:?}");
    assert_eq!(hashes["/x/hooks.json:stop:0:0"], "sha256:aa");
    assert_eq!(hashes["/x/hooks.json:pre_tool_use:1:0"], "sha256:bb");
    assert_eq!(
        codex_trust::key_path(r#"hooks.state."a.b""#).unwrap(),
        ["hooks", "state", "a.b"]
    );
    assert!(codex_trust::key_path("a..b").is_none());
}

#[test]
fn codex_hooks_in_a_second_home_and_its_review() {
    let box_ = Sandbox::new("codex");
    let env = box_.env();
    let home2 = box_.folder("codex-2");
    box_.write(
        "codex-2/hooks.json",
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#,
    );
    agents::install_in(Provider::Codex, &box_.home, &env.exe, "", APP, Some(&home2)).unwrap();
    let settings = installer::read(&home2.join("hooks.json")).unwrap();
    let tag = agents::folder_tag(&home2);
    assert_eq!(tag.len(), 12);
    let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(pre.len(), 1);
    assert_eq!(pre[0]["matcher"], "*");
    let command = pre[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(
        command.contains(&format!("--via {tag} ")) && command.contains("--agent codex"),
        "Codex's hooks name the folder they were written for: {command}"
    );
    assert_eq!(settings["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 3);
    let stop = settings["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert_eq!(
        stop[0]["hooks"][0]["command"], "say done",
        "the person's own Codex hooks stay first"
    );
    assert_eq!(
        agents::state_in(Provider::Codex, &box_.home, &env.exe, "", APP, Some(&home2)),
        Base::Installed
    );
    assert_eq!(
        agents::state(Provider::Codex, &box_.home, &env.exe, "", APP),
        Base::Missing,
        "Codex's own home needs none"
    );
    assert_eq!(
        codex_trust::trusted(&home2, APP),
        Some(false),
        "new hooks wait for Codex's review"
    );
    // The review keeps a hash for each hook; the island reads the same keys.
    let file = home2.join("hooks.json");
    let expected = codex_trust::expected(&file, &settings, &installer::marker(APP));
    let prefix = format!("{}:", codex_trust::real_path(&file));
    assert_eq!(expected.len(), agents::CODEX_EVENTS.len());
    assert!(expected.keys().all(|k| k.starts_with(&prefix)));
    assert!(expected.keys().any(|k| k.ends_with(":stop:1:0")));
    let trust: String = expected
        .iter()
        .map(|(k, v)| format!("[hooks.state.\"{k}\"]\ntrusted_hash = \"{v}\"\n"))
        .collect();
    box_.write("codex-2/config.toml", &trust);
    assert_eq!(
        codex_trust::trusted(&home2, APP),
        Some(true),
        "trusted in the review, they run"
    );
    let setting = FolderSetting {
        agent: "codex".into(),
        path: home2.to_string_lossy().into_owned(),
        name: None,
    };
    let other = Path::new("/opt/other/openbar");
    agents::install_in(Provider::Codex, &box_.home, other, "", APP, Some(&home2)).unwrap();
    assert_eq!(
        codex_trust::trusted(&home2, APP),
        Some(false),
        "rewritten hooks need the review again"
    );
    // The row says so while the hooks wait for the review.
    let ctx = Context {
        exe: other.to_path_buf(),
        ..box_.context(&[], &[Provider::Codex], std::slice::from_ref(&setting), &[])
    };
    let target = row(&box_.list(&ctx), &setting.target_id()).clone();
    assert_eq!(target.state, skipped(Skip::NeedsApproval));
    assert!(target.can_reinstall && target.state.needs_attention());
}

// MARK: Tools with Claude Code style hooks

#[test]
fn tools_with_claude_style_hooks() {
    let box_ = Sandbox::new("tools");
    let env = box_.env();
    let claude = box_.folder(".claude").join("settings.json");
    let accounts = [claude.clone()];
    assert_eq!(
        add_tool(" ", &box_.path("x.json"), &accounts, &[]),
        Err(ToolFailure::NoName)
    );
    assert_eq!(
        add_tool("Kit", &box_.path("no/where.json"), &accounts, &[]),
        Err(ToolFailure::NotInFolder)
    );
    assert_eq!(
        add_tool("Kit", &claude, &accounts, &[]),
        Err(ToolFailure::ClaudeAccount)
    );
    box_.write(
        ".kit/settings.json",
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo bye"}]}]},"model":"x"}"#,
    );
    // A folder stands for the settings.json it keeps.
    let tool = add_tool("Kit", &box_.path(".kit"), &accounts, &[]).unwrap();
    assert_eq!(tool.file(), box_.path(".kit/settings.json"));
    assert!(hook_agents::is_tag(&tool.id) && tool.id.len() == 36);
    assert_eq!(
        add_tool(
            "Kit 2",
            &tool.file(),
            &accounts,
            std::slice::from_ref(&tool)
        ),
        Err(ToolFailure::AlreadyListed)
    );
    agents::install_custom(&tool.file(), &tool.id, &env.exe, APP).unwrap();
    assert_eq!(
        agents::custom_state(&tool.file(), &tool.id, &env.exe, APP),
        Base::Installed,
        "the tool gets Claude Code's hooks"
    );
    let settings = installer::read(&tool.file()).unwrap();
    let mut events: Vec<&str> = settings["hooks"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    events.sort();
    let mut wanted = agents::CUSTOM_EVENTS.to_vec();
    wanted.sort();
    assert_eq!(events, wanted, "for every Claude Code event");
    assert_eq!(settings["model"], "x", "the person's settings kept");
    assert!(
        settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(&format!(
                "--agent custom --event UserPromptSubmit --via {}",
                tool.id
            ))
    );

    // Its sessions are listed, with the tool's name.
    let data = serde_json::to_vec(
        &json!({"openbar": "custom", "hook": "UserPromptSubmit", "pid": 3,
        "via": tool.id, "event": {"session_id": "k1", "cwd": "/w/kit", "prompt": "hi",
        "hook_event_name": "UserPromptSubmit"}}),
    )
    .unwrap();
    let now = 1_800_000_000.0;
    let mut tracker = hook_agents::Tracker::default();
    for event in hook_agents::decode(&data, now, ContentPolicy { previews: true })
        .unwrap()
        .events
    {
        tracker.ingest(event, now);
    }
    let (sessions, _) = tracker.snapshot(
        now,
        ContentPolicy { previews: true },
        |_| hook_agents::Place {
            host: HostKind::Unknown,
            takes_keys: false,
            accepts_replies: false,
            host_name: None,
        },
        |_| None,
    );
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].provider, Provider::Custom);
    assert_eq!(sessions[0].via.as_deref(), Some(tool.id.as_str()));
    assert_eq!(sessions[0].id, format!("custom:{}:k1", tool.id));
    assert!(sessions[0].phase.is_active());
    assert_eq!(sessions[0].chat.len(), 1);
    assert_eq!(sessions[0].chat[0].text, "hi");
    let labels = labels(&sessions, &[], std::slice::from_ref(&tool));
    assert_eq!(labels[&sessions[0].id], "Kit");
    assert_eq!(display_name(Provider::Custom), "Other tool");
    assert!(!Provider::HOOK_AGENTS.contains(&Provider::Custom));

    agents::remove_custom(&tool.file(), APP).unwrap();
    let left = installer::read(&tool.file()).unwrap();
    assert_eq!(
        agents::custom_state(&tool.file(), &tool.id, &env.exe, APP),
        Base::Missing
    );
    assert_eq!(
        left["hooks"]["Stop"].as_array().unwrap().len(),
        1,
        "Remove leaves the tool's own hooks"
    );
}

// MARK: Rows

#[test]
fn rows_on_disk() {
    let box_ = Sandbox::new("rows");
    let env = box_.env();
    box_.write(".claude/settings.json", r#"{"model":"opus"}"#);
    box_.folder(".gemini");
    let found = [Provider::Gemini, Provider::Codex];
    let ctx = || box_.context(&[], &found, &[], &[]);
    let targets = box_.list(&ctx());
    let claude = claude_row(&targets);
    assert_eq!(claude.state, Shown::Missing);
    assert!(claude.can_reinstall && !claude.can_remove);
    let codex = row(&targets, "agent:codex");
    assert_eq!(codex.state, skipped(Skip::NothingNeeded));
    assert!(codex.file.is_none() && !codex.can_reinstall);
    let gemini = row(&targets, "agent:gemini");
    assert_eq!(gemini.state, Shown::Missing);
    assert!(gemini.present);
    assert_eq!(
        row(&targets, "agent:cursor").state,
        skipped(Skip::AgentNotInstalled)
    );
    let off = box_.list(&box_.context(&[Provider::Gemini], &found, &[], &[]));
    assert_eq!(row(&off, "agent:gemini").state, skipped(Skip::SwitchedOff));

    // Reinstall, by hand edits, deletion, Remove.
    reinstall(claude, &env).unwrap();
    reinstall(gemini, &env).unwrap();
    let targets = box_.list(&ctx());
    assert_eq!(claude_row(&targets).state, Shown::Installed);
    assert_eq!(row(&targets, "agent:gemini").state, Shown::Installed);
    // Someone edits the island's command.
    let text = box_
        .read(".claude/settings.json")
        .unwrap()
        .replace("/opt/openbar/openbar", "/elsewhere/openbar");
    box_.write(".claude/settings.json", &text);
    assert_eq!(
        claude_row(&box_.list(&ctx())).state,
        Shown::ChangedByHand,
        "edited entries read as changed by hand"
    );
    // Someone deletes them.
    box_.write(".claude/settings.json", r#"{"model":"opus"}"#);
    let targets = box_.list(&ctx());
    let lost = claude_row(&targets);
    assert_eq!(lost.state, Shown::Missing);
    assert_eq!(lost.derived, Derived::RemovedByHand);
    reinstall(lost, &env).unwrap();
    let targets = box_.list(&ctx());
    assert_eq!(claude_row(&targets).state, Shown::Installed);
    take_out(claude_row(&targets), &env).unwrap();
    let targets = box_.list(&ctx());
    let removed = claude_row(&targets);
    assert_eq!(removed.state, skipped(Skip::RemovedByYou));
    assert!(removed.can_reinstall && !removed.can_remove);
    assert_eq!(
        installer::read(&box_.path(".claude/settings.json")).unwrap()["model"],
        "opus"
    );
    let copies: Vec<String> = box_
        .names(".claude")
        .into_iter()
        .filter(|n| n.starts_with("settings.json.openbar-"))
        .collect();
    assert!(
        copies.contains(&"settings.json.openbar-original.bak".to_owned()) && copies.len() >= 3,
        "every change kept a copy, and the file from before Openbar: {copies:?}"
    );
    assert_eq!(
        summary(&targets).0,
        1,
        "the summary counts the working rows"
    );
    // The page reads each row in camelCase.
    let json = serde_json::to_value(removed).unwrap();
    assert_eq!(json["provider"], "claude");
    assert_eq!(json["canReinstall"], true);
    assert_eq!(json["wholeFile"], false);
    assert_eq!(json["state"]["reason"], "removedByYou");
    assert!(json.get("place").is_none());
}

// MARK: Repairs

#[test]
fn putting_lost_hooks_back() {
    let record = Record {
        installed_at: "2026-11-18T00:00:00.000Z".into(),
        fingerprint: "f".into(),
    };
    let target = |id: &str, derived, state, place: Place| {
        Target::new(
            id.into(),
            Provider::Claude,
            place,
            id.into(),
            PathBuf::from("/x"),
            Some(PathBuf::from("/x/settings.json")),
            false,
            false,
            true,
            derived,
            state,
        )
    };
    let claude = || Place::Claude(PathBuf::from("/x/settings.json"));
    let rows = vec![
        target("lost", Derived::RemovedByHand, Shown::Missing, claude()),
        target("old", Derived::Outdated, Shown::Outdated, claude()),
        target(
            "edited",
            Derived::ChangedByHand,
            Shown::ChangedByHand,
            claude(),
        ),
        target("never", Derived::Missing, Shown::Missing, claude()),
        target(
            "out",
            Derived::RemovedByHand,
            skipped(Skip::RemovedByYou),
            claude(),
        ),
        target(
            "agent",
            Derived::RemovedByHand,
            Shown::Missing,
            Place::Agent(Provider::Gemini, None),
        ),
    ];
    let records: BTreeMap<String, Record> = ["lost", "old", "edited", "out", "agent"]
        .iter()
        .map(|id| (id.to_string(), record.clone()))
        .collect();
    let ids: Vec<&str> = repairs(&rows, &records, true)
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["lost", "old"],
        "only hooks the island put in and lost come back; edits and Remove are the person's"
    );
    assert!(
        repairs(&rows, &records, false).is_empty(),
        "and only when allowed"
    );

    let found = [
        Provider::Gemini,
        Provider::Cursor,
        Provider::Pi,
        Provider::Vibe,
    ];
    let states: HashMap<Provider, Derived> = [
        (Provider::Gemini, Derived::RemovedByHand),
        (Provider::Cursor, Derived::ChangedByHand),
        (Provider::Pi, Derived::Missing),
        (Provider::Vibe, Derived::Outdated),
    ]
    .into_iter()
    .collect();
    let state = |p: Provider| states.get(&p).copied().unwrap_or(Derived::Missing);
    let none = HashSet::new();
    let set = |list: Vec<Provider>| list.into_iter().collect::<HashSet<_>>();
    let (install, remove) = agents::plan_with(&found, &none, state, &none, true);
    assert_eq!(
        set(install),
        set(vec![Provider::Gemini, Provider::Pi, Provider::Vibe]),
        "agents' hooks come back by themselves, hand edits stay"
    );
    assert!(remove.is_empty());
    let pi: HashSet<Provider> = [Provider::Pi].into_iter().collect();
    let (install, _) = agents::plan_with(&found, &none, state, &pi, false);
    assert_eq!(
        install,
        [Provider::Vibe],
        "without putting back, and with Pi removed by the person"
    );
    let all: HashSet<Provider> = Provider::HOOK_AGENTS.into_iter().collect();
    let (_, remove) = agents::plan_with(&found, &all, state, &none, true);
    assert_eq!(
        set(remove),
        set(vec![Provider::Cursor, Provider::Vibe]),
        "a switch turned off takes the island's out"
    );
}

// MARK: Remove everything

#[test]
fn remove_everything_leaves_every_file_as_before() {
    let box_ = Sandbox::new("everything");
    let env = box_.env();
    let found = [
        Provider::Gemini,
        Provider::Copilot,
        Provider::Cursor,
        Provider::Vibe,
        Provider::Codex,
    ];
    // What the person had before Openbar, byte for byte, odd spacing and all.
    let claude = "{\n  \"model\" : \"opus\",\n  \"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"say hi\"}]}]}\n}\n";
    let gemini = "{ \"theme\": \"dark\" }";
    let vibe =
        "# mine\n[[hooks]]\nname = \"lint\"\ntype = \"post_agent\"\ncommand = \"eslint .\"\n";
    box_.write(".claude/settings.json", claude);
    box_.write(".gemini/settings.json", gemini);
    box_.write(".vibe/hooks.toml", vibe);
    box_.folder(".copilot");
    box_.folder(".cursor");
    let codex_home = box_.folder("codex-2");
    let folders = vec![FolderSetting {
        agent: "codex".into(),
        path: codex_home.to_string_lossy().into_owned(),
        name: None,
    }];
    box_.write(".kit/settings.json", "{}");
    let tool = Tool {
        id: new_id(),
        name: "Kit".into(),
        path: box_
            .path(".kit/settings.json")
            .to_string_lossy()
            .into_owned(),
    };
    let tools = vec![tool.clone()];
    let ctx = || box_.context(&[], &found, &folders, &tools);
    for target in box_.list(&ctx()) {
        if target.state == Shown::Missing {
            reinstall(&target, &env).unwrap();
        }
    }
    let installed = box_.list(&ctx());
    let on: HashSet<String> = installed
        .iter()
        .filter(|t| t.state == Shown::Installed || t.state == skipped(Skip::NeedsApproval))
        .map(|t| t.id.clone())
        .collect();
    for id in [
        hook_records::claude_id(&box_.path(".claude/settings.json")),
        "agent:gemini".into(),
        "agent:copilot".into(),
        "agent:cursor".into(),
        "agent:vibe".into(),
        folders[0].target_id(),
        tool.target_id(),
    ] {
        assert!(on.contains(&id), "{id} is in first: {on:?}");
    }
    assert!(
        box_.read(&format!(".copilot/hooks/openbar-{APP}.json"))
            .is_some()
    );
    assert!(box_.read(".cursor/hooks.json").is_some());
    assert!(box_.read("codex-2/hooks.json").is_some());

    let failures = remove_everything(&installed, &env);
    assert!(failures.is_empty(), "nothing failed: {failures:?}");
    assert_eq!(box_.read(".claude/settings.json").unwrap(), claude);
    assert_eq!(box_.read(".gemini/settings.json").unwrap(), gemini);
    assert_eq!(box_.read(".vibe/hooks.toml").unwrap(), vibe);
    assert_eq!(box_.read(".kit/settings.json").unwrap(), "{}");
    // Files and folders the island made are gone.
    assert!(box_.read(".cursor/hooks.json").is_none());
    assert!(box_.read("codex-2/hooks.json").is_none());
    assert!(
        !box_.path(".copilot/hooks").exists(),
        "{:?}",
        box_.names(".copilot/hooks")
    );
    // Copies of what was there stay; the notes from before Openbar go.
    let copies: Vec<String> = box_
        .names(".claude")
        .into_iter()
        .filter(|n| n.starts_with("settings.json.openbar-") && n.ends_with(".bak"))
        .collect();
    assert!(!copies.is_empty() && !copies.iter().any(|n| n.contains("original")));
    assert!(box_.names(".cursor").iter().any(|n| n.ends_with(".bak")));
    assert!(!box_.names(".cursor").iter().any(|n| n.contains("original")));
    let after = box_.list(&ctx());
    assert!(after.iter().all(|t| !t.can_remove), "nothing is left");
    assert!(
        repairs(&after, &hook_records::all(), true).is_empty(),
        "and nothing comes back by itself"
    );
    let agents_out = opted_out_agents(&opt_out());
    for p in [
        Provider::Gemini,
        Provider::Copilot,
        Provider::Cursor,
        Provider::Vibe,
    ] {
        assert!(
            agents_out.contains(&p),
            "the agents' switches don't put the hooks back either"
        );
    }
    assert_eq!(
        claude_row(&after).state,
        skipped(Skip::RemovedByYou),
        "a row that held hooks reads as removed by you"
    );
}

#[test]
fn remove_everything_keeps_later_changes_and_marks_only_rows_with_hooks() {
    // A file the person changed since keeps the change.
    let other = Sandbox::new("later");
    let env = other.env();
    other.write(".claude/settings.json", r#"{"model":"opus"}"#);
    let rows = other.list(&other.context(&[], &[], &[], &[]));
    reinstall(claude_row(&rows), &env).unwrap();
    let file = other.path(".claude/settings.json");
    let mut edited = installer::read(&file).unwrap();
    edited["theme"] = json!("light");
    std::fs::write(&file, serde_json::to_vec(&edited).unwrap()).unwrap();
    remove_everything(&other.list(&other.context(&[], &[], &[], &[])), &env);
    let kept = installer::read(&file).unwrap();
    assert_eq!(kept["theme"], "light");
    assert_eq!(kept["model"], "opus");
    assert!(
        kept.get("hooks").is_none(),
        "a later change of the person's stays"
    );
    drop(other);

    // A row that never had hooks isn't marked as removed.
    let never = Sandbox::new("never");
    never.write(".claude/settings.json", "{}");
    remove_everything(
        &never.list(&never.context(&[], &[], &[], &[])),
        &never.env(),
    );
    assert_eq!(
        claude_row(&never.list(&never.context(&[], &[], &[], &[]))).state,
        Shown::Missing
    );
}

#[test]
fn leftovers_and_same_content() {
    assert!(is_leftover(b"  \n"));
    assert!(is_leftover(br#"{"version":1,"hooks":{}}"#));
    assert!(!is_leftover(br#"{"version":2}"#));
    assert!(!is_leftover(br#"{"theme":"dark"}"#));
    assert!(same_content(
        br#"{"a":1,"hooks":{},"version":1}"#,
        br#"{ "a" : 1 }"#
    ));
    assert!(!same_content(br#"{"a":2}"#, br#"{"a":1}"#));
    assert!(same_content(b"x = 1\n\n", b"x = 1\n"));
}
