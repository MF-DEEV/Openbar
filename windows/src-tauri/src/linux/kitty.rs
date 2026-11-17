// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session in a kitty window: with remote control on, kitty puts its
//! listening address and the window's id in the environment of everything
//! started in the window, and `kitten @` types into and shows that window. The
//! window has to list the session's process before anything is typed: a kitty
//! started from another keeps the address, and ids repeat between kittys.

use super::command::run;
use crate::hosts::wezterm::Step;

pub const WINDOW: &str = "KITTY_WINDOW_ID";
pub const LISTEN: &str = "KITTY_LISTEN_ON";

fn kitten(program: &str, listen: &str, arguments: &[&str], input: Option<&str>) -> Option<String> {
    let mut command = vec!["@", "--to", listen];
    command.extend_from_slice(arguments);
    run(program, &[], &command, input)
}

/// The window still runs the session: kitty lists the session's process in it.
/// The whole listing is read: kitty 0.32, which Ubuntu 24.04 ships, answers
/// `ls --match id:N` with the active window rather than window N.
pub(super) fn holds_with(program: &str, listen: &str, window: u32, pid: u32) -> bool {
    kitten(program, listen, &["ls"], None)
        .is_some_and(|json| pids(&json, window).contains(&u64::from(pid)))
}

/// The processes kitty lists in a window: its shell and what runs in front.
fn pids(json: &str, window: u32) -> Vec<u64> {
    let Ok(serde_json::Value::Array(instances)) = serde_json::from_str(json) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for tab in instances
        .iter()
        .flat_map(|instance| instance["tabs"].as_array().into_iter().flatten())
    {
        for entry in tab["windows"].as_array().into_iter().flatten() {
            if entry["id"].as_u64() != Some(u64::from(window)) {
                continue;
            }
            found.extend(entry["pid"].as_u64());
            found.extend(
                entry["foreground_processes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|process| process["pid"].as_u64()),
            );
        }
    }
    found
}

/// Whether kitty says the window has focus: its OS window, its tab and the
/// window itself (#210). None when kitty doesn't list it with `pid` in it.
pub fn focused(listen: &str, window: u32, pid: u32) -> Option<bool> {
    focused_with("kitten", listen, window, pid)
}

pub(super) fn focused_with(program: &str, listen: &str, window: u32, pid: u32) -> Option<bool> {
    let json = kitten(program, listen, &["ls"], None)?;
    if !pids(&json, window).contains(&u64::from(pid)) {
        return None;
    }
    is_focused(&json, window)
}

fn is_focused(json: &str, window: u32) -> Option<bool> {
    let serde_json::Value::Array(instances) = serde_json::from_str(json).ok()? else {
        return None;
    };
    for instance in &instances {
        for tab in instance["tabs"].as_array().into_iter().flatten() {
            for entry in tab["windows"].as_array().into_iter().flatten() {
                if entry["id"].as_u64() == Some(u64::from(window)) {
                    return Some(
                        instance["is_focused"].as_bool() == Some(true)
                            && tab["is_focused"].as_bool() == Some(true)
                            && entry["is_focused"].as_bool() == Some(true),
                    );
                }
            }
        }
    }
    None
}

/// Types the reply into the window. Err when kitty or the window is gone, or the window does not list `pid`.
pub fn send(listen: &str, window: u32, pid: u32, text: &str, images: &[String]) -> Result<(), ()> {
    send_with("kitten", listen, window, pid, text, images)
}

fn send_with(
    program: &str,
    listen: &str,
    window: u32,
    pid: u32,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    if !holds_with(program, listen, window, pid) {
        return Err(());
    }
    let target = format!("id:{window}");
    for step in crate::hosts::wezterm::steps(text, images) {
        match step {
            Step::Paste(content) => {
                kitten(
                    program,
                    listen,
                    &[
                        "send-text",
                        "--match",
                        &target,
                        "--stdin",
                        "--bracketed-paste",
                    ],
                    Some(&content),
                )
                .ok_or(())?;
            }
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            Step::Enter => {
                kitten(
                    program,
                    listen,
                    &["send-key", "--match", &target, "enter"],
                    None,
                )
                .ok_or(())?;
            }
        }
    }
    Ok(())
}

/// Shows the window inside kitty; the caller raises kitty's own window.
pub fn focus(listen: &str, window: u32, pid: u32) -> bool {
    holds_with("kitten", listen, window, pid)
        && kitten(
            "kitten",
            listen,
            &["focus-window", "--match", &format!("id:{window}")],
            None,
        )
        .is_some()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const LISTING: &str = r#"[{"tabs":[{"windows":[{"id":5,"pid":500,"foreground_processes":[{"pid":501},{"pid":502}]},{"id":6,"pid":600}]}]}]"#;

    /// A `kitten` that records its arguments and stdin, and lists `LISTING`.
    fn fake(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("openbar-kitten-{name}-{}", std::process::id()));
        let log = dir.join("log");
        let body = "echo \"$*\" >> LOG\ncase \"$*\" in\n*' ls') echo 'LISTING';;\n*--stdin*) echo \"stdin:$(cat)\" >> LOG;;\nesac\n"
            .replace("LOG", &log.display().to_string())
            .replace("LISTING", LISTING);
        let program = super::super::command::fake_program(&dir, "kitten", &body);
        (dir, program)
    }

    #[test]
    fn a_window_has_focus_only_in_the_focused_tab_of_the_focused_os_window() {
        let json = r#"[{"is_focused":true,"tabs":[
            {"is_focused":true,"windows":[{"id":1,"is_focused":true},{"id":2,"is_focused":false}]},
            {"is_focused":false,"windows":[{"id":3,"is_focused":false}]}]},
          {"is_focused":false,"tabs":[{"is_focused":true,"windows":[{"id":4,"is_focused":true}]}]}]"#;
        assert_eq!(is_focused(json, 1), Some(true));
        assert_eq!(is_focused(json, 2), Some(false));
        assert_eq!(is_focused(json, 3), Some(false));
        assert_eq!(is_focused(json, 4), Some(false));
        assert_eq!(is_focused(json, 9), None);
        assert_eq!(is_focused("nope", 1), None);
    }

    #[test]
    fn a_window_is_listed_with_its_shell_and_what_runs_in_front() {
        assert_eq!(pids(LISTING, 5), vec![500, 501, 502]);
        assert_eq!(pids(LISTING, 6), vec![600]);
        assert!(pids(LISTING, 9).is_empty());
        assert!(pids("nope", 5).is_empty());
    }

    #[test]
    fn a_reply_is_pasted_and_returned_through_the_listening_address() {
        let (dir, program) = fake("send");
        send_with(program.to_str().unwrap(), "unix:/k", 5, 501, "hi", &[]).unwrap();
        let text = std::fs::read_to_string(dir.join("log")).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines[0], "@ --to unix:/k ls");
        assert_eq!(
            lines[1],
            "@ --to unix:/k send-text --match id:5 --stdin --bracketed-paste"
        );
        assert_eq!(lines[2], "stdin:hi");
        assert_eq!(lines[3], "@ --to unix:/k send-key --match id:5 enter");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_window_that_does_not_list_the_session_is_not_typed_into() {
        // Another kitty can have a window with the same id.
        let (dir, program) = fake("other");
        let program = program.to_str().unwrap();
        assert!(holds_with(program, "unix:/k", 5, 502));
        assert!(send_with(program, "unix:/k", 5, 777, "hi", &[]).is_err());
        let text = std::fs::read_to_string(dir.join("log")).unwrap();
        assert!(!text.contains("send-text") && !text.contains("send-key"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
