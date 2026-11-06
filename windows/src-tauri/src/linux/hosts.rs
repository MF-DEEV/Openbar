// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Where a session runs on Linux and how a reply or Open reaches it. The host is
//! found by the programs among the claude process's ancestors, then by the tmux
//! pane or kitty window its environment names, once that pane or window is shown
//! to hold the session. `hosts/mod.rs` hands over to this for Windows' counterpart.

use std::collections::HashMap;

use super::{kitty, process, tmux};
use crate::hosts::process::Entry;
use crate::hosts::{DeliveryError, Host, Located, wezterm, wezterm_host};

/// Terminals the island can only bring forward, by their program's name (lower
/// case). WezTerm is here too: without the variables of its pane it is one of them.
const TERMINALS: &[(&str, &str)] = &[
    ("wezterm-gui", "WezTerm"),
    ("alacritty", "Alacritty"),
    ("warp", "Warp"),
    ("warp-terminal", "Warp"),
    ("tabby", "Tabby"),
    ("hyper", "Hyper"),
    ("rio", "Rio"),
    ("kitty", "kitty"),
    ("ghostty", "Ghostty"),
    ("gnome-terminal-server", "GNOME Terminal"),
    ("konsole", "Konsole"),
    ("xfce4-terminal", "Xfce Terminal"),
    ("tilix", "Tilix"),
    ("terminator", "Terminator"),
    ("foot", "foot"),
];

const IDES: &[(&str, &str)] = &[
    ("code", "vscode"),
    ("code - insiders", "vscode"),
    ("code-insiders", "vscode"),
    ("cursor", "cursor"),
    ("antigravity", "antigravity"),
    ("antigravity ide", "antigravity"),
    ("windsurf", "windsurf"),
];

/// A program's name in lower case without ".exe".
fn stem(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    match lowered.strip_suffix(".exe") {
        Some(stem) => stem.to_owned(),
        None => lowered,
    }
}

/// Classifies by the first ancestor whose image is a terminal or an IDE.
pub fn classify(chain: &[Entry]) -> Located {
    let ancestors: Vec<u32> = chain.iter().map(|e| e.pid).collect();
    for entry in chain.iter().skip(1) {
        let name = stem(&entry.name);
        if let Some((_, ide)) = IDES.iter().find(|(known, _)| *known == name) {
            return Located {
                host: Host::Vscode {
                    ide: (*ide).into(),
                    ide_pid: entry.pid,
                },
                ancestors,
            };
        }
        if let Some((_, terminal)) = TERMINALS.iter().find(|(known, _)| *known == name) {
            return Located {
                host: Host::App {
                    name: (*terminal).into(),
                },
                ancestors,
            };
        }
    }
    Located {
        host: Host::Unknown,
        ancestors,
    }
}

/// Finds the host of claude process `pid` (None when it is gone).
pub fn locate(pid: u32) -> Option<Located> {
    locate_with(pid, "tmux", "kitten")
}

/// `locate` with the command lines of tmux and kitty named, for tests.
fn locate_with(pid: u32, tmux_program: &str, kitten_program: &str) -> Option<Located> {
    let chain = process::ancestors(pid)?;
    let mut located = classify(&chain);
    // WezTerm is reached by the pane the session's environment names.
    if located.host
        == (Host::App {
            name: "WezTerm".into(),
        })
        && let Some(gui) = chain.iter().find(|e| stem(&e.name) == "wezterm-gui")
        && let Some(host) = wezterm_host(
            gui.pid,
            &process::environment_values(pid, &[wezterm::PANE, wezterm::SOCKET]),
        )
    {
        located.host = host;
    }
    // kitty is reached by the window its remote control names, once it lists the session.
    let window = process::environment_values(pid, &[kitty::WINDOW, kitty::LISTEN]);
    if let Some(host) = kitty_in(&located, pid, &window, kitten_program) {
        located.host = host;
    }
    // A pane of tmux wins over the terminal around it, once it is on the session's terminal.
    if let Some(host) = tmux_in(
        process::terminal_of(pid).as_deref(),
        &process::environment_values(pid, &[tmux::TMUX, tmux::PANE]),
        tmux_program,
    ) {
        located.host = host;
    }
    Some(located)
}

/// The kitty window the session's environment names, once kitty lists the session in it.
fn kitty_in(
    located: &Located,
    pid: u32,
    environment: &HashMap<String, String>,
    program: &str,
) -> Option<Host> {
    if located.host
        != (Host::App {
            name: "kitty".into(),
        })
    {
        return None;
    }
    let host = kitty_host(environment)?;
    let Host::Kitty { window, listen } = &host else {
        return None;
    };
    kitty::holds_with(program, listen, *window, pid).then_some(host)
}

/// The tmux pane the session's environment names, once it is on the session's own terminal:
/// an editor opened from a pane keeps `TMUX` and `TMUX_PANE` too.
fn tmux_in(
    terminal: Option<&str>,
    environment: &HashMap<String, String>,
    program: &str,
) -> Option<Host> {
    let host = tmux_host(environment)?;
    let Host::Tmux { socket, pane } = &host else {
        return None;
    };
    tmux::holds_with(program, socket, pane, terminal).then_some(host)
}

/// A tmux pane, when the session's own environment names one.
fn tmux_host(environment: &HashMap<String, String>) -> Option<Host> {
    let socket = tmux::socket_of(environment.get(tmux::TMUX)?)?;
    let pane = environment.get(tmux::PANE)?.trim();
    pane.strip_prefix('%')?.parse::<u32>().ok()?;
    Some(Host::Tmux {
        socket,
        pane: pane.to_owned(),
    })
}

/// A kitty window, when remote control is on and the environment names it.
fn kitty_host(environment: &HashMap<String, String>) -> Option<Host> {
    let window = environment.get(kitty::WINDOW)?.trim().parse().ok()?;
    let listen = environment
        .get(kitty::LISTEN)
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())?;
    Some(Host::Kitty {
        window,
        listen: listen.to_owned(),
    })
}

/// Types `text` into the tmux pane or kitty window `located` names, as a paste, then Return.
pub fn deliver(
    pid: u32,
    located: &Located,
    text: &str,
    images: &[String],
) -> Result<(), DeliveryError> {
    match &located.host {
        Host::Tmux { socket, pane } => tmux::send(
            socket,
            pane,
            process::terminal_of(pid).as_deref(),
            text,
            images,
        )
        .map_err(|_| DeliveryError::Unreachable),
        Host::Kitty { window, listen } => {
            kitty::send(listen, *window, pid, text, images).map_err(|_| DeliveryError::Unreachable)
        }
        _ => Err(DeliveryError::Unreachable),
    }
}

/// Brings the window of the tmux pane or kitty window `located` names forward.
pub fn focus(pid: u32, located: &Located, cwd: &str) -> bool {
    match &located.host {
        Host::Tmux { socket, pane } => {
            let mut pids = tmux::focus(socket, pane, process::terminal_of(pid).as_deref());
            // The terminal around each attached client, nearest first.
            let clients = pids.clone();
            for client in clients {
                pids.extend(
                    process::ancestors(client)
                        .into_iter()
                        .flatten()
                        .map(|entry| entry.pid),
                );
            }
            !pids.is_empty() && super::x11::raise(&pids, cwd)
        }
        Host::Kitty { window, listen } => {
            let _ = kitty::focus(listen, *window, pid);
            super::x11::raise(&located.ancestors, cwd)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(names: &[&str]) -> Vec<Entry> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| Entry {
                pid: 100 + i as u32,
                name: (*n).into(),
            })
            .collect()
    }

    #[test]
    fn linux_program_names_are_recognized() {
        let l = classify(&chain(&["claude", "bash", "kitty", "systemd"]));
        assert_eq!(
            l.host,
            Host::App {
                name: "kitty".into()
            }
        );
        assert!(!l.host.accepts_replies());
        let l = classify(&chain(&["claude", "zsh", "code", "systemd"]));
        assert_eq!(
            l.host,
            Host::Vscode {
                ide: "vscode".into(),
                ide_pid: 102
            }
        );
        let l = classify(&chain(&["claude", "bash", "gnome-terminal-server"]));
        assert_eq!(l.host.display_name().as_deref(), Some("GNOME Terminal"));
    }

    #[test]
    fn a_session_in_an_unknown_terminal_is_not_a_console_window() {
        let l = classify(&chain(&["claude", "bash", "someterm"]));
        assert_eq!(l.host, Host::Unknown);
        assert!(!l.host.accepts_replies());
    }

    /// A command line of a terminal in a folder of its own: its directory and path.
    fn scripted(name: &str, program: &str, body: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("openbar-host-{name}-{}", std::process::id()));
        let path = super::super::command::fake_program(&dir, program, body);
        (dir, path.display().to_string())
    }

    #[test]
    fn a_tmux_pane_counts_only_on_the_sessions_own_terminal() {
        let body = "case \"$*\" in *pane_tty*) echo /dev/pts/7;; *) exit 1;; esac\n";
        let (dir, program) = scripted("pane", "tmux", body);
        let mut environment = HashMap::new();
        environment.insert(
            tmux::TMUX.to_owned(),
            "/tmp/tmux-1000/default,1234,0".to_owned(),
        );
        environment.insert(tmux::PANE.to_owned(), "%3".to_owned());
        let pane = Host::Tmux {
            socket: "/tmp/tmux-1000/default".into(),
            pane: "%3".into(),
        };
        assert_eq!(
            tmux_in(Some("/dev/pts/7"), &environment, &program),
            Some(pane)
        );
        // An editor opened from the pane has the variables, not the pane's terminal.
        assert_eq!(tmux_in(Some("/dev/pts/9"), &environment, &program), None);
        assert_eq!(tmux_in(None, &environment, &program), None);
        let empty = HashMap::new();
        assert_eq!(tmux_in(Some("/dev/pts/7"), &empty, &program), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_kitty_window_counts_only_when_it_lists_the_session() {
        let body = r#"case "$*" in *' ls '*) echo '[{"tabs":[{"windows":[{"id":3,"pid":500,"foreground_processes":[{"pid":501}]}]}]}]';; *) exit 1;; esac
"#;
        let (dir, program) = scripted("window", "kitten", body);
        let in_kitty = Located {
            host: Host::App {
                name: "kitty".into(),
            },
            ancestors: Vec::new(),
        };
        let mut environment = HashMap::new();
        environment.insert(kitty::WINDOW.to_owned(), "3".to_owned());
        environment.insert(kitty::LISTEN.to_owned(), "unix:/tmp/k".to_owned());
        let window = Host::Kitty {
            window: 3,
            listen: "unix:/tmp/k".into(),
        };
        assert_eq!(
            kitty_in(&in_kitty, 501, &environment, &program),
            Some(window)
        );
        // Another kitty can have a window with the same id.
        assert_eq!(kitty_in(&in_kitty, 777, &environment, &program), None);
        // A session that is not under a kitty at all got the variables from a parent shell.
        let elsewhere = Located {
            host: Host::Unknown,
            ancestors: Vec::new(),
        };
        assert_eq!(kitty_in(&elsewhere, 501, &environment, &program), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_session_in_a_pane_is_found_by_the_terminal_it_runs_on() {
        let Some(session) = process::OnTerminal::start(&[
            (tmux::TMUX, "/tmp/openbar-no-such-socket,1,0"),
            (tmux::PANE, "%3"),
        ]) else {
            eprintln!("no pseudo-terminals here");
            return;
        };
        let own = format!(
            "case \"$*\" in *pane_tty*) echo {};; *) exit 1;; esac\n",
            session.device
        );
        let (dir, program) = scripted("own", "tmux", &own);
        let found = locate_with(session.child.id(), &program, "kitten").unwrap();
        assert_eq!(
            found.host,
            Host::Tmux {
                socket: "/tmp/openbar-no-such-socket".into(),
                pane: "%3".into()
            }
        );
        // The same variables on another terminal: an editor opened from the pane.
        let body = "case \"$*\" in *pane_tty*) echo /dev/pts/99999;; *) exit 1;; esac\n";
        let (other_dir, other) = scripted("elsewhere", "tmux", body);
        let found = locate_with(session.child.id(), &other, "kitten").unwrap();
        assert!(!matches!(found.host, Host::Tmux { .. }));
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(other_dir);
    }
}
