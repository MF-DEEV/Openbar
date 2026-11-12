// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session inside tmux: tmux puts the server's socket and the pane id in
//! the environment of everything started in a pane, and its command line
//! pastes into that pane. The terminal window showing it is found through the
//! tmux clients attached to the session. The pane has to be on the session's
//! own terminal before anything is typed: an editor or a terminal started from
//! a pane keeps the variables too.

use super::command::run;
use crate::hosts::wezterm::Step;

pub const TMUX: &str = "TMUX";
pub const PANE: &str = "TMUX_PANE";

/// The server socket out of `$TMUX` (`socket,server-pid,session`).
pub fn socket_of(variable: &str) -> Option<String> {
    let socket = variable.split(',').next()?.trim();
    (!socket.is_empty()).then(|| socket.to_owned())
}

fn tmux(socket: &str, arguments: &[&str], input: Option<&str>) -> Option<String> {
    tmux_with("tmux", socket, arguments, input)
}

fn tmux_with(
    program: &str,
    socket: &str,
    arguments: &[&str],
    input: Option<&str>,
) -> Option<String> {
    let mut command = vec!["-S", socket];
    command.extend_from_slice(arguments);
    run(program, &[], &command, input)
}

/// The pane still runs the session: it is on `terminal`, the session's device (a pane id comes back on another pane after a server restart).
pub(super) fn holds_with(program: &str, socket: &str, pane: &str, terminal: Option<&str>) -> bool {
    let Some(terminal) = terminal else {
        return false;
    };
    tmux_with(
        program,
        socket,
        &["display-message", "-p", "-t", pane, "#{pane_tty}"],
        None,
    )
    .is_some_and(|device| device.trim() == terminal)
}

/// Types the reply into the pane. Err when tmux or the pane is gone, or the pane is not on `terminal`.
pub fn send(
    socket: &str,
    pane: &str,
    terminal: Option<&str>,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    send_with("tmux", socket, pane, terminal, text, images)
}

fn send_with(
    program: &str,
    socket: &str,
    pane: &str,
    terminal: Option<&str>,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    if !holds_with(program, socket, pane, terminal) {
        return Err(());
    }
    for step in crate::hosts::wezterm::steps(text, images) {
        match step {
            // A bracketed paste, so line breaks stay text.
            Step::Paste(content) => {
                tmux_with(
                    program,
                    socket,
                    &["load-buffer", "-b", "openbar", "-"],
                    Some(&content),
                )
                .ok_or(())?;
                tmux_with(
                    program,
                    socket,
                    &["paste-buffer", "-p", "-d", "-b", "openbar", "-t", pane],
                    None,
                )
                .ok_or(())?;
            }
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            Step::Enter => {
                tmux_with(program, socket, &["send-keys", "-t", pane, "Enter"], None).ok_or(())?;
            }
        }
    }
    Ok(())
}

/// Whether the pane is the one its tmux shows (#210): the active pane of the
/// active window, in a session a terminal is attached to. Ok gives the pid
/// of that terminal's tmux client, used last, whose own terminal decides the
/// rest; Err that tmux shows something else. None when tmux can't say or the
/// pane isn't the session's.
pub fn shown(socket: &str, pane: &str, terminal: Option<&str>) -> Option<Result<u32, ()>> {
    shown_with("tmux", socket, pane, terminal)
}

fn shown_with(
    program: &str,
    socket: &str,
    pane: &str,
    terminal: Option<&str>,
) -> Option<Result<u32, ()>> {
    if !holds_with(program, socket, pane, terminal) {
        return None;
    }
    let state = tmux_with(
        program,
        socket,
        &[
            "display-message",
            "-p",
            "-t",
            pane,
            "#{pane_active}#{window_active}\t#{session_id}",
        ],
        None,
    )?;
    let (flags, session) = state.trim().split_once('\t')?;
    if flags != "11" {
        return Some(Err(()));
    }
    let clients = tmux_with(
        program,
        socket,
        &[
            "list-clients",
            "-F",
            "#{client_pid}\t#{session_id}\t#{client_activity}",
        ],
        None,
    )?;
    Some(client_on(&clients, session).ok_or(()))
}

/// The client showing `session` that was used last.
fn client_on(listed: &str, session: &str) -> Option<u32> {
    listed
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let pid = fields.next()?.trim().parse::<u32>().ok()?;
            (fields.next()? == session).then_some(())?;
            let activity = fields
                .next()
                .and_then(|a| a.trim().parse::<u64>().ok())
                .unwrap_or(0);
            Some((pid, activity))
        })
        .max_by_key(|(_, activity)| *activity)
        .map(|(pid, _)| pid)
}

/// Shows the pane's window and pane inside tmux; the pids of the attached
/// clients, whose terminal window the caller raises.
pub fn focus(socket: &str, pane: &str, terminal: Option<&str>) -> Vec<u32> {
    focus_with("tmux", socket, pane, terminal)
}

fn focus_with(program: &str, socket: &str, pane: &str, terminal: Option<&str>) -> Vec<u32> {
    if !holds_with(program, socket, pane, terminal) {
        return Vec::new();
    }
    let _ = tmux_with(program, socket, &["select-window", "-t", pane], None);
    let _ = tmux_with(program, socket, &["select-pane", "-t", pane], None);
    tmux_with(
        program,
        socket,
        &["list-clients", "-F", "#{client_pid}"],
        None,
    )
    .map(|out| out.lines().filter_map(|l| l.trim().parse().ok()).collect())
    .unwrap_or_default()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_client_is_the_one_on_the_pane_session_used_last() {
        let listed = "41\t$1\t100\n42\t$1\t300\n43\t$2\t900\nnope\n";
        assert_eq!(client_on(listed, "$1"), Some(42));
        assert_eq!(client_on(listed, "$2"), Some(43));
        assert_eq!(client_on(listed, "$3"), None);
    }

    #[test]
    fn the_socket_is_the_first_field() {
        assert_eq!(
            socket_of("/tmp/tmux-1000/default,1234,0").as_deref(),
            Some("/tmp/tmux-1000/default")
        );
        assert_eq!(socket_of(""), None);
        assert_eq!(socket_of(",1,0"), None);
    }

    /// The device of pane %3 in the fake `tmux`.
    const DEVICE: &str = "/dev/pts/7";

    /// A `tmux` that records its arguments and stdin, and knows pane %3 on `DEVICE`.
    struct Fake(PathBuf);

    impl Fake {
        fn new(name: &str) -> Fake {
            let dir =
                std::env::temp_dir().join(format!("openbar-tmux-{name}-{}", std::process::id()));
            let log = dir.join("log");
            super::super::command::fake_program(
                &dir,
                "tmux",
                &format!(
                    "echo \"$*\" >> {log}\ncase \"$*\" in\n*'-t %3 #{{pane_tty}}'*) echo {DEVICE};;\n*display-message*) exit 1;;\n*load-buffer*) echo \"stdin:$(cat)\" >> {log};;\n*list-clients*) printf '41\\n42\\n';;\nesac\n",
                    log = log.display()
                ),
            );
            Fake(dir)
        }

        fn program(&self) -> String {
            self.0.join("tmux").display().to_string()
        }

        fn log(&self) -> String {
            std::fs::read_to_string(self.0.join("log")).unwrap_or_default()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_reply_is_loaded_pasted_and_returned() {
        let fake = Fake::new("send");
        send_with(
            &fake.program(),
            "/s",
            "%3",
            Some(DEVICE),
            "hello\nthere",
            &[],
        )
        .unwrap();
        let log = fake.log();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines[0], "-S /s display-message -p -t %3 #{pane_tty}");
        assert_eq!(lines[1], "-S /s load-buffer -b openbar -");
        assert_eq!(lines[2], "stdin:hello");
        assert_eq!(lines[3], "there");
        assert_eq!(lines[4], "-S /s paste-buffer -p -d -b openbar -t %3");
        assert_eq!(lines[5], "-S /s send-keys -t %3 Enter");
    }

    #[test]
    fn a_pane_that_is_gone_is_not_typed_into() {
        let fake = Fake::new("gone");
        assert!(send_with(&fake.program(), "/s", "%9", Some(DEVICE), "x", &[]).is_err());
        assert!(!fake.log().contains("load-buffer"));
    }

    #[test]
    fn a_pane_on_another_terminal_is_not_typed_into() {
        // An editor started from a pane keeps TMUX_PANE, but not the pane's terminal.
        let fake = Fake::new("other");
        assert!(send_with(&fake.program(), "/s", "%3", Some("/dev/pts/9"), "x", &[]).is_err());
        assert!(send_with(&fake.program(), "/s", "%3", None, "x", &[]).is_err());
        assert!(!fake.log().contains("load-buffer") && !fake.log().contains("send-keys"));
        assert!(focus_with(&fake.program(), "/s", "%3", Some("/dev/pts/9")).is_empty());
        assert!(!fake.log().contains("select-"));
    }

    #[test]
    fn focus_selects_the_pane_and_names_the_clients() {
        let fake = Fake::new("focus");
        assert_eq!(
            focus_with(&fake.program(), "/s", "%3", Some(DEVICE)),
            vec![41, 42]
        );
        let log = fake.log();
        assert!(log.contains("select-window -t %3") && log.contains("select-pane -t %3"));
        assert!(focus_with(&fake.program(), "/s", "%9", Some(DEVICE)).is_empty());
    }
}
