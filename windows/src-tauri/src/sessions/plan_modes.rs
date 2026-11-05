// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The permission mode a plan is approved into, as Claude Code's own plan
//! dialog offers it (ClaudePlanModes.swift).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A mode a plan is approved into. Its name is the mode Claude Code takes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlanMode {
    #[serde(rename = "bypassPermissions")]
    Bypass,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    /// Claude asks before each edit, as Claude Code's "manually approve edits".
    #[serde(rename = "default")]
    Manual,
}

impl PlanMode {
    pub const ALL: [PlanMode; 4] = [
        PlanMode::Bypass,
        PlanMode::Auto,
        PlanMode::AcceptEdits,
        PlanMode::Manual,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlanMode::Bypass => "bypassPermissions",
            PlanMode::Auto => "auto",
            PlanMode::AcceptEdits => "acceptEdits",
            PlanMode::Manual => "default",
        }
    }
}

/// What the session and its settings tell about the modes it can use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    /// Modes the session has been in, from its transcript.
    pub seen: BTreeSet<String>,
    /// A settings file starts sessions in auto mode or configures it.
    pub auto_configured: bool,
    /// A settings file turns auto mode off.
    pub auto_disabled: bool,
}

/// Which modes a plan is approved into, as Claude Code decides them: one
/// that lets Claude work on its own (bypass when the session runs with it,
/// else auto mode where it can be used, else auto-accept edits), then manual
/// approval. The first one is what a plain approval picks.
pub fn offered(reading: &Reading) -> Vec<PlanMode> {
    // A session only runs in bypass when it was launched able to.
    if reading.seen.contains(PlanMode::Bypass.name()) {
        return vec![PlanMode::Bypass, PlanMode::Manual];
    }
    let auto = !reading.auto_disabled
        && (reading.seen.contains(PlanMode::Auto.name()) || reading.auto_configured);
    vec![
        if auto {
            PlanMode::Auto
        } else {
            PlanMode::AcceptEdits
        },
        PlanMode::Manual,
    ]
}

/// Where Claude Code reads the settings an administrator manages.
pub fn managed_settings() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![PathBuf::from(
            r"C:\Program Files\ClaudeCode\managed-settings.json",
        )]
    } else {
        vec![PathBuf::from(
            "/Library/Application Support/ClaudeCode/managed-settings.json",
        )]
    }
}

/// Reads the session's transcript and the settings Claude Code reads. The
/// config folder is the one the transcript sits in, so a session run with
/// CLAUDE_CONFIG_DIR is read from its own.
pub fn read(transcript: Option<&str>, cwd: Option<&str>, managed: &[PathBuf]) -> Reading {
    let mut reading = Reading::default();
    let mut settings: Vec<PathBuf> = managed.to_vec();
    if let Some(transcript) = transcript.filter(|t| !t.is_empty()) {
        reading.seen = modes_in_transcript(Path::new(transcript));
        // <config>/projects/<project>/<session>.jsonl
        if let Some(config) = Path::new(transcript)
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
        {
            settings.push(config.join("settings.json"));
        }
    }
    if let Some(cwd) = cwd.filter(|c| !c.is_empty()) {
        let project = Path::new(cwd).join(".claude");
        settings.push(project.join("settings.json"));
        settings.push(project.join("settings.local.json"));
    }
    for path in settings {
        let Some(json) = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice::<Value>(&data).ok())
        else {
            continue;
        };
        let permission = |key: &str| {
            json.get("permissions")
                .and_then(|p| p.get(key))
                .and_then(Value::as_str)
        };
        if permission("disableAutoMode") == Some("disable")
            || json.get("disableAutoMode").and_then(Value::as_str) == Some("disable")
        {
            reading.auto_disabled = true;
        }
        if permission("defaultMode") == Some(PlanMode::Auto.name())
            || json.get("autoMode").is_some_and(Value::is_object)
        {
            reading.auto_configured = true;
        }
    }
    reading
}

/// The modes a transcript records the session in. Claude Code writes them
/// compact, as `"permissionMode":"auto"`, on its mode lines and prompts.
pub fn modes_in_transcript(path: &Path) -> BTreeSet<String> {
    let Ok(data) = std::fs::read(path) else {
        return BTreeSet::new();
    };
    PlanMode::ALL
        .iter()
        .filter(|mode| {
            let needle = format!(r#""permissionMode":"{}""#, mode.name());
            memchr::memmem::find(&data, needle.as_bytes()).is_some()
        })
        .map(|mode| mode.name().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(seen: &[&str], configured: bool, disabled: bool) -> Reading {
        Reading {
            seen: seen.iter().map(|s| s.to_string()).collect(),
            auto_configured: configured,
            auto_disabled: disabled,
        }
    }

    #[test]
    fn offers_what_the_plan_dialog_offers() {
        assert_eq!(
            offered(&Reading::default()),
            vec![PlanMode::AcceptEdits, PlanMode::Manual]
        );
        assert_eq!(
            offered(&reading(&["auto", "plan"], false, false)),
            vec![PlanMode::Auto, PlanMode::Manual]
        );
        assert_eq!(
            offered(&reading(&[], true, false)),
            vec![PlanMode::Auto, PlanMode::Manual]
        );
        assert_eq!(
            offered(&reading(&["auto"], true, true)),
            vec![PlanMode::AcceptEdits, PlanMode::Manual]
        );
        assert_eq!(
            offered(&reading(&["bypassPermissions", "auto"], false, false)),
            vec![PlanMode::Bypass, PlanMode::Manual]
        );
        assert_eq!(serde_json::to_value(PlanMode::Manual).unwrap(), "default");
        assert_eq!(
            serde_json::to_value(PlanMode::Bypass).unwrap(),
            "bypassPermissions"
        );
        for mode in PlanMode::ALL {
            assert_eq!(serde_json::to_value(mode).unwrap(), mode.name());
        }
    }

    #[test]
    fn reads_the_transcript_and_settings() {
        let root = std::env::temp_dir().join(format!("plan-modes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let config = root.join("config");
        let folder = config.join("projects").join("-work");
        let cwd = root.join("work");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(cwd.join(".claude")).unwrap();
        let transcript = folder.join("s.jsonl");
        std::fs::write(
            &transcript,
            "{\"type\":\"permission-mode\",\"permissionMode\":\"auto\",\"sessionId\":\"s\"}\n{\"type\":\"user\",\"permissionMode\":\"plan\"}\n",
        )
        .unwrap();
        let t = transcript.to_str();
        let c = cwd.to_str();
        assert_eq!(read(t, c, &[]), reading(&["auto"], false, false));

        std::fs::write(
            config.join("settings.json"),
            r#"{"permissions":{"defaultMode":"auto"}}"#,
        )
        .unwrap();
        let managed = read(None, None, &[config.join("settings.json")]);
        assert!(managed.auto_configured && managed.seen.is_empty());
        assert!(
            read(t, None, &[]).auto_configured,
            "the user's settings sit in the transcript's config folder"
        );

        std::fs::write(
            cwd.join(".claude").join("settings.local.json"),
            r#"{"permissions":{"disableAutoMode":"disable"}}"#,
        )
        .unwrap();
        let off = read(t, c, &[]);
        assert!(off.auto_disabled);
        assert_eq!(offered(&off), vec![PlanMode::AcceptEdits, PlanMode::Manual]);

        std::fs::write(cwd.join(".claude").join("settings.local.json"), "not json").unwrap();
        let gone = root.join("gone.jsonl");
        assert_eq!(read(gone.to_str(), c, &[]), Reading::default());
        let _ = std::fs::remove_dir_all(&root);
    }
}
