// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The Hooks page (#236; HookTargets.swift, HookMaintenance.swift): every
//! config file the island writes its hooks into, what each holds now and
//! why one is left alone, Reinstall and Remove with a copy of the file kept
//! first, config folders and tools added by hand, Codex's hook review,
//! putting lost hooks back, and Remove everything leaving each file as it
//! was before Openbar.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::installer;
use crate::diagnostics::hook_records::{self, Record, State as Derived};
use crate::sessions::accounts::{self, Account};
use crate::sessions::hook_agents::{self, installer as agents, installer::State as Base};
use crate::sessions::models::{Provider, Session};

/// How many folders an agent keeps, and how many tools.
pub const FOLDERS_PER_AGENT: usize = 8;
pub const TOOLS: usize = 8;
pub const LONGEST_NAME: usize = 32;

// MARK: Machine state

/// What the page keeps of this machine, beside the hook records: the rows
/// the person pressed Remove on, and the folders the island made for a hook
/// file. Never in prefs, which a backup carries to another machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Machine {
    pub opt_out: Vec<String>,
    pub made_folders: Vec<String>,
}

static STORE: OnceLock<PathBuf> = OnceLock::new();
static STORE_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// A test's own store, so tests running side by side never share one.
    static TEST_STORE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Where the machine state is kept; set once at start, beside the records.
pub fn init(config_dir: &Path) {
    let _ = STORE.set(config_dir.join("hooks-page.json"));
}

#[cfg(test)]
pub fn set_test_store(path: Option<PathBuf>) {
    TEST_STORE.with(|s| *s.borrow_mut() = path);
}

fn store() -> Option<PathBuf> {
    TEST_STORE
        .with(|s| s.borrow().clone())
        .or_else(|| STORE.get().cloned())
}

pub fn machine() -> Machine {
    store()
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default()
}

fn change_machine(change: impl FnOnce(&mut Machine)) {
    let Some(file) = store() else {
        return;
    };
    let _one = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state: Machine = std::fs::read(&file)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();
    change(&mut state);
    if state == Machine::default() {
        let _ = std::fs::remove_file(&file);
        return;
    }
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&state) {
        let partial = file.with_extension("json.partial");
        if std::fs::write(&partial, data).is_ok() {
            let _ = std::fs::rename(&partial, &file);
        }
    }
}

pub fn opt_out() -> HashSet<String> {
    machine().opt_out.into_iter().collect()
}

pub fn set_opt_out(id: &str, out: bool) {
    change_machine(|m| {
        m.opt_out.retain(|i| i != id);
        if out {
            m.opt_out.push(id.to_owned());
            m.opt_out.sort();
        }
    });
}

/// The hook agents whose own row the person took the hooks out of, which
/// keeping hooks in step leaves alone.
pub fn opted_out_agents(ids: &HashSet<String>) -> HashSet<Provider> {
    Provider::HOOK_AGENTS
        .into_iter()
        .filter(|p| ids.contains(&hook_records::agent_id(*p)))
        .collect()
}

/// Notes each folder on the way to `folder` that does not exist yet, so
/// Remove everything can take it away again once empty.
pub fn note_made(folder: &Path) {
    let mut missing = Vec::new();
    let mut current = Some(folder);
    while let Some(path) = current {
        if path.as_os_str().is_empty() || path.exists() {
            break;
        }
        missing.push(path.to_string_lossy().into_owned());
        current = path.parent();
    }
    if missing.is_empty() {
        return;
    }
    change_machine(|m| {
        for path in missing {
            if !m.made_folders.contains(&path) {
                m.made_folders.push(path);
            }
        }
    });
}

fn made_folders() -> Vec<String> {
    machine().made_folders
}

/// Takes away each folder the island made that is empty now, deepest
/// first; a folder that holds anything stays.
pub fn remove_empty() {
    let mut folders = made_folders();
    folders.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let mut kept = Vec::new();
    for path in folders {
        let folder = Path::new(&path);
        if !folder.exists() {
            continue;
        }
        let empty = std::fs::read_dir(folder)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .all(|e| e.file_name() == ".DS_Store")
            })
            .unwrap_or(false);
        if empty && std::fs::remove_dir_all(folder).is_ok() {
            continue;
        }
        kept.push(path);
    }
    change_machine(|m| m.made_folders = kept);
}

// MARK: Prefs

/// A config folder the person added for an agent: a second Codex home, a
/// second Gemini folder. Kept in prefs under `/sessions/agentFolders`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderSetting {
    pub agent: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl FolderSetting {
    pub fn provider(&self) -> Option<Provider> {
        Provider::from_key(&self.agent)
    }

    pub fn folder(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn target_id(&self) -> String {
        format!(
            "agent:{}:{}",
            self.agent,
            accounts::canonical(Path::new(&self.path))
        )
    }
}

/// A command-line tool that runs hooks the way Claude Code does, added by
/// name and settings file. Kept in prefs under `/sessions/customHookTools`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tool {
    /// A UUID, which the tool's hooks carry.
    pub id: String,
    pub name: String,
    /// The settings file its hooks go into.
    pub path: String,
}

impl Tool {
    pub fn file(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn target_id(&self) -> String {
        format!("custom:{}", self.id)
    }
}

/// What the page reads from prefs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub folders: Vec<FolderSetting>,
    pub tools: Vec<Tool>,
    /// "Put lost hooks back", `/hooks/autoRepair`, on unless turned off.
    pub auto_repair: bool,
    /// The hook agents whose switch is off.
    pub hidden: HashSet<Provider>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            folders: Vec::new(),
            tools: Vec::new(),
            auto_repair: true,
            hidden: HashSet::new(),
        }
    }
}

pub fn settings_from_prefs(value: &Value) -> Settings {
    let mut seen = HashSet::new();
    let mut per_agent: HashMap<String, usize> = HashMap::new();
    let folders = value
        .pointer("/sessions/agentFolders")
        .and_then(|v| serde_json::from_value::<Vec<Value>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| serde_json::from_value::<FolderSetting>(v).ok())
        .filter(|s| {
            s.provider()
                .is_some_and(|p| agents::FOLDERS_BY_HAND.contains(&p))
                && Path::new(&s.path).is_absolute()
                && seen.insert(s.target_id())
        })
        .filter(|s| {
            let count = per_agent.entry(s.agent.clone()).or_default();
            *count += 1;
            *count <= FOLDERS_PER_AGENT
        })
        .collect();
    let mut ids = HashSet::new();
    let tools = value
        .pointer("/sessions/customHookTools")
        .and_then(|v| serde_json::from_value::<Vec<Value>>(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Tool>(v).ok())
        .filter(|t| {
            hook_agents::is_tag(&t.id)
                && Path::new(&t.path).is_absolute()
                && !t.name.trim().is_empty()
                && ids.insert(t.id.clone())
        })
        .take(TOOLS)
        .collect();
    Settings {
        folders,
        tools,
        auto_repair: value
            .pointer("/hooks/autoRepair")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        hidden: crate::prefs::hidden_agents(value),
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

/// Prefs changed: the page and putting lost hooks back follow them.
pub fn set_settings(settings: Settings) {
    if let Ok(mut current) = SETTINGS.lock() {
        *current = Some(settings);
    }
}

pub fn settings() -> Settings {
    SETTINGS
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or_default()
}

// MARK: Adding folders and tools

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderFailure {
    NotFolder,
    AlreadyListed,
    TooMany,
    /// The home folder itself, or the disk's root: never an agent's config folder.
    TooBroad,
}

impl FolderFailure {
    pub fn code(self) -> &'static str {
        match self {
            FolderFailure::NotFolder => "notFolder",
            FolderFailure::AlreadyListed => "alreadyListed",
            FolderFailure::TooMany => "tooMany",
            FolderFailure::TooBroad => "tooBroad",
        }
    }
}

/// The folder the agent's hook file lives in, from the one picked: a folder
/// holding `.gemini` stands for that `.gemini`, as Gemini's
/// `GEMINI_CLI_HOME` does.
pub fn config_folder(provider: Provider, picked: &Path) -> PathBuf {
    if provider == Provider::Gemini
        && picked.file_name().is_none_or(|n| n != ".gemini")
        && picked.join(".gemini").is_dir()
    {
        return picked.join(".gemini");
    }
    picked.to_path_buf()
}

/// A folder's name on the page: `work` for `~/work/.gemini`, a hidden
/// config folder going by the folder it sits in.
pub fn folder_name(folder: &Path) -> String {
    let last = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.to_string_lossy().into_owned());
    let parent = folder.parent().filter(|p| p.parent().is_some());
    match parent.and_then(Path::file_name) {
        Some(name) if last.starts_with('.') => name.to_string_lossy().into_owned(),
        _ => last,
    }
}

/// Adds a folder to `existing`; `default_folder` is the one the agent
/// reads already, which needs no second row.
pub fn add_folder(
    provider: Provider,
    picked: &Path,
    default_folder: Option<&Path>,
    home: &Path,
    existing: &[FolderSetting],
) -> Result<FolderSetting, FolderFailure> {
    let folder = config_folder(provider, picked);
    if !folder.is_absolute() || !folder.is_dir() {
        return Err(FolderFailure::NotFolder);
    }
    let id = accounts::canonical(&folder);
    let resolved = std::fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
    if resolved.parent().is_none() || id == accounts::canonical(home) {
        return Err(FolderFailure::TooBroad);
    }
    if default_folder.is_some_and(|d| accounts::canonical(d) == id) {
        return Err(FolderFailure::AlreadyListed);
    }
    let setting = FolderSetting {
        agent: provider.key().to_owned(),
        path: folder.to_string_lossy().into_owned(),
        name: None,
    };
    if existing
        .iter()
        .any(|s| s.target_id() == setting.target_id())
    {
        return Err(FolderFailure::AlreadyListed);
    }
    if existing.iter().filter(|s| s.agent == setting.agent).count() >= FOLDERS_PER_AGENT {
        return Err(FolderFailure::TooMany);
    }
    Ok(setting)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolFailure {
    NoName,
    NotInFolder,
    AlreadyListed,
    TooMany,
    ClaudeAccount,
}

impl ToolFailure {
    pub fn code(self) -> &'static str {
        match self {
            ToolFailure::NoName => "noName",
            ToolFailure::NotInFolder => "notInFolder",
            ToolFailure::AlreadyListed => "alreadyListed",
            ToolFailure::TooMany => "tooMany",
            ToolFailure::ClaudeAccount => "claudeAccount",
        }
    }
}

/// A file's identity, the file there or not: its folder through links.
fn file_key(file: &Path) -> String {
    let resolved = match (file.parent(), file.file_name()) {
        (Some(folder), Some(name)) => std::fs::canonicalize(folder)
            .map(|f| f.join(name))
            .unwrap_or_else(|_| file.to_path_buf()),
        _ => file.to_path_buf(),
    };
    let text = resolved.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// A new lowercase UUID, which only letters, digits and dashes make.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes = nanos.to_le_bytes();
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Adds a tool by its name and the settings file its hooks go into (a
/// folder stands for the `settings.json` it keeps). The file may not exist
/// yet; its folder must.
pub fn add_tool(
    name: &str,
    picked: &Path,
    claude_files: &[PathBuf],
    existing: &[Tool],
) -> Result<Tool, ToolFailure> {
    let name: String = name.trim().chars().take(LONGEST_NAME).collect();
    if name.is_empty() {
        return Err(ToolFailure::NoName);
    }
    let file = if picked.is_dir() {
        picked.join("settings.json")
    } else {
        picked.to_path_buf()
    };
    if !file.is_absolute() || !file.parent().is_some_and(Path::is_dir) || file.is_dir() {
        return Err(ToolFailure::NotInFolder);
    }
    let key = file_key(&file);
    // A Claude Code account's settings get Claude Code's own hooks.
    if claude_files.iter().any(|f| file_key(f) == key) {
        return Err(ToolFailure::ClaudeAccount);
    }
    if existing.iter().any(|t| file_key(&t.file()) == key) {
        return Err(ToolFailure::AlreadyListed);
    }
    if existing.len() >= TOOLS {
        return Err(ToolFailure::TooMany);
    }
    Ok(Tool {
        id: new_id(),
        name,
        path: file.to_string_lossy().into_owned(),
    })
}

// MARK: Codex's hook review

/// Whether Codex trusts the island's hooks in a Codex home. Codex runs a
/// hook from `hooks.json` only once the person trusted it in its hook
/// review, which keeps `[hooks.state."<key>"] trusted_hash` in the home's
/// `config.toml`. The hash is Codex's own: a SHA-256 of the hook's event,
/// matcher and handler as sorted, compact JSON (codex-rs/hooks discovery.rs,
/// `hook_hash`; codex-rs/config fingerprint.rs, `version_for_toml`).
pub mod codex_trust {
    use super::*;
    use sha2::{Digest, Sha256};

    pub fn snake(event: &str) -> String {
        let mut out = String::new();
        for (i, c) in event.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    }

    /// Codex's timeout as it runs it: SessionEnd and Interrupt at most 3
    /// seconds (1 when unset), every other event 600 when unset.
    pub fn timeout(event: &str, value: Option<i64>) -> i64 {
        if event == "SessionEnd" || event == "Interrupt" {
            value.unwrap_or(1).clamp(1, 3)
        } else {
            value.unwrap_or(600).max(1)
        }
    }

    /// JSON text as serde_json writes it.
    fn json(text: &str) -> String {
        serde_json::to_string(text).unwrap_or_default()
    }

    /// The hash Codex trusts one command hook by.
    pub fn hash(
        event: &str,
        matcher: Option<&str>,
        command: &str,
        timeout_value: Option<i64>,
        is_async: bool,
    ) -> String {
        let handler = format!(
            "{{\"async\":{is_async},\"command\":{},\"timeout\":{},\"type\":\"command\"}}",
            json(command),
            timeout(event, timeout_value)
        );
        let mut text = format!(
            "{{\"event_name\":{},\"hooks\":[{handler}]",
            json(&snake(event))
        );
        if let Some(matcher) = matcher {
            text.push_str(&format!(",\"matcher\":{}", json(matcher)));
        }
        text.push('}');
        let digest = Sha256::digest(text.as_bytes());
        format!(
            "sha256:{}",
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    }

    /// The path as Codex resolves it, links and all; on Windows without the
    /// `\\?\` prefix a resolved path carries.
    pub fn real_path(file: &Path) -> String {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(folder) = file.parent().and_then(|f| std::fs::canonicalize(f).ok()) else {
            return file.to_string_lossy().into_owned();
        };
        let folder = folder.to_string_lossy().into_owned();
        let folder = folder.strip_prefix(r"\\?\").unwrap_or(&folder).to_owned();
        let separator = if cfg!(windows) { '\\' } else { '/' };
        format!("{folder}{separator}{name}")
    }

    /// A key as compared: on Windows, where the form Codex writes is not
    /// known, without case and with one kind of separator.
    pub fn compared(key: &str) -> String {
        if cfg!(windows) {
            key.replace('/', "\\").to_lowercase()
        } else {
            key.to_owned()
        }
    }

    /// The island's hooks in `hooks.json`, by Codex's key for each, with the
    /// hash each must be trusted by.
    pub fn expected(hooks_file: &Path, settings: &Value, marker: &str) -> BTreeMap<String, String> {
        let path = real_path(hooks_file);
        let mut expected = BTreeMap::new();
        let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
            return expected;
        };
        for (event, groups) in hooks {
            for (g, group) in groups.as_array().into_iter().flatten().enumerate() {
                let handlers = group.get("hooks").and_then(Value::as_array);
                for (h, handler) in handlers.into_iter().flatten().enumerate() {
                    let Some(command) = handler.get("command").and_then(Value::as_str) else {
                        continue;
                    };
                    if !command.contains(marker) {
                        continue;
                    }
                    let key = format!("{path}:{}:{g}:{h}", snake(event));
                    expected.insert(
                        key,
                        hash(
                            event,
                            group.get("matcher").and_then(Value::as_str),
                            command,
                            handler.get("timeout").and_then(Value::as_i64),
                            handler
                                .get("async")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        ),
                    );
                }
            }
        }
        expected
    }

    /// Whether Codex trusts every hook of the island's in the folder; None
    /// when it holds none or can't be read.
    pub fn trusted(folder: &Path, app: &str) -> Option<bool> {
        let file = folder.join("hooks.json");
        let settings = installer::read(&file).ok()?;
        let expected = expected(&file, &settings, &installer::marker(app));
        if expected.is_empty() {
            return None;
        }
        let config = std::fs::read_to_string(folder.join("config.toml")).unwrap_or_default();
        let state: HashMap<String, String> = trusted_hashes(&config)
            .into_iter()
            .map(|(k, v)| (compared(&k), v))
            .collect();
        Some(
            expected
                .iter()
                .all(|(k, v)| state.get(&compared(k)) == Some(v)),
        )
    }

    /// `trusted_hash` by hook key, from a `config.toml`: tables named
    /// `[hooks.state."<key>"]`, and keys of an inline table under `[hooks.state]`.
    pub fn trusted_hashes(toml: &str) -> HashMap<String, String> {
        let mut hashes = HashMap::new();
        let mut table: Vec<String> = Vec::new();
        for raw in toml.lines() {
            let line = raw.trim();
            if let Some(inner) = line.strip_prefix('[') {
                // `[table]` or `[[array]]`, a comment perhaps after it.
                table = if inner.starts_with('[') {
                    Vec::new()
                } else {
                    inner
                        .rfind(']')
                        .and_then(|end| key_path(&inner[..end]))
                        .unwrap_or_default()
                };
                continue;
            }
            let Some(equals) = equals_index(line) else {
                continue;
            };
            let Some(left) = key_path(&line[..equals]) else {
                continue;
            };
            let right = line[equals + 1..].trim();
            let full: Vec<&str> = table
                .iter()
                .map(String::as_str)
                .chain(left.iter().map(String::as_str))
                .collect();
            if full.len() == 4
                && full[0] == "hooks"
                && full[1] == "state"
                && full[3] == "trusted_hash"
                && let Some(value) = string(right)
            {
                hashes.insert(full[2].to_owned(), value);
            } else if full.len() == 3
                && full[0] == "hooks"
                && full[1] == "state"
                && right.starts_with('{')
                && let Some(value) = inline_hash(right)
            {
                hashes.insert(full[2].to_owned(), value);
            }
        }
        hashes
    }

    /// A dotted TOML key: bare parts and quoted ones.
    pub fn key_path(text: &str) -> Option<Vec<String>> {
        let chars: Vec<char> = text.chars().collect();
        let mut parts = Vec::new();
        let mut i = 0;
        let skip = |i: &mut usize| {
            while *i < chars.len() && (chars[*i] == ' ' || chars[*i] == '\t') {
                *i += 1;
            }
        };
        loop {
            skip(&mut i);
            if i >= chars.len() {
                return (!parts.is_empty()).then_some(parts);
            }
            if chars[i] == '"' || chars[i] == '\'' {
                let (value, end) = quoted(&chars, i)?;
                parts.push(value);
                i = end;
            } else {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                if start == i {
                    return None;
                }
                parts.push(chars[start..i].iter().collect());
            }
            skip(&mut i);
            if i >= chars.len() {
                return Some(parts);
            }
            if chars[i] != '.' {
                return None;
            }
            i += 1;
        }
    }

    /// A quoted TOML string starting at `start`, and where it ends.
    fn quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
        let quote = chars[start];
        let mut i = start + 1;
        let mut value = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c == quote {
                return Some((value, i + 1));
            }
            if quote == '"' && c == '\\' {
                i += 1;
                let escaped = *chars.get(i)?;
                value.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
            } else {
                value.push(c);
            }
            i += 1;
        }
        None
    }

    fn string(text: &str) -> Option<String> {
        let chars: Vec<char> = text.chars().collect();
        match chars.first() {
            Some('"') | Some('\'') => quoted(&chars, 0).map(|(v, _)| v),
            _ => None,
        }
    }

    /// The byte index of a key/value line's `=`, outside quotes.
    fn equals_index(line: &str) -> Option<usize> {
        let mut quote: Option<char> = None;
        for (i, c) in line.char_indices() {
            if let Some(open) = quote {
                if c == open {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' => quote = Some(c),
                '#' => return None,
                '=' => return Some(i),
                _ => {}
            }
        }
        None
    }

    fn inline_hash(text: &str) -> Option<String> {
        let at = text.find("trusted_hash")?;
        let rest = text[at + "trusted_hash".len()..].trim_start_matches([' ', '=']);
        string(rest)
    }
}

// MARK: Rows

/// Why the island leaves a config file without its hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    /// The agent never made its config folder: it was not run yet.
    FolderMissing,
    AgentNotInstalled,
    /// The hooks are in, and the agent waits for the person to trust them
    /// (Codex's hook review).
    NeedsApproval,
    /// The agent's switch in Settings › Sessions is off.
    SwitchedOff,
    /// The Claude account is hidden.
    AccountHidden,
    /// Codex's own home: the island hears it through Codex's app-server.
    NothingNeeded,
    /// The person pressed Remove; Reinstall brings the hooks back.
    RemovedByYou,
}

/// A config file's state as the page shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shown {
    Installed,
    /// The island's hooks from an older version, or pointing elsewhere.
    Outdated,
    ChangedByHand,
    Missing,
    /// The file can't be read as the agent's format; nothing is changed in it.
    Unreadable,
    Skipped {
        reason: Skip,
    },
}

impl Shown {
    /// Something the person may want to fix.
    pub fn needs_attention(self) -> bool {
        match self {
            Shown::Outdated | Shown::ChangedByHand | Shown::Missing | Shown::Unreadable => true,
            Shown::Skipped { reason } => reason == Skip::NeedsApproval,
            Shown::Installed => false,
        }
    }
}

/// Where a row's hooks go.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    /// A Claude Code account's settings.json.
    Claude(PathBuf),
    /// A hook agent where it reads its hooks by default (None), or in a
    /// config folder the person added.
    Agent(Provider, Option<PathBuf>),
    /// A tool with Claude Code style hooks the person added.
    Custom(Tool),
}

/// One config file the island writes its hooks into.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub provider: Provider,
    /// The account, folder or tool name.
    pub name: String,
    /// The config folder, shown and opened.
    pub folder: PathBuf,
    /// The file the hooks live in; None for Codex's own home.
    pub file: Option<PathBuf>,
    /// The whole file is the island's own (Copilot, OpenCode, Pi).
    pub whole_file: bool,
    /// Added by hand, so it can be taken off the page.
    pub manual: bool,
    /// The agent itself is installed.
    pub present: bool,
    /// The state before the reasons to skip are weighed.
    pub derived: Derived,
    pub state: Shown,
    pub can_reinstall: bool,
    pub can_remove: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip)]
    pub place: Place,
}

impl Target {
    #[allow(clippy::too_many_arguments)]
    fn new(
        id: String,
        provider: Provider,
        place: Place,
        name: String,
        folder: PathBuf,
        file: Option<PathBuf>,
        whole_file: bool,
        manual: bool,
        present: bool,
        derived: Derived,
        state: Shown,
    ) -> Self {
        let mut target = Target {
            id,
            provider,
            name,
            folder,
            file,
            whole_file,
            manual,
            present,
            derived,
            state,
            can_reinstall: false,
            can_remove: false,
            failure: None,
            place,
        };
        target.can_reinstall = can_reinstall(&target);
        target.can_remove = can_remove(&target);
        target
    }
}

/// Whether Reinstall can do anything here.
fn can_reinstall(target: &Target) -> bool {
    if target.file.is_none() {
        return false;
    }
    match target.state {
        // A folder the person added is theirs to fill, found agent or not.
        Shown::Skipped { reason } => {
            matches!(reason, Skip::RemovedByYou | Skip::NeedsApproval)
                || (target.manual
                    && matches!(reason, Skip::FolderMissing | Skip::AgentNotInstalled))
        }
        Shown::Unreadable => false,
        _ => true,
    }
}

/// Whether the file holds any of the island's hooks to take out.
fn can_remove(target: &Target) -> bool {
    target.file.is_some()
        && matches!(
            target.derived,
            Derived::Installed | Derived::Outdated | Derived::ChangedByHand
        )
}

/// The page's state of one file, from what it holds and why it may be left alone.
#[allow(clippy::too_many_arguments)]
pub fn shown_state(
    derived: Derived,
    present: bool,
    folder_exists: bool,
    enabled: bool,
    opted_out: bool,
    hidden_account: bool,
    needs_approval: bool,
) -> Shown {
    let skipped = |reason| Shown::Skipped { reason };
    match derived {
        Derived::Unreadable => Shown::Unreadable,
        Derived::Installed if needs_approval => skipped(Skip::NeedsApproval),
        Derived::Installed => Shown::Installed,
        Derived::Outdated => Shown::Outdated,
        Derived::ChangedByHand => Shown::ChangedByHand,
        Derived::Missing | Derived::RemovedByHand => {
            if hidden_account {
                skipped(Skip::AccountHidden)
            } else if !enabled {
                skipped(Skip::SwitchedOff)
            } else if opted_out {
                skipped(Skip::RemovedByYou)
            } else if !present {
                skipped(Skip::AgentNotInstalled)
            } else if !folder_exists {
                skipped(Skip::FolderMissing)
            } else {
                Shown::Missing
            }
        }
    }
}

/// Everything the rows are read from.
#[derive(Clone, Debug)]
pub struct Context {
    pub home: PathBuf,
    pub exe: PathBuf,
    pub endpoint: String,
    pub app: String,
    /// Every Claude Code account, hidden ones too.
    pub accounts: Vec<Account>,
    /// The hook agents whose switch is off.
    pub hidden: HashSet<Provider>,
    /// The agents installed here, Codex among them.
    pub found: HashSet<Provider>,
    pub folders: Vec<FolderSetting>,
    pub tools: Vec<Tool>,
    pub opt_out: HashSet<String>,
    pub records: BTreeMap<String, Record>,
}

fn own_name(provider: Provider, app: &str) -> Option<String> {
    (provider == Provider::Antigravity).then(|| agents::own_name(app))
}

fn current_print(file: &Path, app: &str, own: Option<&str>, whole: bool) -> Option<String> {
    hook_records::fingerprint(
        std::fs::read(file).ok().as_deref(),
        &installer::marker(app),
        own,
        whole,
    )
}

/// A Claude Code account's settings: all the hooks, some, or none.
pub fn claude_state(file: &Path, command: &str, app: &str) -> Base {
    match installer::read(file) {
        Err(_) => Base::Unreadable,
        Ok(s) if installer::is_installed(&s, command) => Base::Installed,
        Ok(s) if installer::has_any(&s, app) => Base::Outdated,
        Ok(_) => Base::Missing,
    }
}

/// Every row, in the page's order: Claude Code's accounts, Codex, each hook
/// agent with its added folders, then the tools.
pub fn list(ctx: &Context) -> Vec<Target> {
    let mut targets = Vec::new();
    let command = installer::command(&ctx.exe, &ctx.app);
    let mut seen = HashSet::new();
    for account in &ctx.accounts {
        let file = account.settings_path();
        // Two folders whose settings are one linked file are one row.
        if !seen.insert(file_key(&file)) {
            continue;
        }
        let id = hook_records::claude_id(&file);
        let base = claude_state(&file, &command, &ctx.app);
        let derived = hook_records::derive(
            base,
            ctx.records.get(&id),
            current_print(&file, &ctx.app, None, false).as_deref(),
        );
        let state = shown_state(
            derived,
            true,
            true,
            true,
            ctx.opt_out.contains(&id),
            account.hidden,
            false,
        );
        targets.push(Target::new(
            id,
            Provider::Claude,
            Place::Claude(file.clone()),
            account.name.clone(),
            PathBuf::from(&account.path),
            Some(file),
            false,
            account.manual,
            true,
            derived,
            state,
        ));
    }
    // Codex's own home needs nothing; folders added by hand get hooks.
    if let Some(own) = agents::agent_folder(Provider::Codex, &ctx.home) {
        targets.push(Target::new(
            hook_records::agent_id(Provider::Codex),
            Provider::Codex,
            Place::Agent(Provider::Codex, None),
            "Codex".into(),
            own,
            None,
            false,
            false,
            ctx.found.contains(&Provider::Codex),
            Derived::Missing,
            Shown::Skipped {
                reason: Skip::NothingNeeded,
            },
        ));
    }
    for setting in ctx.folders.iter().filter(|s| s.agent == "codex") {
        targets.push(folder_target(Provider::Codex, setting, ctx));
    }
    for provider in Provider::HOOK_AGENTS {
        let present = ctx.found.contains(&provider);
        if let Some((file, whole)) = hook_records::hook_file(provider, &ctx.home, &ctx.app) {
            let id = hook_records::agent_id(provider);
            let base = agents::state(provider, &ctx.home, &ctx.exe, &ctx.endpoint, &ctx.app);
            let own = own_name(provider, &ctx.app);
            let derived = hook_records::derive(
                base,
                ctx.records.get(&id),
                current_print(&file, &ctx.app, own.as_deref(), whole).as_deref(),
            );
            let folder = agents::agent_folder(provider, &ctx.home)
                .or_else(|| file.parent().map(Path::to_path_buf))
                .unwrap_or_default();
            let state = shown_state(
                derived,
                present,
                folder.is_dir(),
                !ctx.hidden.contains(&provider),
                ctx.opt_out.contains(&id),
                false,
                false,
            );
            targets.push(Target::new(
                id,
                provider,
                Place::Agent(provider, None),
                display_name(provider).into(),
                folder,
                Some(file),
                whole,
                false,
                present,
                derived,
                state,
            ));
        }
        for setting in ctx.folders.iter().filter(|s| s.agent == provider.key()) {
            targets.push(folder_target(provider, setting, ctx));
        }
    }
    for tool in &ctx.tools {
        let id = tool.target_id();
        let file = tool.file();
        let base = agents::custom_state(&file, &tool.id, &ctx.exe, &ctx.app);
        let derived = hook_records::derive(
            base,
            ctx.records.get(&id),
            current_print(&file, &ctx.app, None, false).as_deref(),
        );
        let folder = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let state = shown_state(
            derived,
            true,
            folder.is_dir(),
            true,
            ctx.opt_out.contains(&id),
            false,
            false,
        );
        targets.push(Target::new(
            id,
            Provider::Custom,
            Place::Custom(tool.clone()),
            tool.name.clone(),
            folder,
            Some(file),
            false,
            true,
            true,
            derived,
            state,
        ));
    }
    targets
}

fn folder_target(provider: Provider, setting: &FolderSetting, ctx: &Context) -> Target {
    let folder = setting.folder();
    let id = setting.target_id();
    let file = agents::file_in(provider, &folder, &ctx.app);
    let whole = matches!(provider, Provider::Copilot | Provider::Pi);
    let base = agents::state_in(
        provider,
        &ctx.home,
        &ctx.exe,
        &ctx.endpoint,
        &ctx.app,
        Some(&folder),
    );
    let derived = hook_records::derive(
        base,
        ctx.records.get(&id),
        file.as_deref()
            .and_then(|f| current_print(f, &ctx.app, None, whole))
            .as_deref(),
    );
    let present = if provider == Provider::Codex {
        ctx.found.contains(&Provider::Codex)
    } else {
        ctx.found.contains(&provider)
    };
    let approval = provider == Provider::Codex
        && derived == Derived::Installed
        && codex_trust::trusted(&folder, &ctx.app) == Some(false);
    let state = shown_state(
        derived,
        present,
        folder.is_dir(),
        provider == Provider::Codex || !ctx.hidden.contains(&provider),
        ctx.opt_out.contains(&id),
        false,
        approval,
    );
    Target::new(
        id,
        provider,
        Place::Agent(provider, Some(folder.clone())),
        setting.name.clone().unwrap_or_else(|| folder_name(&folder)),
        folder,
        file,
        whole,
        true,
        present,
        derived,
        state,
    )
}

/// The agent's name as the page and the notice say it.
pub fn display_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude Code",
        Provider::Codex => "Codex",
        Provider::Gemini => "Gemini CLI",
        Provider::Copilot => "Copilot CLI",
        Provider::Opencode => "OpenCode",
        Provider::Cursor => "Cursor",
        Provider::Pi => "Pi",
        Provider::Vibe => "Mistral Vibe",
        Provider::Devin => "Devin CLI",
        Provider::Antigravity => "Google Antigravity",
        Provider::Grok => "Grok CLI",
        Provider::Custom => "Other tool",
    }
}

/// How many files work, and how many want a look, for Settings › Sessions.
pub fn summary(targets: &[Target]) -> (usize, usize) {
    (
        targets
            .iter()
            .filter(|t| t.state == Shown::Installed)
            .count(),
        targets.iter().filter(|t| t.state.needs_attention()).count(),
    )
}

/// A label for each session heard through a second Codex home's hooks or a
/// tool's, by session id: the folder's or the tool's name.
pub fn labels(
    sessions: &[Session],
    folders: &[FolderSetting],
    tools: &[Tool],
) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    for session in sessions {
        let Some(via) = session.via.as_deref() else {
            continue;
        };
        if session.provider == Provider::Custom {
            let name = tools
                .iter()
                .find(|t| t.id == via)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| display_name(Provider::Custom).into());
            labels.insert(session.id.clone(), name);
        } else if let Some(folder) = folders
            .iter()
            .filter(|f| f.agent == "codex")
            .find(|f| agents::folder_tag(&f.folder()) == via)
        {
            labels.insert(
                session.id.clone(),
                folder
                    .name
                    .clone()
                    .unwrap_or_else(|| folder_name(&folder.folder())),
            );
        }
    }
    labels
}

// MARK: Changing one file

/// What a change is made with.
#[derive(Clone, Debug)]
pub struct Env {
    pub home: PathBuf,
    pub exe: PathBuf,
    pub endpoint: String,
    pub app: String,
}

/// Puts the island's hooks into the target's file, a copy kept first.
pub fn install(target: &Target, env: &Env) -> Result<(), String> {
    match &target.place {
        Place::Claude(file) => {
            let command = installer::command(&env.exe, &env.app);
            installer::update(file, |s| installer::installing(s, &command, &env.app))
                .map_err(|e| e.to_string())?
        }
        Place::Agent(provider, folder) => agents::install_in(
            *provider,
            &env.home,
            &env.exe,
            &env.endpoint,
            &env.app,
            folder.as_deref(),
        )?,
        Place::Custom(tool) => agents::install_custom(&tool.file(), &tool.id, &env.exe, &env.app)?,
    }
    if let Some(file) = &target.file {
        hook_records::note_installed(
            &target.id,
            file,
            &installer::marker(&env.app),
            own_name(target.provider, &env.app).as_deref(),
            target.whole_file,
        );
    }
    Ok(())
}

/// Takes the island's hooks out of the target's file, a copy kept first.
pub fn remove(target: &Target, env: &Env) -> Result<(), String> {
    match &target.place {
        Place::Claude(file) => {
            if file.exists() {
                installer::update(file, |s| installer::removing(s, &env.app))
                    .map_err(|e| e.to_string())?;
            }
        }
        Place::Agent(provider, folder) => {
            agents::remove_in(*provider, &env.home, &env.app, folder.as_deref())?
        }
        Place::Custom(tool) => agents::remove_custom(&tool.file(), &env.app)?,
    }
    hook_records::forget(&target.id);
    Ok(())
}

/// Reinstall: the row is the person's again to keep in step.
pub fn reinstall(target: &Target, env: &Env) -> Result<(), String> {
    set_opt_out(&target.id, false);
    install(target, env)
}

/// Remove: the hooks come out and stay out until Reinstall.
pub fn take_out(target: &Target, env: &Env) -> Result<(), String> {
    set_opt_out(&target.id, true);
    remove(target, env)
}

/// A folder or tool taken off the page: its hooks out, and its file as it
/// was before the island, as Remove everything leaves it.
pub fn forget(target: &Target, env: &Env) -> Result<(), String> {
    remove(target, env)?;
    set_opt_out(&target.id, false);
    let Some(file) = &target.file else {
        return Ok(());
    };
    restore_original(file)?;
    if target.whole_file {
        drop_own_copies(file);
    }
    remove_empty();
    Ok(())
}

/// Takes the island out of every file it wrote into, putting back each file
/// as it was before the island where nothing else changed in it since, and
/// taking away the files and folders the island made. Rows that held hooks,
/// and every agent's own row, are left out of keeping in step until
/// Reinstall. Failures by row id.
pub fn remove_everything(targets: &[Target], env: &Env) -> BTreeMap<String, String> {
    let mut failures = BTreeMap::new();
    let mut done = HashSet::new();
    for target in targets {
        let Some(file) = &target.file else {
            continue;
        };
        // An agent's own row stays out too, or its switch would put the
        // hooks in again; a row that never had any is left as it was.
        if matches!(target.place, Place::Agent(_, None)) || target.can_remove {
            set_opt_out(&target.id, true);
        }
        // Two rows over one linked file change it once.
        if !done.insert(file_key(file)) {
            continue;
        }
        let result = remove(target, env).and_then(|()| {
            restore_original(file)?;
            if target.whole_file {
                drop_own_copies(file);
            }
            Ok(())
        });
        if let Err(e) = result {
            failures.insert(target.id.clone(), e);
        }
    }
    remove_empty();
    failures
}

/// The copies of a file that was the island's alone hold nothing of the
/// person's; in a folder the island made they go with it.
pub fn drop_own_copies(file: &Path) {
    let Some(folder) = file.parent() else {
        return;
    };
    let folder_text = folder.to_string_lossy();
    if !made_folders().iter().any(|m| *m == folder_text) {
        return;
    }
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = format!("{name}.openbar-");
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if entry_name.starts_with(&prefix) && entry_name.ends_with(".bak") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// MARK: The file before the island

/// Once the island's hooks are out: the file as it was before the island
/// when that is all that is left in it, or no file when the island made it.
/// A file the person changed since stays as it is now. A copy of what is
/// replaced or taken away is kept beside it.
pub fn restore_original(file: &Path) -> Result<(), String> {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let current = installer::contents(&file).map_err(|e| e.to_string())?;
    if installer::absent_note(&file).exists() {
        if let Some(current) = current {
            if !is_leftover(&current) {
                installer::forget_original(&file);
                return Ok(());
            }
            installer::keep_copy(&current, &file).map_err(|e| e.to_string())?;
            std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
        installer::forget_original(&file);
        return Ok(());
    }
    let Ok(original) = std::fs::read(installer::original_copy(&file)) else {
        return Ok(());
    };
    let result = match current {
        Some(current) if current != original && same_content(&current, &original) => {
            installer::write_with_backup(&file, Some(&current), &original)
                .map_err(|e| e.to_string())
        }
        _ => Ok(()),
    };
    installer::forget_original(&file);
    result
}

fn is_empty_container(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.is_empty(),
        Value::Array(list) => list.is_empty(),
        _ => false,
    }
}

fn is_version_one(key: &str, value: &Value) -> bool {
    key == "version" && value.as_f64() == Some(1.0)
}

/// Whether a file holds only what the island adds around its hooks: an
/// empty object, an empty `hooks`, Cursor's and Devin's `version`.
pub fn is_leftover(data: &[u8]) -> bool {
    if data.iter().all(u8::is_ascii_whitespace) {
        return true;
    }
    let Ok(Value::Object(json)) = installer::parse(data) else {
        return false;
    };
    json.iter()
        .all(|(k, v)| is_empty_container(v) || is_version_one(k, v))
}

/// Whether what is left after the hooks came out is the file from before:
/// the same JSON once keys the island added empty are set aside (Cursor's
/// and Devin's `version`, an emptied `hooks`), or the same text.
pub fn same_content(current: &[u8], original: &[u8]) -> bool {
    if let (Ok(Value::Object(now)), Ok(Value::Object(before))) =
        (installer::parse(current), installer::parse(original))
    {
        let kept: serde_json::Map<String, Value> = now
            .into_iter()
            .filter(|(k, v)| {
                before.contains_key(k) || !(is_empty_container(v) || is_version_one(k, v))
            })
            .collect();
        return kept == before;
    }
    String::from_utf8_lossy(current).trim() == String::from_utf8_lossy(original).trim()
}

// MARK: Putting lost hooks back

/// The rows whose hooks were lost and come back by themselves: hooks the
/// island put in that are gone or from an older version, on a row that is
/// on, found, and not taken out by the person. Hooks changed by hand are the
/// person's. Agents' own rows are kept in step with their switches
/// elsewhere (`installer::plan_with`).
pub fn repairs<'a>(
    targets: &'a [Target],
    records: &BTreeMap<String, Record>,
    auto_repair: bool,
) -> Vec<&'a Target> {
    if !auto_repair {
        return Vec::new();
    }
    targets
        .iter()
        .filter(|t| !matches!(t.place, Place::Agent(_, None)))
        .filter(|t| t.file.is_some() && records.contains_key(&t.id))
        .filter(|t| {
            (t.state == Shown::Missing && t.derived == Derived::RemovedByHand)
                || t.state == Shown::Outdated
        })
        .collect()
}

#[cfg(test)]
#[path = "page_tests.rs"]
mod tests;
