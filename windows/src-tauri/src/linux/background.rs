// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Running Claude Code (and Codex) for the background sessions on Linux,
//! where Windows has `CreateProcess`, new consoles and pseudo-consoles.
//! `sessions/background.rs` includes this file as `platform`, for it uses the
//! helpers that module keeps private.
//!
//! - one command with no window, its output taken within a time limit,
//! - a command in a terminal of its own: the first terminal program found
//!   (`$TERMINAL`, then the usual ones), asked to run it in a folder,
//! - a reply typed into a background session: `claude attach <job>` is run
//!   under a pseudo-terminal nobody sees, which is typed into once its screen
//!   has settled, and left with the documented detach key.

use std::ffi::{CStr, OsStr};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{DETACH, Output, config_variable, is_job_id, keeps_variable, keystrokes, screen_text};

fn user() -> Option<PathBuf> {
    crate::hooks::installer::home_dir()
}

/// The `claude` a person's `PATH` finds, then where its installers put it.
fn find_claude(path: Option<&str>, home: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let on_path = path
        .unwrap_or("")
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join("claude"));
    let usual = [
        home.join(".local/bin/claude"),
        home.join(".claude/local/claude"),
        home.join(".npm-global/bin/claude"),
        home.join(".bun/bin/claude"),
        home.join(".local/share/pnpm/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/usr/bin/claude"),
    ];
    on_path.chain(usual).find(|candidate| exists(candidate))
}

pub fn executable() -> Option<PathBuf> {
    find_claude(
        std::env::var("PATH").ok().as_deref(),
        &user()?,
        |candidate| candidate.is_file(),
    )
}

/// The environment a command gets: the app's, without a session's marks, with
/// the config folder it acts in, and a terminal type for what draws a screen.
fn environment(config_home: Option<&str>) -> Vec<(String, String)> {
    let setting = user().and_then(|user| config_variable(config_home, &user));
    let mut list: Vec<(String, String)> = std::env::vars()
        .filter(|(name, _)| {
            keeps_variable(name)
                && !name.eq_ignore_ascii_case(super::super::registry::CONFIG_VARIABLE)
        })
        .collect();
    if let Some(setting) = setting {
        list.push((super::super::registry::CONFIG_VARIABLE.to_owned(), setting));
    }
    if !list.iter().any(|(name, _)| name == "TERM") {
        list.push(("TERM".into(), "xterm-256color".into()));
    }
    list
}

fn command(
    executable: &Path,
    arguments: &[String],
    config_home: Option<&str>,
    folder: Option<&str>,
) -> Command {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .env_clear()
        .envs(environment(config_home));
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    command
}

/// Waits for a command and takes what it printed. None when it doesn't
/// finish in time.
fn collect(mut child: Child, timeout: Duration) -> Option<Output> {
    let read = |mut pipe: Box<dyn Read + Send>| {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let _ = pipe.read_to_end(&mut data);
            let _ = sender.send(data);
        });
        receiver
    };
    let out = read(Box::new(child.stdout.take()?));
    let error = read(Box::new(child.stderr.take()?));
    let started = Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    // A service it left running may hold the pipes open.
    let wait = Duration::from_millis(500);
    Some(Output {
        ok: status.success(),
        out: out.recv_timeout(wait).unwrap_or_default(),
        error: String::from_utf8_lossy(&error.recv_timeout(wait).unwrap_or_default())
            .trim()
            .to_owned(),
    })
}

fn run_claude(
    executable: &Path,
    arguments: &[String],
    config_home: Option<&str>,
    folder: Option<&str>,
    timeout: Duration,
) -> Option<Output> {
    let child = command(executable, arguments, config_home, folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    collect(child, timeout)
}

/// One `claude` command, with no window. None when it can't be run or
/// doesn't finish in time.
pub fn claude(
    arguments: &[String],
    config_home: Option<&str>,
    folder: Option<&str>,
    timeout: Duration,
) -> Option<Output> {
    run_claude(&executable()?, arguments, config_home, folder, timeout)
}

/// `codex` with these arguments and no window, in the app's environment.
pub fn codex(arguments: &[String], timeout: Duration) -> Option<Output> {
    let executable = crate::agent_usage::codex_executable()?;
    let child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    collect(child, timeout)
}

// MARK: In a terminal

/// How a terminal program is asked to run a command in a folder: the
/// arguments before the command, and the flag the folder goes after (none
/// where it follows the working directory of the program).
struct Terminal {
    program: &'static str,
    /// Put before the command and its arguments.
    run: &'static [&'static str],
    /// The option that names the folder, taking it as the next argument or
    /// joined with `=`.
    folder: Option<(&'static str, bool)>,
}

const TERMINALS: [Terminal; 14] = [
    Terminal {
        program: "x-terminal-emulator",
        run: &["-e"],
        folder: None,
    },
    Terminal {
        program: "xdg-terminal-exec",
        run: &[],
        folder: None,
    },
    Terminal {
        program: "gnome-terminal",
        run: &["--"],
        folder: Some(("--working-directory", true)),
    },
    Terminal {
        program: "xfce4-terminal",
        run: &["-x"],
        folder: Some(("--working-directory", true)),
    },
    Terminal {
        program: "konsole",
        run: &["-e"],
        folder: Some(("--workdir", false)),
    },
    Terminal {
        program: "kitty",
        run: &[],
        folder: Some(("--directory", false)),
    },
    Terminal {
        program: "alacritty",
        run: &["-e"],
        folder: Some(("--working-directory", false)),
    },
    Terminal {
        program: "wezterm",
        run: &["start", "--"],
        folder: None,
    },
    Terminal {
        program: "foot",
        run: &[],
        folder: Some(("-D", false)),
    },
    Terminal {
        program: "mate-terminal",
        run: &["-x"],
        folder: Some(("--working-directory", true)),
    },
    Terminal {
        program: "lxterminal",
        run: &["-e"],
        folder: Some(("--working-directory", true)),
    },
    Terminal {
        program: "terminator",
        run: &["-x"],
        folder: Some(("--working-directory", true)),
    },
    Terminal {
        program: "tilix",
        run: &["-e"],
        folder: Some(("-w", false)),
    },
    Terminal {
        program: "xterm",
        run: &["-e"],
        folder: None,
    },
];

/// The program to run in a terminal: `command` and its arguments, preceded by
/// `env` and what it sets, so that a terminal that is one process for every
/// window (the program that serves them) still gives the command its own
/// environment.
fn terminal_line(
    program: &Path,
    arguments: &[String],
    variables: &[(String, String)],
) -> Vec<String> {
    let mut line = Vec::new();
    if !variables.is_empty() {
        line.push("env".to_string());
        line.extend(
            variables
                .iter()
                .map(|(name, value)| format!("{name}={value}")),
        );
    }
    line.push(program.to_string_lossy().into_owned());
    line.extend(arguments.iter().cloned());
    line
}

/// The command that starts `terminal` on `line`, in `folder`.
fn terminal_command(
    terminal: &Terminal,
    program: &OsStr,
    line: &[String],
    folder: Option<&str>,
) -> Command {
    let mut command = Command::new(program);
    if let (Some(folder), Some((option, joined))) = (folder, terminal.folder) {
        if joined {
            command.arg(format!("{option}={folder}"));
        } else {
            command.arg(option).arg(folder);
        }
    }
    command.args(terminal.run).args(line);
    if let Some(folder) = folder {
        command.current_dir(folder);
    }
    command
}

/// Runs `line` in a terminal window of the person's: `$TERMINAL` if it is
/// one of those known, else the first of them found on `path`.
fn in_a_terminal(
    path: Option<&str>,
    preferred: Option<&str>,
    line: &[String],
    folder: Option<&str>,
    mut start: impl FnMut(&mut Command) -> bool,
) -> bool {
    let find = |program: &str| -> Option<PathBuf> {
        path.unwrap_or("")
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join(program))
            .find(|candidate| candidate.is_file())
    };
    let preferred = preferred
        .and_then(|p| Path::new(p).file_name())
        .and_then(|name| name.to_str())
        .and_then(|name| TERMINALS.iter().find(|t| t.program == name));
    preferred
        .into_iter()
        .chain(TERMINALS.iter())
        .any(|terminal| {
            find(terminal.program).is_some_and(|program| {
                start(&mut terminal_command(
                    terminal,
                    program.as_os_str(),
                    line,
                    folder,
                ))
            })
        })
}

fn spawn_terminal(line: &[String], folder: Option<&str>) -> bool {
    in_a_terminal(
        std::env::var("PATH").ok().as_deref(),
        std::env::var("TERMINAL").ok().as_deref(),
        line,
        folder,
        |command| match command.spawn() {
            Ok(mut child) => {
                // Reaped when the window closes, so it does not linger as a zombie.
                std::thread::spawn(move || child.wait());
                true
            }
            Err(_) => false,
        },
    )
}

/// Another agent's command in a terminal of its own.
pub fn agent_in_terminal(executable: &Path, arguments: &[String], folder: Option<&str>) -> bool {
    spawn_terminal(&terminal_line(executable, arguments, &[]), folder)
}

pub fn codex_in_terminal(arguments: &[String], folder: Option<&str>) -> bool {
    crate::agent_usage::codex_executable()
        .is_some_and(|executable| agent_in_terminal(&executable, arguments, folder))
}

/// `claude` in a terminal of its own, with this config folder.
pub fn in_terminal(arguments: &[String], config_home: Option<&str>, folder: Option<&str>) -> bool {
    let Some(executable) = executable() else {
        return false;
    };
    let variables: Vec<(String, String)> = user()
        .and_then(|user| config_variable(config_home, &user))
        .map(|value| vec![(super::super::registry::CONFIG_VARIABLE.to_owned(), value)])
        .unwrap_or_default();
    spawn_terminal(&terminal_line(&executable, arguments, &variables), folder)
}

// MARK: Attaching

/// A pseudo-terminal: its master end for us, its slave end for the program.
fn pty(rows: u16, columns: u16) -> Option<(OwnedFd, OwnedFd)> {
    // SAFETY: plain libc calls on descriptors this function creates and owns.
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        if master < 0 {
            return None;
        }
        let master = OwnedFd::from_raw_fd(master);
        if libc::grantpt(master.as_raw_fd()) != 0 || libc::unlockpt(master.as_raw_fd()) != 0 {
            return None;
        }
        let mut name = [0 as libc::c_char; 128];
        if libc::ptsname_r(master.as_raw_fd(), name.as_mut_ptr(), name.len()) != 0 {
            return None;
        }
        let slave = libc::open(
            CStr::from_ptr(name.as_ptr()).as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY,
        );
        if slave < 0 {
            return None;
        }
        let slave = OwnedFd::from_raw_fd(slave);
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &size);
        Some((master, slave))
    }
}

/// Attaches through `claude attach` on a pseudo-terminal nobody sees, once
/// its screen has settled does `work`, and leaves. None when it could not
/// attach; else whether the work went through, and the screen as it first
/// painted.
fn attached_to(
    executable: &Path,
    job: &str,
    config_home: Option<&str>,
    work: impl FnOnce(&mut dyn FnMut(&[u8]) -> bool) -> bool,
) -> Option<(bool, Vec<u8>)> {
    if !is_job_id(job) {
        return None;
    }
    let (master, slave) = pty(50, 200)?;
    let mut attach = command(
        executable,
        &["attach".to_string(), job.to_string()],
        config_home,
        None,
    );
    attach
        .stdin(Stdio::from(slave.try_clone().ok()?))
        .stdout(Stdio::from(slave.try_clone().ok()?))
        .stderr(Stdio::from(slave));
    // SAFETY: only async-signal-safe calls between fork and exec: a new session
    // whose controlling terminal is the pseudo-terminal on standard input.
    unsafe {
        attach.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            libc::ioctl(0, libc::TIOCSCTTY as _, 0);
            Ok(())
        });
    }
    let mut child = attach.spawn().ok()?;
    // Its output must be read for it to go on, and tells when it is ready.
    let seen: Arc<Mutex<(Vec<u8>, Option<Instant>)>> = Arc::new(Mutex::new((Vec::new(), None)));
    let reader = seen.clone();
    let mut output = std::fs::File::from(master.try_clone().ok()?);
    std::thread::spawn(move || {
        let mut buffer = [0u8; 1 << 15];
        // The read fails when the program has left.
        while let Ok(count) = output.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let mut state = reader.lock().unwrap();
            if state.0.len() < 1 << 21 {
                state.0.extend_from_slice(&buffer[..count]);
            }
            state.1 = Some(Instant::now());
        }
    });
    let mut input = std::fs::File::from(master);
    let running = |child: &mut Child| matches!(child.try_wait(), Ok(None));
    // The first output and then a quiet moment. A screen that keeps moving,
    // as a working session's does, counts as ready when the time is up.
    let limit = Instant::now() + Duration::from_secs(6);
    let mut printed = false;
    while Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(100));
        let last = seen.lock().unwrap().1;
        printed = last.is_some();
        if last.is_some_and(|at| at.elapsed() >= Duration::from_millis(500)) {
            break;
        }
    }
    if !printed || !running(&mut child) {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let screen = seen.lock().unwrap().0.clone();
    let mut send = |bytes: &[u8]| {
        running(&mut child) && input.write_all(bytes).is_ok() && input.flush().is_ok()
    };
    let done = work(&mut send);
    let _ = send(&[DETACH]);
    let leave = Instant::now() + Duration::from_secs(3);
    while Instant::now() < leave && running(&mut child) {
        std::thread::sleep(Duration::from_millis(50));
    }
    if running(&mut child) {
        let _ = child.kill();
    }
    let _ = child.wait();
    Some((done, screen))
}

fn attached(
    job: &str,
    config_home: Option<&str>,
    work: impl FnOnce(&mut dyn FnMut(&[u8]) -> bool) -> bool,
) -> Option<(bool, Vec<u8>)> {
    attached_to(&executable()?, job, config_home, work)
}

fn reply_to(
    executable: &Path,
    text: &str,
    images: &[String],
    job: &str,
    config_home: Option<&str>,
) -> bool {
    attached_to(executable, job, config_home, |send| {
        keystrokes(text, images).into_iter().all(|(bytes, pause)| {
            let sent = send(&bytes);
            if sent {
                std::thread::sleep(Duration::from_millis(pause));
            }
            sent
        })
    })
    .is_some_and(|(done, _)| done)
}

pub fn reply(text: &str, images: &[String], job: &str, config_home: Option<&str>) -> bool {
    executable().is_some_and(|executable| reply_to(&executable, text, images, job, config_home))
}

pub fn screen(job: &str, config_home: Option<&str>) -> Option<String> {
    attached(job, config_home, |_| true).map(|(_, screen)| screen_text(&screen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-bg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A program written as a script, ready to run.
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/bash\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Another test's fork can hold the file open for a moment ("text file busy").
        for _ in 0..200 {
            if Command::new(&path).arg("--openbar-probe").output().is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        path
    }

    #[test]
    fn claude_is_found_on_the_path_then_where_installers_put_it() {
        let home = Path::new("/home/me");
        let exists = |paths: &'static [&'static str]| {
            move |p: &Path| paths.iter().any(|x| Path::new(x) == p)
        };
        assert_eq!(
            find_claude(
                Some("/usr/bin:/opt/bin"),
                home,
                exists(&["/opt/bin/claude"])
            ),
            Some(PathBuf::from("/opt/bin/claude"))
        );
        assert_eq!(
            find_claude(None, home, exists(&["/home/me/.local/bin/claude"])),
            Some(PathBuf::from("/home/me/.local/bin/claude"))
        );
        assert_eq!(
            find_claude(
                Some(""),
                home,
                exists(&["/home/me/.claude/local/claude", "/usr/local/bin/claude"])
            ),
            Some(PathBuf::from("/home/me/.claude/local/claude"))
        );
        assert_eq!(find_claude(Some("/usr/bin"), home, exists(&[])), None);
    }

    #[test]
    fn a_command_is_run_without_a_window_and_what_it_printed_is_taken() {
        let dir = temp("run");
        let program = script(
            &dir,
            "claude",
            "[ \"$1\" = --openbar-probe ] && exit 0\necho \"out $*\"\necho \"err $CLAUDE_CONFIG_DIR\" >&2\n[ \"$1\" = fail ] && exit 3\n[ \"$1\" = hang ] && sleep 30\nexit 0\n",
        );
        let ok = run_claude(
            &program,
            &["agents".into(), "--json".into()],
            Some("/data/other"),
            None,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(ok.ok);
        assert_eq!(String::from_utf8_lossy(&ok.out).trim(), "out agents --json");
        // A config folder other than the standard one reaches the command.
        assert_eq!(ok.error, "err /data/other");
        let failed = run_claude(
            &program,
            &["fail".into()],
            None,
            None,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(!failed.ok);
        // One that does not finish is given up on.
        let started = Instant::now();
        assert!(
            run_claude(
                &program,
                &["hang".into()],
                None,
                None,
                Duration::from_millis(300)
            )
            .is_none()
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(
            run_claude(
                &dir.join("missing"),
                &[],
                None,
                None,
                Duration::from_secs(1)
            )
            .is_none()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn what_a_terminal_is_asked_depends_on_the_terminal() {
        let line = terminal_line(
            Path::new("/home/me/.local/bin/claude"),
            &["--resume".into(), "abc".into()],
            &[("CLAUDE_CONFIG_DIR".into(), "/data/other".into())],
        );
        assert_eq!(
            line,
            [
                "env",
                "CLAUDE_CONFIG_DIR=/data/other",
                "/home/me/.local/bin/claude",
                "--resume",
                "abc"
            ]
        );
        let args = |terminal: &str| -> Vec<String> {
            let terminal = TERMINALS.iter().find(|t| t.program == terminal).unwrap();
            let command = terminal_command(
                terminal,
                OsStr::new(terminal.program),
                &line[2..],
                Some("/work/app"),
            );
            command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            args("gnome-terminal"),
            [
                "--working-directory=/work/app",
                "--",
                "/home/me/.local/bin/claude",
                "--resume",
                "abc"
            ]
        );
        assert_eq!(
            args("konsole"),
            [
                "--workdir",
                "/work/app",
                "-e",
                "/home/me/.local/bin/claude",
                "--resume",
                "abc"
            ]
        );
        assert_eq!(args("kitty")[..2], ["--directory", "/work/app"]);
        assert_eq!(args("wezterm")[..2], ["start", "--"]);
        // One with no folder option is started in the folder instead.
        assert_eq!(args("xterm")[0], "-e");
        let none = terminal_line(Path::new("/x/claude"), &[], &[]);
        assert_eq!(none, ["/x/claude"]);
    }

    #[test]
    fn the_first_terminal_found_runs_the_command_and_a_preferred_one_goes_first() {
        let dir = temp("terminal");
        for name in ["xterm", "kitty", "konsole"] {
            std::fs::write(dir.join(name), "").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let path = dir.to_string_lossy().into_owned();
        let line = vec!["/x/claude".to_string(), "--flag".to_string()];
        let tried = std::cell::RefCell::new(Vec::new());
        // Nothing preferred: the first of the known ones that is there (konsole before kitty before xterm).
        assert!(in_a_terminal(Some(&path), None, &line, None, |c| {
            tried
                .borrow_mut()
                .push(c.get_program().to_string_lossy().into_owned());
            true
        }));
        assert!(
            tried.borrow()[0].ends_with("/konsole"),
            "{:?}",
            tried.borrow()
        );
        // $TERMINAL names kitty: it goes first.
        tried.borrow_mut().clear();
        assert!(in_a_terminal(
            Some(&path),
            Some("/usr/bin/kitty"),
            &line,
            None,
            |c| {
                tried
                    .borrow_mut()
                    .push(c.get_program().to_string_lossy().into_owned());
                true
            }
        ));
        assert!(tried.borrow()[0].ends_with("/kitty"));
        // One that cannot be started leaves the next to try.
        tried.borrow_mut().clear();
        assert!(in_a_terminal(Some(&path), None, &line, None, |c| {
            tried
                .borrow_mut()
                .push(c.get_program().to_string_lossy().into_owned());
            tried.borrow().len() > 1
        }));
        assert_eq!(tried.borrow().len(), 2);
        // None found: false, and nothing started.
        assert!(!in_a_terminal(
            Some("/nonexistent"),
            None,
            &line,
            None,
            |_| true
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A `claude` whose `attach` paints a screen, then logs every byte it is typed as hex until the detach key.
    fn attach_script(dir: &Path, log: &Path) -> PathBuf {
        script(
            dir,
            "claude",
            &format!(
                "[ \"$1\" = --openbar-probe ] && exit 0\nif [ \"$1\" = attach ]; then\n  stty raw -echo\n  printf 'Session %s\\r\\n> ready' \"$2\"\n  : > {log}\n  while true; do\n    byte=$(dd bs=1 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n')\n    [ -z \"$byte\" ] && exit 0\n    printf '%s ' \"$byte\" >> {log}\n    [ \"$byte\" = 1a ] && exit 0\n  done\nfi\n",
                log = log.display()
            ),
        )
    }

    #[test]
    fn a_reply_is_typed_into_the_session_and_the_session_is_left() {
        let dir = temp("reply");
        let log = dir.join("typed");
        let program = attach_script(&dir, &log);
        assert!(reply_to(&program, "hi", &[], "abc12345", None));
        let typed = std::fs::read_to_string(&log).unwrap();
        // The text as a paste (ESC [ 200 ~ ... ESC [ 201 ~), the return, then the detach key.
        assert_eq!(
            typed.trim(),
            "1b 5b 32 30 30 7e 68 69 1b 5b 32 30 31 7e 0d 1a",
            "{typed}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_screen_of_a_session_is_read_and_a_bad_job_id_is_refused() {
        let dir = temp("screen");
        let log = dir.join("typed");
        let program = attach_script(&dir, &log);
        let (done, screen) = attached_to(&program, "abc12345", None, |_| true).unwrap();
        assert!(done);
        let text = screen_text(&screen);
        assert!(
            text.contains("Session abc12345") && text.contains("> ready"),
            "{text:?}"
        );
        // Only a job id is attached to: nothing else reaches the command line.
        for bad in ["", "../x", "a b", "a;b", "x\n"] {
            assert!(
                attached_to(&program, bad, None, |_| true).is_none(),
                "{bad:?}"
            );
        }
        // A program that is not there, or that prints nothing, does not attach.
        assert!(attached_to(&dir.join("missing"), "abc12345", None, |_| true).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "runs this machine's own `claude agents` (it only lists)"]
    fn this_machines_claude_lists_its_background_agents() {
        let output = claude(
            &["agents".into(), "--json".into(), "--all".into()],
            None,
            None,
            Duration::from_secs(30),
        )
        .expect("claude did not answer in time");
        println!(
            "ok={} {} bytes, error {:?}",
            output.ok,
            output.out.len(),
            output.error
        );
        let jobs = super::super::decode(&output.out, None);
        println!("decoded: {:?}", jobs.as_ref().map(Vec::len));
        assert!(output.ok);
        assert!(
            jobs.is_some(),
            "the list did not decode: {}",
            String::from_utf8_lossy(&output.out)
        );
    }
}
