// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A session's own git worktree, so sessions started on one repository
//! don't edit each other's files. Claude Code makes its own with
//! `--worktree <name>` and `claude rm` takes it away; for every other agent
//! the island makes one in `<repo>/.openbar/worktrees/<name>`, on a branch
//! `openbar/<name>` from `HEAD`, and removes only those. The same rules as
//! the Mac's `SessionWorktrees`.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::background::Output;

/// Where the island keeps the worktrees it makes, inside the repository so
/// the agents' folder trust carries over, and left out of `git status`
/// through the repository's own `info/exclude`.
pub const OWN_FOLDER: &str = ".openbar/worktrees";
pub const BRANCH_PREFIX: &str = "openbar/";
pub const CLAUDE_BRANCH_PREFIX: &str = "worktree-";

/// Folders agents keep worktrees in, inside the repository they belong to,
/// with `/` for a separator.
pub const MARKERS: [&str; 3] = [
    "/.claude/worktrees/",
    "/.openbar/worktrees/",
    "/.gemini/worktrees/",
];

/// What a folder is to git, as the New session page needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RepoState {
    NotRepo,
    /// Its checkout matches `HEAD`.
    Clean {
        root: String,
    },
    /// It has changes a new worktree, made from `HEAD`, won't have.
    Dirty {
        root: String,
    },
}

impl RepoState {
    pub fn root(&self) -> Option<&str> {
        match self {
            RepoState::NotRepo => None,
            RepoState::Clean { root } | RepoState::Dirty { root } => Some(root),
        }
    }
}

// MARK: Names

const LATIN: &str = "ÀÁÂÃÄÅÇÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝàáâãäåçèéêëìíîïñòóôõöùúûüýÿĀāĂăĄąĆćĈĉĊċČčĎďĒēĔĕĖėĘęĚěĜĝĞğĠġĢģĤĥĨĩĪīĬĭĮįİĴĵĶķĹĺĻļĽľŃńŅņŇňŌōŎŏŐőŔŕŖŗŘřŚśŜŝŞşŠšŢţŤťŨũŪūŬŭŮůŰűŲųŴŵŶŷŸŹźŻżŽžƠơƯưǍǎǏǐǑǒǓǔǕǖǗǘǙǚǛǜǞǟǠǡǦǧǨǩǪǫǬǭǰǴǵǸǹǺǻȀȁȂȃȄȅȆȇȈȉȊȋȌȍȎȏȐȑȒȓȔȕȖȗȘșȚțȞȟȦȧȨȩȪȫȬȭȮȯȰȱȲȳ";
const LATIN_BASE: &str = "AAAAAACEEEEIIIINOOOOOUUUUYaaaaaaceeeeiiiinooooouuuuyyAaAaAaCcCcCcCcDdEeEeEeEeEeGgGgGgGgHhIiIiIiIiIJjKkLlLlLlNnNnNnOoOoOoRrRrRrSsSsSsSsTtTtUuUuUuUuUuUuWwYyYZzZzZzOoUuAaIiOoUuUuUuUuUuAaAaGgKkOoOojGgNnAaAaAaEeEeIiIiOoOoRrRrUuUuSsTtHhAaEeOoOoOoOoYy";

/// Letters that don't decompose into an ASCII one and a mark.
fn own_letter(c: char) -> Option<&'static str> {
    Some(match c {
        'ł' => "l",
        'Ł' => "L",
        'ø' => "o",
        'Ø' => "O",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'đ' | 'ð' => "d",
        'Đ' | 'Ð' => "D",
        'þ' => "th",
        'Þ' => "TH",
        'ı' => "i",
        _ => return None,
    })
}

/// Latin letters as plain ASCII: `ż` as `z`, `ł` as `l`, `ß` as `ss`.
pub fn ascii_folded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else if let Some(at) = LATIN.chars().position(|l| l == c) {
            out.push(LATIN_BASE.chars().nth(at).unwrap_or(' '));
        } else if let Some(plain) = own_letter(c) {
            out.push_str(plain);
        } else if !('\u{300}'..='\u{36F}').contains(&c) {
            out.push(' ');
        }
    }
    out
}

/// A name from the first prompt: `Fix the login bug!` as `fix-the-login-bug`.
pub fn slug(text: &str, limit: usize) -> String {
    let folded = ascii_folded(text);
    let words = folded
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty());
    let mut slug = String::new();
    for word in words {
        let next_len = if slug.is_empty() {
            word.len()
        } else {
            slug.len() + 1 + word.len()
        };
        if next_len > limit {
            if slug.is_empty() {
                slug = word[..limit].to_owned();
            }
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(word);
    }
    slug.to_ascii_lowercase()
}

pub const SLUG_LIMIT: usize = 40;

/// `base`, or `base-2`, `base-3`… when that is taken.
pub fn unique(base: &str, taken: &std::collections::HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_owned();
    }
    let mut number = 2;
    while taken.contains(&format!("{base}-{number}")) {
        number += 1;
    }
    format!("{base}-{number}")
}

/// The worktree's name: the one typed, else one from the prompt, else
/// `session`, made unique.
pub fn name(
    custom: Option<&str>,
    prompt: &str,
    taken: &std::collections::HashSet<String>,
) -> String {
    let typed = slug(custom.unwrap_or(""), SLUG_LIMIT);
    let base = if typed.is_empty() {
        slug(prompt, SLUG_LIMIT)
    } else {
        typed
    };
    unique(if base.is_empty() { "session" } else { &base }, taken)
}

// MARK: Paths

/// Where a marker starts in a path, whichever separator it uses.
fn marker_at(path: &str) -> Option<(usize, usize)> {
    let normal = path.replace('\\', "/").to_ascii_lowercase();
    MARKERS
        .iter()
        .find_map(|marker| normal.find(marker).map(|at| (at, at + marker.len())))
}

/// The repository a worktree kept inside it belongs to; the path itself for
/// any other folder.
pub fn repository_of(path: &str) -> &str {
    match marker_at(path) {
        Some((at, _)) => &path[..at],
        None => path,
    }
}

/// The name of the worktree a session works in, when an agent or the island
/// made it one.
pub fn worktree_name(cwd: &str) -> Option<String> {
    let (_, end) = marker_at(cwd)?;
    let name = cwd[end..].split(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// The worktree's folder, where the session's own may be deeper inside it.
pub fn worktree_folder(cwd: &str) -> Option<String> {
    let (_, end) = marker_at(cwd)?;
    let name = cwd[end..].split(['/', '\\']).next()?;
    (!name.is_empty()).then(|| format!("{}{}", &cwd[..end], name))
}

/// Whether the island made it, so the island may remove it.
pub fn is_own(cwd: &str) -> bool {
    cwd.replace('\\', "/")
        .to_ascii_lowercase()
        .contains(&format!("/{OWN_FOLDER}/"))
}

/// A path as compared: `/` separators, no `\\?\`, no trailing separator,
/// and on Windows, any case.
fn comparable(path: &str) -> String {
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('\\', "/");
    let path = path.trim_end_matches('/').to_owned();
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}

/// The folder in the worktree that matches the one picked, which may be
/// deeper in the repository than its root.
pub fn inside(worktree: &str, folder: &str, root: &str) -> String {
    let base = comparable(root);
    let picked = comparable(folder);
    let plain = folder
        .strip_prefix(r"\\?\")
        .unwrap_or(folder)
        .replace('\\', "/");
    let plain = plain.trim_end_matches('/');
    if picked.len() > base.len() + 1 && picked.starts_with(&format!("{base}/")) {
        return native(&format!(
            "{}/{}",
            worktree.trim_end_matches(['/', '\\']),
            &plain[base.len() + 1..]
        ));
    }
    native(worktree)
}

/// A path with the platform's separator, as the agents report it back.
fn native(path: &str) -> String {
    if cfg!(windows) {
        path.replace('/', "\\")
    } else {
        path.to_owned()
    }
}

// MARK: git

/// `git` on the `PATH`, then where Git for Windows installs it.
pub fn git_executable() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let on_path = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let usual: Vec<std::path::PathBuf> = if cfg!(windows) {
        ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .flat_map(|base| {
                let base = std::path::PathBuf::from(base);
                [
                    base.join(r"Git\cmd\git.exe"),
                    base.join(r"Programs\Git\cmd\git.exe"),
                ]
            })
            .collect()
    } else {
        vec![
            "/opt/homebrew/bin/git".into(),
            "/usr/local/bin/git".into(),
            "/usr/bin/git".into(),
        ]
    };
    on_path
        .into_iter()
        .chain(usual)
        .find(|candidate| candidate.is_file())
}

/// One `git -C <folder>` command with no window and nothing to ask.
pub fn git(arguments: &[&str], folder: &str, timeout: Duration) -> Option<Output> {
    let executable = git_executable()?;
    let mut command = Command::new(executable);
    command
        .arg("-C")
        .arg(folder)
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().ok()?;
    let read = |mut pipe: Box<dyn std::io::Read + Send>| {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let _ = pipe.read_to_end(&mut data);
            let _ = sender.send(data);
        });
        receiver
    };
    let out = read(Box::new(child.stdout.take()?));
    let error = read(Box::new(child.stderr.take()?));
    let started = Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let wait = Duration::from_millis(500);
    Some(Output {
        ok: status.success(),
        out: out.recv_timeout(wait).unwrap_or_default(),
        error: String::from_utf8_lossy(&error.recv_timeout(wait).unwrap_or_default())
            .trim()
            .to_owned(),
    })
}

const QUICK: Duration = Duration::from_secs(15);

fn text(output: Option<Output>) -> Option<String> {
    let output = output.filter(|o| o.ok)?;
    Some(String::from_utf8_lossy(&output.out).trim().to_owned())
}

/// What git said last, which names what went wrong.
pub fn last_line(text: &str) -> String {
    text.lines().last().unwrap_or("").trim().to_owned()
}

/// Whether a folder is in a git checkout, and whether that checkout has
/// changes to tracked files. A bare repository has no checkout.
pub fn state(folder: &str) -> RepoState {
    let Some(answer) = text(git(
        &["rev-parse", "--is-inside-work-tree", "--show-toplevel"],
        folder,
        QUICK,
    )) else {
        return RepoState::NotRepo;
    };
    let mut lines = answer.lines();
    let (Some("true"), Some(root)) = (lines.next(), lines.next()) else {
        return RepoState::NotRepo;
    };
    let root = root.trim().to_owned();
    if root.is_empty() {
        return RepoState::NotRepo;
    }
    let changes = text(git(
        &["status", "--porcelain", "--untracked-files=no"],
        &root,
        QUICK,
    ))
    .unwrap_or_default();
    if changes.is_empty() {
        RepoState::Clean { root }
    } else {
        RepoState::Dirty { root }
    }
}

fn existing_names(root: &str) -> std::collections::HashSet<String> {
    let mut taken = std::collections::HashSet::new();
    for folder in [".claude/worktrees", OWN_FOLDER, ".gemini/worktrees"] {
        if let Ok(entries) = std::fs::read_dir(Path::new(root).join(folder)) {
            taken.extend(
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned()),
            );
        }
    }
    let branches = text(git(
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        root,
        QUICK,
    ))
    .unwrap_or_default();
    for branch in branches.lines() {
        for prefix in [BRANCH_PREFIX, CLAUDE_BRANCH_PREFIX] {
            if let Some(name) = branch.strip_prefix(prefix) {
                taken.insert(name.to_owned());
            }
        }
    }
    taken
}

/// Names held a while for sessions just started, by repository.
static RESERVED: Mutex<Option<HashMap<(String, String), Instant>>> = Mutex::new(None);
const HOLD: Duration = Duration::from_secs(120);

fn reserved(root: &str) -> std::collections::HashSet<String> {
    let mut held = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    let map = held.get_or_insert_with(HashMap::new);
    map.retain(|_, until| *until > Instant::now());
    map.keys()
        .filter(|(r, _)| r == root)
        .map(|(_, n)| n.clone())
        .collect()
}

/// Names in use: worktree folders the agents and the island keep, the
/// branches they would make, and names held for sessions just started.
pub fn taken_names(root: &str) -> std::collections::HashSet<String> {
    let mut taken = existing_names(root);
    taken.extend(reserved(root));
    taken
}

/// Picks the name and holds it a while, so two sessions started at once
/// don't both take it before either has made its folder.
pub fn reserve_name(custom: Option<&str>, prompt: &str, root: &str) -> String {
    let existing = existing_names(root);
    let mut held = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    let map = held.get_or_insert_with(HashMap::new);
    map.retain(|_, until| *until > Instant::now());
    let mut taken = existing;
    taken.extend(
        map.keys()
            .filter(|(r, _)| r == root)
            .map(|(_, n)| n.clone()),
    );
    let picked = name(custom, prompt, &taken);
    map.insert((root.to_owned(), picked.clone()), Instant::now() + HOLD);
    picked
}

/// Makes `<root>/.openbar/worktrees/<name>` on a new branch from `HEAD`.
/// Should another session have taken the name meanwhile, the next free one
/// is used. The worktree's path, or what git said went wrong.
pub fn create(root: &str, name: &str) -> Result<String, String> {
    exclude_own_folder(root);
    let mut name = name.to_owned();
    let mut said = String::new();
    for _ in 0..5 {
        let path = format!("{}/{OWN_FOLDER}/{name}", root.trim_end_matches(['/', '\\']));
        let branch = format!("{BRANCH_PREFIX}{name}");
        let output = git(
            &["worktree", "add", "-b", &branch, &path, "HEAD"],
            root,
            Duration::from_secs(300),
        );
        match output {
            Some(o) if o.ok => return Ok(native(&path)),
            Some(o) => said = last_line(&o.error),
            None => said.clear(),
        }
        if !(Path::new(&path).exists() || said.contains("already exists")) {
            break;
        }
        let mut taken = taken_names(root);
        taken.insert(name.clone());
        name = unique(&name, &taken);
    }
    Err(said)
}

/// Keeps the island's folder out of `git status` without touching any file
/// the repository shares.
pub fn exclude_own_folder(root: &str) {
    let Some(common) = text(git(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        root,
        QUICK,
    ))
    .filter(|c| !c.is_empty()) else {
        return;
    };
    let info = Path::new(&common).join("info");
    let file = info.join("exclude");
    let current = std::fs::read_to_string(&file).unwrap_or_default();
    let line = "/.openbar/";
    if current.lines().any(|l| l.trim() == line) {
        return;
    }
    let _ = std::fs::create_dir_all(&info);
    let separator = if current.is_empty() || current.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let _ = std::fs::write(&file, format!("{current}{separator}{line}\n"));
}

/// How removing a session or its worktree went, as the page reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Removal {
    Done,
    /// Its worktree has changes found nowhere else; nothing was removed.
    Unsaved,
    Failed {
        message: String,
    },
}

/// Removes a worktree the island made. One with changes stays unless
/// `force` says to lose them. Its branch is kept, so commits on it never go
/// with it.
pub fn remove(path: &str, force: bool) -> Removal {
    let failed = |message: String| Removal::Failed { message };
    let (true, Some(folder)) = (is_own(path), worktree_folder(path)) else {
        return failed(String::new());
    };
    let root = repository_of(&folder).to_owned();
    if !Path::new(&folder).exists() {
        let _ = git(&["worktree", "prune"], &root, QUICK);
        return Removal::Done;
    }
    let unsaved =
        || text(git(&["status", "--porcelain"], &folder, QUICK)).is_some_and(|c| !c.is_empty());
    if !force && unsaved() {
        return Removal::Unsaved;
    }
    let mut arguments = vec!["worktree", "remove"];
    if force {
        arguments.push("--force");
    }
    arguments.push(&folder);
    let output = git(&arguments, &root, Duration::from_secs(60));
    let _ = git(&["worktree", "prune"], &root, QUICK);
    match output {
        Some(o) if o.ok => Removal::Done,
        Some(o) => failed(last_line(&o.error)),
        None => failed(String::new()),
    }
}

/// What `claude rm` said when it kept a session's worktree: the value its
/// `--discard-unpushed` takes to lose the changes anyway, when it gives one.
/// None when it kept nothing.
pub fn claude_kept(said: &str) -> Option<Option<String>> {
    if !said.contains("its worktree is still at") {
        return None;
    }
    let Some(at) = said.find("--discard-unpushed ") else {
        return Some(None);
    };
    let value: String = said[at + "--discard-unpushed ".len()..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '\'' && *c != '"')
        .collect();
    Some((!value.is_empty()).then_some(value))
}

/// `claude rm` on a background session, given the arguments after the job.
/// A worktree with changes found nowhere else keeps the session unless
/// `anyway` says to lose them: uncommitted changes are dropped first, as
/// `claude rm` has no flag for them, then unpushed commits go with the
/// `--discard-unpushed` it names.
pub fn remove_claude_job(
    mut run: impl FnMut(&[String]) -> Option<Output>,
    cwd: &str,
    anyway: bool,
    discard: impl Fn(&str) -> bool,
) -> Removal {
    // It says what it kept on standard output.
    let said = |o: &Output| {
        let out = String::from_utf8_lossy(&o.out);
        [out.trim(), o.error.trim()]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let kept_by = |output: &Option<Output>| {
        output
            .as_ref()
            .filter(|o| !o.ok)
            .and_then(|o| claude_kept(&said(o)))
    };
    let mut output = run(&[]);
    let mut kept = kept_by(&output);
    if anyway && kept.is_some() {
        if matches!(kept, Some(None)) && worktree_folder(cwd).is_some_and(|folder| discard(&folder))
        {
            output = run(&[]);
            kept = kept_by(&output);
        }
        if let Some(Some(flag)) = kept.clone() {
            output = run(&["--discard-unpushed".to_owned(), flag]);
            kept = kept_by(&output);
        }
    }
    if kept.is_some() {
        return Removal::Unsaved;
    }
    match output {
        Some(o) if o.ok => Removal::Done,
        Some(o) if !said(&o).is_empty() => Removal::Failed {
            message: last_line(&said(&o)),
        },
        _ => Removal::Failed {
            message: "unavailable".to_owned(),
        },
    }
}

/// Drops a worktree's uncommitted changes, which `claude rm` has no flag
/// for, once the person has said to lose them.
pub fn discard_changes(folder: &str) -> bool {
    git(&["reset", "--hard", "-q"], folder, QUICK).is_some_and(|o| o.ok)
        && git(&["clean", "-fdq"], folder, QUICK).is_some_and(|o| o.ok)
}

/// The branch a worktree is on; None on a detached `HEAD`.
pub fn branch(folder: &str) -> Option<String> {
    text(git(
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        folder,
        QUICK,
    ))
    .filter(|b| !b.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn names() {
        assert_eq!(slug("Fix the login bug!", SLUG_LIMIT), "fix-the-login-bug");
        assert_eq!(
            slug("  Zażółć gęślą jaźń  ", SLUG_LIMIT),
            "zazolc-gesla-jazn"
        );
        assert_eq!(slug("Straße Øre", SLUG_LIMIT), "strasse-ore");
        assert_eq!(slug("日本語だけ", SLUG_LIMIT), "");
        assert_eq!(
            slug(
                "Please refactor the payments worker so retries back off exponentially",
                SLUG_LIMIT
            ),
            "please-refactor-the-payments-worker-so"
        );
        assert_eq!(slug(&"a".repeat(60), SLUG_LIMIT).len(), 40);
        assert_eq!(slug("--rm -rf /", SLUG_LIMIT), "rm-rf");
        assert_eq!(unique("fix", &set(&[])), "fix");
        assert_eq!(unique("fix", &set(&["fix", "fix-2"])), "fix-3");
        assert_eq!(name(None, "Fix login", &set(&["fix-login"])), "fix-login-2");
        assert_eq!(name(Some("My Branch"), "Fix login", &set(&[])), "my-branch");
        assert_eq!(name(Some("  "), "", &set(&[])), "session");
        assert_eq!(name(Some("!!!"), "日本", &set(&["session"])), "session-2");
    }

    #[test]
    fn paths() {
        assert_eq!(
            repository_of("/Users/me/app/.claude/worktrees/fix"),
            "/Users/me/app"
        );
        assert_eq!(
            repository_of(r"C:\code\app\.openbar\worktrees\fix\web"),
            r"C:\code\app"
        );
        assert_eq!(
            repository_of("C:/code/app/.gemini/worktrees/x"),
            "C:/code/app"
        );
        assert_eq!(repository_of(r"C:\code\app"), r"C:\code\app");
        assert_eq!(
            worktree_name(r"C:\code\app\.openbar\worktrees\fix-login\web").as_deref(),
            Some("fix-login")
        );
        assert_eq!(
            worktree_name("/r/.claude/worktrees/fix").as_deref(),
            Some("fix")
        );
        assert_eq!(worktree_name("/r/.openbar/worktrees/"), None);
        assert_eq!(worktree_name(r"C:\code\app"), None);
        assert_eq!(
            worktree_folder(r"C:\r\.openbar\worktrees\fix\web\src").as_deref(),
            Some(r"C:\r\.openbar\worktrees\fix")
        );
        assert!(is_own(r"C:\r\.openbar\worktrees\fix"));
        assert!(!is_own(r"C:\r\.claude\worktrees\fix"));
        assert_eq!(
            inside("/r/.openbar/worktrees/a", "/r", "/r"),
            native("/r/.openbar/worktrees/a")
        );
        assert_eq!(
            inside("/r/.openbar/worktrees/a", "/rest", "/r"),
            native("/r/.openbar/worktrees/a")
        );
        assert_eq!(
            inside("C:/r/.openbar/worktrees/a", r"\\?\C:\r\web", "C:/r"),
            native("C:/r/.openbar/worktrees/a/web")
        );
    }

    #[test]
    fn claude_rm_keeping_a_worktree() {
        let unsaved = "kept f9db0992 — its worktree is still at “C:\\r\\.claude\\worktrees\\wt”\n  The worktree has uncommitted changes. Deleting it would lose them.";
        assert_eq!(claude_kept(unsaved), Some(None));
        let unpushed = "kept f9db0992 — its worktree is still at “/r”\n  push them and run 'claude rm f9db0992' again, or discard the worktree and its commits: claude rm f9db0992 --discard-unpushed e2e9c7e5@ab7ce624";
        assert_eq!(
            claude_kept(unpushed),
            Some(Some("e2e9c7e5@ab7ce624".to_owned()))
        );
        assert_eq!(claude_kept("removed f9db0992"), None);
    }

    #[test]
    fn removing_a_claude_session_with_a_worktree() {
        let unsaved = "kept f9db0992 — its worktree is still at “C:\\r”\n  The worktree has uncommitted changes. Deleting it would lose them.";
        let unpushed = "kept f9db0992 — its worktree is still at “C:\\r”\n  discard the worktree and its commits: claude rm f9db0992 --discard-unpushed e2e9@ab7c";
        let removal = |answers: &[Option<&str>], anyway: bool, discards: bool| {
            let mut answers: Vec<Option<String>> =
                answers.iter().map(|a| a.map(str::to_owned)).collect();
            answers.reverse();
            let mut calls: Vec<Vec<String>> = Vec::new();
            let discarded = std::cell::RefCell::new(Vec::new());
            let outcome = remove_claude_job(
                |arguments| {
                    calls.push(arguments.to_vec());
                    let said = answers.pop().flatten();
                    // Claude Code 2.1.318 prints what it kept on standard output.
                    Some(Output {
                        ok: said.is_none(),
                        out: said.unwrap_or_default().into_bytes(),
                        error: String::new(),
                    })
                },
                r"C:\r\.claude\worktrees\w\src",
                anyway,
                |folder: &str| {
                    discarded.borrow_mut().push(folder.to_owned());
                    discards
                },
            );
            (outcome, calls, discarded.into_inner())
        };
        assert_eq!(removal(&[None], false, true).0, Removal::Done);
        let asked = removal(&[Some(unsaved)], false, true);
        assert_eq!(asked.0, Removal::Unsaved);
        assert!(asked.2.is_empty());
        let dropped = removal(&[Some(unsaved), None], true, true);
        assert_eq!(dropped.0, Removal::Done);
        assert_eq!(dropped.2, vec![r"C:\r\.claude\worktrees\w".to_owned()]);
        let both = removal(&[Some(unsaved), Some(unpushed), None], true, true);
        assert_eq!(both.0, Removal::Done);
        assert_eq!(
            both.1.last().unwrap(),
            &vec!["--discard-unpushed".to_owned(), "e2e9@ab7c".to_owned()]
        );
        assert_eq!(removal(&[Some(unsaved)], true, false).0, Removal::Unsaved);
        assert_eq!(
            removal(&[Some("no such session")], true, true).0,
            Removal::Failed {
                message: "no such session".to_owned()
            }
        );
    }

    #[test]
    fn a_real_repository() {
        if git_executable().is_none() {
            return;
        }
        let base = std::env::temp_dir().join(format!("openbar-worktree-{}", std::process::id()));
        let root = base.join("repo");
        std::fs::create_dir_all(root.join("web")).unwrap();
        let root_text = root.to_string_lossy().into_owned();
        assert_eq!(state(&root_text), RepoState::NotRepo);
        let run = |args: &[&str]| {
            let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t"];
            all.extend_from_slice(args);
            git(&all, &root_text, QUICK)
        };
        std::fs::write(root.join("web/a.txt"), "one\n").unwrap();
        run(&["init", "-q", "-b", "main"]);
        run(&["add", "."]);
        run(&["commit", "-qm", "init"]);
        let found = state(&root.join("web").to_string_lossy());
        let real = found
            .root()
            .expect("a folder in the repository is in it")
            .to_owned();
        assert_eq!(found, RepoState::Clean { root: real.clone() });
        std::fs::write(root.join("web/a.txt"), "two\n").unwrap();
        assert_eq!(state(&real), RepoState::Dirty { root: real.clone() });

        let first = reserve_name(None, "Fix login", &real);
        let second = reserve_name(None, "Fix login", &real);
        assert_eq!(
            (first.as_str(), second.as_str()),
            ("fix-login", "fix-login-2")
        );
        let path = create(&real, &first).expect("the worktree is made");
        assert!(
            path.replace('\\', "/")
                .ends_with("/.openbar/worktrees/fix-login")
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&path).join("web/a.txt"))
                .unwrap()
                .trim(),
            "one"
        );
        assert_eq!(branch(&path).as_deref(), Some("openbar/fix-login"));
        let listed = text(git(&["status", "--porcelain"], &real, QUICK)).unwrap_or_default();
        assert!(!listed.contains(".openbar"), "{listed}");
        exclude_own_folder(&real);
        let exclude = std::fs::read_to_string(Path::new(&real).join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches("/.openbar/").count(), 1);
        let again = create(&real, &first).expect("a taken name still gets a worktree");
        assert!(again.ends_with("fix-login-3"), "{again}");

        std::fs::write(Path::new(&path).join("web/a.txt"), "edit\n").unwrap();
        let deeper = format!("{path}/web");
        assert_eq!(remove(&deeper, false), Removal::Unsaved);
        assert!(Path::new(&path).exists());
        assert_eq!(remove(&deeper, true), Removal::Done);
        assert!(!Path::new(&path).exists());
        let branches =
            text(git(&["branch", "--list", "openbar/*"], &real, QUICK)).unwrap_or_default();
        assert!(branches.contains("openbar/fix-login"));
        assert_eq!(remove(&again, false), Removal::Done);
        assert_eq!(remove(&again, false), Removal::Done);
        assert_eq!(
            remove(&format!("{real}/.claude/worktrees/x"), true),
            Removal::Failed {
                message: String::new()
            }
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
