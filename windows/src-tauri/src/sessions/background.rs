// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Claude Code sessions running in the background, with no terminal, ported
//! from `ClaudeBackgroundSessions.swift`. Everything read from what
//! `claude agents`, `claude logs` and the config files say is plain code; the
//! commands themselves, and the private console a reply is typed through,
//! are at the end.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::models::{self, HostKind, Session, SessionPhase, Time, Tokens, int_of, str_of};

/// The switch that turns Agent View off.
pub const DISABLE_VARIABLE: &str = "CLAUDE_CODE_DISABLE_AGENT_VIEW";
/// How long a finished session stays listed, and how many at most.
pub const KEPT_FOR: Time = 24.0 * 60.0 * 60.0;
pub const KEPT_AT_MOST: usize = 6;
pub const FOLDERS_AT_MOST: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundState {
    Working,
    Blocked,
    Done,
    Failed,
    Stopped,
}

impl BackgroundState {
    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw {
            "working" => Some(Self::Working),
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

/// What makes a session a background one: the job `claude attach`, `logs`,
/// `stop` and `rm` know it by.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundDetails {
    pub job: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<BackgroundState>,
    /// Its process is gone; `claude attach` brings it back.
    pub ended: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_home: Option<String>,
}

/// One background session as `claude agents --json --all` lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub id: String,
    pub session_id: String,
    pub cwd: String,
    pub name: String,
    pub state: Option<BackgroundState>,
    /// None once its process has ended.
    pub pid: Option<i32>,
    pub started: Option<Time>,
    pub config_home: Option<String>,
}

impl Job {
    pub fn ended(&self) -> bool {
        self.pid.is_none()
    }
}

/// Registry records say "bg", Agent View's list says "background".
pub fn is_background(kind: Option<&str>) -> bool {
    matches!(kind, Some("bg" | "background"))
}

/// The job a registry record names, for a background session. The job's short
/// id starts the session's id, should the record not say.
pub fn job_of_record(json: &serde_json::Map<String, Value>, session: &str) -> Option<String> {
    if !is_background(str_of(json.get("kind"))) {
        return None;
    }
    Some(
        str_of(json.get("jobId"))
            .filter(|job| !job.is_empty())
            .map_or_else(|| session.chars().take(8).collect(), str::to_owned),
    )
}

/// The list's background sessions; None when the output isn't that list, as
/// when Agent View is off or this Claude Code has none.
pub fn decode(data: &[u8], config_home: Option<&str>) -> Option<Vec<Job>> {
    let value: Value = serde_json::from_slice(data).ok()?;
    let list = value.as_array()?;
    Some(
        list.iter()
            .filter_map(|item| {
                let item = item.as_object()?;
                if !is_background(str_of(item.get("kind"))) {
                    return None;
                }
                let id = str_of(item.get("id")).filter(|s| !s.is_empty())?;
                let session = str_of(item.get("sessionId")).filter(|s| !s.is_empty())?;
                let cwd = str_of(item.get("cwd")).filter(|s| !s.is_empty())?;
                Some(Job {
                    id: id.to_owned(),
                    session_id: session.to_owned(),
                    cwd: cwd.to_owned(),
                    name: str_of(item.get("name"))
                        .map_or_else(|| models::last_path_component(cwd), str::to_owned),
                    state: str_of(item.get("state")).and_then(BackgroundState::from_raw),
                    pid: int_of(item.get("pid"))
                        .filter(|pid| *pid > 0 && *pid <= i64::from(i32::MAX))
                        .map(|pid| pid as i32),
                    started: item
                        .get("startedAt")
                        .and_then(Value::as_f64)
                        .map(|ms| ms / 1000.0),
                    config_home: config_home.map(str::to_owned),
                })
            })
            .collect(),
    )
}

/// When each ended job ended, as near as can be told: now for one seen alive
/// before, else when it started. Keeps only the jobs still ended.
pub fn endings(
    jobs: &[Job],
    known: &HashMap<String, Time>,
    alive: &[String],
    now: Time,
) -> HashMap<String, Time> {
    jobs.iter()
        .filter(|job| job.ended())
        .map(|job| {
            let at = known.get(&job.id).copied().unwrap_or_else(|| {
                if alive.contains(&job.id) {
                    now
                } else {
                    job.started.unwrap_or(now)
                }
            });
            (job.id.clone(), at)
        })
        .collect()
}

/// The ended jobs still worth listing, newest first.
pub fn kept<'a>(jobs: &'a [Job], endings: &HashMap<String, Time>, now: Time) -> Vec<&'a Job> {
    let mut recent: Vec<&Job> = jobs
        .iter()
        .filter(|job| {
            job.ended()
                && endings
                    .get(&job.id)
                    .is_some_and(|ended| now - ended < KEPT_FOR)
        })
        .collect();
    recent.sort_by(|a, b| {
        let at = |job: &Job| endings.get(&job.id).copied().unwrap_or(0.0);
        at(b).total_cmp(&at(a))
    });
    recent.truncate(KEPT_AT_MOST);
    recent
}

/// A row for a session whose process has ended. Quiet: it counts as no work
/// and asks for nothing.
pub fn ended_session(job: &Job, ended: Time, branch: Option<String>) -> Session {
    Session {
        id: job.session_id.clone(),
        provider: models::Provider::Claude,
        pid: 0,
        cwd: job.cwd.clone(),
        project: models::project_name(&job.cwd),
        name: job.name.clone(),
        name_source: Some("user".into()),
        branch,
        model: None,
        model_name: None,
        host: HostKind::Background,
        accepts_replies: false,
        phase: SessionPhase::Idle,
        phase_since: ended,
        turn_started: None,
        last_active: Some(ended),
        looked_at: None,
        waiting_for: None,
        current_tool: None,
        permission: None,
        timeline: vec![],
        subagents: vec![],
        tasks: None,
        tokens: Tokens::default(),
        cost: 0.0,
        title: None,
        last_text: None,
        hooked: false,
        chat: vec![],
        background_label: None,
        services: 0,
        background: Some(BackgroundDetails {
            job: job.id.clone(),
            state: job.state,
            ended: true,
            config_home: job.config_home.clone(),
        }),
    }
}

pub fn is_on(value: Option<&str>) -> bool {
    let Some(value) = value.map(|v| v.trim().to_lowercase()) else {
        return false;
    };
    !value.is_empty() && !["0", "false", "no", "off"].contains(&value.as_str())
}

/// Whether settings turn Agent View off, as `disableAgentView` or the variable
/// in their `env`.
pub fn settings_disable(data: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return false;
    };
    if value.get("disableAgentView").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    match value.get("env").and_then(|env| env.get(DISABLE_VARIABLE)) {
        Some(Value::String(text)) => is_on(Some(text)),
        Some(Value::Number(number)) => is_on(Some(&number.to_string())),
        Some(Value::Bool(flag)) => *flag,
        _ => false,
    }
}

/// The arguments that start a session; `--` keeps a prompt that begins with a
/// dash from reading as an option.
pub fn start_arguments(prompt: &str, background: bool) -> Vec<String> {
    let mut arguments = Vec::new();
    if background {
        arguments.push("--bg".to_owned());
    }
    let prompt = prompt.trim();
    if !prompt.is_empty() {
        arguments.push("--".to_owned());
        arguments.push(prompt.to_owned());
    }
    arguments
}

/// Only what a job's id can be goes on a command line built by hand.
pub fn is_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// MARK: Recent folders

/// A folder Claude Code has worked in, for starting a new session there.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFolder {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_home: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<Time>,
}

/// `~/.claude.json` belongs to the standard folder; a moved config folder
/// keeps its own inside.
pub fn config_file(folder: &Path, user: &Path) -> PathBuf {
    if folder == user.join(".claude") {
        user.join(".claude.json")
    } else {
        folder.join(".claude.json")
    }
}

/// The name of a folder's transcripts under `projects/`.
pub fn transcript_folder(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

pub fn projects(data: &[u8]) -> Vec<String> {
    serde_json::from_slice::<Value>(data)
        .ok()
        .and_then(|value| {
            value
                .get("projects")?
                .as_object()
                .map(|projects| projects.keys().cloned().collect())
        })
        .unwrap_or_default()
}

/// Folders still on disk across the config folders, the one worked in last
/// first; where one is named twice, the latest use wins.
pub fn recent_folders(homes: &[PathBuf], user: &Path) -> Vec<RecentFolder> {
    let mut found: HashMap<String, RecentFolder> = HashMap::new();
    for home in homes {
        let Ok(data) = std::fs::read(config_file(home, user)) else {
            continue;
        };
        for path in projects(&data) {
            // Claude Code writes these with forward slashes on Windows too.
            let shown = path.replace('/', std::path::MAIN_SEPARATOR_STR);
            if !Path::new(&shown).is_dir() {
                continue;
            }
            let used = std::fs::metadata(home.join("projects").join(transcript_folder(&path)))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs_f64());
            if let Some(known) = found.get(&shown)
                && known.used.unwrap_or(0.0) >= used.unwrap_or(0.0)
            {
                continue;
            }
            let name = models::last_path_component(&shown);
            found.insert(
                shown.clone(),
                RecentFolder {
                    path: shown,
                    name,
                    config_home: Some(home.to_string_lossy().into_owned()),
                    used,
                },
            );
        }
    }
    ordered(found.into_values().collect())
}

pub fn ordered(mut folders: Vec<RecentFolder>) -> Vec<RecentFolder> {
    folders.sort_by(|a, b| {
        b.used
            .unwrap_or(0.0)
            .total_cmp(&a.used.unwrap_or(0.0))
            .then_with(|| a.path.cmp(&b.path))
    });
    folders.truncate(FOLDERS_AT_MOST);
    folders
}

// MARK: Reading a terminal's output

/// How many lines with words in them make output worth reading. A session that
/// only animates a spinner fills `claude logs` with that alone.
const READABLE_LINES: usize = 3;

pub fn is_readable(text: &str) -> bool {
    text.lines()
        .filter(|line| line.chars().any(char::is_alphabetic))
        .count()
        >= READABLE_LINES
}

/// Reads what `claude logs` prints: the last stretch of a terminal's stream,
/// its text placed with cursor moves, which simply removing the codes would
/// run together.
pub fn screen_text(data: &[u8]) -> String {
    use std::collections::BTreeMap;
    let chars: Vec<char> = String::from_utf8_lossy(data).chars().collect();
    let mut rows: BTreeMap<usize, BTreeMap<usize, char>> = BTreeMap::new();
    let (mut row, mut column) = (0usize, 0usize);
    let mut saved = (0usize, 0usize);
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        index += 1;
        match c {
            '\u{1b}' => {
                let Some(&kind) = chars.get(index) else { break };
                index += 1;
                match kind {
                    '[' => {
                        let start = index;
                        while index < chars.len() && !('\u{40}'..='\u{7e}').contains(&chars[index])
                        {
                            index += 1;
                        }
                        let Some(&last) = chars.get(index) else { break };
                        let parameters: String = chars[start..index].iter().collect();
                        index += 1;
                        // A private sequence (`?`, `<`, `>`) sets modes; it moves nothing.
                        if parameters.starts_with(['?', '<', '>']) {
                            continue;
                        }
                        let numbers: Vec<Option<usize>> =
                            parameters.split(';').map(|n| n.parse().ok()).collect();
                        let first = numbers.first().copied().flatten();
                        let step = first.unwrap_or(1).max(1);
                        match last {
                            'A' => row = row.saturating_sub(step),
                            'B' => row += step,
                            'C' => column += step,
                            'D' => column = column.saturating_sub(step),
                            'G' => column = first.unwrap_or(1).saturating_sub(1),
                            'H' | 'f' => {
                                row = first.unwrap_or(1).saturating_sub(1);
                                column = numbers
                                    .get(1)
                                    .copied()
                                    .flatten()
                                    .unwrap_or(1)
                                    .saturating_sub(1);
                            }
                            'J' => match first {
                                Some(2 | 3) => rows.clear(),
                                None | Some(0) => {
                                    rows.retain(|number, _| *number <= row);
                                    if let Some(cells) = rows.get_mut(&row) {
                                        cells.retain(|cell, _| *cell < column);
                                    }
                                }
                                _ => {}
                            },
                            'K' => match first {
                                None | Some(0) => {
                                    if let Some(cells) = rows.get_mut(&row) {
                                        cells.retain(|cell, _| *cell < column);
                                    }
                                }
                                Some(2) => {
                                    rows.remove(&row);
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    ']' | 'P' | '_' | '^' => {
                        // A title or similar, up to its terminator.
                        while index < chars.len() && chars[index] != '\u{7}' {
                            if chars[index] == '\u{1b}' {
                                index += 1;
                                break;
                            }
                            index += 1;
                        }
                        index += 1;
                    }
                    '7' => saved = (row, column),
                    '8' => (row, column) = saved,
                    '(' | ')' => index += 1,
                    _ => {}
                }
            }
            '\r' => column = 0,
            '\n' => {
                row += 1;
                column = 0;
            }
            '\u{8}' => column = column.saturating_sub(1),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            // A mark combining with the letter before it takes no cell.
            '\u{300}'..='\u{36f}' => {}
            c => {
                rows.entry(row).or_default().insert(column, c);
                column += 1;
            }
        }
    }
    let Some(&last) = rows.keys().next_back() else {
        return String::new();
    };
    let mut lines: Vec<String> = Vec::new();
    for number in 0..=last {
        let text = rows.get(&number).map_or_else(String::new, |cells| {
            let end = cells.keys().next_back().copied().unwrap_or(0);
            let line: String = (0..=end)
                .map(|cell| cells.get(&cell).copied().unwrap_or(' '))
                .collect();
            line.trim_end().to_owned()
        });
        // Without the blank stretches a full-screen layout leaves between its parts.
        if text.is_empty() && lines.last().is_none_or(String::is_empty) {
            continue;
        }
        lines.push(text);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// Ctrl+Z, the documented way out of `claude attach`.
pub const DETACH: u8 = 0x1a;

/// What goes to the session's prompt, in order, with the milliseconds each
/// needs: images are pasted as their paths, which Claude Code takes a moment to
/// read, then the text; the return alone at the end sends it.
pub fn keystrokes(text: &str, images: &[String]) -> Vec<(Vec<u8>, u64)> {
    // A terminal pastes line breaks as returns, which is what a console's
    // input carries for the Enter key.
    let paste = |value: &str| {
        let value = value.replace("\r\n", "\r").replace('\n', "\r");
        format!("\u{1b}[200~{value}\u{1b}[201~").into_bytes()
    };
    let mut steps: Vec<(Vec<u8>, u64)> = images.iter().map(|i| (paste(i), 200)).collect();
    if let Some(last) = steps.last_mut() {
        last.1 = 1200 + 300 * images.len() as u64;
    }
    if !text.is_empty() {
        steps.push((paste(text), 400));
    }
    steps.push((vec![b'\r'], 500));
    steps
}

/// The `claude` a person's `PATH` finds, then where its installers put it. The
/// native installer makes an `.exe`, npm a `.cmd`.
pub fn locate(path: Option<&str>, home: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let names = ["claude.exe", "claude.cmd", "claude.bat"];
    let on_path = path
        .unwrap_or("")
        .split(';')
        .filter(|dir| !dir.is_empty())
        .flat_map(|dir| names.iter().map(move |name| Path::new(dir).join(name)));
    let usual = [
        home.join(".local").join("bin").join("claude.exe"),
        home.join(".claude").join("local").join("claude.exe"),
        home.join("AppData")
            .join("Roaming")
            .join("npm")
            .join("claude.cmd"),
    ];
    on_path.chain(usual).find(|candidate| exists(candidate))
}

/// Drops what marks this environment as a session's. The switches stay: they
/// are the person's.
pub fn keeps_variable(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper == DISABLE_VARIABLE
        || upper == super::registry::CONFIG_VARIABLE
        || !upper.starts_with("CLAUDE")
}

/// The `CLAUDE_CONFIG_DIR` a config folder needs; None for the standard one.
pub fn config_variable(config_home: Option<&str>, user: &Path) -> Option<String> {
    let config_home = config_home?;
    (Path::new(config_home) != user.join(".claude")).then(|| config_home.to_owned())
}

// MARK: Running Claude Code

pub struct Output {
    pub ok: bool,
    pub out: Vec<u8>,
    pub error: String,
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{
        CloseHandle, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
    };
    use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows::Win32::System::Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON};
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::{
        CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, STARTUPINFOEXW, TerminateProcess,
        UpdateProcThreadAttribute, WaitForSingleObject,
    };
    use windows::core::{PCWSTR, PWSTR};

    use super::{Output, config_variable, keeps_variable, keystrokes, locate};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    const PSEUDOCONSOLE_ATTRIBUTE: usize = 0x0002_0016;
    const STILL_ACTIVE: u32 = 259;

    fn user() -> Option<PathBuf> {
        crate::hooks::installer::home_dir()
    }

    pub fn executable() -> Option<PathBuf> {
        locate(
            std::env::var("PATH").ok().as_deref(),
            &user()?,
            |candidate| candidate.is_file(),
        )
    }

    /// The environment a command gets: the app's, without a session's marks,
    /// with the config folder it acts in.
    fn environment(config_home: Option<&str>) -> Vec<(String, String)> {
        let setting = user().and_then(|user| config_variable(config_home, &user));
        let mut list: Vec<(String, String)> = std::env::vars()
            .filter(|(name, _)| {
                keeps_variable(name)
                    && !name.eq_ignore_ascii_case(super::super::registry::CONFIG_VARIABLE)
            })
            .collect();
        if let Some(setting) = setting {
            list.push((super::super::registry::CONFIG_VARIABLE.to_owned(), setting));
        }
        list
    }

    fn command(
        executable: &Path,
        arguments: &[String],
        config_home: Option<&str>,
        folder: Option<&str>,
    ) -> Command {
        let mut command = Command::new(executable);
        command
            .args(arguments)
            .env_clear()
            .envs(environment(config_home));
        if let Some(folder) = folder {
            command.current_dir(folder);
        }
        command
    }

    /// One `claude` command, with no window. None when it can't be run or
    /// doesn't finish in time.
    pub fn claude(
        arguments: &[String],
        config_home: Option<&str>,
        folder: Option<&str>,
        timeout: Duration,
    ) -> Option<Output> {
        let executable = executable()?;
        let mut child = command(&executable, arguments, config_home, folder)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
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
            std::thread::sleep(Duration::from_millis(40));
        };
        // A service it left running may hold the pipes open.
        let wait = Duration::from_millis(500);
        Some(Output {
            ok: status.success(),
            out: out.recv_timeout(wait).unwrap_or_default(),
            error: String::from_utf8_lossy(&error.recv_timeout(wait).unwrap_or_default())
                .trim()
                .to_owned(),
        })
    }

    /// `claude` in a console of its own, which opens in the person's default
    /// terminal, Windows Terminal or the classic one, with this environment.
    pub fn in_terminal(
        arguments: &[String],
        config_home: Option<&str>,
        folder: Option<&str>,
    ) -> bool {
        let Some(executable) = executable() else {
            return false;
        };
        command(&executable, arguments, config_home, folder)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .is_ok()
    }

    struct Owned(HANDLE);
    unsafe impl Send for Owned {}
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                let _ = unsafe { CloseHandle(self.0) };
            }
        }
    }

    fn pipe() -> Option<(Owned, Owned)> {
        let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
        unsafe { CreatePipe(&mut read, &mut write, None, 0) }.ok()?;
        let pair = (Owned(read), Owned(write));
        // Only the console gets these, never by inheritance.
        for handle in [pair.0.0, pair.1.0] {
            let _ = unsafe {
                SetHandleInformation(
                    handle,
                    HANDLE_FLAG_INHERIT.0,
                    windows::Win32::Foundation::HANDLE_FLAGS(0),
                )
            };
        }
        Some(pair)
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Attaches through `claude attach` in a console of the app's own that
    /// nobody sees, once its screen has settled does `work`, and leaves. None
    /// when it could not attach; else whether the work went through, and the
    /// screen as it first painted.
    fn attached(
        job: &str,
        config_home: Option<&str>,
        work: impl FnOnce(&mut dyn FnMut(&[u8]) -> bool) -> bool,
    ) -> Option<(bool, Vec<u8>)> {
        if !super::is_job_id(job) {
            return None;
        }
        let executable = executable()?;
        let (input_read, input_write) = pipe()?;
        let (output_read, output_write) = pipe()?;
        let console: HPCON = unsafe {
            CreatePseudoConsole(COORD { X: 200, Y: 50 }, input_read.0, output_write.0, 0)
        }
        .ok()?;
        drop(input_read);
        drop(output_write);
        struct Console(HPCON);
        unsafe impl Send for Console {}
        impl Drop for Console {
            fn drop(&mut self) {
                unsafe { ClosePseudoConsole(self.0) };
            }
        }
        let console = Console(console);

        let mut size = 0usize;
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
        let mut attributes = vec![0u8; size];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(attributes.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut size) }.ok()?;
        struct List(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for List {
            fn drop(&mut self) {
                unsafe { DeleteProcThreadAttributeList(self.0) };
            }
        }
        let guard = List(list);
        unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PSEUDOCONSOLE_ATTRIBUTE,
                Some(console.0.0 as *const core::ffi::c_void),
                std::mem::size_of::<HPCON>(),
                None,
                None,
            )
        }
        .ok()?;

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = list;
        let program = executable.to_string_lossy().into_owned();
        // A `.cmd` from npm runs through its interpreter. The id was checked above.
        let line = if program.to_lowercase().ends_with(".exe") {
            format!("\"{program}\" attach {job}")
        } else {
            format!("cmd.exe /c \"\"{program}\" attach {job}\"")
        };
        let mut line = wide(&line);
        let mut block: Vec<u16> = Vec::new();
        for (name, value) in environment(config_home) {
            block.extend(format!("{name}={value}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        let mut information = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(line.as_mut_ptr())),
                None,
                None,
                false,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                Some(block.as_ptr().cast()),
                PCWSTR::null(),
                &startup.StartupInfo,
                &mut information,
            )
        }
        .ok()?;
        let process = Owned(information.hProcess);
        drop(Owned(information.hThread));

        // Its output must be read for it to go on, and tells when it is ready.
        let seen: Arc<Mutex<(Vec<u8>, Option<Instant>)>> = Arc::new(Mutex::new((Vec::new(), None)));
        let reader = seen.clone();
        std::thread::spawn(move || {
            let output_read = output_read;
            let mut buffer = [0u8; 1 << 15];
            loop {
                let mut count = 0u32;
                let read =
                    unsafe { ReadFile(output_read.0, Some(&mut buffer), Some(&mut count), None) };
                if read.is_err() || count == 0 {
                    break;
                }
                let mut state = reader.lock().unwrap();
                if state.0.len() < 1 << 21 {
                    state.0.extend_from_slice(&buffer[..count as usize]);
                }
                state.1 = Some(Instant::now());
            }
        });
        let running = |process: &Owned| {
            let mut code = 0u32;
            unsafe { GetExitCodeProcess(process.0, &mut code) }.is_ok() && code == STILL_ACTIVE
        };
        // The first output and then a quiet moment. A screen that keeps moving,
        // as a working session's does, counts as ready when the time is up.
        let limit = Instant::now() + Duration::from_secs(6);
        let mut printed = false;
        while Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(100));
            let last = seen.lock().unwrap().1;
            printed = last.is_some();
            if last.is_some_and(|at| at.elapsed() >= Duration::from_millis(500)) {
                break;
            }
        }
        let leave = |process: &Owned| {
            let _ = unsafe { TerminateProcess(process.0, 1) };
        };
        if !printed || !running(&process) {
            leave(&process);
            return None;
        }
        let screen = seen.lock().unwrap().0.clone();
        let mut send = |bytes: &[u8]| {
            let mut written = 0u32;
            running(&process)
                && unsafe { WriteFile(input_write.0, Some(bytes), Some(&mut written), None) }
                    .is_ok()
                && written as usize == bytes.len()
        };
        let done = work(&mut send);
        let _ = send(&[super::DETACH]);
        if unsafe { WaitForSingleObject(process.0, 3000) } != WAIT_OBJECT_0 {
            leave(&process);
        }
        drop(guard);
        drop(console);
        Some((done, screen))
    }

    pub fn reply(text: &str, images: &[String], job: &str, config_home: Option<&str>) -> bool {
        attached(job, config_home, |send| {
            keystrokes(text, images).into_iter().all(|(bytes, pause)| {
                let sent = send(&bytes);
                if sent {
                    std::thread::sleep(Duration::from_millis(pause));
                }
                sent
            })
        })
        .is_some_and(|(done, _)| done)
    }

    pub fn screen(job: &str, config_home: Option<&str>) -> Option<String> {
        attached(job, config_home, |_| true).map(|(_, screen)| super::screen_text(&screen))
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::Output;

    pub fn executable() -> Option<PathBuf> {
        None
    }
    pub fn claude(_: &[String], _: Option<&str>, _: Option<&str>, _: Duration) -> Option<Output> {
        None
    }
    pub fn in_terminal(_: &[String], _: Option<&str>, _: Option<&str>) -> bool {
        false
    }
    pub fn reply(_: &str, _: &[String], _: &str, _: Option<&str>) -> bool {
        false
    }
    pub fn screen(_: &str, _: Option<&str>) -> Option<String> {
        None
    }
}

pub use platform::{claude, executable, in_terminal, reply, screen};

#[cfg(test)]
mod tests {
    use super::*;

    fn job(
        id: &str,
        pid: Option<i32>,
        started: Option<Time>,
        state: Option<BackgroundState>,
    ) -> Job {
        Job {
            id: id.into(),
            session_id: format!("{id}-s"),
            cwd: format!("C:\\p\\{id}"),
            name: id.into(),
            state,
            pid,
            started,
            config_home: None,
        }
    }

    #[test]
    fn the_list_from_what_claude_code_prints() {
        let output = br#"[{"pid":17514,"cwd":"/x","kind":"interactive","sessionId":"i","name":"mine","status":"idle"},
          {"pid":18198,"id":"a5b5e11e","cwd":"/tmp/one","kind":"background","startedAt":1793486734159,"sessionId":"a5-ad","name":"one","status":"idle","state":"done"},
          {"id":"b7f26484","cwd":"/tmp/two","kind":"background","sessionId":"b7-ad","state":"something new"}]"#;
        let jobs = decode(output, Some("H")).unwrap();
        assert_eq!(jobs.len(), 2, "only background sessions");
        assert_eq!(jobs[0].state, Some(BackgroundState::Done));
        assert_eq!(jobs[0].pid, Some(18198));
        assert_eq!(jobs[0].config_home.as_deref(), Some("H"));
        assert!(jobs[1].ended());
        assert_eq!(jobs[1].state, None, "a state not known yet");
        assert_eq!(jobs[1].name, "two");
        assert!(decode(b"'claude agents --json' is disabled", None).is_none());
        assert_eq!(decode(b"[]", None), Some(vec![]));
    }

    #[test]
    fn a_record_names_its_job() {
        let of = |text: &str| {
            let value: Value = serde_json::from_str(text).unwrap();
            job_of_record(value.as_object().unwrap(), "abcdef1234567")
        };
        assert_eq!(
            of(r#"{"kind":"bg","jobId":"a5b5e11e"}"#).as_deref(),
            Some("a5b5e11e")
        );
        assert_eq!(of(r#"{"kind":"background"}"#).as_deref(), Some("abcdef12"));
        assert_eq!(of(r#"{"kind":"interactive"}"#), None);
        assert_eq!(of(r#"{}"#), None, "older Claude Code writes no kind");
    }

    #[test]
    fn ended_sessions_stay_a_while() {
        let now = 1_000_000.0;
        let old = now - 2.0 * KEPT_FOR;
        let jobs = [
            job("seen", None, Some(old), None),
            job("stale", None, Some(old), None),
            job("alive", Some(5), Some(old), None),
        ];
        let ends = endings(&jobs, &HashMap::new(), &["seen".to_owned()], now);
        assert_eq!(ends["seen"], now, "one seen alive ended just now");
        assert_eq!(ends["stale"], old);
        assert!(!ends.contains_key("alive"));
        let ids: Vec<&str> = kept(&jobs, &ends, now)
            .iter()
            .map(|j| j.id.as_str())
            .collect();
        assert_eq!(ids, ["seen"]);

        let many: Vec<Job> = (0..9)
            .map(|n| job(&format!("j{n}"), None, None, None))
            .collect();
        let times: HashMap<String, Time> = many
            .iter()
            .enumerate()
            .map(|(n, j)| (j.id.clone(), now - n as f64))
            .collect();
        let few = kept(&many, &times, now);
        assert_eq!(few.len(), KEPT_AT_MOST);
        assert_eq!(few[0].id, "j0");

        let row = ended_session(
            &job("r", None, None, Some(BackgroundState::Failed)),
            now,
            Some("main".into()),
        );
        assert_eq!(row.phase, SessionPhase::Idle);
        assert_eq!(row.host, HostKind::Background);
        assert_eq!(row.branch.as_deref(), Some("main"));
        assert!(
            !models::can_reply(&row),
            "it takes no reply until Open brings it back"
        );
        assert_eq!(
            models::SessionCounts::of(std::slice::from_ref(&row)),
            models::SessionCounts::default()
        );
    }

    #[test]
    fn agent_view_switched_off() {
        assert!(settings_disable(br#"{"disableAgentView":true}"#));
        assert!(!settings_disable(br#"{"disableAgentView":false}"#));
        assert!(settings_disable(
            br#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":"1"}}"#
        ));
        assert!(settings_disable(
            br#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":1}}"#
        ));
        assert!(!settings_disable(
            br#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":"0"}}"#
        ));
        assert!(!settings_disable(b"not json"));
        assert!(
            keeps_variable("PATH")
                && keeps_variable(DISABLE_VARIABLE)
                && keeps_variable("CLAUDE_CONFIG_DIR")
        );
        assert!(!keeps_variable("CLAUDECODE") && !keeps_variable("CLAUDE_CODE_SESSION_ID"));
    }

    #[test]
    fn the_screen_from_logs() {
        let sample = "\u{1b}7\u{1b}8\u{1b}[2J\u{1b}[H\u{1b}[H\r\u{1b}[1B\u{1b}[91m X\u{1b}[12G\u{1b}[39m\u{1b}[1mClaude Code\u{1b}[24G\u{1b}[22mv2.1.316\r\u{1b}[3B\u{1b}[100m❯ \u{1b}[97mSay hello\u{1b}[39m      \r\u{1b}[38B\u{1b}[49m·\u{1b}[3GBaking… \r\u{1b}[5Cend";
        assert_eq!(
            screen_text(sample.as_bytes()),
            " X         Claude Code v2.1.316\n\n❯ Say hello\n\n· Bakend…"
        );
        assert_eq!(screen_text(b""), "");
        assert_eq!(
            screen_text("\u{1b}]0;title\u{7}a\u{1b}[?25lb\u{1b}[".as_bytes()),
            "ab"
        );
        assert_eq!(screen_text("abc\u{1b}[1G\u{1b}[Kz".as_bytes()), "z");
        assert_eq!(screen_text("\u{1b}[99A\u{1b}[99Dq".as_bytes()), "q");
        assert_eq!(screen_text("e\u{301}x".as_bytes()).chars().count(), 2);
        assert!(!is_readable("⏺"), "a spinner alone");
        assert!(is_readable("Claude Code\n\n❯ hi\n⏺ hello"));
    }

    #[test]
    fn command_lines() {
        assert_eq!(start_arguments("-x", true), ["--bg", "--", "-x"]);
        assert_eq!(start_arguments("  ", true), ["--bg"]);
        assert!(start_arguments("", false).is_empty());
        assert!(
            is_job_id("a5b5e11e")
                && !is_job_id("a b")
                && !is_job_id("")
                && !is_job_id("x\" & calc")
        );
        let user = Path::new("C:\\Users\\me");
        let standard = user.join(".claude");
        assert_eq!(
            config_variable(Some(&standard.to_string_lossy()), user),
            None
        );
        assert_eq!(
            config_variable(Some("D:\\c"), user).as_deref(),
            Some("D:\\c")
        );
        assert_eq!(config_variable(None, user), None);
        let found = locate(Some("C:\\nope;C:\\yes"), user, |p| {
            p == Path::new("C:\\yes").join("claude.cmd")
        });
        assert_eq!(found, Some(Path::new("C:\\yes").join("claude.cmd")));
        let native = user.join(".local").join("bin").join("claude.exe");
        assert_eq!(locate(None, user, |p| p == native), Some(native.clone()));
        assert_eq!(locate(None, user, |_| false), None);
    }

    #[test]
    fn hidden_attach_keystrokes() {
        let text = |steps: Vec<(Vec<u8>, u64)>| -> Vec<String> {
            steps
                .into_iter()
                .map(|(b, _)| String::from_utf8(b).unwrap())
                .collect()
        };
        assert_eq!(
            text(keystrokes("one\ntwo", &[])),
            ["\u{1b}[200~one\rtwo\u{1b}[201~", "\r"]
        );
        let both = keystrokes("look", &["a.png".to_owned(), "b.png".to_owned()]);
        assert_eq!(both[1].1, 1800, "time to read the images");
        assert_eq!(both.len(), 4);
        assert_eq!(keystrokes("", &["a.png".to_owned()]).len(), 2);
    }

    #[test]
    fn recent_folders_read() {
        assert_eq!(
            transcript_folder("C:\\Users\\me\\my.app"),
            "C--Users-me-my-app"
        );
        assert_eq!(projects(br#"{"projects":{"/a":{}}}"#), ["/a"]);
        assert!(projects(b"nope").is_empty());
        let user = Path::new("U");
        assert_eq!(
            config_file(&user.join(".claude"), user),
            user.join(".claude.json")
        );
        assert_eq!(
            config_file(&user.join(".claude-2"), user),
            user.join(".claude-2").join(".claude.json")
        );

        let root = std::env::temp_dir().join(format!("vs-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join(".claude");
        let (a, b) = (root.join("a"), root.join("b"));
        for folder in [&a, &b, &home.join("projects")] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let config = serde_json::json!({"projects": {
            a.to_string_lossy(): {}, b.to_string_lossy(): {}, root.join("gone").to_string_lossy(): {}
        }});
        std::fs::write(root.join(".claude.json"), config.to_string()).unwrap();
        std::fs::create_dir_all(
            home.join("projects")
                .join(transcript_folder(&b.to_string_lossy())),
        )
        .unwrap();
        let read = recent_folders(&[home.clone(), root.join(".missing")], &root);
        let paths: Vec<&str> = read.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [b.to_string_lossy(), a.to_string_lossy()],
            "worked in last first, a gone folder out"
        );
        let many: Vec<RecentFolder> = (0..20)
            .map(|n| RecentFolder {
                path: format!("/p{n}"),
                name: String::new(),
                config_home: None,
                used: None,
            })
            .collect();
        assert_eq!(ordered(many).len(), FOLDERS_AT_MOST);
        let _ = std::fs::remove_dir_all(&root);
    }
}
