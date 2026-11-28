// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session in a Herdr pane (#328), on Linux. Herdr puts the pane's id and
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

// Herdr is reached on Linux; elsewhere the module only builds.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

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
    // A unix socket's path fits in sockaddr_un (108 bytes on Linux).
    if !is_pane_id(pane) || !socket.starts_with('/') || socket.contains('\n') || socket.len() >= 108
    {
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
    let words: Vec<&str> = command_line
        .split(['\0', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    words.len() >= 2
        && Path::new(words[0])
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| stem(n) == "herdr")
        && words[1] == "server"
        && words.get(2).is_none_or(|w| w.starts_with("--"))
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

/// A Herdr session to act on: its API socket and the pane.
pub struct Target<'a> {
    pub socket: &'a str,
    pub pane: &'a str,
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
#[cfg(unix)]
pub fn own_socket(path: &str) -> bool {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| meta.file_type().is_socket() && meta.uid() == unsafe { libc::getuid() })
}

#[cfg(not(unix))]
pub fn own_socket(_path: &str) -> bool {
    false
}

/// Sends one request and reads its answer's result.
#[cfg(unix)]
pub fn request(
    socket: &str,
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
    let mut line =
        serde_json::json!({ "id": "openbar", "method": method, "params": params }).to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).ok()?;
    let mut answer = String::new();
    // No more than an answer can be.
    BufReader::new(std::io::Read::take(stream, 8 << 20))
        .read_line(&mut answer)
        .ok()?;
    result(answer.trim_end())
}

#[cfg(not(unix))]
pub fn request(
    _socket: &str,
    _method: &str,
    _params: serde_json::Value,
    _timeout: std::time::Duration,
) -> Option<serde_json::Value> {
    None
}

const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

fn ask(target: &Target, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    request(target.socket, method, params, WAIT)
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
pub fn clients(server: u32, api: &str) -> Option<Vec<u32>> {
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

#[cfg(not(target_os = "linux"))]
pub fn clients(_server: u32, _api: &str) -> Option<Vec<u32>> {
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
        let good = [
            (ENV, "1"),
            (PANE, "w1:p2"),
            (SOCKET, "/home/v/.config/herdr/sessions/work/herdr.sock"),
            (SESSION, "work"),
        ];
        assert_eq!(
            pane_of(&environment(&good)),
            Some(Pane {
                socket: "/home/v/.config/herdr/sessions/work/herdr.sock".into(),
                pane: "w1:p2".into(),
                session: Some("work".into()),
            })
        );
        let default = environment(&good[..3]);
        assert_eq!(pane_of(&default).and_then(|p| p.session), None);
        let long = "/".to_owned() + &"s".repeat(120);
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
    fn a_server_is_herdr_server() {
        assert!(is_server_command("/home/v/.local/bin/herdr\0server\0"));
        assert!(is_server_command("herdr server --handoff-import /s t"));
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

    #[cfg(unix)]
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
                "pane.focus",
                serde_json::json!({}),
                WAIT
            ),
            None
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
