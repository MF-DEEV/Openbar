// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session in a zellij pane (#235), on Linux and on Windows, where zellij
//! runs natively since 0.44. zellij puts the session's name and the pane's id
//! in the environment of everything started in a pane, and starts each pane's
//! program itself, so its server is an ancestor of the session: that is what
//! makes the pane the session's own, and which of two nested multiplexers is
//! the inner one. Its command line, run with `--session`, pastes into a pane
//! by id without moving focus. Actions from outside a session (selecting a
//! tab or a pane) go to the client that was used last.

use std::path::Path;

use crate::hosts::process::Entry;
use crate::hosts::wezterm::Step;

#[cfg(target_os = "linux")]
use crate::linux::command::run;

#[cfg(not(target_os = "linux"))]
#[path = "../linux/command.rs"]
mod command;
#[cfg(not(target_os = "linux"))]
use command::run;

/// The variables a zellij pane is told by.
pub const ZELLIJ: &str = "ZELLIJ";
pub const SESSION: &str = "ZELLIJ_SESSION_NAME";
pub const PANE: &str = "ZELLIJ_PANE_ID";

/// A program's name in lower case without ".exe".
fn stem(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    lowered
        .strip_suffix(".exe")
        .map(str::to_owned)
        .unwrap_or(lowered)
}

/// The zellij server among the session's ancestors (nearest first, the
/// session itself first of all). A tmux or zellij started in a pane runs a
/// server of its own apart from it, so a session in that one has no zellij
/// above it; the server on Windows can be a child of the client that started
/// it, so the nearest zellij is the server.
pub fn server_of(chain: &[Entry]) -> Option<u32> {
    chain
        .iter()
        .skip(1)
        .find(|entry| stem(&entry.name) == "zellij")
        .map(|entry| entry.pid)
}

/// The session and pane the environment names, when it names them well.
pub fn pane_of(environment: &std::collections::HashMap<String, String>) -> Option<(String, u32)> {
    environment.get(ZELLIJ)?;
    let session = environment.get(SESSION)?.trim();
    let pane = environment.get(PANE)?.trim().parse::<u32>().ok()?;
    (!session.is_empty() && !session.contains('\n')).then(|| (session.to_owned(), pane))
}

/// A zellij session to act on: the server's own program, the session's name
/// and the server's pid.
pub struct Target<'a> {
    pub program: &'a str,
    pub session: &'a str,
    pub server: u32,
}

/// The `--session` that worked last for each server.
static WORKING: std::sync::Mutex<Option<std::collections::HashMap<u32, String>>> =
    std::sync::Mutex::new(None);

/// What `--session` takes for the session, the one that worked last first.
/// zellij finds a session by its name through the metadata it keeps in the
/// user's cache folder; with another cache folder (the app started with
/// another HOME or XDG_CACHE_HOME than the shell) only the name of the
/// server's socket, a hash, finds it.
fn names(target: &Target) -> Vec<String> {
    let mut names = vec![target.session.to_owned()];
    if let Some(socket) = crate::hosts::process::metadata(target.server)
        .and_then(|meta| socket_name(&meta.command_line))
        .filter(|socket| socket != target.session)
    {
        names.push(socket);
    }
    let known = WORKING
        .lock()
        .ok()
        .and_then(|map| map.as_ref()?.get(&target.server).cloned());
    if let Some(index) = known.and_then(|known| names.iter().position(|n| *n == known)) {
        names.swap(0, index);
    }
    names
}

/// The last part of the path after `--server` in a server's command line.
pub fn socket_name(command_line: &str) -> Option<String> {
    let words = split_command_line(command_line);
    let path = words.iter().skip_while(|w| *w != "--server").nth(1)?;
    let name = path.rsplit(['/', '\\']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn act(target: &Target, arguments: &[&str]) -> Option<String> {
    for name in names(target) {
        let mut command = vec!["--session", name.as_str(), "action"];
        command.extend_from_slice(arguments);
        if let Some(output) = run(target.program, &[], &command, None) {
            if let Ok(mut map) = WORKING.lock() {
                map.get_or_insert_with(Default::default)
                    .insert(target.server, name);
            }
            return Some(output);
        }
    }
    None
}

fn pane_id(pane: u32) -> String {
    format!("terminal_{pane}")
}

/// Types the reply into the pane, whichever pane is in front. Err when
/// zellij or the pane is gone.
pub fn send(target: &Target, pane: u32, text: &str, images: &[String]) -> Result<(), ()> {
    let id = pane_id(pane);
    for step in crate::hosts::wezterm::steps(text, images) {
        match step {
            // A bracketed paste, so line breaks stay text.
            Step::Paste(content) => {
                act(target, &["paste", "--pane-id", &id, &content]).ok_or(())?;
            }
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            Step::Enter => {
                act(target, &["send-keys", "--pane-id", &id, "Enter"]).ok_or(())?;
            }
        }
    }
    Ok(())
}

/// Presses Ctrl-C in the pane (#243).
pub fn interrupt(target: &Target, pane: u32) -> Result<(), ()> {
    act(
        target,
        &["send-keys", "--pane-id", &pane_id(pane), "Ctrl c"],
    )
    .map(|_| ())
    .ok_or(())
}

/// The tab of a terminal pane, from `list-panes --json`.
pub fn tab_of(json: &str, pane: u32) -> Option<u64> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .ok()?
        .iter()
        .find(|entry| {
            entry.get("is_plugin").and_then(|p| p.as_bool()) == Some(false)
                && entry.get("id").and_then(|id| id.as_u64()) == Some(u64::from(pane))
        })?
        .get("tab_id")?
        .as_u64()
}

/// Each client's focused pane, from `list-clients`: a header, then
/// "id pane command" lines. None at all means no terminal is attached.
pub fn client_panes(listed: &str) -> Vec<String> {
    listed
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            fields.next()?.parse::<u32>().ok()?;
            fields.next().map(str::to_owned)
        })
        .collect()
}

/// The pane is what zellij shows when every attached client has it focused
/// (whichever of them is in front then shows it), and isn't when none has.
/// None when only some have: which terminal shows which client can't be told.
pub fn shown_by(focused: &[String], pane: u32) -> Option<bool> {
    if focused.is_empty() {
        return Some(false);
    }
    let id = pane_id(pane);
    let mine = focused.iter().filter(|f| **f == id).count();
    match mine {
        0 => Some(false),
        n if n == focused.len() => Some(true),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Focus {
    /// The pane is selected in the client used last.
    Selected,
    /// No terminal is attached to the session.
    Detached,
    Missing,
}

/// Selects the pane's tab, then the pane (`focus-pane-id` alone never
/// changes tabs), in the client used last.
pub fn focus(target: &Target, pane: u32) -> Focus {
    let Some(tab) = act(target, &["list-panes", "--json"]).and_then(|json| tab_of(&json, pane))
    else {
        return Focus::Missing;
    };
    let Some(clients) = act(target, &["list-clients"]) else {
        return Focus::Missing;
    };
    if client_panes(&clients).is_empty() {
        return Focus::Detached;
    }
    let tab = tab.to_string();
    let id = pane_id(pane);
    if act(target, &["go-to-tab-by-id", &tab]).is_none() {
        return Focus::Missing;
    }
    // It fails with "already focused" when it is: list-panes already told
    // the pane is there.
    let _ = act(target, &["focus-pane-id", &id]);
    Focus::Selected
}

/// Whether the pane is in front inside zellij, as `shown_by` tells.
pub fn shown(target: &Target, pane: u32) -> Option<bool> {
    let listed = act(target, &["list-clients"])?;
    shown_by(&client_panes(&listed), pane)
}

/// Whether any terminal is attached to the session.
pub fn attached(target: &Target) -> Option<bool> {
    act(target, &["list-clients"]).map(|listed| !client_panes(&listed).is_empty())
}

/// The arguments that attach the session in a new terminal window.
pub fn attach_arguments(session: &str) -> Vec<String> {
    vec!["attach".into(), session.into()]
}

/// The program for the session: the server's own, so client and server are
/// the same version, or the client can't reach the session at all.
pub fn program_of(server_image: Option<&Path>) -> String {
    server_image
        .filter(|path| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("zellij"))
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "zellij".into())
}

/// Whether a client's command line asks for `session` by name: `zellij
/// attach <name>`, `zellij a <name>`, `zellij -s <name>`, `zellij
/// --session <name>`.
pub fn names_session(command_line: &str, session: &str) -> bool {
    let words = split_command_line(command_line);
    words.windows(2).any(|pair| {
        matches!(pair[0].as_str(), "attach" | "a" | "-s" | "--session") && pair[1] == session
    })
}

/// A command line split into words, honouring double quotes (Windows) and
/// the NUL separators of `/proc/<pid>/cmdline` (Linux).
fn split_command_line(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if !quoted && (c.is_whitespace() || c == '\0') => {
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

/// The peers of unix sockets from `ss -xn`: (local inode, peer inode).
pub fn socket_peers(listed: &str) -> Vec<(u64, u64)> {
    listed
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // Netid State Recv-Q Send-Q Local-Address Port Peer-Address Port
            if fields.len() < 8 || fields[0] != "u_str" {
                return None;
            }
            let local = fields[5].parse().ok()?;
            let peer = fields[7].parse().ok()?;
            Some((local, peer))
        })
        .collect()
}

/// The clients among `candidates` (pid, its socket inodes) whose socket's
/// peer is one of the server's sockets.
pub fn clients_by_socket(
    server_sockets: &[u64],
    candidates: &[(u32, Vec<u64>)],
    peers: &[(u64, u64)],
) -> Vec<u32> {
    candidates
        .iter()
        .filter(|(_, sockets)| {
            peers
                .iter()
                .any(|(local, peer)| sockets.contains(local) && server_sockets.contains(peer))
        })
        .map(|(pid, _)| *pid)
        .collect()
}

/// The zellij clients attached to the server, the one used last first.
#[cfg(target_os = "linux")]
pub fn clients(server: u32, session: &str) -> Vec<u32> {
    let _ = session;
    fn sockets(pid: u32) -> Vec<u64> {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let target = std::fs::read_link(entry.ok()?.path()).ok()?;
                let target = target.to_str()?;
                target
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse()
                    .ok()
            })
            .collect()
    }
    let candidates: Vec<(u32, Vec<u64>)> = crate::hosts::process::pids_named("zellij")
        .into_iter()
        .filter(|&pid| pid != server && crate::linux::process::terminal_of(pid).is_some())
        .map(|pid| (pid, sockets(pid)))
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let Some(listed) = run("ss", &[], &["-xn"], None) else {
        return Vec::new();
    };
    let mut found = clients_by_socket(&sockets(server), &candidates, &socket_peers(&listed));
    // The person typed into the one read last.
    found.sort_by(|a, b| last_read(*b).total_cmp(&last_read(*a)));
    found
}

#[cfg(target_os = "linux")]
fn last_read(pid: u32) -> f64 {
    use std::os::unix::fs::MetadataExt;
    crate::linux::process::terminal_of(pid)
        .and_then(|device| std::fs::metadata(device).ok())
        .map(|meta| meta.atime() as f64 + meta.atime_nsec() as f64 / 1e9)
        .unwrap_or(0.0)
}

/// The zellij clients attached to the session. Windows has no way to tell
/// a socket's peer from outside, so a client counts when its command line
/// names the session, or when it started the server (a plain `zellij`).
#[cfg(windows)]
pub fn clients(server: u32, session: &str) -> Vec<u32> {
    let starter = crate::hosts::process::ancestors(server)
        .and_then(|chain| chain.get(1).map(|entry| entry.pid));
    crate::hosts::process::pids_named("zellij.exe")
        .into_iter()
        .filter(|&pid| pid != server)
        .filter(|&pid| {
            Some(pid) == starter
                || crate::hosts::process::metadata(pid)
                    .is_some_and(|meta| names_session(&meta.command_line, session))
        })
        .collect()
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn clients(_server: u32, _session: &str) -> Vec<u32> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn entry(pid: u32, name: &str) -> Entry {
        Entry {
            pid,
            name: name.into(),
        }
    }

    #[test]
    fn the_nearest_zellij_above_the_session_is_its_server() {
        let chain = [
            entry(50, "node"),
            entry(40, "bash"),
            entry(30, "zellij"),
            entry(1, "systemd"),
        ];
        assert_eq!(server_of(&chain), Some(30));
        let windows = [
            entry(50, "node.exe"),
            entry(40, "pwsh.exe"),
            entry(30, "zellij.exe"),
            entry(20, "zellij.exe"),
            entry(10, "WindowsTerminal.exe"),
        ];
        assert_eq!(server_of(&windows), Some(30));
        // A tmux started in a pane runs its server apart: no zellij above.
        let in_tmux = [
            entry(70, "node"),
            entry(60, "tmux: server"),
            entry(1, "systemd"),
        ];
        assert_eq!(server_of(&in_tmux), None);
        // The session itself is never its own server.
        assert_eq!(server_of(&[entry(30, "zellij")]), None);
    }

    #[test]
    fn the_environment_names_session_and_pane() {
        let mut env = HashMap::new();
        env.insert(ZELLIJ.to_owned(), "0".to_owned());
        env.insert(SESSION.to_owned(), "work".to_owned());
        env.insert(PANE.to_owned(), "3".to_owned());
        assert_eq!(pane_of(&env), Some(("work".into(), 3)));
        env.insert(PANE.to_owned(), "x".to_owned());
        assert_eq!(pane_of(&env), None);
        env.insert(PANE.to_owned(), "3".to_owned());
        env.remove(ZELLIJ);
        assert_eq!(pane_of(&env), None);
    }

    #[test]
    fn list_panes_gives_a_terminal_panes_tab() {
        let json = r#"[{"id":4,"is_plugin":true,"tab_id":0},
            {"id":0,"is_plugin":false,"tab_id":0},
            {"id":3,"is_plugin":false,"tab_id":2}]"#;
        assert_eq!(tab_of(json, 3), Some(2));
        assert_eq!(
            tab_of(json, 4),
            None,
            "a plugin's id is never a terminal pane's"
        );
        assert_eq!(tab_of("nope", 3), None);
    }

    #[test]
    fn list_clients_gives_each_clients_pane() {
        let listed = "CLIENT_ID  ZELLIJ_PANE_ID RUNNING_COMMAND\n7  terminal_2  N/A\n8  terminal_3  claude --resume\n";
        assert_eq!(client_panes(listed), vec!["terminal_2", "terminal_3"]);
        assert!(client_panes("CLIENT_ID  ZELLIJ_PANE_ID RUNNING_COMMAND\n").is_empty());
        let both = vec!["terminal_3".to_owned(), "terminal_3".to_owned()];
        assert_eq!(shown_by(&both, 3), Some(true));
        assert_eq!(shown_by(&["terminal_2".to_owned()], 3), Some(false));
        assert_eq!(shown_by(&[], 3), Some(false));
        assert_eq!(
            shown_by(&["terminal_3".to_owned(), "terminal_2".to_owned()], 3),
            None
        );
    }

    #[test]
    fn a_client_names_its_session() {
        assert!(names_session(r#""C:\z\zellij.exe" attach work"#, "work"));
        assert!(names_session("zellij\0-s\0work\0", "work"));
        assert!(names_session("zellij a work", "work"));
        assert!(names_session("zellij --session \"my work\"", "my work"));
        assert!(!names_session("zellij attach worker", "work"));
        assert!(!names_session("zellij", "work"));
    }

    #[test]
    fn a_client_is_found_by_its_sockets_peer() {
        let ss = "Netid State Recv-Q Send-Q Local Address:Port Peer Address:Port\n\
            u_str ESTAB 0 0 /run/user/501/zellij/contract_version_1/0e95 634874 * 635574\n\
            u_str ESTAB 0 0 * 635574 * 634874\n\
            u_str ESTAB 0 0 * 639303 * 639304\n";
        let peers = socket_peers(ss);
        assert!(peers.contains(&(635574, 634874)));
        let found = clients_by_socket(&[634874], &[(10, vec![635574]), (20, vec![639303])], &peers);
        assert_eq!(found, vec![10]);
    }

    #[test]
    fn the_servers_socket_is_named_with_a_hash() {
        assert_eq!(
            socket_name("/z/zellij --server /tmp/zellij-501/contract_version_1/69447bcc5479")
                .as_deref(),
            Some("69447bcc5479")
        );
        assert_eq!(
            socket_name(r#""C:\z\zellij.exe" --server C:\Users\v\AppData\Local\Temp\zellij\contract_version_1\58e6c9a68744"#)
                .as_deref(),
            Some("58e6c9a68744")
        );
        assert_eq!(socket_name("zellij attach work"), None);
    }

    #[test]
    fn the_servers_own_program_is_used() {
        assert_eq!(
            program_of(Some(Path::new("/opt/z/zellij"))),
            "/opt/z/zellij"
        );
        assert_eq!(program_of(Some(Path::new("/usr/bin/bash"))), "zellij");
        assert_eq!(program_of(None), "zellij");
    }
}
