// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Where a Claude Code session runs, and how a reply reaches it (spec
//! 02-sessions §6.2). The host is found by walking the claude process's
//! ancestors: Windows Terminal, a VS Code-family IDE, or a plain console
//! window. Replies go through the IDE bridge extension in an IDE, and are
//! written into the session's console input everywhere else, from a helper
//! process so the island never attaches to a console itself.

use serde::Serialize;

pub mod bridge;
#[cfg(windows)]
pub mod console;
pub mod process;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Host {
    WindowsTerminal {
        terminal_pid: u32,
    },
    Vscode {
        ide: String,
        ide_pid: u32,
    },
    Conhost,
    /// Some other app owns the session (no replies, Open only raises it).
    #[allow(dead_code)]
    App {
        name: String,
    },
    Unknown,
}

impl Host {
    pub fn accepts_replies(&self) -> bool {
        matches!(
            self,
            Host::WindowsTerminal { .. } | Host::Vscode { .. } | Host::Conhost
        )
    }
}

/// The host and the claude process's ancestors (nearest first), which the
/// IDE bridge matches against its terminals' shell pids.
#[derive(Clone, Debug)]
pub struct Located {
    pub host: Host,
    pub ancestors: Vec<u32>,
}

const IDES: [(&str, &str); 6] = [
    ("code.exe", "vscode"),
    ("code - insiders.exe", "vscode"),
    ("cursor.exe", "cursor"),
    ("antigravity.exe", "antigravity"),
    ("antigravity ide.exe", "antigravity"),
    ("windsurf.exe", "windsurf"),
];

/// Classifies by the first ancestor whose image is a terminal or an IDE.
pub fn classify(chain: &[process::Entry]) -> Located {
    let ancestors: Vec<u32> = chain.iter().map(|e| e.pid).collect();
    for entry in chain.iter().skip(1) {
        let name = entry.name.to_ascii_lowercase();
        if name == "windowsterminal.exe" {
            return Located {
                host: Host::WindowsTerminal {
                    terminal_pid: entry.pid,
                },
                ancestors,
            };
        }
        if let Some((_, ide)) = IDES.iter().find(|(exe, _)| *exe == name) {
            return Located {
                host: Host::Vscode {
                    ide: (*ide).into(),
                    ide_pid: entry.pid,
                },
                ancestors,
            };
        }
        if name == "explorer.exe" || name == "services.exe" || name == "svchost.exe" {
            break;
        }
    }
    // A shell started from Explorer or Run owns a console window of its own.
    let host = if chain.len() > 1 {
        Host::Conhost
    } else {
        Host::Unknown
    };
    Located { host, ancestors }
}

/// Finds the host of claude process `pid` (None when it is gone).
pub fn locate(pid: u32) -> Option<Located> {
    let chain = process::ancestors(pid)?;
    Some(classify(&chain))
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeliveryError {
    Unreachable,
}

/// Types `text` into the session as a paste, then Return.
pub fn deliver(
    pid: u32,
    located: &Located,
    text: &str,
    images: &[String],
) -> Result<(), DeliveryError> {
    match &located.host {
        Host::Vscode { .. } => {
            bridge::send(&located.ancestors, text, images).map_err(|_| DeliveryError::Unreachable)
        }
        Host::WindowsTerminal { .. } | Host::Conhost => console_type(pid, text, images),
        _ => Err(DeliveryError::Unreachable),
    }
}

/// Brings the session's window forward.
pub fn focus(pid: u32, located: &Located) -> bool {
    match &located.host {
        Host::Vscode { ide_pid, .. } => {
            let shown = bridge::focus(&located.ancestors).is_ok();
            platform_raise_process(*ide_pid) || shown
        }
        Host::WindowsTerminal { .. } | Host::Conhost => console_focus(pid),
        _ => false,
    }
}

#[cfg(windows)]
fn console_type(pid: u32, text: &str, images: &[String]) -> Result<(), DeliveryError> {
    console::run_helper(
        pid,
        console::Action::Type {
            text: text.into(),
            images: images.to_vec(),
        },
    )
    .map(|_| ())
    .map_err(|_| DeliveryError::Unreachable)
}

#[cfg(windows)]
fn console_focus(pid: u32) -> bool {
    console::run_helper(pid, console::Action::Window)
        .ok()
        .and_then(|out| out.trim().parse::<isize>().ok())
        .is_some_and(console::raise)
}

#[cfg(windows)]
fn platform_raise_process(pid: u32) -> bool {
    console::raise_process_window(pid)
}

#[cfg(not(windows))]
fn console_type(_pid: u32, _text: &str, _images: &[String]) -> Result<(), DeliveryError> {
    Err(DeliveryError::Unreachable)
}

#[cfg(not(windows))]
fn console_focus(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
fn platform_raise_process(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::process::Entry;
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
    fn windows_terminal_is_found_through_the_npm_shim() {
        let l = classify(&chain(&[
            "node.exe",
            "cmd.exe",
            "pwsh.exe",
            "OpenConsole.exe",
            "WindowsTerminal.exe",
        ]));
        assert_eq!(l.host, Host::WindowsTerminal { terminal_pid: 104 });
        assert_eq!(l.ancestors, vec![100, 101, 102, 103, 104]);
    }

    #[test]
    fn ides_are_told_apart() {
        let l = classify(&chain(&["claude.exe", "powershell.exe", "Code.exe"]));
        assert_eq!(
            l.host,
            Host::Vscode {
                ide: "vscode".into(),
                ide_pid: 102
            }
        );
        let l = classify(&chain(&["claude.exe", "pwsh.exe", "Cursor.exe"]));
        assert_eq!(
            l.host,
            Host::Vscode {
                ide: "cursor".into(),
                ide_pid: 102
            }
        );
    }

    #[test]
    fn a_shell_from_explorer_is_a_console_window() {
        let l = classify(&chain(&["claude.exe", "cmd.exe", "explorer.exe"]));
        assert_eq!(l.host, Host::Conhost);
        assert!(l.host.accepts_replies());
        assert_eq!(classify(&chain(&["claude.exe"])).host, Host::Unknown);
    }
}

/// Brings forward the terminal of a Codex session, which Codex never ties
/// to a pid: the only interactive `codex.exe`, or else the window titled
/// after the thread (spec 07 §6.3, §9.2).
pub fn focus_codex(thread_name: &str) -> bool {
    let terminals: Vec<(u32, Located)> = process::pids_named("codex.exe")
        .into_iter()
        .filter_map(|pid| {
            locate(pid)
                .filter(|l| l.host.accepts_replies())
                .map(|l| (pid, l))
        })
        .collect();
    if terminals.len() == 1 {
        let (pid, located) = &terminals[0];
        return focus(*pid, located);
    }
    #[cfg(windows)]
    if !thread_name.is_empty() && console::raise_window_titled(thread_name) {
        return true;
    }
    #[cfg(not(windows))]
    let _ = thread_name;
    terminals
        .first()
        .is_some_and(|(pid, located)| focus(*pid, located))
}
