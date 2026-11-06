// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Runs a terminal's own command line (`wezterm cli`, `tmux`, `kitten @`) with
//! a time limit, optionally feeding it text on stdin.

use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Standard output of a command that exited successfully within four seconds.
pub fn run(
    program: impl AsRef<OsStr>,
    environment: &[(&str, &str)],
    arguments: &[&str],
    input: Option<&str>,
) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .envs(environment.iter().copied())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
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
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    reader.join().ok().flatten()
}

/// A shell script standing in for a terminal's command line, in `dir`.
#[cfg(all(test, unix))]
pub(super) fn fake_program(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let program = dir.join(name);
    std::fs::write(
        &program,
        format!("#!/bin/sh\n[ \"$1\" = --openbar-probe ] && exit 0\n{body}"),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    // Another test's fork can hold the file open for a moment ("text file busy").
    for _ in 0..200 {
        if Command::new(&program)
            .arg("--openbar-probe")
            .status()
            .is_ok()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    program
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn output_and_input_pass_through() {
        assert_eq!(run("echo", &[], &["hi"], None).as_deref(), Some("hi\n"));
        assert_eq!(
            run("cat", &[], &[], Some("typed")).as_deref(),
            Some("typed")
        );
        assert_eq!(
            run(
                "sh",
                &[("OPENBAR_X", "1")],
                &["-c", "echo $OPENBAR_X"],
                None
            )
            .as_deref(),
            Some("1\n")
        );
    }

    #[test]
    fn failure_and_a_missing_program_are_none() {
        assert!(run("false", &[], &[], None).is_none());
        assert!(run("/nonexistent/openbar", &[], &[], None).is_none());
    }
}
