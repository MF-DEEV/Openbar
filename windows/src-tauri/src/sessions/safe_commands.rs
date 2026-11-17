// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Commands a new session may run from its first turn without asking (#270):
//! the project's build, test and lint, git's read-only three, and any the
//! person adds. Claude Code gets them as `--allowedTools` rules; Codex threads
//! get their approvals answered "for this session" by the island. Anything
//! else still asks. The Mac's SafeCommands.swift does the same; both run
//! `safe_command_cases.json`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub command: String,
    /// build, test, lint, git or custom: the mark its chip shows.
    pub kind: &'static str,
}

/// The agents whose sessions can start with the list.
pub fn supports(agent: &str) -> bool {
    agent == "claude" || agent == "codex"
}

fn entry(command: &str, kind: &'static str) -> Entry {
    Entry {
        command: command.to_owned(),
        kind,
    }
}

/// What a folder suggests, read from the files at its top: a package
/// manager's scripts, Cargo, SwiftPM, Go or make; then git's three.
pub fn suggested(folder: &str) -> Vec<Entry> {
    let root = Path::new(folder);
    let has = |name: &str| root.join(name).exists();
    let mut found: Vec<Entry> = Vec::new();
    let add = |found: &mut Vec<Entry>, command: &str, kind: &'static str| {
        if !found.iter().any(|e| e.command == command) {
            found.push(entry(command, kind));
        }
    };
    if let Some(json) = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        let manager = if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else {
            "npm"
        };
        for (script, kind) in [
            ("build", "build"),
            ("test", "test"),
            ("lint", "lint"),
            ("typecheck", "lint"),
        ] {
            if json.pointer(&format!("/scripts/{script}")).is_some() {
                add(&mut found, &format!("{manager} run {script}"), kind);
            }
        }
    }
    if has("Cargo.toml") {
        add(&mut found, "cargo build", "build");
        add(&mut found, "cargo test", "test");
        add(&mut found, "cargo clippy", "lint");
    }
    if has("Package.swift") {
        add(&mut found, "swift build", "build");
        add(&mut found, "swift test", "test");
    }
    if has("go.mod") {
        add(&mut found, "go build", "build");
        add(&mut found, "go test", "test");
        add(&mut found, "go vet", "lint");
    }
    if let Ok(makefile) = std::fs::read_to_string(root.join("Makefile")) {
        let targets: HashSet<&str> = makefile
            .lines()
            .filter(|line| !line.starts_with('\t') && !line.starts_with(' '))
            .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim()))
            .collect();
        if !found.iter().any(|e| e.kind == "build") {
            add(&mut found, "make", "build");
        }
        if targets.contains("test") {
            add(&mut found, "make test", "test");
        }
        if targets.contains("lint") {
            add(&mut found, "make lint", "lint");
        }
    }
    found.extend(["git status", "git diff", "git log"].map(|c| entry(c, "git")));
    found
}

/// Characters that chain, redirect or substitute: a command with any of them
/// is never one of the list, so `git status; rm -rf ~` still asks.
const JOINERS: &[char] = &[
    ';', '&', '|', '<', '>', '`', '$', '(', ')', '\n', '\r', '{', '}',
];

/// A command a person may add: one line, plain words, no joiners, short.
pub fn cleaned(text: &str) -> Option<String> {
    let command = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let refused = command.is_empty()
        || command.chars().count() > 80
        || command.contains(JOINERS)
        || command.contains(['*', '"', '\'', '\\']);
    (!refused).then_some(command)
}

/// Claude Code's rules for the list: each a prefix of a Bash command.
pub fn claude_rules(commands: &[String]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| cleaned(c))
        .map(|c| format!("Bash({c}:*)"))
        .collect()
}

/// Splits a line into words as a POSIX shell would for plain text: quotes
/// group, nothing else is read. None when a quote is left open.
fn words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => word.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                in_word = true;
            }
            None if ch.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            None => {
                word.push(ch);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

/// The script of `<shell> [-l] -c '<script>'` with nothing after it, as
/// Codex runs commands on macOS and Linux.
fn posix_script(command: &str) -> Option<String> {
    let words = words(command.trim())?;
    let shell = words.first()?.rsplit(['/', '\\']).next()?;
    if !["sh", "bash", "zsh", "dash"].contains(&shell) || words.len() < 3 {
        return None;
    }
    let options = &words[1..words.len() - 1];
    let allowed = ["-c", "-lc", "-cl", "-l", "--login"];
    let last = options.last()?;
    (options.iter().all(|o| allowed.contains(&o.as_str()))
        && ["-c", "-lc", "-cl"].contains(&last.as_str()))
    .then(|| words[words.len() - 1].clone())
}

/// Whether a command a Codex approval names is one of the list: its script,
/// unwrapped from the shell Codex runs it in, has no joiners and starts with
/// all the words of an entry.
pub fn matches(command: &str, allowed: &[String]) -> bool {
    let script = super::codex::state::approval_script(command)
        .or_else(|| posix_script(command))
        .unwrap_or_else(|| command.to_owned());
    if script.contains(JOINERS) {
        return false;
    }
    let Some(words) = words(&script) else {
        return false;
    };
    allowed.iter().filter_map(|c| cleaned(c)).any(|entry| {
        let wanted: Vec<&str> = entry.split(' ').collect();
        words.len() >= wanted.len() && words.iter().zip(&wanted).all(|(a, b)| a == b)
    })
}

/// Which Codex threads run with a list, and lists waiting for the thread a
/// console is about to start.
#[derive(Default)]
pub struct Grants {
    threads: HashMap<String, Vec<String>>,
    waiting: Vec<Waiting>,
}

struct Waiting {
    cwd: String,
    commands: Vec<String>,
    until: Instant,
    /// The threads known when it was made: the console's one is new.
    known: HashSet<String>,
}

pub const CLAIM_WINDOW: Duration = Duration::from_secs(120);

fn standard(path: &str) -> String {
    let path = path.trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        path.replace('/', "\\").to_lowercase()
    } else {
        path.strip_prefix("/private")
            .filter(|p| p.starts_with("/tmp/") || p.starts_with("/var/"))
            .unwrap_or(path)
            .to_owned()
    }
}

impl Grants {
    pub fn grant(&mut self, thread: &str, commands: Vec<String>) {
        if !commands.is_empty() {
            self.threads.insert(thread.to_owned(), commands);
        }
    }

    pub fn expect(
        &mut self,
        cwd: &str,
        commands: Vec<String>,
        now: Instant,
        known: HashSet<String>,
    ) {
        if !commands.is_empty() {
            self.waiting.push(Waiting {
                cwd: standard(cwd),
                commands,
                until: now + CLAIM_WINDOW,
                known,
            });
        }
    }

    /// The list a thread runs with: its own, or a waiting one it is the first
    /// new thread in that folder to claim.
    pub fn commands(&mut self, thread: &str, cwd: &str, now: Instant) -> Option<Vec<String>> {
        if let Some(own) = self.threads.get(thread) {
            return Some(own.clone());
        }
        self.waiting.retain(|w| w.until >= now);
        let cwd = standard(cwd);
        let index = self
            .waiting
            .iter()
            .position(|w| w.cwd == cwd && !w.known.contains(thread))?;
        let claimed = self.waiting.remove(index);
        self.threads
            .insert(thread.to_owned(), claimed.commands.clone());
        Some(claimed.commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases() -> Value {
        serde_json::from_str(include_str!("safe_command_cases.json")).unwrap()
    }

    #[test]
    fn shared_cases() {
        let json = cases();
        let allowed: Vec<String> = json["allowed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        for case in json["matches"].as_array().unwrap() {
            let command = case["command"].as_str().unwrap();
            assert_eq!(
                matches(command, &allowed),
                case["expect"].as_bool().unwrap(),
                "`{command}`"
            );
        }
        for case in json["cleaned"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            assert_eq!(
                cleaned(text).as_deref(),
                case["expect"].as_str(),
                "`{text}`"
            );
        }
    }

    #[test]
    fn claude_rules_and_support() {
        let commands = ["git status", "a; b", "npm run test"].map(str::to_owned);
        assert_eq!(
            claude_rules(&commands),
            ["Bash(git status:*)", "Bash(npm run test:*)"]
        );
        assert!(supports("claude") && supports("codex") && !supports("gemini"));
    }

    #[test]
    fn suggested_by_the_project() {
        let root = std::env::temp_dir().join(format!("safe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"build":"tsc","test":"vitest","lint":"eslint .","dev":"vite"}}"#,
        )
        .unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let found: Vec<String> = suggested(root.to_str().unwrap())
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(
            found,
            [
                "pnpm run build",
                "pnpm run test",
                "pnpm run lint",
                "cargo build",
                "cargo test",
                "cargo clippy",
                "git status",
                "git diff",
                "git log"
            ]
        );
        let bare: Vec<String> = suggested(root.join("missing").to_str().unwrap())
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(bare, ["git status", "git diff", "git log"]);
        let make = root.join("m");
        std::fs::create_dir_all(&make).unwrap();
        std::fs::write(make.join("Makefile"), "all:\n\tcc x\ntest: all\n\t./t\n").unwrap();
        let made: Vec<String> = suggested(make.to_str().unwrap())
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(&made[..2], ["make", "make test"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn posix_wrappers() {
        let allowed = vec!["git diff".to_owned()];
        assert!(matches("/bin/bash -lc 'git diff --stat'", &allowed));
        assert!(matches("bash -c \"git diff\"", &allowed));
        assert!(!matches("/bin/bash -lc 'git diff' extra", &allowed));
        assert!(!matches("/bin/bash -x -c 'git diff'", &allowed));
    }

    #[test]
    fn grants() {
        let now = Instant::now();
        let mut grants = Grants::default();
        grants.grant("t1", vec!["git diff".into()]);
        assert_eq!(
            grants.commands("t1", "/w", now),
            Some(vec!["git diff".to_owned()])
        );
        grants.expect(
            "/w/app/",
            vec!["make".into()],
            now,
            HashSet::from(["old".to_owned()]),
        );
        assert_eq!(grants.commands("old", "/w/app", now), None);
        assert_eq!(grants.commands("t2", "/w/other", now), None);
        assert_eq!(
            grants.commands("t3", "/w/app", now + CLAIM_WINDOW + Duration::from_secs(1)),
            None
        );
        grants.expect("/w/app", vec!["make".into()], now, HashSet::new());
        assert_eq!(
            grants.commands("t4", "/w/app", now + Duration::from_secs(5)),
            Some(vec!["make".to_owned()])
        );
        assert_eq!(
            grants.commands("t5", "/w/app", now + Duration::from_secs(6)),
            None
        );
    }
}
