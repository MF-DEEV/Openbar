// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Several Claude Code accounts, one per config folder (issue #216;
//! ClaudeAccounts.swift). `%USERPROFILE%\.claude` and every folder a
//! `CLAUDE_CONFIG_DIR` points at keep their own sign-in, settings and
//! sessions. The person can name each one, hide one, and add a folder the
//! app did not find; those choices live in the app's own prefs
//! (`/sessions/claudeAccounts`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

use super::models::{Provider, Session};

pub const DEFAULT_NAME: &str = "Default";
const MAXIMUM: usize = 16;
const LONGEST_NAME: usize = 32;

/// What the person changed about one account.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Setting {
    pub path: String,
    pub name: Option<String>,
    pub hidden: bool,
    pub manual: bool,
}

/// One account as Settings and the island show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// The folder's identity (`canonical`), which sessions are matched by.
    pub id: String,
    pub path: String,
    pub name: String,
    pub named: bool,
    pub manual: bool,
    pub hidden: bool,
}

impl Account {
    pub fn settings_path(&self) -> PathBuf {
        PathBuf::from(&self.path).join("settings.json")
    }
}

/// A folder's identity: through links, without case or a trailing separator.
pub fn canonical(folder: &Path) -> String {
    super::registry::canonical_home(folder)
}

/// The choices kept in prefs; anything malformed is left out.
pub fn settings_from_prefs(value: &Value) -> Vec<Setting> {
    let mut seen = HashSet::new();
    value
        .pointer("/sessions/claudeAccounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?.trim();
            if path.is_empty() || !Path::new(path).is_absolute() {
                return None;
            }
            if !seen.insert(canonical(Path::new(path))) {
                return None;
            }
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .map(|n| n.trim().chars().take(LONGEST_NAME).collect::<String>())
                .filter(|n| !n.is_empty());
            Some(Setting {
                path: path.to_string(),
                name,
                hidden: entry.get("hidden").and_then(Value::as_bool) == Some(true),
                manual: entry.get("manual").and_then(Value::as_bool) == Some(true),
            })
        })
        .take(MAXIMUM)
        .collect()
}

static SETTINGS: Mutex<Vec<Setting>> = Mutex::new(Vec::new());

/// Prefs changed: the island and the hooks follow the new choices.
pub fn set_settings(settings: Vec<Setting>) {
    if let Ok(mut current) = SETTINGS.lock() {
        *current = settings;
    }
}

pub fn settings() -> Vec<Setting> {
    SETTINGS.lock().map(|s| s.clone()).unwrap_or_default()
}

/// The folders added by hand that exist, which the session list reads too.
pub fn manual_folders() -> Vec<PathBuf> {
    settings()
        .into_iter()
        .filter(|s| s.manual)
        .map(|s| PathBuf::from(s.path))
        .filter(|p| p.is_dir())
        .collect()
}

/// `~\.claude` is the default; `~\.claude-work` is Work; any other folder
/// goes by its own name.
pub fn name_for(folder: &Path, home: &Path) -> String {
    if canonical(folder) == canonical(&home.join(".claude")) {
        return DEFAULT_NAME.to_string();
    }
    let last = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = folder
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    if parent.as_deref() == Some(".config") && last == "claude" {
        return "Config".to_string();
    }
    let mut base = last.strip_prefix(".claude").unwrap_or(&last).to_string();
    base = base.trim_start_matches(['-', '_', '.']).to_string();
    if base.is_empty() {
        base = last.trim_start_matches('.').to_string();
    }
    if base.is_empty() {
        return DEFAULT_NAME.to_string();
    }
    let mut chars = base.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    (first + chars.as_str()).chars().take(LONGEST_NAME).collect()
}

/// Every account, hidden ones too: each config folder the session list reads
/// that exists, then those added by hand.
pub fn all(found: &[PathBuf], settings: &[Setting], home: &Path) -> Vec<Account> {
    let mut seen = HashSet::new();
    let manual: Vec<PathBuf> = settings
        .iter()
        .filter(|s| s.manual)
        .map(|s| PathBuf::from(&s.path))
        .collect();
    found
        .iter()
        .chain(manual.iter())
        .filter(|folder| folder.is_dir())
        .filter_map(|folder| {
            let id = canonical(folder);
            if !seen.insert(id.clone()) {
                return None;
            }
            let setting = settings.iter().find(|s| canonical(Path::new(&s.path)) == id);
            Some(Account {
                id,
                path: folder.to_string_lossy().into_owned(),
                name: setting
                    .and_then(|s| s.name.clone())
                    .unwrap_or_else(|| name_for(folder, home)),
                named: setting.is_some_and(|s| s.name.is_some()),
                manual: setting.is_some_and(|s| s.manual),
                hidden: setting.is_some_and(|s| s.hidden),
            })
        })
        .collect()
}

/// The accounts on this PC, as the person set them.
pub fn current(home: &Path) -> Vec<Account> {
    all(&super::registry::homes(home), &settings(), home)
}

/// The account of a config folder, by its identity.
pub fn find<'a>(folder: &str, accounts: &'a [Account]) -> Option<&'a Account> {
    let id = canonical(Path::new(folder));
    accounts.iter().find(|a| a.id == id)
}

/// The accounts the listed sessions run under, by id, each once: only
/// Claude sessions on this PC.
pub fn in_use(sessions: &[Session]) -> Vec<String> {
    let mut seen = HashSet::new();
    sessions
        .iter()
        .filter(|s| s.provider == Provider::Claude && s.remote.is_none())
        .filter_map(|s| s.account.as_deref())
        .map(|a| canonical(Path::new(a)))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// A label for each session, by session id, only while sessions of more
/// than one account are listed: with one, nothing changes.
pub fn labels(sessions: &[Session], accounts: &[Account], home: &Path) -> HashMap<String, String> {
    if in_use(sessions).len() < 2 {
        return HashMap::new();
    }
    sessions
        .iter()
        .filter(|s| s.provider == Provider::Claude && s.remote.is_none())
        .filter_map(|s| {
            let folder = s.account.as_deref()?;
            let name = find(folder, accounts)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| name_for(Path::new(folder), home));
            Some((s.id.clone(), name))
        })
        .collect()
}

// MARK: In use

/// The other accounts' folders the limits read beside the main one, by id,
/// as the sessions service last listed them. Hidden accounts are left out.
static IN_USE: Mutex<BTreeMap<String, PathBuf>> = Mutex::new(BTreeMap::new());

pub fn note_in_use(sessions: &[Session], accounts: &[Account]) {
    let hidden: HashSet<&str> = accounts.iter().filter(|a| a.hidden).map(|a| a.id.as_str()).collect();
    let mut found = BTreeMap::new();
    for session in sessions
        .iter()
        .filter(|s| s.provider == Provider::Claude && s.remote.is_none())
    {
        let Some(folder) = session.account.as_deref() else {
            continue;
        };
        let id = canonical(Path::new(folder));
        if !hidden.contains(id.as_str()) {
            found.entry(id).or_insert_with(|| PathBuf::from(folder));
        }
    }
    if let Ok(mut current) = IN_USE.lock() {
        *current = found;
    }
}

/// The Claude accounts in use by their folder, by id.
pub fn in_use_folders() -> BTreeMap<String, PathBuf> {
    IN_USE.lock().map(|m| m.clone()).unwrap_or_default()
}

/// The display name of an account by id, for the limits cards.
pub fn display_name(id: &str, home: &Path) -> String {
    let accounts = current(home);
    accounts
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| {
            in_use_folders()
                .get(id)
                .map(|f| name_for(f, home))
                .unwrap_or_else(|| name_for(Path::new(id), home))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vs-accounts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefs_are_checked() {
        let value = json!({"sessions": {"claudeAccounts": [
            {"path": "/u/.claude-work", "name": "  Work  "},
            {"path": "/u/.claude-work/", "name": "Twice"},
            {"path": "relative", "name": "Nope"},
            {"path": "/u/x", "name": "n".repeat(80), "hidden": true, "manual": true},
            "garbage"
        ]}});
        let settings = settings_from_prefs(&value);
        assert_eq!(settings.len(), 2, "one per folder, absolute paths only: {settings:?}");
        assert_eq!(settings[0].name.as_deref(), Some("Work"), "names are trimmed");
        assert!(settings[1].hidden && settings[1].manual);
        assert_eq!(settings[1].name.as_ref().map(|n| n.chars().count()), Some(LONGEST_NAME));
        assert!(settings_from_prefs(&json!({})).is_empty());
    }

    #[test]
    fn names_come_from_folders() {
        let home = Path::new("/u");
        assert_eq!(name_for(&home.join(".claude"), home), DEFAULT_NAME);
        assert_eq!(name_for(&home.join(".claude-work"), home), "Work");
        assert_eq!(name_for(&home.join(".claude-2"), home), "2");
        assert_eq!(name_for(&home.join(".config").join("claude"), home), "Config");
        assert_eq!(name_for(Path::new("/v/claude-personal"), home), "Claude-personal");
    }

    #[test]
    fn accounts_found_and_set() {
        let home = temp("found");
        for folder in [".claude", ".claude-work/sessions", ".claude-old"] {
            std::fs::create_dir_all(home.join(folder)).unwrap();
        }
        let elsewhere = home.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let settings = vec![
            Setting {
                path: home.join(".claude-work").to_string_lossy().into_owned(),
                name: Some("Job".into()),
                hidden: true,
                manual: false,
            },
            Setting {
                path: elsewhere.to_string_lossy().into_owned(),
                manual: true,
                ..Setting::default()
            },
        ];
        let found = super::super::registry::homes(&home);
        let accounts = all(&found, &settings, &home);
        let names: Vec<&str> = accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            [DEFAULT_NAME, "Job", "Elsewhere"],
            "~/.claude, folders with a registry, then those added by hand; a missing standard folder is none"
        );
        assert!(accounts[1].hidden && accounts[1].named);
        assert!(accounts[2].manual);
        let _ = std::fs::remove_dir_all(&home);
    }

    fn session(id: &str, account: Option<&str>, provider: Provider, remote: Option<&str>) -> Session {
        let mut s = super::super::sample::waiting(0, "/u", 0.0);
        s.id = id.to_string();
        s.provider = provider;
        s.account = account.map(str::to_string);
        s.remote = remote.map(str::to_string);
        s
    }

    #[test]
    fn labels_only_with_two_accounts() {
        let home = Path::new("/u");
        let accounts = vec![Account {
            id: canonical(Path::new("/u/.claude")),
            path: "/u/.claude".into(),
            name: "Personal".into(),
            named: true,
            manual: false,
            hidden: false,
        }];
        let one = vec![
            session("a", Some("/u/.claude"), Provider::Claude, None),
            session("b", Some("/u/.claude"), Provider::Claude, None),
            session("c", None, Provider::Codex, None),
            session("r", Some("/home/dev/.claude-x"), Provider::Claude, Some("box")),
        ];
        assert!(labels(&one, &accounts, home).is_empty(), "one account here, a host's folder is not another");
        let mut two = one.clone();
        two.push(session("w", Some("/u/.claude-work"), Provider::Claude, None));
        let found = labels(&two, &accounts, home);
        assert_eq!(found.get("a").map(String::as_str), Some("Personal"));
        assert_eq!(found.get("w").map(String::as_str), Some("Work"));
        assert!(!found.contains_key("c") && !found.contains_key("r"));
        assert_eq!(in_use(&two).len(), 2);
    }
}
