// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The parts of a diagnostics file (#230) and the files they are written
//! to. The screen before saving, the README inside the file and the files
//! written all come from this one list, so what the person is shown is what
//! the file holds. The same parts, names and fields as the Mac app's
//! `DiagnosticsReport.swift`, so one reader works for every report.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

pub const FORMAT: u32 = 1;
/// The folder every file sits in inside the zip.
pub const FOLDER: &str = "openbar-diagnostics";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Part {
    System,
    Features,
    Agents,
    Events,
    Hosts,
    Logs,
}

impl Part {
    pub const ALL: [Part; 6] = [
        Part::System,
        Part::Features,
        Part::Agents,
        Part::Events,
        Part::Hosts,
        Part::Logs,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Part::System => "system",
            Part::Features => "features",
            Part::Agents => "agents",
            Part::Events => "events",
            Part::Hosts => "hosts",
            Part::Logs => "logs",
        }
    }

    pub fn from_key(key: &str) -> Option<Part> {
        Part::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Parts the person can leave out; the others are always in.
    pub fn optional(self) -> bool {
        matches!(self, Part::Events | Part::Hosts | Part::Logs)
    }

    /// The file this part is written to; the logs are a folder of files.
    pub fn file_name(self) -> String {
        match self {
            Part::Logs => "logs/".into(),
            _ => format!("{}.json", self.key()),
        }
    }

    /// What the part holds, in the README every report carries.
    pub fn readme(self) -> String {
        match self {
            Part::System => "Openbar version and update channel, the system version, the CPU type and the displays.".into(),
            Part::Features => "Which of Openbar's features are on.".into(),
            Part::Agents => "Each coding agent: installed or not, its version, the state of Openbar's hooks in it and where its settings live.".into(),
            Part::Events => format!(
                "The last {} events Openbar heard from sessions: event type, agent, time and state change.",
                super::journal::CAPACITY
            ),
            Part::Hosts => "The terminals and editors found, and Claude, Codex and remote host names (labels only).".into(),
            Part::Logs => format!("Openbar's own logs from the last {} days.", super::log::EXPORT_DAYS),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub channel: String,
    pub developer_build: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Os {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
    /// Linux only: X11 or Wayland, and the desktop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub width: i64,
    pub height: i64,
    pub scale: f64,
    pub main: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_in: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notch: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct System {
    pub app: App,
    pub os: Os,
    pub cpu: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub language: String,
    pub displays: Vec<Display>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feature {
    pub id: String,
    pub group: String,
    pub on: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hooks {
    /// The Claude account the hooks are in, by its label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// How Openbar hears the agent, where that isn't hooks (Codex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub hooks: Vec<Hooks>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Editor {
    pub name: String,
    /// Openbar's extension for typing replies: installed, outdated or missing.
    pub bridge: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub label: String,
    pub state: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hosts {
    pub terminals: Vec<String>,
    pub editors: Vec<Editor>,
    /// How many sessions run in each terminal or editor now.
    pub in_use: BTreeMap<String, u32>,
    pub claude_accounts: Vec<String>,
    pub codex_accounts: Vec<String>,
    pub remote_hosts: Vec<Remote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub created: String,
    pub platform: String,
    pub app: String,
    pub parts: Vec<String>,
    pub files: Vec<String>,
}

/// What a diagnostics file is made from, once collected.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub system: System,
    pub features: Vec<Feature>,
    pub agents: Vec<Agent>,
    pub events: Vec<super::journal::Event>,
    pub hosts: Hosts,
    /// Log files by name, oldest first.
    pub logs: Vec<(String, String)>,
}

/// `openbar-diagnostics-20261116-1530.zip`, in local time.
pub fn file_name(now: DateTime<Local>) -> String {
    now.format("openbar-diagnostics-%Y%m%d-%H%M.zip").to_string()
}

/// Every file of the report, by its path inside the zip's folder, masked.
/// The parts not chosen are left out entirely; the README and the manifest
/// say which went in.
pub fn files(
    chosen: &[Part],
    input: &Input,
    platform: &str,
    now: DateTime<Utc>,
    home: &str,
) -> Vec<(String, Vec<u8>)> {
    let parts: Vec<Part> = Part::ALL
        .into_iter()
        .filter(|p| !p.optional() || chosen.contains(p))
        .collect();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for part in &parts {
        match part {
            Part::System => files.push((part.file_name(), json(&input.system, home))),
            Part::Features => files.push((
                part.file_name(),
                json(&serde_json::json!({ "features": input.features }), home),
            )),
            Part::Agents => files.push((
                part.file_name(),
                json(&serde_json::json!({ "agents": input.agents }), home),
            )),
            Part::Events => files.push((
                part.file_name(),
                json(&serde_json::json!({ "events": input.events }), home),
            )),
            Part::Hosts => files.push((part.file_name(), json(&input.hosts, home))),
            Part::Logs => {
                for (name, text) in &input.logs {
                    files.push((format!("logs/{name}"), redact_lines(text, home).into_bytes()));
                }
            }
        }
    }
    let app = format!("{} {}", input.system.app.name, input.system.app.version);
    let created = super::log::timestamp(now);
    let logs: Vec<String> = input.logs.iter().map(|(n, _)| n.clone()).collect();
    let readme = readme(&parts, platform, &app, &created, &logs);
    files.insert(0, ("README.txt".into(), readme.into_bytes()));
    let mut listed: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    listed.push("manifest.json".into());
    listed.sort();
    let manifest = Manifest {
        format: FORMAT,
        created,
        platform: platform.into(),
        app,
        parts: parts.iter().map(|p| p.key().to_string()).collect(),
        files: listed,
    };
    files.insert(1, ("manifest.json".into(), json(&manifest, home)));
    files
}

pub fn readme(parts: &[Part], platform: &str, app: &str, created: &str, logs: &[String]) -> String {
    let mut lines = vec![
        "Openbar diagnostics".to_string(),
        String::new(),
        format!("Made by {app} on {platform}, {created}."),
        String::new(),
        "What is inside:".to_string(),
    ];
    for part in parts {
        if *part == Part::Logs {
            let none = if logs.is_empty() { " (none were written yet)" } else { "" };
            lines.push(format!("- logs/: {}{none}", part.readme()));
        } else {
            lines.push(format!("- {}: {}", part.file_name(), part.readme()));
        }
    }
    let left: Vec<Part> = Part::ALL.into_iter().filter(|p| !parts.contains(p)).collect();
    if !left.is_empty() {
        lines.push(String::new());
        lines.push("Left out before saving:".into());
        for part in left {
            lines.push(format!("- {}: {}", part.file_name(), part.readme()));
        }
    }
    lines.extend([
        String::new(),
        "Never included: prompts, replies, commands and file contents.".into(),
        "Masked everywhere: tokens and API keys (<secret>), email addresses (<email>) and the home folder (~).".into(),
        String::new(),
        "Openbar sent this file nowhere. Whoever holds it decides where it goes.".into(),
        String::new(),
    ]);
    lines.join("\n")
}

pub fn json<T: Serialize>(value: &T, home: &str) -> Vec<u8> {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
    super::redact::redact(&text, home).into_bytes()
}

/// A log masked once more, line by line, in case it came from an older
/// build or was edited.
pub fn redact_lines(text: &str, home: &str) -> String {
    text.split('\n')
        .map(|line| super::redact::redact(line, home))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The version in a command's `--version` output, like `2.1.309` or `0.164.0-beta.2`.
pub fn version_in(output: &str) -> Option<String> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"\d+\.\d+(?:\.\d+)?(?:-[0-9A-Za-z.]+)?").unwrap())
        .find(output)
        .map(|m| m.as_str().to_string())
}

/// Writes the files into a zip whose one folder is `openbar-diagnostics`,
/// beside `path` first and then over it, so a failed write leaves nothing half done.
pub fn write_zip(files: &[(String, Vec<u8>)], path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "diagnostics.zip".into());
    let partial = path.with_file_name(format!(".{name}.partial"));
    let result = (|| -> Result<(), String> {
        let file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        zip.add_directory(format!("{FOLDER}/"), options)
            .map_err(|e| e.to_string())?;
        let mut folders: Vec<String> = Vec::new();
        for (path, data) in files {
            if let Some((folder, _)) = path.rsplit_once('/')
                && !folders.iter().any(|f| f == folder)
            {
                folders.push(folder.to_string());
                zip.add_directory(format!("{FOLDER}/{folder}/"), options)
                    .map_err(|e| e.to_string())?;
            }
            zip.start_file(format!("{FOLDER}/{path}"), options)
                .map_err(|e| e.to_string())?;
            zip.write_all(data).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
        std::fs::rename(&partial, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::journal::Event;

    fn input() -> Input {
        Input {
            system: System {
                app: App {
                    name: "Openbar".into(),
                    version: "0.4.0".into(),
                    channel: "stable".into(),
                    ..Default::default()
                },
                os: Os {
                    name: "Windows".into(),
                    version: "11 24H2".into(),
                    ..Default::default()
                },
                cpu: "x86_64".into(),
                language: "en-US".into(),
                displays: vec![Display {
                    width: 1920,
                    height: 1080,
                    scale: 1.0,
                    main: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            features: vec![Feature {
                id: "sessions".into(),
                group: "island".into(),
                on: true,
            }],
            agents: vec![Agent {
                id: "gemini".into(),
                name: "Gemini CLI".into(),
                installed: true,
                version: Some("0.20.0".into()),
                status: None,
                hooks: vec![Hooks {
                    account: None,
                    state: "removedByHand".into(),
                    config_path: Some(r"C:\Users\jane\.gemini\settings.json".into()),
                }],
            }],
            events: vec![Event {
                time: "2025-11-16T15:30:02.123Z".into(),
                agent: "claude".into(),
                event: "toolStarted".into(),
                detail: Some("Bash".into()),
                session: "abcdef12".into(),
                from: Some("generating".into()),
                to: Some("runningTool".into()),
            }],
            hosts: Hosts {
                terminals: vec!["Windows Terminal".into()],
                claude_accounts: vec!["jane@example.com".into()],
                ..Default::default()
            },
            logs: vec![(
                "openbar-2025-11-16.log".into(),
                "2025-11-16T15:30:02.123Z ERROR [hooks] failed in C:\\Users\\jane\\.claude: token=CANARY123 sk-ant-api03-CANARYKEY99\n".into(),
            )],
        }
    }

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_763_307_002, 0).unwrap()
    }

    #[test]
    fn every_part_has_its_file_and_nothing_private() {
        let all = files(&Part::ALL, &input(), "Windows", at(), r"C:\Users\jane");
        let paths: Vec<&str> = all.iter().map(|(p, _)| p.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            [
                "README.txt",
                "agents.json",
                "events.json",
                "features.json",
                "hosts.json",
                "logs/openbar-2025-11-16.log",
                "manifest.json",
                "system.json"
            ]
        );
        let manifest: Manifest = serde_json::from_slice(&all[1].1).unwrap();
        assert_eq!(manifest.files, sorted);
        assert_eq!(manifest.parts, ["system", "features", "agents", "events", "hosts", "logs"]);
        let everything: String = all
            .iter()
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
            .collect();
        for canary in ["CANARY123", "CANARYKEY99", r"C:\Users\jane", r"C:\\Users\\jane", "jane@example.com"] {
            assert!(!everything.contains(canary), "{canary} leaked");
        }
        assert!(everything.contains("removedByHand"));
    }

    #[test]
    fn switched_off_parts_stay_out() {
        let required = files(&[], &input(), "Linux", at(), "/home/jane");
        let paths: Vec<&str> = required.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            ["README.txt", "manifest.json", "system.json", "features.json", "agents.json"]
        );
        let readme = String::from_utf8_lossy(&required[0].1);
        assert!(readme.contains("Left out before saving:") && readme.contains("- logs/:"));
        let no_logs = files(&[Part::Events, Part::Hosts], &input(), "Linux", at(), "/home/jane");
        assert!(!no_logs.iter().any(|(p, _)| p.starts_with("logs/")));
    }

    #[test]
    fn the_zip_opens_to_one_folder() {
        let dir = std::env::temp_dir().join(format!("openbar-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("d.zip");
        let all = files(&Part::ALL, &input(), "Windows", at(), r"C:\Users\jane");
        write_zip(&all, &path).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().all(|n| n.starts_with("openbar-diagnostics/")));
        assert!(names.contains(&"openbar-diagnostics/logs/openbar-2025-11-16.log".to_string()));
        assert!(!dir.join(".d.zip.partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn versions_and_names() {
        assert_eq!(version_in("2.1.309 (Claude Code)").as_deref(), Some("2.1.309"));
        assert_eq!(version_in("codex-cli 0.164.0\n").as_deref(), Some("0.164.0"));
        assert_eq!(version_in("v1.2.3-beta.1").as_deref(), Some("1.2.3-beta.1"));
        assert_eq!(version_in("no version"), None);
        assert_eq!(Part::from_key("logs"), Some(Part::Logs));
        assert_eq!(Part::from_key("nope"), None);
    }
}
