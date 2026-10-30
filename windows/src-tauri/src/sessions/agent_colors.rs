// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Claude agent definition colors, matching `ClaudeAgentColors.swift`.
//! Only definition heads are read; answers (including misses) live for 30 s.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::models::{Session, Subagent, Time};
use super::registry;

const HEAD_LIMIT: usize = 4096;
const FILES_LIMIT: usize = 200;
const JSON_LIMIT: usize = 1 << 20;
const KEPT_FOR: Time = 30.0;

#[derive(Default)]
pub struct AgentColors {
    known: HashMap<(String, String), (Option<String>, Time)>,
}

impl AgentColors {
    pub fn decorate(&mut self, session: &mut Session, home: &Path, now: Time) {
        for agent in &mut session.subagents {
            agent.color = self.color(&agent.agent_type, &session.cwd, home, now);
        }
    }

    fn color(&mut self, kind: &str, cwd: &str, home: &Path, now: Time) -> Option<String> {
        if kind.is_empty() || kind == Subagent::UNTYPED {
            return None;
        }
        let key = (kind.to_owned(), cwd.to_owned());
        if let Some((color, at)) = self.known.get(&key)
            && now >= *at
            && now - at < KEPT_FOR
        {
            return color.clone();
        }
        let color = lookup(kind, cwd, home);
        if self.known.len() > 500 {
            self.known.clear();
        }
        self.known.insert(key, (color.clone(), now));
        color
    }
}

fn lookup(kind: &str, cwd: &str, home: &Path) -> Option<String> {
    let homes = registry::homes(home);
    let (name, folders) = if let Some((plugin, name)) = kind.split_once(':') {
        // As on macOS, the first readable installed list is authoritative.
        let installed = homes
            .iter()
            .find_map(|h| read(&h.join("plugins/installed_plugins.json"), JSON_LIMIT));
        let mut folders = installed
            .as_deref()
            .map(|data| plugin_folders(data, plugin))
            .unwrap_or_default();
        for h in &homes {
            folders.extend(synced_plugin_folders(&h.join("plugins/synced"), plugin));
        }
        (name, folders)
    } else {
        let mut folders = Vec::new();
        if !cwd.is_empty() {
            for folder in Path::new(cwd).ancestors() {
                if folder == home || folder.parent().is_none() {
                    break;
                }
                folders.push(folder.join(".claude/agents"));
            }
        }
        folders.extend(homes.iter().map(|h| h.join("agents")));
        (kind, folders)
    };
    for folder in folders {
        if let Some(color) = definition(name, &folder) {
            return color;
        }
    }
    None
}

// The outer Option distinguishes a matching, colorless definition from a miss:
// a project's colorless definition must still shadow the user's definition.
fn definition(name: &str, folder: &Path) -> Option<Option<String>> {
    let mut files = entries(folder);
    files.retain(|file| file.ends_with(".md"));
    files.sort();
    let mut by_file = None;
    for file in files.into_iter().take(FILES_LIMIT) {
        let Some(data) = read(&folder.join(&file), HEAD_LIMIT) else {
            continue;
        };
        let (agent, color) = frontmatter(&data);
        if agent.as_deref() == Some(name) {
            return Some(color);
        }
        if agent.is_none() && Path::new(&file).file_stem().and_then(|s| s.to_str()) == Some(name) {
            by_file = Some(color);
        }
    }
    by_file
}

fn read(path: &Path, limit: usize) -> Option<Vec<u8>> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || (limit == JSON_LIMIT && meta.len() >= JSON_LIMIT as u64) {
        return None;
    }
    let mut data = Vec::new();
    File::open(path)
        .ok()?
        .take(limit as u64)
        .read_to_end(&mut data)
        .ok()?;
    Some(data)
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&read(path, JSON_LIMIT)?).ok()
}

fn entries(folder: &Path) -> Vec<String> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect()
}

fn frontmatter(data: &[u8]) -> (Option<String>, Option<String>) {
    let head = String::from_utf8_lossy(&data[..data.len().min(HEAD_LIMIT)]);
    let head = head.strip_prefix('\u{feff}').unwrap_or(&head);
    let mut lines = head.split(['\r', '\n']);
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut color) = (None, None);
    for line in lines {
        let text = line.trim();
        if text == "---" {
            return (name, color);
        }
        let Some((key, raw)) = text.split_once(':') else {
            continue;
        };
        let value = scalar(raw);
        if value.is_empty() {
            continue;
        }
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => name = Some(value.to_owned()),
            "color" => color = Some(value.to_owned()),
            _ => {}
        }
    }
    (None, None)
}

fn scalar(raw: &str) -> &str {
    let text = raw.trim();
    if text.starts_with(['\'', '"']) {
        let quote = text.as_bytes()[0] as char;
        let inside = &text[1..];
        let Some(end) = inside.find(quote) else {
            return text;
        };
        let rest = &inside[end + 1..];
        let after = rest.trim();
        if after.is_empty() || (after.starts_with('#') && rest.starts_with(char::is_whitespace)) {
            return &inside[..end];
        }
        return text;
    }
    if text.starts_with('#') {
        return "";
    }
    let end = text
        .find(" #")
        .or_else(|| text.find("\t#"))
        .unwrap_or(text.len());
    text[..end].trim()
}

fn plugin_folders(data: &[u8], plugin: &str) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return folders;
    };
    let Some(plugins) = value.get("plugins").and_then(Value::as_object) else {
        return folders;
    };
    let mut keys: Vec<_> = plugins.keys().collect();
    keys.sort();
    for key in keys {
        if key.split('@').next() != Some(plugin) {
            continue;
        }
        let Some(installs) = plugins[key]
            .as_array()
            .filter(|a| a.iter().all(Value::is_object))
        else {
            continue;
        };
        for install in installs {
            if let Some(path) = install
                .get("installPath")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                folders.push(Path::new(path).join("agents"));
            }
        }
    }
    folders
}

fn live_folders(manifest: &Value) -> Vec<String> {
    let Some(plugins) = manifest
        .get("plugins")
        .and_then(Value::as_array)
        .filter(|a| a.iter().all(Value::is_object))
    else {
        return Vec::new();
    };
    plugins
        .iter()
        .take(FILES_LIMIT)
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?;
            // Windows also treats backslashes and drive prefixes as path syntax.
            if name.is_empty() || name.contains(['/', '\\', ':']) || matches!(name, "." | "..") {
                return None;
            }
            Some(match entry.get("generation").and_then(Value::as_i64) {
                Some(generation) if generation > 1 => format!("{name}~g{generation}"),
                _ => name.to_owned(),
            })
        })
        .collect()
}

fn generation(folder: &str) -> (&str, i64) {
    folder
        .rsplit_once("~g")
        .and_then(|(name, n)| Some((name, n.parse().ok()?)))
        .unwrap_or((folder, 1))
}

fn synced_plugin_folders(root: &Path, plugin: &str) -> Vec<PathBuf> {
    let mut accounts = entries(root);
    accounts.sort();
    let mut folders = Vec::new();
    for account in accounts.into_iter().take(20) {
        let base = root.join(account);
        let names =
            if let Some(manifest) = json(&base.join("manifest.json")).filter(Value::is_object) {
                live_folders(&manifest)
            } else {
                let mut names = entries(&base);
                names.sort_by(|a, b| generation(b).cmp(&generation(a)));
                names.truncate(FILES_LIMIT);
                names
            };
        for name in names {
            let folder = base.join(name);
            if ["plugin.json", ".claude-plugin/plugin.json"]
                .iter()
                .any(|file| {
                    json(&folder.join(file))
                        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_owned))
                        .as_deref()
                        == Some(plugin)
                })
            {
                folders.push(folder.join("agents"));
            }
        }
    }
    folders
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let mut random = [0; 16];
            getrandom::fill(&mut random).unwrap();
            let root = std::env::temp_dir().join(format!(
                "vorssaint-agent-colors-{:x}",
                u128::from_ne_bytes(random)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn color(&self, kind: &str, cwd: &str) -> Option<String> {
            lookup(kind, &self.0.join(cwd).to_string_lossy(), &self.0)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_definition_heads_like_macos() {
        for ending in ["\n", "\r\n", "\r"] {
            let text = format!(
                "\u{feff}---{ending}Name: 'frontend' # name{ending}color: \"blue\" # shell{ending}---{ending}color: red"
            );
            assert_eq!(
                frontmatter(text.as_bytes()),
                (Some("frontend".into()), Some("blue".into()))
            );
        }
        for text in [
            "color: blue",
            "---\nname: x\ncolor: blue",
            "\n---\ncolor: blue\n---",
            "---\ncolor: # empty\n---",
        ] {
            assert_eq!(frontmatter(text.as_bytes()), (None, None), "{text}");
        }
        for (raw, expected) in [
            (" green\t# comment", "green"),
            ("green#tight", "green#tight"),
            ("'blue'", "blue"),
            ("\"blue\"#tight", "\"blue\"#tight"),
            ("'blue' trailing", "'blue' trailing"),
            ("'unterminated", "'unterminated"),
            ("# empty", ""),
            ("''", ""),
        ] {
            assert_eq!(scalar(raw), expected);
        }
        let long = format!("---\nname: x\n{}\ncolor: blue\n---", "x".repeat(HEAD_LIMIT));
        assert_eq!(frontmatter(long.as_bytes()), (None, None));
        assert_eq!(frontmatter(&[0xff, 0xfe]), (None, None));
    }

    #[test]
    fn resolves_project_ancestors_and_both_user_homes() {
        let f = Fixture::new();
        f.write(
            ".claude/agents/frontend.md",
            "---\nname: frontend\ncolor: blue\n---",
        );
        f.write(
            "code/.claude/agents/a.md",
            "---\nname: frontend\ncolor: green\n---",
        );
        f.write(
            "code/app/.claude/agents/local.md",
            "---\nname: frontend\n---",
        );
        assert_eq!(f.color("frontend", "elsewhere").as_deref(), Some("blue"));
        assert_eq!(f.color("frontend", "code/other").as_deref(), Some("green"));
        assert_eq!(
            f.color("frontend", "code/app"),
            None,
            "colorless local definitions shadow user colors"
        );
        f.write(".config/claude/agents/fallback.md", "---\ncolor: pink\n---");
        assert_eq!(f.color("fallback", "code/app").as_deref(), Some("pink"));
        f.write(
            ".claude/agents/alias.md",
            "---\nname: named\ncolor: red\n---",
        );
        assert_eq!(f.color("alias", ""), None);
        assert_eq!(f.color("named", "").as_deref(), Some("red"));
        // An explicit name wins over a filename fallback in the same folder.
        f.write(".claude/agents/named.md", "---\ncolor: yellow\n---");
        assert_eq!(f.color("named", "").as_deref(), Some("red"));
        f.write(".claude/agents/plain.md", "no frontmatter");
        fs::create_dir_all(f.0.join(".claude/agents/directory.md")).unwrap();
        assert_eq!(f.color("plain", ""), None);
        assert_eq!(f.color("directory", ""), None);
        assert_eq!(f.color("missing", ""), None);
    }

    #[test]
    fn resolves_installed_plugins_without_local_fallback() {
        let f = Fixture::new();
        f.write(
            "install/agents/a.md",
            "---\nname: frontend\ncolor: purple\n---",
        );
        f.write(".claude/agents/frontend.md", "---\ncolor: blue\n---");
        f.write(
            ".claude/plugins/installed_plugins.json",
            &json!({"plugins": {
                "tools@market": [{"installPath": f.0.join("install")}],
                "tools-extra@market": [{"installPath": f.0.join("wrong")}],
                "broken@market": "bad"
            }})
            .to_string(),
        );
        assert_eq!(f.color("tools:frontend", "").as_deref(), Some("purple"));
        assert_eq!(f.color("other:frontend", ""), None);
        assert_eq!(f.color("broken:frontend", ""), None);
        f.write(".claude/plugins/installed_plugins.json", "invalid json");
        assert_eq!(f.color("tools:frontend", ""), None);
        fs::remove_file(f.0.join(".claude/plugins/installed_plugins.json")).unwrap();
        f.write(
            ".config/claude/plugins/installed_plugins.json",
            &json!({"plugins": {
                "tools@market": [{"installPath": f.0.join("install")}]
            }})
            .to_string(),
        );
        assert_eq!(f.color("tools:frontend", "").as_deref(), Some("purple"));
    }

    #[test]
    fn synced_plugins_use_live_manifest_or_newest_generation() {
        let f = Fixture::new();
        let base = ".claude/plugins/synced/account";
        for (folder, color) in [("team", "green"), ("team~g2", "red"), ("team~g10", "blue")] {
            f.write(
                &format!("{base}/{folder}/.claude-plugin/plugin.json"),
                r#"{"name":"team"}"#,
            );
            f.write(
                &format!("{base}/{folder}/agents/lead.md"),
                &format!("---\ncolor: {color}\n---"),
            );
        }
        assert_eq!(f.color("team:lead", "").as_deref(), Some("blue"));
        f.write(
            &format!("{base}/manifest.json"),
            r#"{"plugins":[{"name":"team"}]}"#,
        );
        assert_eq!(f.color("team:lead", "").as_deref(), Some("green"));
        f.write(
            &format!("{base}/manifest.json"),
            r#"{"plugins":[{"name":"team","generation":2}]}"#,
        );
        assert_eq!(f.color("team:lead", "").as_deref(), Some("red"));
        f.write(&format!("{base}/manifest.json"), r#"{"plugins":[]}"#);
        assert_eq!(f.color("team:lead", ""), None, "stale copies are skipped");
        f.write(
            ".config/claude/plugins/synced/account/hash/plugin.json",
            r#"{"name":"top"}"#,
        );
        f.write(
            ".config/claude/plugins/synced/account/hash/agents/lead.md",
            "---\ncolor: cyan\n---",
        );
        assert_eq!(f.color("top:lead", "").as_deref(), Some("cyan"));
        assert_eq!(
            live_folders(&json!({"plugins": [
                {"name": "../bad"}, {"name": "..\\bad"}, {"name": "C:bad"},
                {"name": ".."}, {"name": ""}, {"name": "ok", "generation": 3}
            ]})),
            vec!["ok~g3"]
        );
    }

    #[test]
    fn decorates_snapshot_subagents_and_serializes_their_colors() {
        use super::super::engine::Engine;
        use super::super::models::{ContentPolicy, TranscriptFact};

        let f = Fixture::new();
        f.write(
            ".claude/agents/frontend.md",
            "---\nname: frontend\ncolor: blue\n---",
        );
        let record = registry::record(
            &serde_json::to_vec(&json!({
                "pid": 1, "sessionId": "s", "cwd": f.0, "status": "busy"
            }))
            .unwrap(),
            registry::WINDOWS_PID_DOMAINS,
        )
        .unwrap();
        let mut engine = Engine::new(ContentPolicy::default());
        engine.ingest_records(vec![record], 100.0);
        for (id, kind) in [("a", "frontend"), ("b", "Explore")] {
            engine.ingest_facts(
                "s",
                &[TranscriptFact::SubagentInfo {
                    id: id.into(),
                    agent_type: kind.into(),
                    description: None,
                }],
                100.0,
            );
        }
        let mut session = engine.snapshot(100.0).sessions.remove(0);
        let mut colors = AgentColors::default();
        colors.decorate(&mut session, &f.0, 100.0);
        let value = serde_json::to_value(&session).unwrap();
        assert_eq!(value["subagents"][0]["color"], "blue");
        assert!(value["subagents"][1].get("color").is_none());
        session.subagents.clear();
        let empty = session.clone();
        colors.decorate(&mut session, &f.0, 101.0);
        assert_eq!(session, empty);
    }

    #[test]
    fn caches_colors_and_misses_and_refreshes_after_expiry_or_clock_reversal() {
        let f = Fixture::new();
        let mut colors = AgentColors::default();
        assert_eq!(colors.color("frontend", "", &f.0, 100.0), None);
        f.write(".claude/agents/frontend.md", "---\ncolor: blue\n---");
        assert_eq!(colors.color("frontend", "", &f.0, 105.0), None);
        assert_eq!(
            colors.color("frontend", "", &f.0, 130.0).as_deref(),
            Some("blue")
        );
        f.write(".claude/agents/frontend.md", "---\ncolor: red\n---");
        assert_eq!(
            colors.color("frontend", "", &f.0, 135.0).as_deref(),
            Some("blue")
        );
        assert_eq!(
            colors.color("frontend", "", &f.0, 99.0).as_deref(),
            Some("red")
        );
        f.write(".claude/agents/Agent.md", "---\ncolor: blue\n---");
        assert_eq!(colors.color(Subagent::UNTYPED, "", &f.0, 100.0), None);
        assert_eq!(colors.color("", "", &f.0, 100.0), None);
    }
}
