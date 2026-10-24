// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Adds the island's hooks to Claude Code's user settings, beside the
//! person's own, and takes them out again (ClaudeHookInstaller.swift). The
//! hook is this app run with `--hook`, which hands the event to the running
//! island and prints its answer; when the island is not running it prints
//! nothing and Claude Code carries on as if there were no hook. Only entries
//! carrying this app's marker are ever changed, and the file is copied aside
//! before every change.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

pub const EVENTS: [&str; 11] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "SubagentStart",
    "SubagentStop",
];
/// Events matched against a tool name take a matcher.
const TOOL_EVENTS: [&str; 4] = [
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
];
/// A permission waits on the person; everything else is answered at once.
const PERMISSION_TIMEOUT: u64 = 600;
const EVENT_TIMEOUT: u64 = 5;
/// How many copies of earlier settings are kept beside the file.
const BACKUPS: usize = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The file is not JSON Claude Code could read either; it is left alone.
    Unreadable,
    Unwritable(String),
    /// Claude Code changed the file each time it was about to be written.
    Busy,
    /// A copy of the old settings could not be kept, so nothing changed.
    NoBackup,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Unreadable => write!(f, "unreadable"),
            Failure::Unwritable(e) => write!(f, "unwritable: {e}"),
            Failure::Busy => write!(f, "busy"),
            Failure::NoBackup => write!(f, "noBackup"),
        }
    }
}

/// `%USERPROFILE%\.claude\settings.json`, or the folder `CLAUDE_CONFIG_DIR` names.
pub fn settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    home_dir().map(|h| h.join(".claude").join("settings.json"))
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The marker is an argument, not a shell comment: on Windows the hook may
/// run under Git Bash, cmd or PowerShell, and only an argument survives all three.
pub fn marker(app: &str) -> String {
    format!("--marker vorssaint-hook --app {app}")
}

/// The command Claude Code runs: this executable, quoted, with forward
/// slashes so no shell reads a backslash as an escape.
pub fn command(exe: &Path, app: &str) -> String {
    let path = exe.to_string_lossy().replace('\\', "/");
    format!("\"{path}\" --hook {}", marker(app))
}

fn groups(hooks: &Map<String, Value>, event: &str) -> Vec<Value> {
    hooks
        .get(event)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn entry_commands(group: &Value) -> impl Iterator<Item = &str> {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| e.get("command").and_then(Value::as_str))
}

/// Whether every event has this app's current hook.
pub fn is_installed(settings: &Value, command: &str) -> bool {
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    EVENTS.iter().all(|event| {
        groups(hooks, event)
            .iter()
            .any(|g| entry_commands(g).any(|c| c == command))
    })
}

pub fn has_any(settings: &Value, app: &str) -> bool {
    let marker = marker(app);
    settings
        .get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|hooks| {
            hooks
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .any(|g| entry_commands(g).any(|c| c.contains(&marker)))
        })
}

/// The settings without this app's hooks; groups and events left empty go too.
pub fn removing(settings: &Value, app: &str) -> Value {
    let marker = marker(app);
    let mut settings = settings.clone();
    let Some(object) = settings.as_object_mut() else {
        return settings;
    };
    let Some(hooks) = object.get_mut("hooks").and_then(Value::as_object_mut) else {
        return settings;
    };
    let events: Vec<String> = hooks.keys().cloned().collect();
    for event in events {
        let Some(list) = hooks.get(&event).and_then(Value::as_array) else {
            continue;
        };
        let kept: Vec<Value> = list
            .iter()
            .filter_map(|group| {
                let Some(entries) = group.get("hooks").and_then(Value::as_array) else {
                    return Some(group.clone());
                };
                let remaining: Vec<Value> = entries
                    .iter()
                    .filter(|e| {
                        !e.get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| c.contains(&marker))
                    })
                    .cloned()
                    .collect();
                if remaining.len() == entries.len() {
                    return Some(group.clone());
                }
                if remaining.is_empty() {
                    return None;
                }
                let mut group = group.clone();
                group["hooks"] = Value::Array(remaining);
                Some(group)
            })
            .collect();
        if kept.is_empty() {
            hooks.remove(&event);
        } else {
            hooks.insert(event, Value::Array(kept));
        }
    }
    if hooks.is_empty() {
        object.remove("hooks");
    }
    settings
}

pub fn installing(settings: &Value, command: &str, app: &str) -> Value {
    let mut settings = removing(settings, app);
    if !settings.is_object() {
        settings = json!({});
    }
    let object = settings.as_object_mut().unwrap();
    let hooks = object.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for event in EVENTS {
        let timeout = if event == "PermissionRequest" {
            PERMISSION_TIMEOUT
        } else {
            EVENT_TIMEOUT
        };
        let mut group =
            json!({ "hooks": [{ "type": "command", "command": command, "timeout": timeout }] });
        if TOOL_EVENTS.contains(&event) {
            group["matcher"] = json!("*");
        }
        let mut list = groups(hooks, event);
        list.push(group);
        hooks.insert(event.to_string(), Value::Array(list));
    }
    settings
}

/// What the file holds, None when there is no file at all.
fn contents(path: &Path) -> Result<Option<Vec<u8>>, Failure> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Failure::Unreadable),
        Ok(_) => {}
    }
    // A link whose target is gone is not a missing file to create.
    let meta = fs::metadata(path).map_err(|_| Failure::Unreadable)?;
    if meta.is_dir() {
        return Err(Failure::Unreadable);
    }
    fs::read(path).map(Some).map_err(|_| Failure::Unreadable)
}

/// The settings, or `{}` when there is no file yet (or only whitespace).
pub fn read(path: &Path) -> Result<Value, Failure> {
    let Some(data) = contents(path)? else {
        return Ok(json!({}));
    };
    parse(&data)
}

fn parse(data: &[u8]) -> Result<Value, Failure> {
    if data
        .iter()
        .all(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
    {
        return Ok(json!({}));
    }
    // Claude Code tolerates a BOM; so do we.
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    match serde_json::from_slice::<Value>(data) {
        Ok(v) if v.is_object() => Ok(v),
        _ => Err(Failure::Unreadable),
    }
}

/// Keys sorted at every level, like the Mac writes them, whatever map order serde_json was built with.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Reads the file, changes it and writes it back in one step, keeping a copy
/// of what was there. Claude Code writes the same file, so a change made
/// meanwhile starts the step over.
pub fn update(link: &Path, change: impl Fn(&Value) -> Value) -> Result<(), Failure> {
    // Settings kept elsewhere and linked here are changed where they live.
    let path = fs::canonicalize(link).unwrap_or_else(|_| link.to_path_buf());
    for _ in 0..3 {
        let before = modified(&path);
        let original = contents(&path)?;
        let settings = match &original {
            Some(data) => parse(data)?,
            None => json!({}),
        };
        let changed = change(&settings);
        // Nothing to change, and no file made just to hold nothing.
        if changed == settings {
            return Ok(());
        }
        let mut data = serde_json::to_vec_pretty(&sorted(&changed))
            .map_err(|e| Failure::Unwritable(e.to_string()))?;
        data.push(b'\n');
        if modified(&path) != before || contents(&path).ok().flatten() != original {
            continue;
        }
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        fs::create_dir_all(&folder).map_err(|e| Failure::Unwritable(e.to_string()))?;
        if let Some(original) = &original {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = folder.join(format!("{name}.vorssaint-{stamp}.bak"));
            write_private(&backup, original).map_err(|_| Failure::NoBackup)?;
            prune(&folder, &name);
        }
        let temporary = folder.join(format!(".{name}.vorssaint-{}", std::process::id()));
        fs::write(&temporary, &data).map_err(|e| Failure::Unwritable(e.to_string()))?;
        if let Ok(meta) = fs::metadata(&path) {
            let _ = fs::set_permissions(&temporary, meta.permissions());
        }
        if let Err(e) = replace(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(Failure::Unwritable(e.to_string()));
        }
        return Ok(());
    }
    Err(Failure::Busy)
}

/// Settings can hold tokens: the copy is the owner's alone (0600 on Unix;
/// on Windows a file under the profile already is).
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    fs::write(path, data)
}

/// Atomic on both platforms: rename on Unix, MoveFileEx(REPLACE_EXISTING) on Windows.
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

/// Keeps the newest copies only, by the number in their names.
fn prune(folder: &Path, name: &str) {
    let prefix = format!("{name}.vorssaint-");
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut copies: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let stamp = file
                .strip_prefix(&prefix)?
                .strip_suffix(".bak")?
                .parse()
                .ok()?;
            Some((stamp, e.path()))
        })
        .collect();
    copies.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in copies.into_iter().skip(BACKUPS) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "com.vorssaint.island";

    fn cmd() -> String {
        command(
            Path::new("C:\\Users\\Vors Name\\AppData\\Local\\Vorssaint\\Vorssaint.exe"),
            APP,
        )
    }

    #[test]
    fn command_is_quoted_with_forward_slashes_and_marked() {
        assert_eq!(
            cmd(),
            "\"C:/Users/Vors Name/AppData/Local/Vorssaint/Vorssaint.exe\" --hook --marker vorssaint-hook --app com.vorssaint.island"
        );
    }

    #[test]
    fn installs_every_event_beside_the_persons_own_hooks() {
        let mine = json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}, "model": "opus"});
        let installed = installing(&mine, &cmd(), APP);
        assert!(is_installed(&installed, &cmd()));
        assert_eq!(installed["model"], "opus");
        let stop = installed["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        assert_eq!(
            installed["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            600
        );
        assert_eq!(installed["hooks"]["PermissionRequest"][0]["matcher"], "*");
        assert!(installed["hooks"]["Stop"][1].get("matcher").is_none());
        assert_eq!(installed["hooks"]["Stop"][1]["hooks"][0]["timeout"], 5);
    }

    #[test]
    fn installing_twice_keeps_one_entry_per_event() {
        let once = installing(&json!({}), &cmd(), APP);
        let twice = installing(&once, &cmd(), APP);
        assert_eq!(once, twice);
    }

    #[test]
    fn a_changed_command_reads_as_not_installed() {
        let installed = installing(&json!({}), &cmd(), APP);
        assert!(!is_installed(
            &installed,
            &command(Path::new("D:/elsewhere.exe"), APP)
        ));
        assert!(has_any(&installed, APP));
    }

    #[test]
    fn removing_takes_only_this_apps_entries_and_empty_groups() {
        let mine = json!({"hooks": {
            "Stop": [{"hooks": [{"type": "command", "command": "say done"}, {"type": "command", "command": cmd()}]}],
            "Other": [{"hooks": [{"type": "command", "command": "keep"}]}]
        }});
        let with_ours = installing(&mine, &cmd(), APP);
        let removed = removing(&with_ours, APP);
        assert_eq!(
            removed["hooks"]["Stop"],
            json!([{"hooks": [{"type": "command", "command": "say done"}]}])
        );
        assert_eq!(removed["hooks"]["Other"], mine["hooks"]["Other"]);
        assert!(removed["hooks"].get("PermissionRequest").is_none());
        assert_eq!(
            removing(&installing(&json!({}), &cmd(), APP), APP),
            json!({})
        );
    }

    #[test]
    fn another_build_is_left_alone() {
        let other = installing(
            &json!({}),
            "\"x\" --hook --marker vorssaint-hook --app com.vorssaint.other",
            "com.vorssaint.other",
        );
        let both = installing(&other, &cmd(), APP);
        let removed = removing(&both, APP);
        assert!(has_any(&removed, "com.vorssaint.other"));
        assert!(!has_any(&removed, APP));
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vorssaint-hooks-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn update_writes_sorted_json_and_keeps_three_backups() {
        let dir = temp_dir("update");
        let file = dir.join("settings.json");
        fs::write(&file, b"{\"z\":1,\"a\":{\"y\":2,\"b\":3}}").unwrap();
        for (i, stamp) in [100u64, 200, 300].iter().enumerate() {
            fs::write(
                dir.join(format!("settings.json.vorssaint-{stamp}.bak")),
                format!("{i}"),
            )
            .unwrap();
        }
        update(&file, |s| installing(s, &cmd(), APP)).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.ends_with('\n'));
        assert!(text.find("\"a\"").unwrap() < text.find("\"hooks\"").unwrap());
        assert!(text.find("\"hooks\"").unwrap() < text.find("\"z\"").unwrap());
        assert!(is_installed(&read(&file).unwrap(), &cmd()));
        let backups = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".bak")
            })
            .count();
        assert_eq!(backups, 3);
        assert!(!dir.join("settings.json.vorssaint-100.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_without_a_change_creates_nothing() {
        let dir = temp_dir("nochange");
        let file = dir.join("settings.json");
        update(&file, |s| removing(s, APP)).unwrap();
        assert!(!file.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_file_is_left_alone() {
        let dir = temp_dir("broken");
        let file = dir.join("settings.json");
        fs::write(&file, b"{ not json").unwrap();
        assert_eq!(
            update(&file, |s| installing(s, &cmd(), APP)),
            Err(Failure::Unreadable)
        );
        assert_eq!(fs::read(&file).unwrap(), b"{ not json");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn whitespace_and_bom_read_as_settings() {
        assert_eq!(parse(b"  \n").unwrap(), json!({}));
        assert_eq!(parse(b"\xEF\xBB\xBF{\"a\":1}").unwrap(), json!({"a": 1}));
        assert_eq!(parse(b"[1]"), Err(Failure::Unreadable));
    }
}
