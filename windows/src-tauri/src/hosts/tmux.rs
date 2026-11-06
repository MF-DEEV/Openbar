// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session inside tmux: tmux puts the server's socket and the pane id in
//! the environment of everything started in a pane, and its command line
//! pastes into that pane. The terminal window showing it is found through the
//! tmux clients attached to the session.

use super::command::run;
use super::wezterm::Step;

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

/// Whether the server still has the pane.
fn exists(program: &str, socket: &str, pane: &str) -> bool {
    tmux_with(
        program,
        socket,
        &["display-message", "-p", "-t", pane, "#{pane_id}"],
        None,
    )
    .is_some_and(|id| id.trim() == pane)
}

/// Types the reply into the pane. Err when tmux or the pane is gone.
pub fn send(socket: &str, pane: &str, text: &str, images: &[String]) -> Result<(), ()> {
    send_with("tmux", socket, pane, text, images)
}

fn send_with(
    program: &str,
    socket: &str,
    pane: &str,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    if !exists(program, socket, pane) {
        return Err(());
    }
    for step in super::wezterm::steps(text, images) {
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

/// Shows the pane's window and pane inside tmux; the pids of the attached
/// clients, whose terminal window the caller raises.
pub fn focus(socket: &str, pane: &str) -> Vec<u32> {
    focus_with("tmux", socket, pane)
}

fn focus_with(program: &str, socket: &str, pane: &str) -> Vec<u32> {
    if !exists(program, socket, pane) {
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
    fn the_socket_is_the_first_field() {
        assert_eq!(
            socket_of("/tmp/tmux-1000/default,1234,0").as_deref(),
            Some("/tmp/tmux-1000/default")
        );
        assert_eq!(socket_of(""), None);
        assert_eq!(socket_of(",1,0"), None);
    }

    /// A `tmux` that records its arguments and stdin, and knows pane %3.
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
                    "echo \"$*\" >> {log}\ncase \"$*\" in\n*display-message*) echo %3;;\n*load-buffer*) echo \"stdin:$(cat)\" >> {log};;\n*list-clients*) printf '41\\n42\\n';;\nesac\n",
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
        send_with(&fake.program(), "/s", "%3", "hello\nthere", &[]).unwrap();
        let log = fake.log();
        let lines: Vec<_> = log.lines().collect();
        assert_eq!(lines[0], "-S /s display-message -p -t %3 #{pane_id}");
        assert_eq!(lines[1], "-S /s load-buffer -b openbar -");
        assert_eq!(lines[2], "stdin:hello");
        assert_eq!(lines[3], "there");
        assert_eq!(lines[4], "-S /s paste-buffer -p -d -b openbar -t %3");
        assert_eq!(lines[5], "-S /s send-keys -t %3 Enter");
    }

    #[test]
    fn a_pane_that_is_gone_is_not_typed_into() {
        let fake = Fake::new("gone");
        assert!(send_with(&fake.program(), "/s", "%9", "x", &[]).is_err());
        assert!(!fake.log().contains("load-buffer"));
    }

    #[test]
    fn focus_selects_the_pane_and_names_the_clients() {
        let fake = Fake::new("focus");
        assert_eq!(focus_with(&fake.program(), "/s", "%3"), vec![41, 42]);
        let log = fake.log();
        assert!(log.contains("select-window -t %3") && log.contains("select-pane -t %3"));
        assert!(focus_with(&fake.program(), "/s", "%9").is_empty());
    }
}
