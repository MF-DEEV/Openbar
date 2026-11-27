// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Remote hosts over SSH (issue #189; RemoteHosts.swift and
//! RemoteHostLink.swift on the Mac, docs/remote-hosts.md). Per host the
//! island runs the system `ssh` (Windows' own OpenSSH client) with the
//! person's config and keys: a short probe, then one `ssh` that forwards the
//! host's hook socket to the island and runs `Resources/remote-agent.sh`,
//! answering requests on its standard input and output.
//!
//! On Windows the island's hooks arrive on a named pipe, which `-R` cannot
//! reach, so the host's socket is forwarded to a loopback TCP port instead,
//! and the port only takes connections whose owner is one of these `ssh`
//! processes.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};

use super::containers;
use super::models::{HostKind, SessionRecord};
use super::registry;
use crate::hooks::installer;

/// The script every host runs, shared with the Mac.
pub const SCRIPT: &str = include_str!("../../../../Resources/remote-agent.sh");

/// What the host's login shell is given: one `sh` in single quotes, which
/// fish, zsh, bash and sh all read the same way.
pub const LOADER: &str = r#"exec sh -c 'IFS= read -r s; eval "$(printf %s "$s" | base64 -d)"'"#;

// MARK: Hosts

/// An alias `ssh` takes as a destination and never as an option.
pub fn is_valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias.len() <= 255
        && !alias.starts_with('-')
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-@:[]%".contains(c))
}

/// The hosts from the prefs (`/sessions/remoteHosts`), once each.
pub fn hosts_from_prefs(value: &Value) -> Vec<String> {
    let enabled = value
        .pointer("/sessions/remoteHostsEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return Vec::new();
    }
    let mut seen = Vec::new();
    for host in value
        .pointer("/sessions/remoteHosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(alias) = host.as_str()
            && is_valid_alias(alias)
            && !seen.iter().any(|s| s == alias)
        {
            seen.push(alias.to_string());
        }
    }
    seen
}

pub fn socket_name(app: &str) -> String {
    app.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect::<String>()
        + ".sock"
}

/// Tries again sooner first, then every half minute.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(2u64.pow(attempt.clamp(1, 5)).min(30))
}

fn options() -> Vec<String> {
    [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=10",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "LogLevel=ERROR",
    ]
    .map(String::from)
    .to_vec()
}

pub fn one_shot_args(alias: &str) -> Vec<String> {
    let mut args = options();
    args.extend(["-o", "ClearAllForwardings=yes", "--", alias, LOADER].map(String::from));
    args
}

/// The connection that stays: the host's socket forwarded to `target`
/// (a loopback port on Windows, the island's socket elsewhere).
pub fn link_args(alias: &str, remote_socket: &str, target: &str) -> Vec<String> {
    let mut args = options();
    args.extend(
        [
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ControlMaster=no",
            "-o",
            "ControlPath=none",
            "-R",
            &format!("{remote_socket}:{target}"),
            "--",
            alias,
            LOADER,
        ]
        .map(String::from),
    );
    args
}

pub fn preamble(mode: &str) -> String {
    format!("{}\n{mode}\n", STANDARD.encode(SCRIPT))
}

/// The last line ssh printed, for Settings.
pub fn reason(stderr: &str, status: Option<i32>) -> String {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| l.chars().take(160).collect())
        .unwrap_or_else(|| match status {
            // Windows' ssh says nothing when the host refuses the connection.
            Some(code) => format!("The host can’t be reached (ssh exited with {code})"),
            None => "The connection ended".into(),
        })
}

// MARK: Answers

fn decode(field: &str) -> Option<String> {
    if field == "-" {
        return None;
    }
    STANDARD
        .decode(field)
        .ok()
        .and_then(|d| String::from_utf8(d).ok())
}

pub fn encode(text: &str) -> String {
    STANDARD.encode(text)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Client {
    Nc(String),
    Socat(String),
    Python3(String),
    /// A container's hooks go through its relay (issue #330).
    Relay,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub home: String,
    pub system: String,
    pub client: Option<Client>,
    pub tmux: bool,
    pub claude: bool,
    pub hooks_installed: bool,
    /// Where Codex's app-server socket is on the host, there or not yet.
    pub codex_socket: Option<String>,
    /// The container this is, as the link found it (issue #330).
    pub container: Option<containers::Info>,
}

pub fn probe(lines: &[String]) -> Option<HostInfo> {
    let mut info = HostInfo::default();
    for line in lines {
        let words: Vec<&str> = line.split(' ').collect();
        match words.as_slice() {
            ["H", home] => info.home = decode(home).unwrap_or_default(),
            ["U", rest @ ..] => info.system = rest.join(" "),
            ["C", kind, path] => {
                let Some(path) = decode(path).filter(|p| p.starts_with('/')) else {
                    continue;
                };
                info.client = match *kind {
                    "nc" => Some(Client::Nc(path)),
                    "socat" => Some(Client::Socat(path)),
                    "python3" => Some(Client::Python3(path)),
                    _ => None,
                };
            }
            ["X", path] => info.codex_socket = decode(path).filter(|p| p.starts_with('/')),
            ["M", yes] => info.tmux = *yes == "yes",
            ["A", yes] => info.claude = *yes == "yes",
            ["I", yes] => info.hooks_installed = *yes == "yes",
            ["E", ..] => return None,
            _ => {}
        }
    }
    info.home.starts_with('/').then_some(info)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub path: String,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The host's clock when it looked, in seconds.
    pub host_time: Option<f64>,
    /// What to add to the host's times to put them on this PC's clock.
    pub clock_offset: f64,
    pub records: Vec<SessionRecord>,
    /// Session id to its transcript on the host.
    pub transcripts: HashMap<String, String>,
    pub chunks: Vec<Chunk>,
    pub gone: Vec<String>,
    /// Codex's app-server socket is on the host: its daemon runs.
    pub codex_live: bool,
    /// The tmux socket and pane each session runs in, by session id.
    pub panes: HashMap<String, (String, String)>,
}

/// A record from the host, which checked its process itself: Linux and
/// macOS hosts count.
pub fn record(data: &[u8], alias: &str, tmux: bool) -> Option<SessionRecord> {
    let mut value: Value = serde_json::from_slice(data).ok()?;
    let object = value.as_object_mut()?;
    if let Some(domain) = object.get("pidDomain").and_then(Value::as_str)
        && !(domain.starts_with("linux") || domain.starts_with("darwin"))
    {
        return None;
    }
    object.remove("pidDomain");
    // The host's own background jobs are not this PC's to stop or attach.
    object.remove("kind");
    let bytes = serde_json::to_vec(&value).ok()?;
    let mut record = registry::record(&bytes, &[])?;
    record.job = None;
    record.host = HostKind::Remote;
    record.accepts_replies = tmux;
    record.host_name = Some(if containers::is_container(alias) {
        let title = containers::title_of(alias);
        if tmux { format!("tmux in {title}") } else { title }
    } else if tmux {
        format!("tmux on {alias}")
    } else {
        alias.to_string()
    });
    record.remote = Some(alias.to_string());
    Some(record)
}

pub fn snapshot(lines: &[String], alias: &str) -> Snapshot {
    let mut out = Snapshot::default();
    for line in lines {
        let words: Vec<&str> = line.split(' ').collect();
        match words.as_slice() {
            ["R", pid, json, _home, socket, pane, tty] => {
                let Ok(pid) = pid.parse::<i32>() else {
                    continue;
                };
                let Ok(data) = STANDARD.decode(json) else {
                    continue;
                };
                let tmux =
                    decode(socket).is_some() && decode(tty).is_some() && pane.starts_with('%');
                if let Some(record) = record(&data, alias, tmux).filter(|r| r.pid == pid) {
                    if tmux && let Some(socket) = decode(socket) {
                        out.panes.insert(record.session_id.clone(), (socket, pane.to_string()));
                    }
                    out.records.push(record);
                }
            }
            ["J", session, path] => {
                if let Some(path) = decode(path) {
                    out.transcripts.insert(session.to_string(), path);
                }
            }
            ["T", path, offset, rest @ ..] => {
                let (Some(path), Ok(offset)) = (decode(path), offset.parse::<u64>()) else {
                    continue;
                };
                let bytes = rest
                    .first()
                    .and_then(|b| STANDARD.decode(b).ok())
                    .unwrap_or_default();
                out.chunks.push(Chunk {
                    path,
                    offset,
                    bytes,
                });
            }
            ["G", path] => {
                if let Some(path) = decode(path) {
                    out.gone.push(path);
                }
            }
            ["N", millis] => out.host_time = millis.parse::<f64>().ok().map(|m| m / 1000.0),
            ["X", yes] => out.codex_live = *yes == "yes",
            _ => {}
        }
    }
    out
}

// MARK: Clocks

/// A host's clock is rarely this PC's: an hour off in a VM set to another
/// zone, or seconds off without time sync. The registry's times decide
/// whether a permission is still open against the hook's time here, so
/// every time from the host is put on this PC's clock first. The offset is
/// read against the middle of the request; under a tenth of a second it is
/// the request's own time.
pub fn clock_offset(sent: f64, received: f64, host: Option<f64>) -> f64 {
    let Some(host) = host else { return 0.0 };
    let offset = (sent + received) / 2.0 - host;
    if offset.abs() < 0.1 { 0.0 } else { offset }
}

pub fn shifted(mut snapshot: Snapshot, offset: f64) -> Snapshot {
    snapshot.clock_offset = offset;
    if offset != 0.0 {
        for record in &mut snapshot.records {
            record.status_updated = record.status_updated.map(|t| t + offset);
            record.started = record.started.map(|t| t + offset);
        }
    }
    snapshot
}

/// A transcript line with its `timestamp` on this PC's clock; lines without
/// one, and small offsets, pass unchanged.
pub fn shift_line(line: &[u8], offset: f64) -> Vec<u8> {
    if offset.abs() < 1.0 {
        return line.to_vec();
    }
    let Ok(mut json) = serde_json::from_slice::<Value>(line) else {
        return line.to_vec();
    };
    let Some(at) = super::transcript::timestamp(json.get("timestamp")) else {
        return line.to_vec();
    };
    let moved = at + offset;
    let Some(date) = chrono::DateTime::from_timestamp_millis((moved * 1000.0).round() as i64)
    else {
        return line.to_vec();
    };
    json["timestamp"] = json!(date.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    let mut out = serde_json::to_vec(&json).unwrap_or_else(|_| line.to_vec());
    out.push(b'\n');
    out
}

/// None when the agent did it; otherwise "waiting" or "unreachable".
pub fn outcome(lines: Option<&[String]>) -> Option<&'static str> {
    match lines {
        Some(lines) if lines.iter().any(|l| l == "O") => None,
        Some(lines) if lines.iter().any(|l| l == "F waiting") => Some("waiting"),
        _ => Some("unreachable"),
    }
}

/// Reads a remote transcript in the pieces the agent sends.
#[derive(Default)]
pub struct Tail {
    pub offset: u64,
    partial: Vec<u8>,
}

impl Tail {
    /// Whole lines, and whether the reading started over.
    pub fn take(&mut self, chunk: &Chunk) -> (Vec<Vec<u8>>, bool) {
        let mut restarted = false;
        if chunk.offset != self.offset {
            restarted = true;
            self.partial.clear();
            self.offset = chunk.offset;
        }
        self.offset += chunk.bytes.len() as u64;
        self.partial.extend_from_slice(&chunk.bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            if line.len() > 1 {
                lines.push(line);
            }
        }
        if self.partial.len() > 8 << 20 {
            self.partial.clear();
        }
        (lines, restarted)
    }
}

// MARK: Hooks on the host

pub fn marker(app: &str) -> String {
    format!("# openbar-remote-hook {app}")
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub fn hook_command(client: &Client, socket: &str, app: &str) -> String {
    let run = match client {
        Client::Nc(path) => format!("{} -U {}", quoted(path), quoted(socket)),
        Client::Socat(path) => format!(
            "{} - {}",
            quoted(path),
            quoted(&format!("UNIX-CONNECT:{socket}"))
        ),
        Client::Python3(path) => {
            let code = "import socket,sys;s=socket.socket(socket.AF_UNIX);s.connect(sys.argv[1]);\
                        s.sendall(sys.stdin.buffer.read());s.shutdown(socket.SHUT_WR);\
                        [sys.stdout.buffer.write(c) for c in iter(lambda:s.recv(65536),b\"\")]";
            format!("{} -c {} {}", quoted(path), quoted(code), quoted(socket))
        }
        Client::Relay => {
            // `socket` is <home>/.openbar/<name>; the relay's hook is beside it.
            let home = socket.rsplitn(3, '/').nth(2).unwrap_or("");
            return containers::hook_command(home, app);
        }
    };
    format!("{run} 2>/dev/null; exit 0 {}", marker(app))
}

pub fn parse_settings(data: Option<&[u8]>) -> Result<Value, installer::Failure> {
    match data {
        None => Ok(json!({})),
        Some(data) => installer::parse(data),
    }
}

pub fn serialize(settings: &Value) -> Vec<u8> {
    let mut data = serde_json::to_vec_pretty(&installer::sorted(settings)).unwrap_or_default();
    data.push(b'\n');
    data
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backup {
    File(Vec<u8>),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removal {
    /// Only the hooks changed since setup: the kept copy goes back as it was.
    Restore,
    /// Something else changed too: the hooks come out and the rest stays.
    /// The copy stays (`true`) while another Openbar's hooks are still there,
    /// so the last one taken out can still put the file back as it was.
    Write(Vec<u8>, bool),
}

/// Any Openbar's remote hooks, this build's or another's.
const ANY_MARKER: &str = "# openbar-remote-hook ";

pub fn removal(
    current: Option<&[u8]>,
    backup: Option<&Backup>,
    app: &str,
) -> Result<Removal, installer::Failure> {
    removal_marked(current, backup, &marker(app))
}

/// The hooks carrying `marker` out: an SSH host's or a container's.
pub fn removal_marked(
    current: Option<&[u8]>,
    backup: Option<&Backup>,
    marker: &str,
) -> Result<Removal, installer::Failure> {
    let without = installer::removing_marked(&parse_settings(current)?, marker);
    if let Some(backup) = backup {
        let before = match backup {
            Backup::None => Some(json!({})),
            Backup::File(data) => installer::parse(data).ok(),
        };
        if before.as_ref() == Some(&without) {
            return Ok(Removal::Restore);
        }
    }
    // Another Openbar's hooks, over SSH or in a container, keep the copy.
    let keep = installer::has_any_marked(&without, ANY_MARKER)
        || installer::has_any_marked(&without, containers::ANY_MARKER);
    Ok(Removal::Write(serialize(&without), keep))
}

// MARK: ssh

/// Lines read so far, and whether standard output has ended.
type Lines = Arc<(Mutex<(VecDeque<String>, bool)>, Condvar)>;

/// A running `ssh`, fed lines on standard input and read line by line.
pub struct Ssh {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    lines: Lines,
    stderr: Arc<Mutex<String>>,
    pub pid: u32,
}

impl Ssh {
    pub fn spawn(args: &[String]) -> Option<Ssh> {
        let mut command = Command::new(ssh_binary());
        command.args(args);
        Ssh::spawn_command(command)
    }

    /// Any program that runs the loader: `docker exec` for a container.
    pub fn spawn_command(mut command: Command) -> Option<Ssh> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("SSH_ASKPASS_REQUIRE", "never");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console flashes for a background connection.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let stderr = child.stderr.take()?;
        let lines: Lines = Arc::new((Mutex::new((VecDeque::new(), false)), Condvar::new()));
        let feed = lines.clone();
        thread::Builder::new()
            .name("remote-ssh-out".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match reader.read_until(b'\n', &mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                                line.pop();
                            }
                            let (lock, ready) = &*feed;
                            lock.lock()
                                .unwrap()
                                .0
                                .push_back(String::from_utf8_lossy(&line).into_owned());
                            ready.notify_all();
                        }
                    }
                }
                let (lock, ready) = &*feed;
                lock.lock().unwrap().1 = true;
                ready.notify_all();
            })
            .ok()?;
        let errors = Arc::new(Mutex::new(String::new()));
        let sink = errors.clone();
        let _ = thread::Builder::new()
            .name("remote-ssh-err".into())
            .spawn(move || {
                // Windows' ssh may print in the console's code page, not UTF-8.
                let mut bytes = Vec::new();
                let mut reader = BufReader::new(stderr).take(64 * 1024);
                let mut chunk = [0u8; 4096];
                while let Ok(n) = reader.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    *sink.lock().unwrap() = String::from_utf8_lossy(&bytes).into_owned();
                }
            });
        Some(Ssh {
            pid: child.id(),
            stdin: Mutex::new(child.stdin.take()),
            child: Mutex::new(child),
            lines,
            stderr: errors,
        })
    }

    pub fn write(&self, text: &str) -> bool {
        let mut stdin = self.stdin.lock().unwrap();
        let Some(pipe) = stdin.as_mut() else {
            return false;
        };
        pipe.write_all(text.as_bytes())
            .and_then(|_| pipe.flush())
            .is_ok()
    }

    pub fn close_input(&self) {
        self.stdin.lock().unwrap().take();
    }

    /// The next line as it comes, for the relay's stream, which has no
    /// closing "."; None once standard output ends.
    pub fn next_line(&self) -> Option<String> {
        let (lock, ready) = &*self.lines;
        let mut state = lock.lock().unwrap();
        loop {
            if let Some(line) = state.0.pop_front() {
                return Some(line);
            }
            if state.1 {
                return None;
            }
            state = ready.wait(state).unwrap();
        }
    }

    /// The lines of one answer up to its closing "."; None when it does not
    /// come in time or ssh ends first.
    pub fn answer(&self, timeout: Duration) -> Option<Vec<String>> {
        let deadline = Instant::now() + timeout;
        let (lock, ready) = &*self.lines;
        let mut state = lock.lock().unwrap();
        let mut taken = Vec::new();
        loop {
            while let Some(line) = state.0.pop_front() {
                if line == "." {
                    return Some(taken);
                }
                taken.push(line);
            }
            if state.1 {
                return None;
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            state = ready.wait_timeout(state, left).unwrap().0;
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    pub fn stop(&self) {
        self.close_input();
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Why ssh ended, from what it printed.
    pub fn reason(&self) -> String {
        let status = {
            let mut child = self.child.lock().unwrap();
            for _ in 0..20 {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            child.try_wait().ok().flatten().and_then(|s| s.code())
        };
        thread::sleep(Duration::from_millis(100));
        reason(&self.stderr.lock().unwrap(), status)
    }
}

impl Drop for Ssh {
    fn drop(&mut self) {
        self.stop();
    }
}

fn epoch() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn ssh_binary() -> String {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let built_in = format!(r"{root}\System32\OpenSSH\ssh.exe");
        if std::path::Path::new(&built_in).exists() {
            return built_in;
        }
        "ssh.exe".into()
    }
    #[cfg(not(windows))]
    {
        "/usr/bin/ssh".into()
    }
}

/// One short ssh running a mode of the script: its answer, or why not.
pub fn one_shot(alias: &str, mode: &str, input: &[String]) -> Result<Vec<String>, String> {
    let ssh = Ssh::spawn(&one_shot_args(alias)).ok_or("ssh could not start")?;
    one_shot_on(ssh, mode, input)
}

/// The same through `docker exec` into a container (issue #330).
pub fn one_shot_container(
    engine: &containers::Engine,
    container: &containers::Info,
    mode: &str,
    input: &[String],
) -> Result<Vec<String>, String> {
    let mut command = engine.command();
    command.args(containers::Engine::exec_args(&container.id, container.user.as_deref()));
    let ssh = Ssh::spawn_command(command).ok_or_else(|| format!("{} could not start", engine.name()))?;
    one_shot_on(ssh, mode, input).map_err(container_reason)
}

/// What a container's `docker exec` said when it ended; ssh's words when
/// it said nothing don't fit.
fn container_reason(reason: String) -> String {
    if reason.starts_with("The host can") { RELAY_ENDED.into() } else { reason }
}

pub const RELAY_ENDED: &str = "The connection into the container ended";
pub const NOT_RUNNING: &str = "Not running. Openbar connects once it starts.";
pub const NO_ENGINE: &str = "No docker or podman command line was found.";

fn one_shot_on(ssh: Ssh, mode: &str, input: &[String]) -> Result<Vec<String>, String> {
    let mut text = preamble(mode);
    for line in input {
        text.push_str(line);
        text.push('\n');
    }
    ssh.write(&text);
    ssh.close_input();
    match ssh.answer(Duration::from_secs(25)) {
        Some(lines) => Ok(lines),
        None => Err(ssh.reason()),
    }
}

// MARK: Codex on the host

/// Standard input and output of an `ssh` running the script's `codex` mode,
/// which joins them to Codex's app-server socket on the host (issue #200).
/// Windows' OpenSSH can't forward to a local Unix socket, and a loopback
/// port would take any local process's connection, so the island talks to
/// the host's Codex through this pipe instead. Reads time out like the
/// local socket's, so the client sends between them.
pub struct CodexPipe {
    child: Child,
    stdin: ChildStdin,
    chunks: std::sync::mpsc::Receiver<Vec<u8>>,
    pending: Vec<u8>,
}

impl CodexPipe {
    pub fn open(alias: &str) -> Result<CodexPipe, String> {
        let mut command = Command::new(ssh_binary());
        command.args(one_shot_args(alias));
        CodexPipe::open_command(command)
    }

    /// The same through `docker exec` into a container (issue #330).
    pub fn open_container(engine: &containers::Engine, container: &containers::Info) -> Result<CodexPipe, String> {
        let mut command = engine.command();
        command.args(containers::Engine::exec_args(&container.id, container.user.as_deref()));
        CodexPipe::open_command(command)
    }

    fn open_command(mut command: Command) -> Result<CodexPipe, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("SSH_ASKPASS_REQUIRE", "never");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("ssh has no pipes".into());
        };
        let (feed, chunks) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("remote-codex-out".into())
            .spawn(move || {
                let mut buffer = [0u8; 64 * 1024];
                while let Ok(n) = stdout.read(&mut buffer) {
                    if n == 0 || feed.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        if stdin
            .write_all(preamble("codex").as_bytes())
            .and_then(|_| stdin.flush())
            .is_err()
        {
            let _ = child.kill();
            return Err("ssh ended".into());
        }
        Ok(CodexPipe {
            child,
            stdin,
            chunks,
            pending: Vec::new(),
        })
    }
}

impl Read for CodexPipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            match self.chunks.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => self.pending = chunk,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(std::io::ErrorKind::WouldBlock.into());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = out.len().min(self.pending.len());
        out[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Write for CodexPipe {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.stdin.write(data)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stdin.flush()
    }
}

impl Drop for CodexPipe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// MARK: The connection

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum State {
    Connecting,
    // rename_all on the enum names the variants only; the fields need their own.
    #[serde(rename_all = "camelCase")]
    Connected {
        has_client: bool,
        hooks_installed: bool,
        /// The container's ID, to tell a rebuilt one (issue #330).
        #[serde(skip_serializing_if = "Option::is_none")]
        container: Option<String>,
    },
    Disconnected {
        reason: String,
    },
}

/// One host: probe, then the ssh that stays, started again with growing
/// pauses whenever it ends. All of it runs on the link's own thread.
pub struct Link {
    pub alias: String,
    app: String,
    target: String,
    /// The container picked, for a container's link (issue #330).
    pub container_target: Option<containers::Target>,
    /// Its CLI and the container found last.
    engine: Mutex<Option<containers::Engine>>,
    found: Mutex<Option<containers::Info>>,
    /// The relay carrying the container's hooks.
    relay: Mutex<Option<Arc<Ssh>>>,
    on_hook: Option<HookSink>,
    ssh: Mutex<Option<Arc<Ssh>>>,
    /// One request at a time on the connection.
    busy: Mutex<()>,
    stopped: Mutex<bool>,
    /// Connect again at once, without counting a failure (`reconnect`).
    restart: Mutex<bool>,
    pub state: Mutex<State>,
    pub info: Mutex<Option<HostInfo>>,
    offsets: Mutex<HashMap<String, u64>>,
    /// The last look found Codex's daemon on the host.
    codex_live: Mutex<bool>,
}

/// Where a container's relayed hooks go: the service's handler.
pub type HookSink = Arc<dyn Fn(crate::hooks::transport::HookConnection) + Send + Sync>;

impl Link {
    /// `target` is what the host's socket is forwarded to; `on_snapshot`
    /// and `on_state` run on the link's thread.
    pub fn start(
        alias: &str,
        app: &str,
        target: String,
        on_snapshot: impl Fn(&str, Snapshot) + Send + 'static,
        on_state: impl Fn(&str, &State) + Send + 'static,
    ) -> Arc<Link> {
        Link::launch(alias, app, target, None, None, on_snapshot, on_state)
    }

    /// A container's link: `docker exec -i` for the agent and for the relay,
    /// whose hooks go to `on_hook` (issue #330).
    pub fn start_container(
        target: containers::Target,
        app: &str,
        on_hook: HookSink,
        on_snapshot: impl Fn(&str, Snapshot) + Send + 'static,
        on_state: impl Fn(&str, &State) + Send + 'static,
    ) -> Arc<Link> {
        let alias = target.alias();
        Link::launch(&alias, app, String::new(), Some(target), Some(on_hook), on_snapshot, on_state)
    }

    fn launch(
        alias: &str,
        app: &str,
        target: String,
        container_target: Option<containers::Target>,
        on_hook: Option<HookSink>,
        on_snapshot: impl Fn(&str, Snapshot) + Send + 'static,
        on_state: impl Fn(&str, &State) + Send + 'static,
    ) -> Arc<Link> {
        let link = Arc::new(Link {
            alias: alias.to_string(),
            app: app.to_string(),
            target,
            container_target,
            engine: Mutex::new(None),
            found: Mutex::new(None),
            relay: Mutex::new(None),
            on_hook,
            ssh: Mutex::new(None),
            busy: Mutex::new(()),
            stopped: Mutex::new(false),
            restart: Mutex::new(false),
            state: Mutex::new(State::Connecting),
            info: Mutex::new(None),
            offsets: Mutex::new(HashMap::new()),
            codex_live: Mutex::new(false),
        });
        let worker = link.clone();
        let _ = thread::Builder::new()
            .name(format!("remote-{alias}"))
            .spawn(move || worker.run(on_snapshot, on_state));
        link
    }

    pub fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        if let Some(ssh) = self.ssh.lock().unwrap().take() {
            ssh.stop();
        }
        if let Some(relay) = self.relay.lock().unwrap().take() {
            relay.stop();
        }
    }

    pub fn is_container(&self) -> bool {
        self.container_target.is_some()
    }

    /// The container found last and its CLI.
    pub fn container(&self) -> Option<(containers::Engine, containers::Info)> {
        let engine = self.engine.lock().unwrap().clone()?;
        let found = self.found.lock().unwrap().clone()?;
        Some((engine, found))
    }

    /// One mode of the agent script on the host, as Set up and Remove run it.
    pub fn one_shot(&self, mode: &str, input: &[String]) -> Result<Vec<String>, String> {
        if self.is_container() {
            let (engine, found) = self.container().ok_or(NOT_RUNNING)?;
            return one_shot_container(&engine, &found, mode, input);
        }
        one_shot(&self.alias, mode, input)
    }

    fn stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// Whether the host's Codex can be joined now: connected, the last look
    /// found its daemon, and the host has a program to bridge to its socket.
    pub fn codex_ready(&self) -> bool {
        if self.is_container() && self.relay.lock().unwrap().is_none() {
            return false;
        }
        self.ssh.lock().unwrap().is_some()
            && *self.codex_live.lock().unwrap()
            && self
                .info
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|i| i.client.is_some() && i.codex_socket.is_some())
    }

    /// The ssh behind the forwarded socket, to tell this host's hooks apart.
    pub fn peer(&self) -> Option<u32> {
        if self.is_container() {
            return self.relay.lock().unwrap().as_ref().map(|s| s.pid);
        }
        self.ssh.lock().unwrap().as_ref().map(|s| s.pid)
    }

    /// Connects again at once: after a setup, as Remove from host may have
    /// taken the forwarded socket with it.
    pub fn reconnect(&self) {
        *self.restart.lock().unwrap() = true;
        if let Some(ssh) = self.ssh.lock().unwrap().as_ref() {
            ssh.stop();
        }
        if let Some(relay) = self.relay.lock().unwrap().as_ref() {
            relay.stop();
        }
    }

    pub fn reread_transcripts(&self) {
        for offset in self.offsets.lock().unwrap().values_mut() {
            *offset = 0;
        }
    }

    fn set_state(&self, state: State, on_state: &impl Fn(&str, &State)) {
        *self.state.lock().unwrap() = state.clone();
        on_state(&self.alias, &state);
    }

    fn run(self: Arc<Self>, on_snapshot: impl Fn(&str, Snapshot), on_state: impl Fn(&str, &State)) {
        let mut attempt = 0u32;
        while !self.stopped() {
            self.set_state(State::Connecting, &on_state);
            let failure = match self.connect() {
                Ok(info) => {
                    attempt = 0;
                    self.set_state(
                        State::Connected {
                            has_client: info.client.is_some(),
                            hooks_installed: info.hooks_installed,
                            container: info.container.as_ref().map(|c| c.id.clone()),
                        },
                        &on_state,
                    );
                    self.poll(&on_snapshot)
                }
                Err(reason) => reason,
            };
            if let Some(ssh) = self.ssh.lock().unwrap().take() {
                ssh.stop();
            }
            if let Some(relay) = self.relay.lock().unwrap().take() {
                relay.stop();
            }
            *self.codex_live.lock().unwrap() = false;
            if self.stopped() {
                break;
            }
            if std::mem::take(&mut *self.restart.lock().unwrap()) {
                attempt = 0;
                continue;
            }
            attempt += 1;
            self.set_state(State::Disconnected { reason: failure }, &on_state);
            let until = Instant::now() + backoff(attempt);
            while Instant::now() < until && !self.stopped() {
                thread::sleep(Duration::from_millis(250));
            }
        }
    }

    fn connect(&self) -> Result<HostInfo, String> {
        if let Some(target) = &self.container_target {
            return self.connect_container(target);
        }
        let name = socket_name(&self.app);
        let lines = one_shot(&self.alias, &format!("probe {name} {}", self.app), &[])?;
        let info = probe(&lines).ok_or("The host answered in a way Openbar didn’t expect")?;
        *self.info.lock().unwrap() = Some(info.clone());
        let remote = format!("{}/.openbar/{name}", info.home);
        let ssh = Arc::new(
            Ssh::spawn(&link_args(&self.alias, &remote, &self.target))
                .ok_or("ssh could not start")?,
        );
        ssh.write(&preamble("agent"));
        match ssh.answer(Duration::from_secs(20)) {
            Some(hello) if hello.first().map(String::as_str) == Some("HELLO 1") => {}
            _ => return Err(ssh.reason()),
        }
        if self.stopped() {
            return Err("stopped".into());
        }
        self.offsets.lock().unwrap().clear();
        *self.ssh.lock().unwrap() = Some(ssh);
        Ok(info)
    }

    /// A container, found again by its folder or name, as a rebuilt one has
    /// a new ID; probed, then the agent and the relay started in it.
    fn connect_container(&self, target: &containers::Target) -> Result<HostInfo, String> {
        let engine = containers::Engine::find().ok_or(NO_ENGINE)?;
        let found = engine
            .running()?
            .into_iter()
            .find(|c| target.matches(c))
            .ok_or(NOT_RUNNING)?;
        *self.engine.lock().unwrap() = Some(engine.clone());
        *self.found.lock().unwrap() = Some(found.clone());
        let name = socket_name(&self.app);
        let lines = one_shot_container(&engine, &found, &format!("probe {name} {} container", self.app), &[])?;
        let mut info = probe(&lines).ok_or("The container answered in a way Openbar didn’t expect")?;
        let spawn = || {
            let mut command = engine.command();
            command.args(containers::Engine::exec_args(&found.id, found.user.as_deref()));
            Ssh::spawn_command(command).map(Arc::new)
        };
        let agent = spawn().ok_or_else(|| format!("{} could not start", engine.name()))?;
        agent.write(&preamble("agent"));
        match agent.answer(Duration::from_secs(20)) {
            Some(hello) if hello.first().map(String::as_str) == Some("HELLO 1") => {}
            _ => return Err(container_reason(agent.reason())),
        }
        let relay = spawn().ok_or_else(|| format!("{} could not start", engine.name()))?;
        relay.write(&preamble(&format!("relay {}", socket_name(&self.app).trim_end_matches(".sock"))));
        match relay.answer(Duration::from_secs(20)) {
            Some(hello) if hello.first().map(String::as_str) == Some("HELLO 1") => {}
            _ => {
                agent.stop();
                return Err(container_reason(relay.reason()));
            }
        }
        if self.stopped() {
            agent.stop();
            relay.stop();
            return Err("stopped".into());
        }
        if let Some(sink) = self.on_hook.clone() {
            relay_hooks(relay.clone(), sink);
        }
        info.client = Some(Client::Relay);
        info.container = Some(found);
        *self.info.lock().unwrap() = Some(info.clone());
        self.offsets.lock().unwrap().clear();
        *self.ssh.lock().unwrap() = Some(agent);
        *self.relay.lock().unwrap() = Some(relay);
        Ok(info)
    }

    /// Looks at the host's sessions until the connection ends; why it ended.
    fn poll(&self, on_snapshot: &impl Fn(&str, Snapshot)) -> String {
        while !self.stopped() {
            // A container's hooks come only while its relay runs.
            if self.is_container() && !self.relay.lock().unwrap().as_ref().is_some_and(|r| r.running()) {
                return RELAY_ENDED.into();
            }
            let mut request = vec!["snap".to_string()];
            for (path, offset) in self.offsets.lock().unwrap().iter() {
                request.push(format!("t {offset} {}", encode(path)));
            }
            request.push("end".into());
            let sent = epoch();
            let Some(lines) = self.request(&request, Duration::from_secs(15)) else {
                let reason = self
                    .ssh
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.reason())
                    .unwrap_or_else(|| "The connection ended".into());
                return if self.is_container() { container_reason(reason) } else { reason };
            };
            let read = snapshot(&lines, &self.alias);
            let offset = clock_offset(sent, epoch(), read.host_time);
            let snap = shifted(read, offset);
            {
                let mut offsets = self.offsets.lock().unwrap();
                for chunk in &snap.chunks {
                    offsets.insert(chunk.path.clone(), chunk.offset + chunk.bytes.len() as u64);
                }
                let live: Vec<&String> = snap.transcripts.values().collect();
                offsets.retain(|path, _| live.contains(&path));
                for path in live {
                    offsets.entry(path.clone()).or_insert(0);
                }
            }
            *self.codex_live.lock().unwrap() = snap.codex_live;
            on_snapshot(&self.alias, snap);
            thread::sleep(Duration::from_millis(1500));
        }
        "stopped".into()
    }

    pub fn request(&self, lines: &[String], timeout: Duration) -> Option<Vec<String>> {
        let _turn = self.busy.lock().unwrap();
        let ssh = self.ssh.lock().unwrap().clone()?;
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        if !ssh.write(&text) {
            return None;
        }
        ssh.answer(timeout)
    }

    /// Pastes the reply into the session's tmux pane on the host, then Return.
    pub fn type_reply(&self, pid: i32, session: &str, text: &str) -> Option<&'static str> {
        let paste = format!("paste {pid} {session} {}", encode(text));
        if let Some(failure) = outcome(self.request(&[paste], Duration::from_secs(8)).as_deref()) {
            return Some(failure);
        }
        thread::sleep(Duration::from_millis(80));
        outcome(
            self.request(&[format!("enter {pid} {session}")], Duration::from_secs(8))
                .as_deref(),
        )
    }

    pub fn focus(&self, pid: i32, session: &str) -> bool {
        outcome(
            self.request(&[format!("focus {pid} {session}")], Duration::from_secs(8))
                .as_deref(),
        )
        .is_none()
    }

    /// Where a session's tmux pane is, from the last look (Open, #330).
    pub fn pane_of(snapshot: &Snapshot, session: &str) -> Option<(String, String)> {
        snapshot.panes.get(session).cloned()
    }

    pub fn mark_hooks(&self, installed: bool) {
        if let Some(info) = self.info.lock().unwrap().as_mut() {
            info.hooks_installed = installed;
        }
        let mut state = self.state.lock().unwrap();
        if let State::Connected {
            hooks_installed, ..
        } = &mut *state
        {
            *hooks_installed = installed;
        }
    }
}

// MARK: The relay (issue #330)

/// Reads the relay's lines: each `H <id> <event>` becomes a hook connection
/// handed to the service, like one from the pipe, and its answer (or its
/// end without one) goes back as `A <id> <answer>`.
fn relay_hooks(relay: Arc<Ssh>, sink: HookSink) {
    let _ = thread::Builder::new()
        .name("container-relay".into())
        .spawn(move || {
            while let Some(line) = relay.next_line() {
                let mut words = line.splitn(3, ' ');
                let (Some("H"), Some(id)) = (words.next(), words.next()) else {
                    continue;
                };
                if id.is_empty() || id.len() > 12 || !id.bytes().all(|b| b.is_ascii_digit()) {
                    continue;
                }
                let id = id.to_string();
                let answer = {
                    let relay = relay.clone();
                    let id = id.clone();
                    move |reply: Vec<u8>| {
                        let field = if reply.is_empty() { "-".to_string() } else { STANDARD.encode(reply) };
                        relay.write(&format!("A {id} {field}\n"));
                    }
                };
                let event = words
                    .next()
                    .and_then(|b| STANDARD.decode(b).ok())
                    .filter(|e| e.len() <= crate::hooks::transport::MAX_MESSAGE && crate::hooks::transport::is_complete(e));
                let Some(event) = event else {
                    answer(Vec::new());
                    continue;
                };
                let connection = crate::hooks::transport::HookConnection::relayed(event, Some(relay.pid), Box::new(answer));
                let sink = sink.clone();
                // A permission holds its hook while the island asks; others go on.
                let _ = thread::Builder::new()
                    .name("container-hook".into())
                    .spawn(move || sink(connection));
            }
        });
}

// MARK: Setup

struct Settings {
    current: Option<Vec<u8>>,
    backup: Option<Backup>,
    sum: String,
}

/// Runs one mode of the agent script on the host: ssh's or a container's.
pub type Runner<'a> = &'a dyn Fn(&str, &[String]) -> Result<Vec<String>, String>;

fn read_settings(run: Runner) -> Result<Settings, String> {
    let lines = run("read", &[])?;
    let mut settings = Settings {
        current: None,
        backup: None,
        sum: String::new(),
    };
    for line in &lines {
        let words: Vec<&str> = line.split(' ').collect();
        match words.as_slice() {
            ["F"] => settings.current = Some(Vec::new()),
            ["F", data] => settings.current = STANDARD.decode(data).ok(),
            ["B", "-"] => settings.backup = Some(Backup::None),
            ["B", data] => settings.backup = STANDARD.decode(data).ok().map(Backup::File),
            ["S", sum] => settings.sum = sum.to_string(),
            _ => {}
        }
    }
    if settings.sum.is_empty() {
        return Err("The host’s settings couldn’t be read".into());
    }
    Ok(settings)
}

const UNREADABLE: &str = "The host’s ~/.claude/settings.json isn’t JSON, so it was left alone";
const BUSY: &str = "Claude Code kept changing its settings; try again";

pub fn install(alias: &str, info: &HostInfo, app: &str) -> Result<(), String> {
    install_with(&|mode: &str, input: &[String]| one_shot(alias, mode, input), info, app)
}

pub fn install_with(run: Runner, info: &HostInfo, app: &str) -> Result<(), String> {
    let client = info
        .client
        .as_ref()
        .ok_or("The host has none of nc, socat or python3 for the hooks to write with")?;
    let (command, marker) = if *client == Client::Relay {
        (containers::hook_command(&info.home, app), containers::marker(app))
    } else {
        let socket = format!("{}/.openbar/{}", info.home, socket_name(app));
        (hook_command(client, &socket, app), marker(app))
    };
    for _ in 0..3 {
        let settings = read_settings(run)?;
        let parsed = parse_settings(settings.current.as_deref()).map_err(|_| UNREADABLE)?;
        let new = serialize(&installer::installing_marked(&parsed, &command, &marker));
        let lines = run("install", &[settings.sum, STANDARD.encode(new)])?;
        if lines.iter().any(|l| l == "O") {
            return Ok(());
        }
        if !lines.iter().any(|l| l == "F changed") {
            return Err(lines.first().cloned().unwrap_or_default());
        }
    }
    Err(BUSY.into())
}

pub fn remove(alias: &str, app: &str) -> Result<(), String> {
    remove_with(&|mode: &str, input: &[String]| one_shot(alias, mode, input), &marker(app), app)
}

/// Takes the hooks carrying `marker` out; `app` names what `~/.openbar` held.
pub fn remove_with(run: Runner, marker: &str, app: &str) -> Result<(), String> {
    for _ in 0..3 {
        let settings = read_settings(run)?;
        let plan = removal_marked(settings.current.as_deref(), settings.backup.as_ref(), marker)
            .map_err(|_| UNREADABLE)?;
        let mut input = vec![socket_name(app), settings.sum];
        match plan {
            Removal::Restore => input.push("restore".into()),
            Removal::Write(data, keep) => {
                input.push(if keep { "keep" } else { "write" }.into());
                input.push(STANDARD.encode(data));
            }
        }
        let lines = run("remove", &input)?;
        if lines.iter().any(|l| l == "O") {
            return Ok(());
        }
        if !lines.iter().any(|l| l == "F changed") {
            return Err(lines.first().cloned().unwrap_or_default());
        }
    }
    Err(BUSY.into())
}

// MARK: The loopback bridge (Windows)

/// Takes the forwarded hook connections on a loopback port and hands each
/// on like one from the pipe. Only connections opened by one of the links'
/// own ssh processes count; anything else on this PC is turned away.
#[cfg(windows)]
pub mod bridge {
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::thread;

    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };

    use crate::hooks::transport::{self, HookConnection};

    /// The listening port.
    pub fn serve(
        allowed: impl Fn(u32) -> bool + Send + Sync + 'static,
        on_message: impl Fn(HookConnection) + Send + Sync + 'static,
    ) -> std::io::Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let allowed = Arc::new(allowed);
        let on_message = Arc::new(on_message);
        thread::Builder::new()
            .name("remote-bridge".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let (allowed, on_message) = (allowed.clone(), on_message.clone());
                    thread::spawn(move || {
                        let Some(owner) = stream.peer_addr().ok().and_then(owner_of) else {
                            return;
                        };
                        if !allowed(owner) {
                            return;
                        }
                        if let Some(connection) = transport::receive_tcp(stream, owner) {
                            on_message(connection);
                        }
                    });
                }
            })?;
        Ok(port)
    }

    /// The process that opened the connection whose local end is `peer`.
    fn owner_of(peer: SocketAddr) -> Option<u32> {
        let SocketAddr::V4(peer) = peer else {
            return None;
        };
        unsafe {
            let mut size = 0u32;
            let _ = GetExtendedTcpTable(None, &mut size, false, 2, TCP_TABLE_OWNER_PID_ALL, 0);
            let mut buf = vec![0u8; size as usize + 4096];
            size = buf.len() as u32;
            if GetExtendedTcpTable(
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                false,
                2,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            ) != 0
            {
                return None;
            }
            let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
            let rows =
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
            let address = u32::from_ne_bytes(peer.ip().octets());
            let port = peer.port().to_be() as u32;
            rows.iter()
                .find(|row| row.dwLocalAddr == address && (row.dwLocalPort & 0xFFFF) == port)
                .map(|row| row.dwOwningPid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(text: &str) -> String {
        encode(text)
    }

    const LINUX: &str = r#"{"pid":1128,"sessionId":"5a3bef52-775f-4659-8b4b-099b67ea877e","cwd":"/home/dev/project","startedAt":1794042383051,"procStart":"12807","version":"2.1.319","kind":"interactive","pidDomain":"linux:91bc82a9:pid:[4026531836]","name":"project-6c","status":"waiting","waitingFor":"permission prompt","statusUpdatedAt":1794042383085}"#;

    #[test]
    fn aliases_never_read_as_options() {
        assert!(is_valid_alias("openbar-linux") && is_valid_alias("dev@box.example.com"));
        assert!(
            !is_valid_alias("-oProxyCommand=x")
                && !is_valid_alias("a b")
                && !is_valid_alias("box;rm")
        );
        let args = link_args("box", "/home/dev/.openbar/a.sock", "127.0.0.1:5000");
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dash + 1], "box");
        assert_eq!(args.last().unwrap(), LOADER);
        assert!(args.contains(&"/home/dev/.openbar/a.sock:127.0.0.1:5000".to_string()));
        assert!(
            args.contains(&"BatchMode=yes".to_string())
                && args.contains(&"ExitOnForwardFailure=yes".to_string())
        );
        assert_eq!(socket_name("com.openbar.island"), "com.openbar.island.sock");
        assert_eq!(socket_name("a/../b c"), "a..bc.sock");
        assert_eq!(backoff(1), Duration::from_secs(2));
        assert_eq!(backoff(9), Duration::from_secs(30));
        assert!(!LOADER.contains('\\'));
        assert_eq!(
            reason(
                "Warning: x\nssh: connect to host box port 22: Connection refused\n",
                Some(255)
            ),
            "ssh: connect to host box port 22: Connection refused"
        );
        assert_eq!(
            reason("", Some(255)),
            "The host can’t be reached (ssh exited with 255)"
        );
    }

    #[test]
    fn prefs_list_hosts_only_with_the_switch_on() {
        let off = json!({"sessions": {"remoteHosts": ["box"]}});
        assert!(hosts_from_prefs(&off).is_empty());
        let on = json!({"sessions": {"remoteHostsEnabled": true, "remoteHosts": ["box", "box", "-x", 3]}});
        assert_eq!(hosts_from_prefs(&on), vec!["box".to_string()]);
    }

    #[test]
    fn reads_the_probe_and_a_snapshot() {
        let info = probe(&[
            format!("H {}", b64("/home/dev")),
            "U Linux".into(),
            format!("C nc {}", b64("/usr/bin/nc")),
            "M yes".into(),
            "I no".into(),
        ])
        .unwrap();
        assert_eq!(info.home, "/home/dev");
        assert_eq!(info.client, Some(Client::Nc("/usr/bin/nc".into())));
        assert!(info.tmux && !info.hooks_installed);
        assert!(probe(&["E name".into()]).is_none());

        let lines = vec![
            format!(
                "R 1128 {} {} {} %0 {}",
                STANDARD.encode(LINUX),
                b64("/home/dev/.claude"),
                b64("/tmp/tmux-1000/default"),
                b64("/dev/pts/0")
            ),
            format!(
                "R 77 {} {} - - -",
                STANDARD.encode(LINUX),
                b64("/home/dev/.claude")
            ),
            format!(
                "J 5a3bef52-775f-4659-8b4b-099b67ea877e {}",
                b64("/home/dev/t.jsonl")
            ),
            format!(
                "T {} 10 {}",
                b64("/home/dev/t.jsonl"),
                STANDARD.encode("{}\n")
            ),
            format!("G {}", b64("/gone")),
        ];
        let snap = snapshot(&lines, "box");
        assert_eq!(snap.records.len(), 1);
        let record = &snap.records[0];
        assert_eq!(record.host, HostKind::Remote);
        assert!(record.accepts_replies);
        assert_eq!(record.host_name.as_deref(), Some("tmux on box"));
        assert_eq!(record.remote.as_deref(), Some("box"));
        assert_eq!(record.waiting_for.as_deref(), Some("permission prompt"));
        assert!(record.job.is_none());
        assert_eq!(
            snap.transcripts["5a3bef52-775f-4659-8b4b-099b67ea877e"],
            "/home/dev/t.jsonl"
        );
        assert_eq!(
            snap.chunks,
            vec![Chunk {
                path: "/home/dev/t.jsonl".into(),
                offset: 10,
                bytes: b"{}\n".to_vec()
            }]
        );
        assert_eq!(snap.gone, vec!["/gone".to_string()]);
        assert!(registry::record(LINUX.as_bytes(), registry::WINDOWS_PID_DOMAINS).is_none());
        let outside = record_of_host(LINUX, false);
        assert!(!outside.accepts_replies && outside.host_name.as_deref() == Some("box"));
        assert_eq!(outcome(Some(&["O".into()])), None);
        assert_eq!(outcome(Some(&["F waiting".into()])), Some("waiting"));
        assert_eq!(outcome(None), Some("unreachable"));
    }

    fn record_of_host(json: &str, tmux: bool) -> SessionRecord {
        record(json.as_bytes(), "box", tmux).unwrap()
    }

    #[test]
    fn the_state_reads_as_settings_expects_it() {
        let connected = serde_json::to_value(State::Connected {
            has_client: true,
            hooks_installed: false,
            container: None,
        })
        .unwrap();
        assert_eq!(
            connected,
            json!({"state": "connected", "hasClient": true, "hooksInstalled": false})
        );
        let container = serde_json::to_value(State::Connected {
            has_client: true,
            hooks_installed: true,
            container: Some("e9f2".into()),
        })
        .unwrap();
        assert_eq!(
            container,
            json!({"state": "connected", "hasClient": true, "hooksInstalled": true, "container": "e9f2"})
        );
        let gone = serde_json::to_value(State::Disconnected { reason: "x".into() }).unwrap();
        assert_eq!(gone, json!({"state": "disconnected", "reason": "x"}));
        assert_eq!(
            serde_json::to_value(State::Connecting).unwrap(),
            json!({"state": "connecting"})
        );
    }

    #[test]
    fn host_times_move_to_this_clock() {
        let sent = 1_794_045_700.0;
        let offset = clock_offset(sent, sent + 0.2, Some(sent + 3600.1));
        assert!((offset + 3600.0).abs() < 0.001);
        assert_eq!(clock_offset(sent, sent + 0.2, Some(sent + 0.15)), 0.0);
        assert_eq!(clock_offset(sent, sent + 0.2, None), 0.0);
        let lines = vec![
            format!("N {}", ((sent + 3600.1) * 1000.0) as i64),
            format!(
                "R 1128 {} {} - - -",
                STANDARD.encode(LINUX),
                b64("/home/dev/.claude")
            ),
        ];
        let read = snapshot(&lines, "box");
        assert!((read.host_time.unwrap() - (sent + 3600.1)).abs() < 0.01);
        let before = read.records[0].status_updated.unwrap();
        let moved = shifted(read, offset);
        assert!((moved.records[0].status_updated.unwrap() - (before + offset)).abs() < 0.001);
        let line = br#"{"type":"user","timestamp":"2026-11-07T10:00:00.500Z","sessionId":"s"}"#;
        let out: Value = serde_json::from_slice(&shift_line(line, -3600.0)).unwrap();
        assert_eq!(out["timestamp"], "2026-11-07T09:00:00.500Z");
        assert_eq!(out["sessionId"], "s");
        assert_eq!(shift_line(line, 0.4), line.to_vec());
        assert_eq!(shift_line(b"not json", 60.0), b"not json".to_vec());
    }

    #[test]
    fn transcripts_come_in_pieces() {
        let mut tail = Tail::default();
        let (lines, restarted) = tail.take(&Chunk {
            path: "p".into(),
            offset: 0,
            bytes: b"{\"a\":1}\n{\"b\"".to_vec(),
        });
        assert_eq!(lines, vec![b"{\"a\":1}\n".to_vec()]);
        assert!(!restarted && tail.offset == 12);
        let (lines, _) = tail.take(&Chunk {
            path: "p".into(),
            offset: 12,
            bytes: b":2}\n".to_vec(),
        });
        assert_eq!(lines, vec![b"{\"b\":2}\n".to_vec()]);
        let (_, restarted) = tail.take(&Chunk {
            path: "p".into(),
            offset: 0,
            bytes: b"x\n".to_vec(),
        });
        assert!(restarted);
    }

    #[test]
    fn hooks_on_the_host_come_out_as_they_went_in() {
        let app = "com.openbar.island";
        let nc = hook_command(
            &Client::Nc("/usr/bin/nc".into()),
            "/home/d'v/.openbar/a.sock",
            app,
        );
        assert_eq!(
            nc,
            r#"'/usr/bin/nc' -U '/home/d'\''v/.openbar/a.sock' 2>/dev/null; exit 0 # openbar-remote-hook com.openbar.island"#
        );
        let python = hook_command(
            &Client::Python3("/usr/bin/python3".into()),
            "/h/a.sock",
            app,
        );
        assert!(python.starts_with("'/usr/bin/python3' -c 'import socket,sys;"));
        assert!(!marker(app).contains(&installer::marker(app)));

        let original = b"{\n  \"theme\" :   \"dark\"\n}\n".to_vec();
        let installed = installer::installing_marked(
            &parse_settings(Some(&original)).unwrap(),
            &nc,
            &marker(app),
        );
        assert!(installer::is_installed(&installed, &nc));
        let current = serialize(&installed);
        assert_eq!(
            removal(Some(&current), Some(&Backup::File(original.clone())), app).unwrap(),
            Removal::Restore
        );
        let fresh = serialize(&installer::installing_marked(&json!({}), &nc, &marker(app)));
        assert_eq!(
            removal(Some(&fresh), Some(&Backup::None), app).unwrap(),
            Removal::Restore
        );
        let mut edited = installed.clone();
        edited["model"] = json!("opus");
        match removal(
            Some(&serialize(&edited)),
            Some(&Backup::File(original)),
            app,
        )
        .unwrap()
        {
            Removal::Write(data, keep) => {
                let kept: Value = serde_json::from_slice(&data).unwrap();
                assert_eq!(kept["model"], "opus");
                assert!(kept.get("hooks").is_none());
                assert!(!keep);
            }
            Removal::Restore => panic!("an edited file keeps the edit"),
        }
        // Two Openbars on one host: the first out keeps the copy for the last.
        let mac = hook_command(
            &Client::Nc("/usr/bin/nc".into()),
            "/h/b.sock",
            "com.openbar.app",
        );
        let both = installer::installing_marked(&installed, &mac, &marker("com.openbar.app"));
        let backup = Backup::File(b"{\n  \"theme\" :   \"dark\"\n}\n".to_vec());
        match removal(Some(&serialize(&both)), Some(&backup), app).unwrap() {
            Removal::Write(data, keep) => {
                assert!(keep);
                assert_eq!(
                    removal(Some(&data), Some(&backup), "com.openbar.app").unwrap(),
                    Removal::Restore
                );
            }
            Removal::Restore => panic!("another Openbar's hooks are still there"),
        }
        assert!(removal(Some(b"not json"), None, app).is_err());
    }

    #[test]
    fn the_script_is_the_macs() {
        assert!(SCRIPT.contains("ob_agent") && SCRIPT.starts_with("# SPDX-License-Identifier"));
        let decoded = STANDARD
            .decode(preamble("agent").lines().next().unwrap())
            .unwrap();
        assert_eq!(decoded, SCRIPT.as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn the_probe_and_each_look_say_where_codex_is() {
        let socket = "/home/dev/.codex/app-server-control/app-server-control.sock";
        let info = probe(&[
            format!("H {}", b64("/home/dev")),
            format!("X {}", b64(socket)),
        ])
        .unwrap();
        assert_eq!(info.codex_socket.as_deref(), Some(socket));
        let relative = probe(&[
            format!("H {}", b64("/home/dev")),
            format!("X {}", b64("rel")),
        ])
        .unwrap();
        assert_eq!(relative.codex_socket, None);
        assert!(snapshot(&["X yes".into()], "box").codex_live);
        assert!(!snapshot(&["X no".into()], "box").codex_live);
        assert!(!snapshot(&[], "box").codex_live);
    }

    /// The host script's Codex parts, run by this machine's sh.
    #[cfg(unix)]
    #[test]
    fn the_script_finds_codex_and_bridges_to_it() {
        use std::io::BufRead;
        let home =
            std::env::temp_dir().join(format!("openbar-remote-codex-rs-{}", std::process::id()));
        // A socket's path must be short: CODEX_HOME goes under /tmp.
        let codex = std::path::PathBuf::from(format!("/tmp/obcx-rs-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(codex.join("app-server-control")).unwrap();
        let run = |mode: &str, input: &[u8]| -> Vec<String> {
            let mut child = Command::new("/bin/sh")
                .args([
                    "-c",
                    r#"IFS= read -r s; eval "$(printf %s "$s" | base64 -d)""#,
                ])
                .env("HOME", &home)
                .env("CODEX_HOME", &codex)
                .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(preamble(mode).as_bytes()).unwrap();
            stdin.write_all(input).unwrap();
            drop(stdin);
            let out = child.wait_with_output().unwrap();
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(String::from)
                .collect()
        };
        let path = codex.join("app-server-control/app-server-control.sock");
        let info = probe(&run("probe x.sock com.openbar.island", b"")).unwrap();
        assert_eq!(info.codex_socket.as_deref(), path.to_str());
        assert_eq!(
            run("codex", b""),
            ["E codex", "."],
            "no daemon: nothing to join"
        );
        assert!(run("agent", b"snap\nend\n").contains(&"X no".to_string()));
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(run("agent", b"snap\nend\n").contains(&"X yes".to_string()));
        // The bridge carries a line there and the answer back.
        let echo = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            (&stream)
                .write_all(format!("echo {line}").as_bytes())
                .unwrap();
        });
        let answer = run("codex", b"hello\n");
        echo.join().unwrap();
        assert_eq!(answer, ["echo hello"]);
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&codex);
    }

    #[test]
    fn the_script_runs_under_sh() {
        let home = std::env::temp_dir().join(format!("openbar-remote-rs-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let run = |mode: &str, input: &[&str]| -> Vec<String> {
            let mut child = Command::new("/bin/sh")
                .args([
                    "-c",
                    r#"IFS= read -r s; eval "$(printf %s "$s" | base64 -d)""#,
                ])
                .env("HOME", &home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut text = preamble(mode);
            for line in input {
                text.push_str(line);
                text.push('\n');
            }
            child
                .stdin
                .take()
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(String::from)
                .collect()
        };
        let lines = run("probe x.sock com.openbar.island", &[]);
        assert_eq!(lines.last().map(String::as_str), Some("."));
        assert_eq!(probe(&lines).unwrap().home, home.to_string_lossy());
        let settings = home.join(".claude/settings.json");
        std::fs::write(&settings, b"{\"theme\":\"dark\"}").unwrap();
        let read = run("read", &[]);
        let sum = read
            .iter()
            .find_map(|l| l.strip_prefix("S "))
            .unwrap()
            .to_string();
        let nc = hook_command(
            &Client::Nc("/usr/bin/nc".into()),
            "/x.sock",
            "com.openbar.island",
        );
        let new = serialize(&installer::installing_marked(
            &json!({"theme": "dark"}),
            &nc,
            &marker("com.openbar.island"),
        ));
        assert_eq!(
            run("install", &[&sum, &STANDARD.encode(&new)])
                .first()
                .map(String::as_str),
            Some("O")
        );
        let sum = run("read", &[])
            .iter()
            .find_map(|l| l.strip_prefix("S ").map(String::from))
            .unwrap();
        assert_eq!(
            run("remove", &["x.sock", &sum, "restore"])
                .first()
                .map(String::as_str),
            Some("O")
        );
        assert_eq!(std::fs::read(&settings).unwrap(), b"{\"theme\":\"dark\"}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A container's relay (issue #330), run by this machine's `sh` against
    /// a throwaway home as `docker exec -i` would run it: a hook's event
    /// reaches the sink whole, and the answer is what the hook prints.
    #[cfg(unix)]
    #[test]
    fn a_container_relay_hands_hooks_on_and_answers() {
        use std::io::Write as _;
        let home = std::env::temp_dir().join(format!("openbar-relay-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", containers::LOADER])
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin");
        let relay = Arc::new(Ssh::spawn_command(command).unwrap());
        relay.write(&preamble("relay com.openbar.test"));
        assert_eq!(relay.answer(Duration::from_secs(5)).unwrap(), vec!["HELLO 1"]);
        let (tx, rx) = std::sync::mpsc::channel();
        relay_hooks(
            relay.clone(),
            Arc::new(move |connection: crate::hooks::transport::HookConnection| {
                tx.send(connection.payload.clone()).unwrap();
                let _ = connection.reply(b"{\"decision\":\"allow\"}");
            }),
        );
        let hook = containers::hook_path(home.to_str().unwrap(), "com.openbar.test");
        let event = format!("{{\"hook_event_name\":\"PermissionRequest\",\"pad\":\"{}\"}}", "x".repeat(70_000));
        let mut child = Command::new("/bin/sh")
            .arg(&hook)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
        let printed = child.wait_with_output().unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), event.as_bytes());
        assert_eq!(printed.stdout, b"{\"decision\":\"allow\"}");
        // The island goes: the relay ends and a hook doesn't wait on it.
        relay.close_input();
        let until = Instant::now() + Duration::from_secs(5);
        while relay.running() && Instant::now() < until {
            thread::sleep(Duration::from_millis(50));
        }
        let started = Instant::now();
        let late = Command::new("/bin/sh").arg(&hook).env("HOME", &home).stdin(Stdio::null()).output().unwrap();
        assert!(!relay.running() && late.status.success() && started.elapsed() < Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn both_kinds_of_hooks_keep_the_copy_for_each_other() {
        let original = b"{\"theme\": \"dark\"}\n".to_vec();
        let ssh = hook_command(&Client::Nc("/usr/bin/nc".into()), "/home/n/.openbar/a.sock", "app");
        let inside = containers::hook_command("/home/n", "app");
        let both = installer::installing_marked(
            &installer::installing_marked(&parse_settings(Some(&original)).unwrap(), &ssh, &marker("app")),
            &inside,
            &containers::marker("app"),
        );
        let data = serialize(&both);
        let backup = Backup::File(original.clone());
        assert!(matches!(removal_marked(Some(&data), Some(&backup), &containers::marker("app")), Ok(Removal::Write(_, true))));
        assert!(matches!(removal(Some(&data), Some(&backup), "app"), Ok(Removal::Write(_, true))));
        let only = serialize(&installer::installing_marked(&parse_settings(Some(&original)).unwrap(), &inside, &containers::marker("app")));
        assert_eq!(removal_marked(Some(&only), Some(&backup), &containers::marker("app")), Ok(Removal::Restore));
        assert_eq!(hook_command(&Client::Relay, "/home/n/.openbar/app.sock", "app"), inside);
    }
}
