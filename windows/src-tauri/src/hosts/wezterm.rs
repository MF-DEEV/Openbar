// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A session in a WezTerm pane (issue #121): WezTerm puts the pane's id and
//! the socket of the WezTerm that runs it in the environment of everything
//! started in that pane, and its own command line, `wezterm cli`, shows that
//! pane and types into it. The pane has to be one that WezTerm still lists
//! before anything is typed.

use std::path::PathBuf;

/// The variables a WezTerm pane is told by.
pub const PANE: &str = "WEZTERM_PANE";
pub const SOCKET: &str = "WEZTERM_UNIX_SOCKET";

/// One step of typing a reply: a paste that keeps line breaks as text, a
/// pause, or the return that sends it.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Paste(String),
    Wait(u64),
    Enter,
}

/// Claude Code attaches an image when a paste is only its path and reads it
/// a moment later, so the text and the return wait for the images, as the
/// console and the IDE bridge do.
pub fn steps(text: &str, images: &[String]) -> Vec<Step> {
    let mut steps = Vec::new();
    for image in images {
        steps.push(Step::Paste(image.clone()));
        steps.push(Step::Wait(60));
    }
    if !images.is_empty() {
        steps.push(Step::Wait(1200 + 300 * images.len() as u64));
    }
    if !text.is_empty() {
        steps.push(Step::Paste(text.to_owned()));
    }
    steps.push(Step::Wait(80));
    steps.push(Step::Enter);
    steps
}

/// The tab of the pane, when `wezterm cli list --format json` still lists it.
pub fn tab_of(json: &str, pane: u32) -> Option<u64> {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .ok()?
        .iter()
        .find(|entry| entry.get("pane_id").and_then(|id| id.as_u64()) == Some(u64::from(pane)))
        .map(|entry| {
            entry
                .get("tab_id")
                .and_then(|id| id.as_u64())
                .unwrap_or(u64::MAX)
        })
}

/// Whether WezTerm still lists the pane.
pub fn lists_pane(json: &str, pane: u32) -> bool {
    tab_of(json, pane).is_some()
}

/// `wezterm.exe` beside the `wezterm-gui.exe` that runs the pane, so the
/// command line always matches that WezTerm.
fn cli(gui_pid: u32) -> Option<PathBuf> {
    let gui = super::process::image_path(gui_pid)?;
    let cli = gui.parent()?.join("wezterm.exe");
    cli.is_file().then_some(cli)
}

#[cfg(windows)]
fn run(cli: &PathBuf, socket: &str, arguments: &[&str], input: Option<&str>) -> Option<String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = Command::new(cli)
        .arg("cli")
        .args(arguments)
        .env(SOCKET, socket)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    if let Some(input) = input {
        let mut stdin = child.stdin.take()?;
        stdin.write_all(input.as_bytes()).ok()?;
    }
    // Read while it runs, so a long pane list never fills the pipe and stalls it.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut output)
            .ok()
            .map(|_| output)
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    reader.join().ok().flatten()
}

#[cfg(not(windows))]
fn run(_cli: &PathBuf, _socket: &str, _arguments: &[&str], _input: Option<&str>) -> Option<String> {
    None
}

fn listed(cli: &PathBuf, socket: &str, pane: u32) -> bool {
    run(cli, socket, &["list", "--format", "json"], None)
        .is_some_and(|json| lists_pane(&json, pane))
}

/// Types the reply into the pane. Err when WezTerm or the pane is gone.
pub fn send(
    gui_pid: u32,
    pane: u32,
    socket: &str,
    text: &str,
    images: &[String],
) -> Result<(), ()> {
    let cli = cli(gui_pid).ok_or(())?;
    if !listed(&cli, socket, pane) {
        return Err(());
    }
    let id = pane.to_string();
    for step in steps(text, images) {
        match step {
            // A paste, bracketed when the program asked for one.
            Step::Paste(content) => {
                run(
                    &cli,
                    socket,
                    &["send-text", "--pane-id", &id],
                    Some(&content),
                )
                .ok_or(())?;
            }
            Step::Wait(ms) => std::thread::sleep(std::time::Duration::from_millis(ms)),
            Step::Enter => {
                run(
                    &cli,
                    socket,
                    &["send-text", "--pane-id", &id, "--no-paste"],
                    Some("\r"),
                )
                .ok_or(())?;
            }
        }
    }
    Ok(())
}

/// Shows the pane's tab, then the pane in it; the caller raises the
/// window. Activating the pane alone leaves another tab in front.
pub fn focus(gui_pid: u32, pane: u32, socket: &str) -> bool {
    let Some(cli) = cli(gui_pid) else {
        return false;
    };
    let Some(tab) =
        run(&cli, socket, &["list", "--format", "json"], None).and_then(|json| tab_of(&json, pane))
    else {
        return false;
    };
    if tab != u64::MAX {
        let _ = run(
            &cli,
            socket,
            &["activate-tab", "--tab-id", &tab.to_string()],
            None,
        );
    }
    run(
        &cli,
        socket,
        &["activate-pane", "--pane-id", &pane.to_string()],
        None,
    )
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_one_paste_and_a_return() {
        assert_eq!(
            steps("one\ntwo", &[]),
            vec![Step::Paste("one\ntwo".into()), Step::Wait(80), Step::Enter]
        );
    }

    #[test]
    fn images_go_first_and_the_text_waits_for_them() {
        let images = vec![r"C:\a.png".to_owned(), r"C:\b.png".to_owned()];
        assert_eq!(
            steps("look", &images),
            vec![
                Step::Paste(r"C:\a.png".into()),
                Step::Wait(60),
                Step::Paste(r"C:\b.png".into()),
                Step::Wait(60),
                Step::Wait(1800),
                Step::Paste("look".into()),
                Step::Wait(80),
                Step::Enter,
            ]
        );
        assert!(!steps("", &images[..1]).contains(&Step::Paste(String::new())));
    }

    #[test]
    fn only_a_listed_pane_is_typed_into() {
        let json = r#"[{"window_id":0,"tab_id":0,"pane_id":3,"tty_name":null},{"pane_id":7}]"#;
        assert!(lists_pane(json, 3) && lists_pane(json, 7));
        assert!(!lists_pane(json, 4));
        assert!(!lists_pane("not json", 3));
        assert_eq!(tab_of(r#"[{"pane_id":3,"tab_id":5}]"#, 3), Some(5));
    }
}
