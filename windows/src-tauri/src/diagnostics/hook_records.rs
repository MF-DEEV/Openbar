// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! What Openbar last wrote into each agent's hook settings, kept so a
//! diagnostics file (#230) can say more than "outdated": hooks an older
//! Openbar wrote, hooks someone changed by hand, and hooks someone took out
//! by hand. Only a fingerprint and a date are kept, in
//! `hook-records.json` beside the settings. As the Mac app's `HookRecords.swift`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::sessions::hook_agents::installer::{self as agents, State as Base};
use crate::sessions::models::Provider;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub installed_at: String,
    pub fingerprint: String,
}

/// A hook's state as a diagnostics file shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Installed,
    /// Openbar's own hooks from an older version, or pointing elsewhere.
    Outdated,
    /// Openbar's hooks are there, but not as Openbar wrote them.
    ChangedByHand,
    /// Openbar put hooks in and never took them out, and they are gone.
    RemovedByHand,
    Missing,
    Unreadable,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Installed => "installed",
            State::Outdated => "outdated",
            State::ChangedByHand => "changedByHand",
            State::RemovedByHand => "removedByHand",
            State::Missing => "missing",
            State::Unreadable => "unreadable",
        }
    }
}

static FILE: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

/// Where the records are kept; set once at start.
pub fn init(config_dir: &Path) {
    let _ = FILE.set(config_dir.join("hook-records.json"));
}

pub fn agent_id(provider: Provider) -> String {
    format!("agent:{}", provider.key())
}

/// The folder is resolved, not the file: a settings file deleted by hand
/// must still name the record Openbar made for it.
pub fn claude_id(settings: &Path) -> String {
    let path = match (settings.parent(), settings.file_name()) {
        (Some(folder), Some(name)) => std::fs::canonicalize(folder)
            .map(|f| f.join(name))
            .unwrap_or_else(|_| settings.to_path_buf()),
        _ => settings.to_path_buf(),
    };
    format!("claude:{}", path.to_string_lossy())
}

// MARK: Store

pub fn all() -> BTreeMap<String, Record> {
    FILE.get().map(|f| all_in(f)).unwrap_or_default()
}

pub fn all_in(file: &Path) -> BTreeMap<String, Record> {
    std::fs::read(file)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

/// Notes what Openbar just wrote into `config`.
pub fn note_installed(id: &str, config: &Path, marker: &str, own_name: Option<&str>, whole: bool) {
    if let Some(file) = FILE.get() {
        note_installed_in(file, id, config, marker, own_name, whole);
    }
}

pub fn note_installed_in(
    file: &Path,
    id: &str,
    config: &Path,
    marker: &str,
    own_name: Option<&str>,
    whole: bool,
) {
    let print = fingerprint(
        std::fs::read(config).ok().as_deref(),
        marker,
        own_name,
        whole,
    );
    update(file, |records| match print {
        Some(print) => {
            records.insert(
                id.to_string(),
                Record {
                    installed_at: super::log::timestamp(chrono::Utc::now()),
                    fingerprint: print,
                },
            );
        }
        None => {
            records.remove(id);
        }
    });
}

/// Openbar took its hooks out itself: nothing to miss any more.
pub fn forget(id: &str) {
    if let Some(file) = FILE.get() {
        forget_in(file, id);
    }
}

pub fn forget_in(file: &Path, id: &str) {
    update(file, |records| {
        records.remove(id);
    });
}

fn update(file: &Path, change: impl FnOnce(&mut BTreeMap<String, Record>)) {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut records = all_in(file);
    change(&mut records);
    if records.is_empty() {
        let _ = std::fs::remove_file(file);
        return;
    }
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&records) {
        let partial = file.with_extension("json.partial");
        if std::fs::write(&partial, data).is_ok() {
            let _ = std::fs::rename(&partial, file);
        }
    }
}

// MARK: Reading

/// What the state the settings show becomes once the record is weighed.
pub fn derive(base: Base, record: Option<&Record>, current: Option<&str>) -> State {
    match base {
        Base::Installed => State::Installed,
        Base::Unreadable => State::Unreadable,
        Base::Missing if record.is_some() => State::RemovedByHand,
        Base::Missing => State::Missing,
        // Hooks from before records were kept can't be told apart.
        Base::Outdated => match record {
            None => State::Outdated,
            Some(r) if current == Some(r.fingerprint.as_str()) => State::Outdated,
            Some(_) => State::ChangedByHand,
        },
    }
}

/// A fingerprint of Openbar's part of a settings file: the whole file when
/// the file is Openbar's own (a Copilot hook file, an OpenCode or Pi plugin),
/// otherwise every string that carries Openbar's marker, plus the entry under
/// Openbar's own name (Antigravity) and the lines of a marked block (Vibe's
/// hooks.toml). None when there is no file.
pub fn fingerprint(
    data: Option<&[u8]>,
    marker: &str,
    own_name: Option<&str>,
    whole: bool,
) -> Option<String> {
    let data = data?;
    if whole {
        return Some(hex(&Sha256::digest(data)));
    }
    let mut parts: Vec<String> = Vec::new();
    if let Ok(json) = serde_json::from_slice::<Value>(data) {
        collect(&json, marker, &mut parts);
        if let (Some(own), Some(object)) = (own_name, json.as_object())
            && let Some(entry) = object.get(own)
        {
            let canonical = crate::hooks::installer::sorted(entry);
            parts.push(format!(
                "{own}={}",
                serde_json::to_string(&canonical).unwrap_or_default()
            ));
        }
    } else if let Ok(text) = std::str::from_utf8(data) {
        let start = format!("{marker} start");
        let end = format!("{marker} end");
        let mut inside = false;
        for line in text.split('\n') {
            let marked = line.contains(marker);
            if marked && line.contains(&start) {
                inside = true;
            }
            if inside || marked {
                parts.push(line.to_string());
            }
            if marked && line.ends_with(&end) {
                inside = false;
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    Some(hex(&Sha256::digest(parts.join("\n").as_bytes())))
}

fn collect(value: &Value, marker: &str, parts: &mut Vec<String>) {
    match value {
        Value::String(text) if text.contains(marker) => parts.push(text.clone()),
        Value::Array(list) => list.iter().for_each(|v| collect(v, marker, parts)),
        Value::Object(map) => map.values().for_each(|v| collect(v, marker, parts)),
        _ => {}
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// MARK: Agents

/// The file that holds an agent's hooks, and whether the whole file is
/// Openbar's own.
pub fn hook_file(provider: Provider, home: &Path, app: &str) -> Option<(PathBuf, bool)> {
    Some(match provider {
        Provider::Gemini => (agents::gemini_settings(home), false),
        Provider::Cursor => (agents::cursor_hooks(home), false),
        Provider::Copilot => (agents::copilot_hooks(home, app), true),
        Provider::Opencode => (agents::opencode_plugin(home, app), true),
        Provider::Pi => (agents::pi_extension(home, app), true),
        Provider::Vibe => (agents::vibe_hooks(home), false),
        Provider::Devin => (agents::devin_config(home), false),
        Provider::Antigravity => (agents::antigravity_hooks(home), false),
        Provider::Claude | Provider::Codex => return None,
    })
}

fn own_name(provider: Provider, app: &str) -> Option<String> {
    (provider == Provider::Antigravity).then(|| agents::own_name(app))
}

/// Records what the installer just wrote into an agent.
pub fn note_agent(provider: Provider, home: &Path, app: &str) {
    if let Some((file, whole)) = hook_file(provider, home, app) {
        let marker = crate::hooks::installer::marker(app);
        note_installed(
            &agent_id(provider),
            &file,
            &marker,
            own_name(provider, app).as_deref(),
            whole,
        );
    }
}

/// An agent's hooks as a diagnostics file shows them.
pub fn agent_state(
    provider: Provider,
    base: Base,
    home: &Path,
    app: &str,
    records: &BTreeMap<String, Record>,
) -> State {
    let current = hook_file(provider, home, app).and_then(|(file, whole)| {
        fingerprint(
            std::fs::read(&file).ok().as_deref(),
            &crate::hooks::installer::marker(app),
            own_name(provider, app).as_deref(),
            whole,
        )
    });
    derive(base, records.get(&agent_id(provider)), current.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record(print: &str) -> Record {
        Record {
            installed_at: "2026-11-16T00:00:00.000Z".into(),
            fingerprint: print.into(),
        }
    }

    #[test]
    fn states_from_records() {
        let r = record("abc");
        assert_eq!(derive(Base::Installed, None, None), State::Installed);
        assert_eq!(derive(Base::Missing, None, None), State::Missing);
        assert_eq!(derive(Base::Missing, Some(&r), None), State::RemovedByHand);
        assert_eq!(
            derive(Base::Outdated, Some(&r), Some("abc")),
            State::Outdated
        );
        assert_eq!(
            derive(Base::Outdated, Some(&r), Some("xyz")),
            State::ChangedByHand
        );
        assert_eq!(derive(Base::Outdated, None, Some("xyz")), State::Outdated);
        assert_eq!(derive(Base::Unreadable, Some(&r), None), State::Unreadable);
    }

    #[test]
    fn fingerprints_cover_only_openbars_part() {
        let marker = "--marker openbar-hook --app com.openbar.island";
        let ours = json!({"hooks": {"SessionStart": [{"hooks": [{"command": format!("x --hook {marker}")}]}]}});
        let mut theirs = ours.clone();
        theirs["theme"] = json!("dark");
        let first = fingerprint(Some(ours.to_string().as_bytes()), marker, None, false);
        assert!(first.is_some());
        assert_eq!(
            first,
            fingerprint(Some(theirs.to_string().as_bytes()), marker, None, false)
        );
        let changed = json!({"hooks": {"SessionStart": [{"hooks": [{"command": format!("y --hook {marker}")}]}]}});
        assert_ne!(
            first,
            fingerprint(Some(changed.to_string().as_bytes()), marker, None, false)
        );
        assert_eq!(
            fingerprint(Some(b"{\"theme\":1}"), marker, None, false),
            None
        );
        assert_eq!(fingerprint(None, marker, None, false), None);
        assert_ne!(
            fingerprint(Some(b"x"), marker, None, true),
            fingerprint(Some(b"y"), marker, None, true)
        );
        let toml = format!(
            "[[hooks]]\nname = \"mine\"\n\n# {marker} start: added\n[[hooks]]\ntimeout = 5.0\n# {marker} end\n"
        );
        let edited = toml.replace("timeout = 5.0", "timeout = 9.0");
        assert_ne!(
            fingerprint(Some(toml.as_bytes()), marker, None, false),
            fingerprint(Some(edited.as_bytes()), marker, None, false)
        );
        assert_eq!(
            fingerprint(Some(toml.as_bytes()), marker, None, false),
            fingerprint(Some(format!("[x]\n{toml}").as_bytes()), marker, None, false)
        );
    }

    #[test]
    fn records_are_kept_and_forgotten() {
        let folder = std::env::temp_dir().join(format!("openbar-records-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let store = folder.join("hook-records.json");
        let config = folder.join("settings.json");
        let marker = "--marker openbar-hook --app test";
        std::fs::write(
            &config,
            json!({"hooks": {"A": [{"hooks": [{"command": format!("z {marker}")}]}]}}).to_string(),
        )
        .unwrap();
        note_installed_in(&store, "agent:gemini", &config, marker, None, false);
        assert!(all_in(&store).contains_key("agent:gemini"));
        forget_in(&store, "agent:gemini");
        assert!(all_in(&store).is_empty() && !store.exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A settings file deleted by hand still names the same record.
    #[test]
    fn claude_ids_survive_a_deleted_file() {
        let folder = std::env::temp_dir().join(format!("openbar-claude-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("settings.json");
        std::fs::write(&file, "{}").unwrap();
        let before = claude_id(&file);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(claude_id(&file), before);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Gemini CLI's hooks put in by the installer, then changed and taken out by hand.
    #[test]
    fn a_real_install_then_hand_edits() {
        let home = std::env::temp_dir().join(format!("openbar-diag-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        let store = home.join("hook-records.json");
        let exe = Path::new("/opt/openbar/openbar");
        let app = "com.openbar.test";
        agents::install(Provider::Gemini, &home, exe, "", app).unwrap();
        let (file, whole) = hook_file(Provider::Gemini, &home, app).unwrap();
        let marker = crate::hooks::installer::marker(app);
        note_installed_in(
            &store,
            &agent_id(Provider::Gemini),
            &file,
            &marker,
            None,
            whole,
        );
        let state = |exe: &Path| {
            let base = agents::state(Provider::Gemini, &home, exe, "", app);
            agent_state(Provider::Gemini, base, &home, app, &all_in(&store))
        };
        assert_eq!(state(exe), State::Installed);
        assert_eq!(state(Path::new("/opt/newer/openbar")), State::Outdated);
        let text = std::fs::read_to_string(&file)
            .unwrap()
            .replace("/opt/openbar/openbar", "/elsewhere/openbar");
        std::fs::write(&file, text).unwrap();
        assert_eq!(state(exe), State::ChangedByHand);
        std::fs::write(&file, "{\"theme\":\"dark\"}").unwrap();
        assert_eq!(state(exe), State::RemovedByHand);
        forget_in(&store, &agent_id(Provider::Gemini));
        assert_eq!(state(exe), State::Missing);
        let _ = std::fs::remove_dir_all(&home);
    }
}
