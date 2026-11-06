// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! A session in a kitty window: with remote control on, kitty puts its
//! listening address and the window's id in the environment of everything
//! started in the window, and `kitten @` types into and shows that window.

use super::command::run;
use super::wezterm::Step;

pub const WINDOW: &str = "KITTY_WINDOW_ID";
pub const LISTEN: &str = "KITTY_LISTEN_ON";

fn kitten(program: &str, listen: &str, arguments: &[&str], input: Option<&str>) -> Option<String> {
    let mut command = vec!["@", "--to", listen];
    command.extend_from_slice(arguments);
    run(program, &[], &command, input)
}

/// Types the reply into the window. Err when kitty or the window is gone.
pub fn send(listen: &str, window: u32, text: &str, images: &[String]) -> Result<(), ()> {
    send_with("kitten", listen, window, text, images)
}

fn send_with(
    program: &str,
    listen: &str,
    window: u32,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    let target = format!("id:{window}");
    for step in super::wezterm::steps(text, images) {
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
pub fn focus(listen: &str, window: u32) -> bool {
    kitten(
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

    #[test]
    fn a_reply_is_pasted_and_returned_through_the_listening_address() {
        let dir = std::env::temp_dir().join(format!("openbar-kitten-{}", std::process::id()));
        let log = dir.join("log");
        let program = super::super::command::fake_program(
            &dir,
            "kitten",
            &format!(
                "echo \"$*\" >> {log}\ncase \"$*\" in *--stdin*) echo \"stdin:$(cat)\" >> {log};; esac\n",
                log = log.display()
            ),
        );
        send_with(program.to_str().unwrap(), "unix:/k", 5, "hi", &[]).unwrap();
        let text = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(
            lines[0],
            "@ --to unix:/k send-text --match id:5 --stdin --bracketed-paste"
        );
        assert_eq!(lines[1], "stdin:hi");
        assert_eq!(lines[2], "@ --to unix:/k send-key --match id:5 enter");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
