// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Newline-delimited JSON with a vendor CLI over its stdin and stdout
//! (AgentCLIConversation.swift), and where those CLIs live on Windows
//! (spec 03 §2.2, §3.2, §13.9). The app never reads their credentials: each
//! CLI finds its own sign-in.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Output beyond this ends the conversation.
const MAXIMUM_OUTPUT: usize = 4 << 20;

pub struct Conversation {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
    deadline: Instant,
}

impl Conversation {
    /// Starts `executable` headless: no console window, stderr dropped, run
    /// from the system drive's root (Mac: `/`).
    pub fn start(
        executable: &Path,
        args: &[&str],
        env: &[(String, String)],
        remove: &[&str],
        timeout: Duration,
    ) -> Option<Conversation> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in remove {
            command.env_remove(key);
        }
        command.envs(env.iter().map(|(k, v)| (k, v)));
        let root = std::env::var_os("SystemDrive")
            .map(|d| PathBuf::from(format!("{}\\", d.to_string_lossy())))
            .filter(|p| p.is_dir())
            .or_else(|| std::env::var_os("TEMP").map(PathBuf::from));
        if let Some(root) = root {
            command.current_dir(root);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console flashes for the check.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut total = 0usize;
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match (&mut reader)
                    .take((MAXIMUM_OUTPUT - total) as u64 + 1)
                    .read_until(b'\n', &mut buffer)
                {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        total += n;
                        if total > MAXIMUM_OUTPUT {
                            break;
                        }
                        if let Ok(value) = serde_json::from_slice::<Value>(&buffer)
                            && value.is_object()
                            && tx.send(value).is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        let stdin = child.stdin.take();
        Some(Conversation {
            child,
            stdin,
            lines,
            deadline: Instant::now() + timeout,
        })
    }

    pub fn send(&mut self, message: &Value) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        let mut text = message.to_string();
        text.push('\n');
        stdin
            .write_all(text.as_bytes())
            .and_then(|_| stdin.flush())
            .is_ok()
    }

    /// The next JSON object, until the conversation's one deadline.
    pub fn next(&mut self) -> Option<Value> {
        let left = self.deadline.checked_duration_since(Instant::now())?;
        self.lines.recv_timeout(left).ok()
    }

    /// Closes stdin and waits up to `grace` for the CLI to exit, then kills it.
    pub fn end(&mut self, grace: Duration) {
        self.stdin = None;
        let until = Instant::now() + grace;
        while Instant::now() < until {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // A .cmd shim's node.exe outlives cmd.exe; the whole tree goes.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000)
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.end(Duration::from_secs(2));
        }
    }
}

// MARK: Finding the CLIs

/// `PATH` as the app has it, then the user's and the machine's from the
/// registry, so a CLI installed after sign-in is still found.
pub fn search_path() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    #[cfg(windows)]
    for (root, key) in [
        (
            windows::Win32::System::Registry::HKEY_CURRENT_USER,
            "Environment",
        ),
        (
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
    ] {
        if let Some(value) = registry_string(root, key, "Path") {
            dirs.extend(std::env::split_paths(&value));
        }
    }
    let mut seen = Vec::<String>::new();
    dirs.retain(|d| {
        let key = d.to_string_lossy().to_lowercase();
        d.is_absolute() && !seen.contains(&key) && {
            seen.push(key);
            true
        }
    });
    dirs
}

#[cfg(windows)]
fn registry_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::HSTRING;
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let mut size = 0u32;
    unsafe {
        // REG_EXPAND_SZ values come back with their %VARIABLES% expanded.
        RegGetValueW(
            root,
            &key,
            &name,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        RegGetValueW(
            root,
            &key,
            &name,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

/// `PATHEXT`'s extensions, lowercase, the executable kinds first.
pub fn extensions() -> Vec<String> {
    let mut out: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_lowercase())
        .collect();
    out.retain(|e| matches!(e.as_str(), ".exe" | ".cmd" | ".bat" | ".com"));
    if out.is_empty() {
        out = vec![".exe".into(), ".cmd".into()];
    }
    out
}

/// The first existing file among `candidates`, never one inside `excluded`.
pub fn first_file(
    candidates: impl IntoIterator<Item = PathBuf>,
    excluded: &[&str],
) -> Option<PathBuf> {
    candidates.into_iter().find(|p| {
        let text = p.to_string_lossy().to_lowercase();
        p.is_file() && !excluded.iter().any(|x| text.contains(x))
    })
}

/// `name` with each of `extensions` in each of `dirs`.
pub fn in_dirs(dirs: &[PathBuf], name: &str, extensions: &[String]) -> Vec<PathBuf> {
    dirs.iter()
        .flat_map(|d| extensions.iter().map(move |e| d.join(format!("{name}{e}"))))
        .collect()
}

/// Node tool folders where npm-style CLIs land on Windows.
pub fn tool_dirs() -> Vec<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let mut out = Vec::new();
    if let Some(appdata) = env("APPDATA") {
        out.push(appdata.join("npm"));
    }
    if let Some(local) = env("LOCALAPPDATA") {
        out.push(local.join("pnpm"));
        out.push(local.join("Volta").join("bin"));
    }
    if let Some(home) = super::files::home() {
        out.push(home.join(".bun").join("bin"));
        out.push(home.join("scoop").join("shims"));
        out.push(home.join(".volta").join("bin"));
    }
    if let Some(volta) = env("VOLTA_HOME") {
        out.push(volta.join("bin"));
    }
    if let Some(nvm) = env("NVM_SYMLINK") {
        out.push(nvm);
    }
    out
}

/// The child's PATH: the executable's folder, then the search path and the tool folders.
pub fn child_path(executable: &Path) -> String {
    let mut dirs: Vec<PathBuf> = executable
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    dirs.extend(search_path());
    dirs.extend(tool_dirs());
    std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_skip_folders_that_are_excluded() {
        let dir = std::env::temp_dir().join(format!("vorssaint-cli-{}", std::process::id()));
        let app = dir.join("AnthropicClaude");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("claude.exe"), b"").unwrap();
        std::fs::write(dir.join("claude.cmd"), b"").unwrap();
        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        let found = first_file(
            in_dirs(&[app.clone(), dir.clone()], "claude", &exts),
            &["anthropicclaude"],
        );
        assert_eq!(found, Some(dir.join("claude.cmd")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_conversation_reads_json_lines_until_its_deadline() {
        let script = "read line; echo 'noise'; echo '{\"id\":1}'; sleep 5";
        let mut c = Conversation::start(
            Path::new("/bin/sh"),
            &["-c", script],
            &[],
            &[],
            Duration::from_millis(800),
        )
        .unwrap();
        assert!(c.send(&serde_json::json!({"hello": true})));
        assert_eq!(c.next().unwrap()["id"], 1);
        let started = Instant::now();
        assert!(c.next().is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        c.end(Duration::from_millis(100));
    }
}
