// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Why an ask stays out of the queue's Allow all, or `Safe` when it may join
//! it (#269). Only the queue reads this: every ask can still be allowed on
//! its own. A list of what looks dangerous, not a sandbox: a tool the island
//! does not know, or an ask it could not read, never counts as safe. Mirrors
//! the Mac's PermissionRisk.swift; both run `risk_cases.json`.

use super::file_diff::Change;
use super::models::{Permission, Session};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    Safe,
    /// Not looked at yet; never allowed in bulk.
    #[default]
    Unassessed,
    /// It matches a `deny` or `ask` rule in the Claude Code settings.
    Rule,
    Deletes,
    /// It writes, reads or moves outside the session's project folder.
    Outside,
    Network,
    /// Privileges, processes, permissions or system settings.
    System,
    /// It runs code it builds or fetches: `eval`, a pipe into a shell.
    RunsCode,
    /// A tool the island does not know, an MCP server's included.
    Unfamiliar,
}

impl Risk {
    pub fn name(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Unassessed => "unassessed",
            Risk::Rule => "rule",
            Risk::Deletes => "deletes",
            Risk::Outside => "outside",
            Risk::Network => "network",
            Risk::System => "system",
            Risk::RunsCode => "runsCode",
            Risk::Unfamiliar => "unfamiliar",
        }
    }

    /// The most telling reason first, when an ask has several.
    const ORDER: [Risk; 8] = [
        Risk::Rule,
        Risk::Deletes,
        Risk::Outside,
        Risk::Network,
        Risk::System,
        Risk::RunsCode,
        Risk::Unfamiliar,
        Risk::Unassessed,
    ];

    pub fn worst(risks: &[Risk]) -> Risk {
        Self::ORDER
            .into_iter()
            .find(|r| risks.contains(r))
            .unwrap_or(Risk::Safe)
    }
}

/// What kind of thing a tool does, by the name the island gives it
/// (Claude Code's own, or another agent's mapped onto it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Command,
    Read,
    Write,
    Delete,
    Network,
    Folder,
    System,
    Benign,
    Unknown,
}

impl Kind {
    pub fn of(tool: &str) -> Kind {
        match tool {
            "Bash" | "PowerShell" | "Command" => Kind::Command,
            "Read" | "Glob" | "Grep" | "LS" | "NotebookRead" => Kind::Read,
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Kind::Write,
            "Delete" => Kind::Delete,
            "WebFetch" | "WebSearch" => Kind::Network,
            // OpenCode asks before it works in a folder outside the project.
            "Folder" => Kind::Folder,
            // Codex asking for more sandbox access.
            "Permissions" => Kind::System,
            "Task" | "Agent" | "TodoWrite" | "TodoRead" | "BashOutput" | "Skill" | "TaskCreate"
            | "TaskUpdate" | "TaskGet" | "TaskList" | "TaskOutput" => Kind::Benign,
            _ => Kind::Unknown,
        }
    }
}

/// What an ask would do, judged from what the card shows: the tool, its
/// whole command or path, and the files a change touches.
pub fn assess(permission: &Permission, cwd: &str, rules: &Rules, root: &Paths) -> Risk {
    let full: String = permission
        .content
        .as_ref()
        .map(|c| c.full.to_string())
        .unwrap_or_else(|| permission.detail.clone());
    let _ = cwd;
    let kind = Kind::of(&permission.tool);
    let paths: Vec<String> = match kind {
        Kind::Write | Kind::Delete => {
            let from_diffs: Vec<String> = permission
                .diffs
                .iter()
                .flat_map(|d| std::iter::once(d.path.clone()).chain(d.moved_to.clone()))
                .collect();
            if from_diffs.is_empty() {
                paths_in(&full)
            } else {
                from_diffs
            }
        }
        Kind::Read => paths_in(&full),
        _ => Vec::new(),
    };
    let script = if kind == Kind::Command {
        parse(&full)
    } else {
        Vec::new()
    };
    if rules.matches(&permission.tool, kind, &full, &paths, &script, root) {
        return Risk::Rule;
    }
    match kind {
        Kind::Command => command(&script, root),
        Kind::Read => {
            if paths.iter().all(|p| root.inside(p)) {
                Risk::Safe
            } else {
                Risk::Outside
            }
        }
        Kind::Write => {
            if paths.is_empty() {
                Risk::Unfamiliar
            } else if permission.diffs.iter().any(|d| d.change == Change::Delete) {
                Risk::Deletes
            } else if paths.iter().all(|p| root.inside(p)) {
                Risk::Safe
            } else {
                Risk::Outside
            }
        }
        Kind::Delete => Risk::Deletes,
        Kind::Network => Risk::Network,
        Kind::Folder => Risk::Outside,
        Kind::System => Risk::System,
        Kind::Benign => Risk::Safe,
        Kind::Unknown => Risk::Unfamiliar,
    }
}

/// The paths a read or a change names: its `path:` style arguments, or the
/// whole text when it is one bare path.
pub fn paths_in(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut keyed: Vec<String> = Vec::new();
    for line in &lines {
        for key in [
            "file_path: ",
            "notebook_path: ",
            "path: ",
            "filePath: ",
            "filepath: ",
        ] {
            if let Some(rest) = line.strip_prefix(key) {
                keyed.push(rest.to_owned());
                break;
            }
        }
    }
    // A glob names a folder only when it starts from one.
    for line in &lines {
        if let Some(rest) = line.strip_prefix("pattern: ")
            && (rest.starts_with('/') || rest.starts_with('~') || is_drive(rest))
        {
            keyed.push(rest.to_owned());
        }
    }
    if !keyed.is_empty() {
        return keyed;
    }
    // Codex lists a change's reason above its paths; OpenCode its patterns.
    lines
        .into_iter()
        .filter(|l| {
            (!l.is_empty() && !l.contains(": ") && !l.contains(' '))
                || l.starts_with('/')
                || l.starts_with('~')
                || is_drive(l)
        })
        .map(str::to_owned)
        .collect()
}

fn is_drive(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

// MARK: Commands

const DELETING: &[&str] = &[
    "rm",
    "rmdir",
    "unlink",
    "shred",
    "srm",
    "trash",
    "del",
    "erase",
    "rd",
    "remove-item",
    "ri",
    "truncate",
    "clear-content",
];
const NETWORKING: &[&str] = &[
    "curl",
    "wget",
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "nc",
    "ncat",
    "netcat",
    "telnet",
    "ftp",
    "tftp",
    "http",
    "https",
    "xh",
    "ping",
    "gh",
    "npx",
    "bunx",
    "pnpx",
    "invoke-webrequest",
    "iwr",
    "invoke-restmethod",
    "irm",
    "start-bitstransfer",
    "winget",
    "choco",
    "scoop",
    "aria2c",
    "mosh",
];
const PRIVILEGED: &[&str] = &[
    "sudo",
    "su",
    "doas",
    "dd",
    "fdisk",
    "diskutil",
    "chmod",
    "chown",
    "chgrp",
    "chflags",
    "xattr",
    "kill",
    "killall",
    "pkill",
    "taskkill",
    "stop-process",
    "shutdown",
    "reboot",
    "halt",
    "launchctl",
    "systemctl",
    "service",
    "crontab",
    "defaults",
    "reg",
    "set-executionpolicy",
    "csrutil",
    "spctl",
    "security",
    "mount",
    "umount",
    "passwd",
    "useradd",
    "usermod",
    "sysctl",
    "iptables",
    "ufw",
    "netsh",
    "sc",
    "icacls",
    "takeown",
    "setx",
    "pmset",
    "nvram",
    "tccutil",
    "runas",
    "start-process",
];
const EVALUATING: &[&str] = &[
    "eval",
    "source",
    ".",
    "iex",
    "invoke-expression",
    "osascript",
];
const INTERPRETERS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "pwsh",
    "powershell",
    "cmd",
    "python",
    "python3",
    "python2",
    "node",
    "deno",
    "bun",
    "perl",
    "ruby",
    "php",
    "lua",
    "osascript",
];
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "cmd",
    "pwsh",
    "powershell",
];
/// Commands that write the paths they are given.
const WRITING: &[&str] = &[
    "tee",
    "mkdir",
    "touch",
    "new-item",
    "ni",
    "set-content",
    "out-file",
    "add-content",
    "chmod",
    "ln",
    "patch",
];
/// Commands whose last path is where they write.
const COPYING: &[&str] = &[
    "cp",
    "mv",
    "install",
    "ditto",
    "copy-item",
    "move-item",
    "copy",
    "move",
    "cpi",
    "mi",
];

/// Package managers that download when they add or update packages.
fn package_verbs(name: &str) -> &'static [&'static str] {
    match name {
        "npm" => &[
            "install", "i", "add", "ci", "update", "up", "upgrade", "publish", "exec", "x", "dlx",
            "login", "create", "init",
        ],
        "pnpm" => &[
            "install", "i", "add", "update", "up", "publish", "dlx", "create",
        ],
        "yarn" => &[
            "install", "add", "upgrade", "up", "publish", "dlx", "create",
        ],
        "bun" => &["install", "i", "add", "update", "publish", "x", "create"],
        "pip" | "pip3" => &["install", "download"],
        "uv" => &["add", "pip", "sync", "tool"],
        "brew" => &["install", "upgrade", "update", "tap", "reinstall"],
        "apt" | "apt-get" | "dnf" => &["install", "update", "upgrade"],
        "yum" => &["install", "update"],
        "pacman" => &["-s", "-syu", "-sy"],
        "apk" => &["add", "update"],
        "cargo" => &["install", "publish", "add", "update", "login"],
        "gem" => &["install", "update", "push"],
        "go" => &["get", "install", "mod"],
        "docker" => &["pull", "push", "login", "run", "build"],
        "podman" => &["pull", "push", "login", "run"],
        "composer" => &["install", "require", "update"],
        "dotnet" => &["add", "restore", "nuget"],
        "pod" => &["install", "update"],
        "swift" => &["package"],
        _ => &[],
    }
}

pub fn command(script: &[Segment], root: &Paths) -> Risk {
    Risk::worst(
        &script
            .iter()
            .map(|s| segment_risk(s, root, 0))
            .collect::<Vec<_>>(),
    )
}

fn segment_risk(segment: &Segment, root: &Paths, depth: usize) -> Risk {
    let mut risks = Vec::new();
    for target in &segment.redirects {
        if target == "/dev/null" || target.eq_ignore_ascii_case("nul") || target.starts_with('&') {
            continue;
        }
        if !root.inside(target) {
            risks.push(Risk::Outside);
        }
    }
    let words = command_words(&segment.words);
    let Some(first) = words.first() else {
        return Risk::worst(&risks);
    };
    let name = command_name(first);
    let name = name.as_str();
    let args: Vec<String> = words[1..].to_vec();
    let plain: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let has = |list: &[&str]| list.contains(&name);
    if segment.piped && has(INTERPRETERS) {
        risks.push(Risk::RunsCode);
    }
    if has(EVALUATING) {
        risks.push(Risk::RunsCode);
    }
    if has(DELETING) {
        risks.push(Risk::Deletes);
    }
    if has(NETWORKING) {
        risks.push(Risk::Network);
    }
    if has(PRIVILEGED) || name.starts_with("mkfs") {
        risks.push(Risk::System);
    }
    if has(WRITING) && !plain.iter().all(|p| root.inside(p)) {
        risks.push(Risk::Outside);
    }
    if has(COPYING)
        && let Some(last) = plain.last()
        && !root.inside(last)
    {
        risks.push(Risk::Outside);
    }
    if (name == "sed" || name == "perl")
        && args.iter().any(|a| a.starts_with("-i"))
        && !plain.iter().skip(1).all(|p| root.inside(p))
    {
        risks.push(Risk::Outside);
    }
    if ["cd", "pushd", "set-location", "sl", "chdir"].contains(&name)
        && let Some(target) = plain.first()
        && !root.inside(target)
    {
        risks.push(Risk::Outside);
    }
    if name == "find"
        && args
            .iter()
            .any(|a| ["-delete", "-exec", "-execdir", "-ok"].contains(&a.as_str()))
    {
        risks.push(if args.iter().any(|a| a == "-delete") {
            Risk::Deletes
        } else {
            Risk::RunsCode
        });
    }
    let verbs = package_verbs(name);
    let verb = |w: Option<&&String>| w.is_some_and(|w| verbs.contains(&w.to_lowercase().as_str()));
    if verb(plain.first()) || verb(args.iter().collect::<Vec<_>>().first()) {
        risks.push(Risk::Network);
    }
    if ["npm", "pnpm", "yarn"].contains(&name) && args.iter().any(|a| a == "-g" || a == "--global")
    {
        risks.push(Risk::System);
    }
    if name == "git" {
        risks.push(git(&args, root));
    }
    // A shell given its script inline is judged by that script; any other
    // interpreter's inline code can do anything.
    if has(INTERPRETERS) {
        let flags = [
            "-c", "-e", "/c", "/k", "-command", "--eval", "-ec", "-lc", "-ic",
        ];
        if let Some(flag) = args
            .iter()
            .position(|a| flags.contains(&a.to_lowercase().as_str()))
        {
            let inline = args.get(flag + 1).cloned().unwrap_or_default();
            if SHELLS.contains(&name) && depth < 3 {
                risks.push(Risk::worst(
                    &parse(&inline)
                        .iter()
                        .map(|s| segment_risk(s, root, depth + 1))
                        .collect::<Vec<_>>(),
                ));
            } else {
                risks.push(Risk::RunsCode);
            }
        }
    }
    Risk::worst(&risks)
}

fn git(args: &[String], root: &Paths) -> Risk {
    let mut risks = Vec::new();
    let mut i = 0;
    // Options before the verb; -C moves where it works.
    while i < args.len() && args[i].starts_with('-') {
        let flag = args[i].as_str();
        i += 1;
        if ["-C", "-c", "--git-dir", "--work-tree"].contains(&flag) && i < args.len() {
            if flag != "-c" && !root.inside(&args[i]) {
                risks.push(Risk::Outside);
            }
            i += 1;
        }
    }
    let Some(verb) = args.get(i) else {
        return Risk::worst(&risks);
    };
    let rest = &args[i + 1..];
    let has = |w: &str| rest.iter().any(|a| a == w);
    let first = rest.first().map(String::as_str);
    let risk = match verb.as_str() {
        "push" | "pull" | "fetch" | "clone" | "ls-remote" | "send-email" | "lfs" => {
            Some(Risk::Network)
        }
        "remote" if first == Some("update") => Some(Risk::Network),
        "submodule" if has("update") => Some(Risk::Network),
        "clean" | "rm" | "restore" => Some(Risk::Deletes),
        "reset" if has("--hard") || has("--merge") || has("--keep") => Some(Risk::Deletes),
        "checkout" if has("--") || has(".") || has("-f") || has("--force") => Some(Risk::Deletes),
        "stash" if first == Some("drop") || first == Some("clear") => Some(Risk::Deletes),
        "branch" if has("-D") || has("-d") || has("--delete") => Some(Risk::Deletes),
        "tag" if has("-d") || has("--delete") => Some(Risk::Deletes),
        "worktree" if first == Some("remove") || first == Some("prune") => Some(Risk::Deletes),
        "config" if has("--global") || has("--system") => Some(Risk::System),
        "filter-branch" | "filter-repo" | "gc" | "prune" => Some(Risk::Deletes),
        "reflog" if first == Some("expire") || first == Some("delete") => Some(Risk::Deletes),
        _ => None,
    };
    risks.extend(risk);
    Risk::worst(&risks)
}

// MARK: Paths

/// Whether a path stays inside the project folder, after `~`, `..` and
/// symbolic links. Windows paths compare without case.
#[derive(Clone, Debug)]
pub struct Paths {
    pub cwd: String,
    pub home: String,
    pub resolve_links: bool,
    pub ignore_case: bool,
}

impl Paths {
    pub fn new(cwd: &str, home: &str, resolve_links: bool) -> Self {
        Paths {
            cwd: cwd.to_owned(),
            home: home.to_owned(),
            resolve_links,
            ignore_case: cfg!(windows),
        }
    }

    pub fn absolute(&self, path: &str) -> String {
        let mut path = path.to_owned();
        if path.len() >= 2 {
            let b = path.as_bytes();
            if b[0] == b[path.len() - 1] && (b[0] == b'"' || b[0] == b'\'') {
                path = path[1..path.len() - 1].to_owned();
            }
        }
        let home = self.home.replace('\\', "/");
        for prefix in ["$HOME", "$env:USERPROFILE", "%USERPROFILE%", "~"] {
            if path == prefix {
                path = home.clone();
                break;
            }
            for sep in ["/", "\\"] {
                if let Some(rest) = path.strip_prefix(&format!("{prefix}{sep}")) {
                    path = format!("{home}/{rest}");
                    break;
                }
            }
        }
        let path = path.replace('\\', "/");
        let cwd = self.cwd.replace('\\', "/");
        let joined = if path.starts_with('/') || is_drive(&path) {
            path
        } else {
            format!("{cwd}/{path}")
        };
        normalize(&joined)
    }

    fn resolved(&self, path: &str) -> String {
        let absolute = self.absolute(path);
        if !self.resolve_links {
            return absolute;
        }
        // Links in the part that exists; the rest is yet to be written.
        let mut existing = PathBuf::from(&absolute);
        let mut tail: Vec<String> = Vec::new();
        while !existing.exists() {
            let Some(name) = existing
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
            else {
                return absolute;
            };
            tail.insert(0, name);
            if !existing.pop() {
                return absolute;
            }
        }
        let Ok(real) = std::fs::canonicalize(&existing) else {
            return absolute;
        };
        let mut real = real.to_string_lossy().replace('\\', "/");
        if let Some(stripped) = real.strip_prefix("//?/") {
            real = stripped.to_owned();
        }
        let mut out = real.trim_end_matches('/').to_owned();
        for part in tail {
            out.push('/');
            out.push_str(&part);
        }
        if out.is_empty() { "/".to_owned() } else { out }
    }

    pub fn inside(&self, path: &str) -> bool {
        // No project folder to stay in: nothing counts as inside it.
        let cwd = self.absolute(&self.cwd);
        if self.cwd.is_empty()
            || cwd == "/"
            || cwd == self.absolute(&self.home)
            || cwd.ends_with(":/")
        {
            return false;
        }
        if path.is_empty() {
            return true;
        }
        // A glob is judged by the folder it starts from.
        let fixed = match path.find(['*', '?', '[', '{']) {
            Some(i) => &path[..i],
            None => path,
        };
        if fixed.is_empty() {
            return true;
        }
        if fixed.starts_with("\\\\") || fixed.contains('`') || fixed.contains('%') {
            return false;
        }
        if fixed.contains('$')
            && !fixed.starts_with("$HOME")
            && !fixed.starts_with("$env:USERPROFILE")
        {
            return false;
        }
        let mut root = self.resolved(&self.cwd);
        let mut target = self.resolved(fixed);
        if self.ignore_case {
            root = root.to_lowercase();
            target = target.to_lowercase();
        }
        let prefix = if root.ends_with('/') {
            root.clone()
        } else {
            format!("{root}/")
        };
        target == root || target.starts_with(&prefix)
    }
}

/// `.` and `..` taken out of an absolute, `/`-separated path.
fn normalize(path: &str) -> String {
    let (lead, rest) = if is_drive(path) {
        (path[..2].to_owned(), &path[2..])
    } else {
        (String::new(), path)
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("{lead}/{}", parts.join("/"))
}

// MARK: Shell text

/// One part of a command: what it runs, where its output goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Segment {
    pub words: Vec<String>,
    /// Files its output goes to (`>`, `>>`, `2>`, `&>`).
    pub redirects: Vec<String>,
    /// Its input comes from the part before it through `|`.
    pub piped: bool,
    /// The text of the part, for rules that match a command's start.
    pub text: String,
}

/// What a backslash escapes: what the shell would otherwise read itself.
const ESCAPED: &str = " \t'\"\\$`;&|<>()*?#!{}[]~";

/// A rough reading of a shell command, enough to find what each part runs.
/// Quotes and escapes are honoured; anything it cannot follow splits into
/// more parts, which only ever makes an ask look riskier.
pub fn parse(command: &str) -> Vec<Segment> {
    struct State {
        segments: Vec<Segment>,
        current: Segment,
        word: String,
        in_word: bool,
        redirect_next: bool,
        input_next: bool,
        text: String,
    }
    impl State {
        fn end_word(&mut self) {
            if !self.in_word {
                return;
            }
            self.in_word = false;
            let word = std::mem::take(&mut self.word);
            // An empty quote, or what is left of one around a substitution.
            if word.is_empty() && !self.redirect_next {
                self.input_next = false;
                return;
            }
            if self.redirect_next {
                self.current.redirects.push(word);
                self.redirect_next = false;
            } else if self.input_next {
                self.input_next = false;
            } else {
                self.current.words.push(word);
            }
        }
        fn end_segment(&mut self, piped: bool) {
            self.end_word();
            self.current.text = self.text.trim().to_owned();
            let done = std::mem::take(&mut self.current);
            if !done.words.is_empty() || !done.redirects.is_empty() {
                self.segments.push(done);
            }
            self.current.piped = piped;
            self.text.clear();
        }
    }
    let mut s = State {
        segments: Vec::new(),
        current: Segment::default(),
        word: String::new(),
        in_word: false,
        redirect_next: false,
        input_next: false,
        text: String::new(),
    };
    // Open substitutions and subshells: what closes each, and the quote to
    // go back into.
    let mut subs: Vec<(char, Option<char>)> = Vec::new();
    let mut quote: Option<char> = None;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if let Some(open) = quote {
            if c == open {
                quote = None;
                s.text.push(c);
                i += 1;
                continue;
            }
            if open == '"' {
                if c == '\\'
                    && let Some(n) = next
                {
                    s.word.push(n);
                    s.text.push(c);
                    s.text.push(n);
                    i += 2;
                    continue;
                }
                // Inside double quotes a substitution still runs; the quote
                // goes on once it closes.
                if c == '`' || (c == '$' && next == Some('(')) {
                    s.end_segment(false);
                    subs.push((if c == '`' { '`' } else { ')' }, Some('"')));
                    quote = None;
                    i += if c == '$' { 2 } else { 1 };
                    continue;
                }
            }
            s.word.push(c);
            s.text.push(c);
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                s.in_word = true;
                s.text.push(c);
            }
            '\\' => {
                // An escape only before what the shell would read itself; a
                // Windows path keeps its backslashes.
                match next {
                    Some('\n') => i += 1,
                    Some(n) if ESCAPED.contains(n) => {
                        s.word.push(n);
                        s.in_word = true;
                        s.text.push(c);
                        s.text.push(n);
                        i += 1;
                    }
                    _ => {
                        s.word.push(c);
                        s.in_word = true;
                        s.text.push(c);
                    }
                }
            }
            ' ' | '\t' => {
                s.end_word();
                s.text.push(c);
            }
            '\n' | '\r' | ';' => s.end_segment(false),
            '(' => {
                s.end_segment(false);
                subs.push((')', None));
            }
            ')' | '`' if subs.last().is_some_and(|(close, _)| *close == c) => {
                s.end_segment(false);
                if let Some((_, Some(resume))) = subs.pop() {
                    quote = Some(resume);
                    s.in_word = true;
                }
            }
            '`' => {
                s.end_segment(false);
                subs.push(('`', None));
            }
            ')' => s.end_segment(false),
            '$' if next == Some('(') => {
                s.end_segment(false);
                subs.push((')', None));
                i += 1;
            }
            '<' if next == Some('(') => {
                s.end_segment(false);
                subs.push((')', None));
                i += 1;
            }
            '<' => {
                // What a command reads from is not what it runs or writes.
                s.end_word();
                s.input_next = true;
                s.text.push(c);
                while chars.get(i + 1) == Some(&'<') {
                    i += 1;
                }
            }
            '&' => {
                if next == Some('&') {
                    s.end_segment(false);
                    i += 1;
                } else if next == Some('>') {
                    // `&>` and `&>>` send both streams to a file.
                    s.end_word();
                    s.redirect_next = true;
                    i += 1;
                    if chars.get(i + 1) == Some(&'>') {
                        i += 1;
                    }
                } else {
                    s.end_segment(false);
                }
            }
            '|' => {
                if next == Some('|') {
                    s.end_segment(false);
                    i += 1;
                } else {
                    s.end_segment(true);
                    if next == Some('&') {
                        i += 1;
                    }
                }
            }
            '>' => {
                // `2>` and `1>` name the stream, not a word.
                if s.in_word && (s.word == "1" || s.word == "2") {
                    s.word.clear();
                    s.in_word = false;
                }
                s.end_word();
                if next == Some('(') {
                    s.end_segment(true);
                    subs.push((')', None));
                    i += 2;
                    continue;
                }
                s.redirect_next = true;
                if next == Some('>') || next == Some('|') {
                    i += 1;
                }
                // `>&2` and `2>&1` join a stream, not a file.
                if chars.get(i + 1) == Some(&'&') {
                    s.redirect_next = false;
                    i += 1;
                    while chars
                        .get(i + 1)
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-')
                    {
                        i += 1;
                    }
                }
                s.text.push(c);
            }
            _ => {
                s.word.push(c);
                s.in_word = true;
                s.text.push(c);
            }
        }
        i += 1;
    }
    s.end_segment(false);
    s.segments
}

/// The words that name the command, past assignments and the wrappers that
/// only run what follows them.
pub fn command_words(words: &[String]) -> Vec<String> {
    let mut i = 0;
    let assignment = |w: &str| {
        let mut chars = w.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && w.split_once('=')
                .is_some_and(|(k, _)| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    };
    while i < words.len() {
        let first = &words[i];
        if assignment(first) {
            i += 1;
            continue;
        }
        let name = command_name(first);
        match name.as_str() {
            "{" | "}" | "!" | "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "fi"
            | "done" | "&" => i += 1,
            "command" | "builtin" | "exec" | "nohup" | "time" | "caffeinate" | "stdbuf"
            | "unbuffer" | "noglob" => {
                i += 1;
                while i < words.len() && words[i].starts_with('-') {
                    i += 1;
                }
            }
            "env" => {
                i += 1;
                while i < words.len() && (words[i].starts_with('-') || words[i].contains('=')) {
                    let flag = words[i].clone();
                    i += 1;
                    if ["-u", "-C", "-S"].contains(&flag.as_str()) {
                        i += 1;
                    }
                }
            }
            "nice" | "ionice" | "timeout" | "gtimeout" => {
                i += 1;
                while i < words.len() && words[i].starts_with('-') {
                    let flag = words[i].clone();
                    i += 1;
                    if ["-n", "-s", "-k", "-c"].contains(&flag.as_str()) {
                        i += 1;
                    }
                }
                if name.ends_with("timeout")
                    && words
                        .get(i)
                        .is_some_and(|d| d.chars().next().is_some_and(|c| c.is_ascii_digit()))
                {
                    i += 1;
                }
            }
            "xargs" | "parallel" | "watch" => {
                i += 1;
                while i < words.len() && words[i].starts_with('-') {
                    let flag = words[i].clone();
                    i += 1;
                    if ["-I", "-n", "-P", "-L", "-d", "-E", "-s", "-a", "-j"]
                        .contains(&flag.as_str())
                    {
                        i += 1;
                    }
                }
            }
            _ => break,
        }
    }
    words.get(i..).map(<[String]>::to_vec).unwrap_or_default()
}

/// A command's name without its folder or `.exe`, in lower case.
pub fn command_name(word: &str) -> String {
    let name = word
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .to_lowercase();
    name.strip_suffix(".exe").map(str::to_owned).unwrap_or(name)
}

// MARK: Settings rules

/// The `deny` and `ask` rules of the Claude Code settings a session reads:
/// an ask they name always waits for the person.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rules {
    pub rules: Vec<String>,
}

impl Rules {
    /// Managed settings, the account's own, then the project's.
    pub fn read(
        config_home: Option<&str>,
        cwd: Option<&str>,
        managed: &[PathBuf],
        home: &str,
    ) -> Rules {
        let mut files: Vec<PathBuf> = managed.to_vec();
        let config = config_home
            .filter(|c| !c.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(home).join(".claude"));
        files.push(config.join("settings.json"));
        if let Some(cwd) = cwd.filter(|c| !c.is_empty()) {
            let project = Path::new(cwd).join(".claude");
            files.push(project.join("settings.json"));
            files.push(project.join("settings.local.json"));
        }
        let mut found = Rules::default();
        for path in files {
            let Some(json) = std::fs::read(&path)
                .ok()
                .and_then(|d| serde_json::from_slice::<Value>(&d).ok())
            else {
                continue;
            };
            for key in ["deny", "ask"] {
                if let Some(list) = json
                    .get("permissions")
                    .and_then(|p| p.get(key))
                    .and_then(Value::as_array)
                {
                    found.rules.extend(
                        list.iter()
                            .filter_map(Value::as_str)
                            .filter(|r| !r.is_empty())
                            .map(str::to_owned),
                    );
                }
            }
        }
        found
    }

    pub fn matches(
        &self,
        tool: &str,
        kind: Kind,
        text: &str,
        paths: &[String],
        segments: &[Segment],
        root: &Paths,
    ) -> bool {
        self.rules
            .iter()
            .any(|r| rule_matches(r, tool, kind, text, paths, segments, root))
    }
}

/// A rule as Claude Code writes them: `Tool`, `Tool(spec)`,
/// `Bash(prefix:*)`, `Read(./.env)`, `WebFetch(domain:host)`, `mcp__server`.
/// It errs on matching, which only keeps an ask out of Allow all.
pub fn rule_matches(
    rule: &str,
    tool: &str,
    kind: Kind,
    text: &str,
    paths: &[String],
    segments: &[Segment],
    root: &Paths,
) -> bool {
    let rule = rule.trim();
    let (name, spec) = match rule.find('(') {
        Some(open) if rule.ends_with(')') => (&rule[..open], Some(&rule[open + 1..rule.len() - 1])),
        _ => (rule, None),
    };
    if !covers(name, tool, kind) {
        return false;
    }
    let Some(spec) = spec.filter(|s| !s.is_empty() && *s != "*") else {
        return true;
    };
    match kind {
        Kind::Command => {
            let mut lines: Vec<String> = segments
                .iter()
                .map(|s| command_words(&s.words).join(" "))
                .collect();
            lines.extend(segments.iter().map(|s| s.text.clone()));
            lines.push(text.to_owned());
            if let Some(prefix) = spec.strip_suffix(":*") {
                return lines.iter().any(|l| l.starts_with(prefix));
            }
            lines.iter().any(|l| glob(spec, l, false))
        }
        Kind::Read | Kind::Write | Kind::Delete => {
            let pattern = if let Some(rest) = spec.strip_prefix("//") {
                format!("/{rest}")
            } else if let Some(rest) = spec.strip_prefix("~/") {
                format!("{}/{rest}", root.home.replace('\\', "/"))
            } else if spec.starts_with('/') {
                format!("{}{spec}", root.cwd.replace('\\', "/"))
            } else if let Some(rest) = spec.strip_prefix("./") {
                format!("{}/{rest}", root.cwd.replace('\\', "/"))
            } else {
                spec.to_owned()
            };
            paths.iter().any(|path| {
                let absolute = root.absolute(path);
                if spec.starts_with('/') && glob(spec, &absolute, true) {
                    return true;
                }
                if !pattern.starts_with('/') && !is_drive(&pattern) {
                    let base = absolute.rsplit('/').next().unwrap_or(&absolute);
                    return glob(&format!("**/{pattern}"), &absolute, true)
                        || glob(&pattern, base, true);
                }
                glob(&pattern, &absolute, true) || glob(&format!("{pattern}/**"), &absolute, true)
            })
        }
        Kind::Network => {
            if let Some(domain) = spec.strip_prefix("domain:") {
                let domain = domain.to_lowercase();
                let host = host_of(text).to_lowercase();
                return host == domain
                    || host.ends_with(&format!(".{domain}"))
                    || glob(&domain, &host, false);
            }
            glob(spec, text, false)
        }
        _ => glob(spec, text, false),
    }
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    host.split(':').next().unwrap_or(host)
}

fn covers(name: &str, tool: &str, kind: Kind) -> bool {
    if name == tool {
        return true;
    }
    if name.starts_with("mcp__") {
        let server = name.strip_suffix("__*").unwrap_or(name);
        return tool == server
            || tool.starts_with(&format!("{server}__"))
            || format!("mcp__{tool}").starts_with(server);
    }
    match name {
        "Bash" | "PowerShell" => kind == Kind::Command,
        // Edit rules cover every tool that changes files; Read rules every
        // tool that reads them.
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            kind == Kind::Write || kind == Kind::Delete
        }
        "Read" | "Glob" | "Grep" | "LS" => kind == Kind::Read,
        _ => false,
    }
}

/// `*` within a part, `**` across parts when `slash`; anything else literal.
/// Compared without case.
pub fn glob(pattern: &str, text: &str, slash: bool) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    // Tokens: literal, any-within-part, any-across, one-char.
    #[derive(Clone, Copy, PartialEq)]
    enum Tok {
        Lit(char),
        Star,
        Stars,
        One,
    }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '*' if p.get(i + 1) == Some(&'*') => {
                toks.push(Tok::Stars);
                i += 1;
                // `**/` also matches no folder at all.
                if slash && p.get(i + 1) == Some(&'/') {
                    i += 1;
                }
            }
            '*' => toks.push(if slash { Tok::Star } else { Tok::Stars }),
            '?' => toks.push(Tok::One),
            c => toks.push(Tok::Lit(c)),
        }
        i += 1;
    }
    // Plain backtracking match over the tokens.
    fn go(
        toks: &[Tok],
        t: &[char],
        slash: bool,
        memo: &mut HashMap<(usize, usize), bool>,
        ti: usize,
        xi: usize,
    ) -> bool {
        if let Some(&known) = memo.get(&(ti, xi)) {
            return known;
        }
        let result = if ti == toks.len() {
            xi == t.len()
        } else {
            match toks[ti] {
                Tok::Lit(c) => {
                    xi < t.len() && t[xi] == c && go(toks, t, slash, memo, ti + 1, xi + 1)
                }
                Tok::One => {
                    xi < t.len()
                        && (!slash || t[xi] != '/')
                        && go(toks, t, slash, memo, ti + 1, xi + 1)
                }
                Tok::Star => {
                    let mut k = xi;
                    loop {
                        if go(toks, t, slash, memo, ti + 1, k) {
                            break true;
                        }
                        if k >= t.len() || t[k] == '/' {
                            break false;
                        }
                        k += 1;
                    }
                }
                Tok::Stars => (xi..=t.len()).any(|k| go(toks, t, slash, memo, ti + 1, k)),
            }
        };
        memo.insert((ti, xi), result);
        result
    }
    let mut memo = HashMap::new();
    go(&toks, &t, slash, &mut memo, 0, 0)
}

// MARK: Publishing

/// Gives every ask the island publishes its risk, judged once per ask, with
/// each project's settings rules read again at most every few seconds.
/// Reads a session's rules from its account's config folder and its project.
pub type RulesReader = Box<dyn Fn(Option<&str>, &str) -> Rules + Send>;

pub struct Cache {
    judged: HashMap<String, Risk>,
    rules: HashMap<String, (Rules, f64)>,
    read: RulesReader,
    home: String,
}

pub const RULES_LIFETIME: f64 = 5.0;

impl Cache {
    pub fn new(home: &str, read: RulesReader) -> Self {
        Cache {
            judged: HashMap::new(),
            rules: HashMap::new(),
            read,
            home: home.to_owned(),
        }
    }

    pub fn assess(&mut self, sessions: &mut [Session], now: f64) {
        let mut seen = HashSet::new();
        for session in sessions.iter_mut() {
            if session.permission.is_none() {
                continue;
            }
            // A host's files are not on this computer: its rules cannot be
            // read, nor its links followed.
            let local = session.remote.is_none();
            let rules = if local {
                self.rules_for(session.account.as_deref(), &session.cwd, now)
            } else {
                Rules::default()
            };
            let root = Paths::new(&session.cwd, &self.home, local);
            let id = session.id.clone();
            let cwd = session.cwd.clone();
            for ask in session
                .permission
                .iter_mut()
                .chain(session.later_asks.iter_mut())
            {
                let revision = ask
                    .content
                    .as_ref()
                    .map(|c| c.metadata(ask).revision)
                    .unwrap_or_else(|| ask.detail.clone());
                let key = format!("{id}\u{1}{}\u{1}{revision}", ask.request_id);
                let risk = match self.judged.get(&key) {
                    Some(known) => *known,
                    None => {
                        let judged = assess(ask, &cwd, &rules, &root);
                        self.judged.insert(key.clone(), judged);
                        judged
                    }
                };
                ask.risk = risk;
                seen.insert(key);
            }
        }
        self.judged.retain(|k, _| seen.contains(k));
        self.rules
            .retain(|_, (_, read)| now - *read < RULES_LIFETIME * 4.0);
    }

    fn rules_for(&mut self, config_home: Option<&str>, cwd: &str, now: f64) -> Rules {
        let key = format!("{}\u{1}{cwd}", config_home.unwrap_or(""));
        if let Some((rules, read)) = self.rules.get(&key)
            && now - read < RULES_LIFETIME
        {
            return rules.clone();
        }
        let found = (self.read)(config_home, cwd);
        // A changed rule judges every ask again.
        if self
            .rules
            .get(&key)
            .is_some_and(|(before, _)| *before != found)
        {
            self.judged.clear();
        }
        self.rules.insert(key, (found.clone(), now));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::file_diff::FileDiff;

    fn ask(tool: &str, text: &str, diffs: Vec<FileDiff>) -> Permission {
        let mut p = Permission::new("r", tool, None, true, 0.0);
        p.set_content(text, true, Some(text.chars().count()));
        p.diffs = diffs;
        p
    }

    #[test]
    fn shared_cases() {
        let json: Value = serde_json::from_str(include_str!("risk_cases.json")).unwrap();
        let cwd = json["cwd"].as_str().unwrap();
        let home = json["home"].as_str().unwrap();
        let rules = Rules {
            rules: json["rules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_owned())
                .collect(),
        };
        let mut root = Paths::new(cwd, home, false);
        root.ignore_case = false;
        let mut failures = Vec::new();
        for case in json["cases"].as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            let text = case["text"].as_str().unwrap();
            let diffs = case["diffs"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .map(|d| FileDiff {
                            path: d["path"].as_str().unwrap().to_owned(),
                            change: if d["change"] == "delete" {
                                Change::Delete
                            } else {
                                Change::Update
                            },
                            ..FileDiff::default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            let got = assess(&ask(tool, text, diffs), cwd, &rules, &root);
            if got.name() != case["expect"].as_str().unwrap() {
                failures.push(format!(
                    "{tool} `{text}`: expected {}, got {}",
                    case["expect"],
                    got.name()
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn no_project_folder_nothing_inside() {
        let write = ask("Write", "/w/a.txt", Vec::new());
        for cwd in ["", "/w", "/"] {
            let root = Paths::new(cwd, "/w", false);
            assert_eq!(
                assess(&write, cwd, &Rules::default(), &root),
                Risk::Outside,
                "cwd {cwd:?}"
            );
        }
    }

    #[test]
    fn windows_paths() {
        let mut root = Paths::new(r"C:\Users\me\proj", r"C:\Users\me", false);
        root.ignore_case = true;
        assert!(root.inside(r"C:\Users\me\proj\src\a.ts"));
        assert!(
            root.inside(r"c:\users\ME\Proj\b.txt"),
            "drive paths compare without case"
        );
        assert!(root.inside(r"src\a.ts"));
        assert!(!root.inside(r"C:\Users\me\other\a.ts"));
        assert!(!root.inside(r"..\other\a.ts"));
        assert!(!root.inside(r"D:\proj\a.ts"));
        assert!(!root.inside(r"%APPDATA%\x"));
        assert!(!root.inside(r"\\server\share\x"));
        assert!(!root.inside(r"~\Desktop\x"));
        let rm = ask(
            "PowerShell",
            r"Remove-Item -Recurse C:\Users\me\proj\dist",
            Vec::new(),
        );
        assert_eq!(
            assess(&rm, &root.cwd.clone(), &Rules::default(), &root),
            Risk::Deletes
        );
        let out = ask(
            "PowerShell",
            r"Set-Content -Path C:\Windows\x.txt -Value 1",
            Vec::new(),
        );
        assert_eq!(
            assess(&out, &root.cwd.clone(), &Rules::default(), &root),
            Risk::Outside
        );
        let ok = ask("PowerShell", "cargo test", Vec::new());
        assert_eq!(
            assess(&ok, &root.cwd.clone(), &Rules::default(), &root),
            Risk::Safe
        );
    }

    #[test]
    fn links_out_of_the_project_count_as_outside() {
        let base = std::env::temp_dir().join(format!("risk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("proj")).unwrap();
        std::fs::create_dir_all(base.join("other")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(base.join("other"), base.join("proj/link")).unwrap();
        let cwd = base.join("proj").to_string_lossy().into_owned();
        let root = Paths::new(&cwd, "/nowhere", true);
        assert!(
            root.inside(&format!("{cwd}/new/a.txt")),
            "a new file inside"
        );
        #[cfg(unix)]
        assert!(
            !root.inside(&format!("{cwd}/link/a.txt")),
            "a link leads out"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn settings_rules_are_read_from_every_file() {
        let base = std::env::temp_dir().join(format!("rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let write = |p: &Path, s: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        write(
            &base.join("managed.json"),
            r#"{"permissions":{"deny":["Bash(shutdown:*)"]}}"#,
        );
        write(
            &base.join("config/settings.json"),
            r#"{"permissions":{"ask":["Bash(git push:*)"],"allow":["Bash(ls)"]}}"#,
        );
        write(
            &base.join("proj/.claude/settings.json"),
            r#"{"permissions":{"deny":["Read(./secrets/**)"]}}"#,
        );
        write(&base.join("proj/.claude/settings.local.json"), "not json");
        let config = base.join("config").to_string_lossy().into_owned();
        let cwd = base.join("proj").to_string_lossy().into_owned();
        let rules = Rules::read(
            Some(&config),
            Some(&cwd),
            &[base.join("managed.json")],
            "/nowhere",
        );
        assert_eq!(
            rules.rules,
            ["Bash(shutdown:*)", "Bash(git push:*)", "Read(./secrets/**)"]
        );
        let root = Paths::new(&cwd, "/nowhere", true);
        let secret = ask("Read", &format!("{cwd}/secrets/key.pem"), Vec::new());
        assert_eq!(assess(&secret, &cwd, &rules, &root), Risk::Rule);
        assert_eq!(
            assess(&ask("Bash", "ls", Vec::new()), &cwd, &rules, &root),
            Risk::Safe
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn shell_reading() {
        let parts = parse("a 'b; c' && d \"$(e f)\" | g > out 2>&1 < in");
        let firsts: Vec<&str> = parts.iter().map(|p| p.words[0].as_str()).collect();
        assert_eq!(firsts, ["a", "d", "e", "g"]);
        assert_eq!(parts[0].words, ["a", "b; c"]);
        assert!(parts[3].piped && parts[3].redirects == ["out"]);
        assert!(!parts[3].words.contains(&"in".to_owned()));
        assert_eq!(command_name("/usr/bin/RM"), "rm");
        assert_eq!(command_name(r"C:\bin\curl.exe"), "curl");
    }

    #[test]
    fn globs() {
        assert!(glob("/etc/**", "/etc/hosts", true));
        assert!(glob("**/.env", "/w/proj/.env", true));
        assert!(!glob("src/*", "src/a/b.ts", true));
        assert!(glob("git commit *", "git commit -m x", false));
    }
}
