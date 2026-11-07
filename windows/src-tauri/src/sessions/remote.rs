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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub home: String,
    pub system: String,
    pub client: Option<Client>,
    pub tmux: bool,
    pub claude: bool,
    pub hooks_installed: bool,
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
    record.host_name = Some(if tmux {
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
    let without = installer::removing_marked(&parse_settings(current)?, &marker(app));
    if let Some(backup) = backup {
        let before = match backup {
            Backup::None => Some(json!({})),
            Backup::File(data) => installer::parse(data).ok(),
        };
        if before.as_ref() == Some(&without) {
            return Ok(Removal::Restore);
        }
    }
    let keep = installer::has_any_marked(&without, ANY_MARKER);
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
        command
            .args(args)
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
    ssh: Mutex<Option<Arc<Ssh>>>,
    /// One request at a time on the connection.
    busy: Mutex<()>,
    stopped: Mutex<bool>,
    /// Connect again at once, without counting a failure (`reconnect`).
    restart: Mutex<bool>,
    pub state: Mutex<State>,
    pub info: Mutex<Option<HostInfo>>,
    offsets: Mutex<HashMap<String, u64>>,
}

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
        let link = Arc::new(Link {
            alias: alias.to_string(),
            app: app.to_string(),
            target,
            ssh: Mutex::new(None),
            busy: Mutex::new(()),
            stopped: Mutex::new(false),
            restart: Mutex::new(false),
            state: Mutex::new(State::Connecting),
            info: Mutex::new(None),
            offsets: Mutex::new(HashMap::new()),
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
    }

    fn stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// The ssh behind the forwarded socket, to tell this host's hooks apart.
    pub fn peer(&self) -> Option<u32> {
        self.ssh.lock().unwrap().as_ref().map(|s| s.pid)
    }

    /// Connects again at once: after a setup, as Remove from host may have
    /// taken the forwarded socket with it.
    pub fn reconnect(&self) {
        *self.restart.lock().unwrap() = true;
        if let Some(ssh) = self.ssh.lock().unwrap().as_ref() {
            ssh.stop();
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

    /// Looks at the host's sessions until the connection ends; why it ended.
    fn poll(&self, on_snapshot: &impl Fn(&str, Snapshot)) -> String {
        while !self.stopped() {
            let mut request = vec!["snap".to_string()];
            for (path, offset) in self.offsets.lock().unwrap().iter() {
                request.push(format!("t {offset} {}", encode(path)));
            }
            request.push("end".into());
            let sent = epoch();
            let Some(lines) = self.request(&request, Duration::from_secs(15)) else {
                return self
                    .ssh
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.reason())
                    .unwrap_or_else(|| "The connection ended".into());
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

// MARK: Setup

struct Settings {
    current: Option<Vec<u8>>,
    backup: Option<Backup>,
    sum: String,
}

fn read_settings(alias: &str) -> Result<Settings, String> {
    let lines = one_shot(alias, "read", &[])?;
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
    let client = info
        .client
        .as_ref()
        .ok_or("The host has none of nc, socat or python3 for the hooks to write with")?;
    let socket = format!("{}/.openbar/{}", info.home, socket_name(app));
    let command = hook_command(client, &socket, app);
    for _ in 0..3 {
        let settings = read_settings(alias)?;
        let parsed = parse_settings(settings.current.as_deref()).map_err(|_| UNREADABLE)?;
        let new = serialize(&installer::installing_marked(
            &parsed,
            &command,
            &marker(app),
        ));
        let lines = one_shot(alias, "install", &[settings.sum, STANDARD.encode(new)])?;
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
    for _ in 0..3 {
        let settings = read_settings(alias)?;
        let plan = removal(settings.current.as_deref(), settings.backup.as_ref(), app)
            .map_err(|_| UNREADABLE)?;
        let mut input = vec![socket_name(app), settings.sum];
        match plan {
            Removal::Restore => input.push("restore".into()),
            Removal::Write(data, keep) => {
                input.push(if keep { "keep" } else { "write" }.into());
                input.push(STANDARD.encode(data));
            }
        }
        let lines = one_shot(alias, "remove", &input)?;
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
        })
        .unwrap();
        assert_eq!(
            connected,
            json!({"state": "connected", "hasClient": true, "hooksInstalled": false})
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
}
