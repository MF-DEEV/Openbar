// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session in a Herdr pane (#328), on Linux and Windows. Herdr puts the pane's id and
//! its server's API socket in the environment of everything started in a
//! pane, and starts each pane's program itself, so its server is an ancestor
//! of the session: that is what makes the pane the session's own. Herdr's
//! server runs apart from the terminal, so a tmux or zellij started in a pane
//! has the variables too; one met on the way up is the innermost multiplexer.
//!
//! Herdr is reached through its socket API: one JSON request per line, one
//! answer per line. `pane.send_input` pastes into a pane (bracketed when the
//! pane asked for it) without moving focus, `pane.focus` switches the
//! workspace and tab with the pane, and Herdr keeps one focus for the whole
//! session, which every attached terminal shows.
//!
//! On Windows the socket is a named pipe named after `HERDR_SOCKET_PATH`
//! (`\\.\pipe\C:\Users\…\herdr.sock`), and the file at that path only holds
//! the server's pid and a stamp ("9632:1795872492618554900").

// Herdr is reached on Linux and Windows; elsewhere the module only builds.
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

use std::collections::HashMap;
use std::path::Path;

use crate::hosts::process::Entry;
use crate::hosts::wezterm::Step;

/// The variables a Herdr pane is told by.
pub const ENV: &str = "HERDR_ENV";
pub const PANE: &str = "HERDR_PANE_ID";
pub const SOCKET: &str = "HERDR_SOCKET_PATH";
pub const SESSION: &str = "HERDR_SESSION";
pub const VARIABLES: [&str; 4] = [ENV, PANE, SOCKET, SESSION];

/// The pane a session's environment names: the API socket, the pane id
/// (`w1:p2`) and the named session (None for the default one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    pub socket: String,
    pub pane: String,
    pub session: Option<String>,
}

/// The pane the environment names, when it names it well.
pub fn pane_of(environment: &HashMap<String, String>) -> Option<Pane> {
    if environment.get(ENV).map(String::as_str) != Some("1") {
        return None;
    }
    let pane = environment.get(PANE)?.trim();
    let socket = environment.get(SOCKET)?;
    if !is_pane_id(pane) || socket.contains('\n') || !socket_path_fits(socket) {
        return None;
    }
    let session = environment
        .get(SESSION)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.contains('\n'))
        .map(str::to_owned);
    Some(Pane {
        socket: socket.clone(),
        pane: pane.to_owned(),
        session,
    })
}

/// An absolute path that fits a unix socket's address (108 bytes on Linux),
/// or on Windows a pipe's name (256 characters with `\\.\pipe\`).
fn socket_path_fits(socket: &str) -> bool {
    if cfg!(windows) {
        let bytes = socket.as_bytes();
        bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
            && socket.encode_utf16().count() + 9 <= 256
    } else {
        socket.starts_with('/') && socket.len() < 108
    }
}

/// `w<n>:p<n>`, the form Herdr gives pane ids.
pub fn is_pane_id(id: &str) -> bool {
    let Some((workspace, pane)) = id.split_once(':') else {
        return false;
    };
    let digits = |part: &str, lead: char| {
        part.strip_prefix(lead)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    digits(workspace, 'w') && digits(pane, 'p')
}

/// A program's name in lower case without ".exe".
fn stem(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    lowered
        .strip_suffix(".exe")
        .map(str::to_owned)
        .unwrap_or(lowered)
}

/// Whether a command line is a Herdr server's: `herdr server`, or `herdr
/// server --handoff-import …` after an update. `herdr server stop` and the
/// like only ask a server.
pub fn is_server_command(command_line: &str) -> bool {
    let words = words(command_line);
    words.len() >= 2
        && is_herdr(&words[0])
        && words[1] == "server"
        && words.get(2).is_none_or(|w| w.starts_with("--"))
}

/// A program path naming Herdr (`herdr`, `C:\\h\\herdr.exe`).
fn is_herdr(program: &str) -> bool {
    program
        .rsplit(['/', '\\'])
        .next()
        .is_some_and(|name| stem(name) == "herdr")
}

/// A command line in words: `/proc/<pid>/cmdline`'s NULs, or Windows'
/// spaces with double quotes around a word that has them.
fn words(command_line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in command_line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if !quoted && (c == ' ' || c == '\t' || c == '\0') => {
                if any || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                any = false;
            }
            c => word.push(c),
        }
    }
    if any || !word.is_empty() {
        words.push(word);
    }
    words
}

/// The session a Herdr client's command line attaches: `herdr` (the default
/// session, None), `herdr --session <name>`, `herdr session attach <name>`.
/// Anything else (`herdr pane list`, `herdr server`) is no client.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn client_session(command_line: &str) -> Option<Option<String>> {
    let words = words(command_line);
    if !is_herdr(words.first()?) {
        return None;
    }
    match &words[1..] {
        [] => Some(None),
        [flag, name] if flag == "--session" => Some(Some(name.clone())),
        [session, attach, name] if session == "session" && attach == "attach" => {
            Some(Some(name.clone()))
        }
        _ => None,
    }
}

/// The Herdr server among the session's ancestors (nearest first, the
/// session itself first of all), unless a zellij or tmux server comes first:
/// then that one holds the session.
pub fn server_of(chain: &[Entry], is_server: impl Fn(u32) -> bool) -> Option<u32> {
    for entry in chain.iter().skip(1) {
        let name = stem(&entry.name);
        if name == "herdr" && is_server(entry.pid) {
            return Some(entry.pid);
        }
        if name == "zellij" || name.starts_with("tmux") {
            return None;
        }
    }
    None
}

/// Whether process `pid` is a Herdr server.
pub fn is_server(pid: u32) -> bool {
    crate::hosts::process::metadata(pid).is_some_and(|meta| is_server_command(&meta.command_line))
}

/// A Herdr session to act on: its API socket, the pane, and the server
/// that answers on that socket.
pub struct Target<'a> {
    pub socket: &'a str,
    pub pane: &'a str,
    pub server: u32,
}

/// The `result` of an answer line; None when it carries an `error`.
pub fn result(line: &str) -> Option<serde_json::Value> {
    let answer: serde_json::Value = serde_json::from_str(line).ok()?;
    if answer.get("error").is_some() {
        return None;
    }
    answer.get("result").cloned()
}

/// A socket of this user's, as Herdr makes it, never anything the
/// environment could point at instead.
#[cfg(target_os = "linux")]
pub fn own_socket(path: &str) -> bool {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.file_type().is_socket() && meta.uid() == unsafe { libc::getuid() })
}

/// On Windows the path is a plain file naming the server's pid; the pipe
/// itself is checked to be served by that process when it is opened.
#[cfg(windows)]
pub fn own_socket(path: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
        && marker_server(&std::fs::read_to_string(path).unwrap_or_default()).is_some()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn own_socket(_path: &str) -> bool {
    false
}

/// The server's pid from a Windows socket file ("9632:1795872492618554900").
#[cfg_attr(not(windows), allow(dead_code))]
pub fn marker_server(marker: &str) -> Option<u32> {
    let (pid, stamp) = marker.trim().split_once(':')?;
    stamp.bytes().all(|b| b.is_ascii_digit()).then_some(())?;
    pid.parse().ok().filter(|&pid| pid > 0)
}

fn request_line(method: &str, params: serde_json::Value) -> String {
    let mut line =
        serde_json::json!({ "id": "openbar", "method": method, "params": params }).to_string();
    line.push('\n');
    line
}

/// Sends one request and reads its answer's result.
#[cfg(target_os = "linux")]
pub fn request(
    socket: &str,
    _server: u32,
    method: &str,
    params: serde_json::Value,
    timeout: std::time::Duration,
) -> Option<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    if !own_socket(socket) {
        return None;
    }
    let mut stream = std::os::unix::net::UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    stream
        .write_all(request_line(method, params).as_bytes())
        .ok()?;
    let mut answer = String::new();
    // No more than an answer can be.
    BufReader::new(std::io::Read::take(stream, 8 << 20))
        .read_line(&mut answer)
        .ok()?;
    result(answer.trim_end())
}

/// Sends one request over the server's named pipe and reads its answer's
/// result. Only a pipe the session's own server serves is written to:
/// another process could have opened one of the same name first.
#[cfg(windows)]
pub fn request(
    socket: &str,
    server: u32,
    method: &str,
    params: serde_json::Value,
    timeout: std::time::Duration,
) -> Option<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_NONE, OPEN_EXISTING, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    };
    use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows::core::{HSTRING, PCWSTR};
    if !own_socket(socket) || marker_server(&std::fs::read_to_string(socket).ok()?) != Some(server)
    {
        return None;
    }
    let name = HSTRING::from(format!(r"\\.\pipe\{socket}"));
    let handle = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            // Herdr learns who connected but can't act as them.
            FILE_FLAGS_AND_ATTRIBUTES(SECURITY_SQOS_PRESENT.0 | SECURITY_IDENTIFICATION.0),
            None,
        )
    }
    .ok()?;
    // Owned from here on: the handle closes when the file drops.
    let pipe = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let mut served_by = 0;
    if unsafe { GetNamedPipeServerProcessId(handle, &mut served_by) }.is_err()
        || served_by != server
    {
        return None;
    }
    let line = request_line(method, params);
    // A pipe has no timeouts of its own: the exchange runs on a thread of its
    // own, and a server that never answers leaves only that thread waiting.
    let (done, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let answer = (|| {
            pipe.write_all(line.as_bytes()).ok()?;
            let mut answer = String::new();
            BufReader::new(std::io::Read::take(pipe, 8 << 20))
                .read_line(&mut answer)
                .ok()?;
            Some(answer)
        })();
        let _ = done.send(answer);
    });
    result(answer.recv_timeout(timeout).ok()??.trim_end())
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn request(
    _socket: &str,
    _server: u32,
    _method: &str,
    _params: serde_json::Value,
    _timeout: std::time::Duration,
) -> Option<serde_json::Value> {
    None
}

const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

fn ask(target: &Target, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    request(target.socket, target.server, method, params, WAIT)
}

/// Types the reply into the pane, whichever pane is in front. Err when
/// Herdr or the pane is gone.
pub fn send(target: &Target, text: &str, images: &[String]) -> Result<(), ()> {
    for step in crate::hosts::wezterm::steps(text, images) {
        match step {
            Step::Paste(content) => {
                ask(
                    target,
                    "pane.send_input",
                    serde_json::json!({ "pane_id": target.pane, "text": content }),
                )
                .ok_or(())?;
            }
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            Step::Enter => keys(target, "enter")?,
        }
    }
    Ok(())
}

/// Presses a key in the pane by Herdr's name for it (`down`, `esc`, `2`).
pub fn keys(target: &Target, name: &str) -> Result<(), ()> {
    ask(
        target,
        "pane.send_keys",
        serde_json::json!({ "pane_id": target.pane, "keys": [name] }),
    )
    .map(|_| ())
    .ok_or(())
}

/// Whether the pane is the one Herdr shows: its one focus, which every
/// attached terminal shows. None when Herdr didn't answer.
pub fn focused(target: &Target) -> Option<bool> {
    let answer = request(
        target.socket,
        target.server,
        "session.snapshot",
        serde_json::json!({}),
        std::time::Duration::from_secs(1),
    )?;
    Some(focused_pane(&answer)? == target.pane)
}

/// The focused pane of a `session.snapshot` answer.
pub fn focused_pane(answer: &serde_json::Value) -> Option<&str> {
    answer.get("snapshot")?.get("focused_pane_id")?.as_str()
}

/// Whether Herdr still has the pane.
pub fn has_pane(target: &Target) -> bool {
    ask(
        target,
        "pane.get",
        serde_json::json!({ "pane_id": target.pane }),
    )
    .is_some()
}

/// Focuses the pane, its workspace and tab with it.
pub fn focus(target: &Target) -> bool {
    ask(
        target,
        "pane.focus",
        serde_json::json!({ "pane_id": target.pane }),
    )
    .is_some()
}

/// The arguments that attach the session in a new terminal window.
pub fn attach_arguments(session: Option<&str>) -> Vec<String> {
    match session {
        Some(name) => vec!["session".into(), "attach".into(), name.into()],
        None => Vec::new(),
    }
}

/// The program for the session: the server's own.
pub fn program_of(server_image: Option<&Path>) -> String {
    server_image
        .filter(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("herdr"))
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "herdr".into())
}

/// Unix sockets from `ss -xn`: (local path, local inode, peer inode). The
/// path is "*" for a socket without one.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn sockets(listed: &str) -> Vec<(String, u64, u64)> {
    listed
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // Netid State Recv-Q Send-Q Local-Address Port Peer-Address Port
            if fields.len() < 8 || fields[0] != "u_str" {
                return None;
            }
            Some((
                fields[4].to_owned(),
                fields[5].parse().ok()?,
                fields[7].parse().ok()?,
            ))
        })
        .collect()
}

/// The clients among `candidates` (pid, its socket inodes) whose socket's
/// peer is one of the server's sockets, the API socket left out: a `herdr
/// pane …` asking the server from some terminal talks over that one and is
/// no client.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn clients_by_socket(
    server_sockets: &[u64],
    api: &str,
    candidates: &[(u32, Vec<u64>)],
    listed: &[(String, u64, u64)],
) -> Vec<u32> {
    let own: Vec<u64> = server_sockets
        .iter()
        .copied()
        .filter(|inode| {
            !listed
                .iter()
                .any(|(path, local, _)| local == inode && path == api)
        })
        .collect();
    candidates
        .iter()
        .filter(|(_, sockets)| {
            listed
                .iter()
                .any(|(_, local, peer)| sockets.contains(local) && own.contains(peer))
        })
        .map(|(pid, _)| *pid)
        .collect()
}

/// The terminals attached to the server, the one used last first; None when
/// that can't be told.
#[cfg(target_os = "linux")]
pub fn clients(server: u32, api: &str, _session: Option<&str>) -> Option<Vec<u32>> {
    use crate::linux::command::run;
    fn sockets_of(pid: u32) -> Vec<u64> {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let target = std::fs::read_link(entry.ok()?.path()).ok()?;
                target
                    .to_str()?
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse()
                    .ok()
            })
            .collect()
    }
    let candidates: Vec<(u32, Vec<u64>)> = crate::hosts::process::pids_named("herdr")
        .into_iter()
        .filter(|&pid| {
            pid != server && crate::linux::process::terminal_of(pid).is_some() && !is_server(pid)
        })
        .map(|pid| (pid, sockets_of(pid)))
        .collect();
    if candidates.is_empty() {
        return Some(Vec::new());
    }
    let listed = run("ss", &[], &["-xn"], None)?;
    let mut found = clients_by_socket(&sockets_of(server), api, &candidates, &sockets(&listed));
    // The person typed into the one read last.
    found.sort_by(|a, b| last_read(*b).total_cmp(&last_read(*a)));
    Some(found)
}

#[cfg(target_os = "linux")]
fn last_read(pid: u32) -> f64 {
    use std::os::unix::fs::MetadataExt;
    crate::linux::process::terminal_of(pid)
        .and_then(|device| std::fs::metadata(device).ok())
        .map(|meta| meta.atime() as f64 + meta.atime_nsec() as f64 / 1e9)
        .unwrap_or(0.0)
}

/// The Herdr clients attached to the session. Windows can't tell a pipe's
/// other end from outside, so a client counts when its command line attaches
/// this session (`herdr --session <name>`, `herdr session attach <name>`, a
/// plain `herdr` for the default one), or when it started the server.
#[cfg(windows)]
pub fn clients(server: u32, _api: &str, session: Option<&str>) -> Option<Vec<u32>> {
    let starter = crate::hosts::process::ancestors(server)
        .and_then(|chain| chain.get(1).map(|entry| entry.pid));
    let mut found: Vec<u32> = crate::hosts::process::pids_named("herdr.exe")
        .into_iter()
        .filter(|&pid| pid != server)
        .filter(|&pid| {
            crate::hosts::process::metadata(pid).is_some_and(|meta| {
                client_session(&meta.command_line)
                    .is_some_and(|attached| Some(pid) == starter || attached.as_deref() == session)
            })
        })
        .collect();
    // The one that started the server first: it was there from the start.
    found.sort_by_key(|&pid| Some(pid) != starter);
    Some(found)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn clients(_server: u32, _api: &str, _session: Option<&str>) -> Option<Vec<u32>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pid: u32, name: &str) -> Entry {
        Entry {
            pid,
            name: name.into(),
        }
    }

    fn environment(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn the_environment_names_pane_socket_and_session() {
        let socket = if cfg!(windows) {
            r"C:\Users\v\AppData\Roaming\herdr\sessions\work\herdr.sock"
        } else {
            "/home/v/.config/herdr/sessions/work/herdr.sock"
        };
        let good = [
            (ENV, "1"),
            (PANE, "w1:p2"),
            (SOCKET, socket),
            (SESSION, "work"),
        ];
        assert_eq!(
            pane_of(&environment(&good)),
            Some(Pane {
                socket: socket.into(),
                pane: "w1:p2".into(),
                session: Some("work".into()),
            })
        );
        let default = environment(&good[..3]);
        assert_eq!(pane_of(&default).and_then(|p| p.session), None);
        let long = if cfg!(windows) {
            r"C:\".to_owned() + &"s".repeat(250)
        } else {
            "/".to_owned() + &"s".repeat(120)
        };
        for (key, value) in [
            (ENV, "0"),
            (PANE, "p2"),
            (PANE, "w1:p"),
            (PANE, "w:p2"),
            (PANE, "w1:p2;x"),
            (SOCKET, "herdr.sock"),
            (SOCKET, "/a\nb"),
            (SOCKET, long.as_str()),
        ] {
            let mut env = environment(&good);
            env.insert(key.into(), value.into());
            assert_eq!(pane_of(&env), None, "{key}={value:?} names no pane");
        }
        let mut without = environment(&good);
        without.remove(ENV);
        assert_eq!(pane_of(&without), None);
    }

    #[test]
    fn the_nearest_herdr_server_runs_the_pane_unless_another_multiplexer_comes_first() {
        let servers = [30];
        let is_server = |pid: u32| servers.contains(&pid);
        let chain = [
            entry(50, "node"),
            entry(40, "bash"),
            entry(30, "herdr"),
            entry(1, "systemd"),
        ];
        assert_eq!(server_of(&chain, is_server), Some(30));
        // zellij in a Herdr pane: its sessions are zellij's.
        let in_zellij = [
            entry(70, "node"),
            entry(60, "zellij"),
            entry(55, "bash"),
            entry(30, "herdr"),
        ];
        assert_eq!(server_of(&in_zellij, is_server), None);
        let in_tmux = [
            entry(70, "node"),
            entry(65, "tmux: server"),
            entry(30, "herdr"),
        ];
        assert_eq!(server_of(&in_tmux, is_server), None);
        // A herdr client a shell runs is no server.
        let client = [entry(80, "node"), entry(75, "herdr"), entry(1, "systemd")];
        assert_eq!(server_of(&client, is_server), None);
        // The session itself is never its own server.
        assert_eq!(server_of(&[entry(30, "herdr")], is_server), None);
    }

    #[test]
    fn a_client_names_the_session_it_attaches() {
        assert_eq!(client_session("herdr"), Some(None));
        assert_eq!(
            client_session(r#"C:\vors\h328\herdr.exe  --session work"#),
            Some(Some("work".into()))
        );
        assert_eq!(
            client_session(r#""C:\Program Files\herdr\herdr.exe" session attach "my work""#),
            Some(Some("my work".into()))
        );
        assert_eq!(
            client_session("/usr/bin/herdr\0--session\0w\0"),
            Some(Some("w".into()))
        );
        assert_eq!(client_session("herdr pane list"), None);
        assert_eq!(client_session("herdr server"), None);
        assert_eq!(client_session("node herdr"), None);
        assert_eq!(client_session(""), None);
    }

    #[test]
    fn a_windows_socket_file_names_its_server() {
        assert_eq!(marker_server("9632:1795872492618554900"), Some(9632));
        assert_eq!(marker_server("9632:1795872492618554900\r\n"), Some(9632));
        assert_eq!(marker_server("0:1"), None);
        assert_eq!(marker_server("x:1"), None);
        assert_eq!(marker_server("12:ab"), None);
        assert_eq!(marker_server(""), None);
    }

    #[test]
    fn a_server_is_herdr_server() {
        assert!(is_server_command("/home/v/.local/bin/herdr\0server\0"));
        assert!(is_server_command("herdr server --handoff-import /s t"));
        assert!(is_server_command(r#""C:\vors\h328\herdr.exe" server"#));
        assert!(!is_server_command("herdr\0server\0stop\0"));
        assert!(!is_server_command("herdr --session work"));
        assert!(!is_server_command("/bin/sh server"));
    }

    #[test]
    fn an_answer_gives_its_result() {
        assert_eq!(
            result(r#"{"id":"openbar","result":{"type":"ok"}}"#),
            Some(serde_json::json!({"type": "ok"}))
        );
        assert_eq!(
            result(r#"{"id":"openbar","error":{"code":"pane_not_found"}}"#),
            None
        );
        assert_eq!(result("nope"), None);
        let snapshot = serde_json::json!({"type": "session_snapshot", "snapshot": {"focused_pane_id": "w1:p2"}});
        assert_eq!(focused_pane(&snapshot), Some("w1:p2"));
        assert_eq!(focused_pane(&serde_json::json!({})), None);
    }

    #[test]
    fn a_client_keeps_a_socket_to_the_server_not_to_its_api() {
        let api = "/home/v/.config/herdr/sessions/work/herdr.sock";
        let ss = format!(
            "Netid State Recv-Q Send-Q Local Address:Port Peer Address:Port\n\
            u_str ESTAB 0 0 /home/v/.config/herdr/sessions/work/herdr-client.sock 700 * 800\n\
            u_str ESTAB 0 0 * 800 * 700\n\
            u_str ESTAB 0 0 {api} 710 * 900\n\
            u_str ESTAB 0 0 * 900 * 710\n"
        );
        let listed = sockets(&ss);
        assert_eq!(listed.len(), 4);
        let found = clients_by_socket(
            &[700, 710],
            api,
            &[(10, vec![800]), (20, vec![900])],
            &listed,
        );
        assert_eq!(found, vec![10]);
    }

    #[test]
    fn attach_names_the_session() {
        assert_eq!(
            attach_arguments(Some("work")),
            vec!["session", "attach", "work"]
        );
        assert!(attach_arguments(None).is_empty());
        assert_eq!(program_of(Some(Path::new("/opt/h/herdr"))), "/opt/h/herdr");
        assert_eq!(program_of(Some(Path::new("/usr/bin/bash"))), "herdr");
        assert_eq!(program_of(None), "herdr");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_request_is_one_json_line_and_only_a_socket_is_spoken_to() {
        use std::io::{BufRead, BufReader, Write};
        let folder = std::env::temp_dir().join(format!("ob328-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&folder);
        let path = folder.join("h.sock");
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            (&stream)
                .write_all(b"{\"id\":\"openbar\",\"result\":{\"type\":\"ok\"}}\n")
                .unwrap();
            line
        });
        let socket = path.to_str().unwrap();
        let answer = request(
            socket,
            0,
            "pane.send_keys",
            serde_json::json!({"pane_id": "w1:p2", "keys": ["enter"]}),
            WAIT,
        );
        let sent: serde_json::Value =
            serde_json::from_str(server.join().unwrap().trim_end()).unwrap();
        assert_eq!(answer, Some(serde_json::json!({"type": "ok"})));
        assert_eq!(sent["method"], "pane.send_keys");
        assert_eq!(sent["params"]["pane_id"], "w1:p2");
        let plain = folder.join("plain");
        std::fs::write(&plain, b"").unwrap();
        assert!(!own_socket(plain.to_str().unwrap()));
        assert_eq!(
            request(
                plain.to_str().unwrap(),
                0,
                "pane.focus",
                serde_json::json!({}),
                WAIT
            ),
            None
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
