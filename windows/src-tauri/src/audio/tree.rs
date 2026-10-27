// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Which app an audio session belongs to. Browsers and Electron apps play
//! from a helper process (Chrome's audio service, a WebView2 child), so a
//! session rolls up its process tree to the app that started it, the way the
//! Mac rolls up "responsible processes" (spec 05 §2.4). A parent only counts
//! when it started before its child, so a reused PID never adopts a session.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub parent: u32,
    /// The exe's full path; empty when the process could not be opened.
    pub exe: String,
    /// Creation time in 100 ns ticks (FILETIME).
    pub created: u64,
}

/// Processes that start apps without owning them: a session never rolls up
/// into one of these.
const LAUNCHERS: &[&str] = &[
    "explorer.exe",
    "svchost.exe",
    "services.exe",
    "wininit.exe",
    "winlogon.exe",
    "userinit.exe",
    "sihost.exe",
    "runtimebroker.exe",
    "dllhost.exe",
    "taskhostw.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "steam.exe",
];

/// Hosts that only ever play for the app that embeds them.
const HELPERS: &[&str] = &["msedgewebview2.exe", "cefsharp.browsersubprocess.exe", "crashpad_handler.exe"];

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase()
}

fn directory(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    match lower.rfind(['\\', '/']) {
        Some(i) => lower[..i].to_string(),
        None => String::new(),
    }
}

/// Whether `child`'s sound is really `parent`'s: the same exe, a known
/// embedding host, or a helper installed under the parent's own folder.
fn rolls_up(child: &str, parent: &str, windows_dir: &str) -> bool {
    if child.is_empty() || parent.is_empty() {
        return false;
    }
    let parent_name = file_name(parent);
    if LAUNCHERS.contains(&parent_name.as_str()) {
        return false;
    }
    if child.eq_ignore_ascii_case(parent) {
        return true;
    }
    let parent_dir = directory(parent);
    // A system folder holds every app's helpers, so it says nothing.
    if !windows_dir.is_empty() && parent_dir.starts_with(&windows_dir.to_ascii_lowercase()) {
        return false;
    }
    if HELPERS.contains(&file_name(child).as_str()) {
        return true;
    }
    let child_dir = directory(child);
    !parent_dir.is_empty() && (child_dir == parent_dir || child_dir.starts_with(&format!("{parent_dir}\\")))
}

/// The PID whose app owns `pid`'s sound.
pub fn root(pid: u32, procs: &HashMap<u32, Proc>, windows_dir: &str) -> u32 {
    let mut current = pid;
    // A tree deeper than this is a loop of reused PIDs.
    for _ in 0..16 {
        let Some(child) = procs.get(&current) else { break };
        let Some(parent) = procs.get(&child.parent) else { break };
        if child.parent == current || parent.created > child.created {
            break;
        }
        if !rolls_up(&child.exe, &parent.exe, windows_dir) {
            break;
        }
        current = child.parent;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: &str = r"C:\Windows";

    fn tree(entries: &[(u32, u32, &str, u64)]) -> HashMap<u32, Proc> {
        entries
            .iter()
            .map(|&(pid, parent, exe, created)| (pid, Proc { parent, exe: exe.into(), created }))
            .collect()
    }

    #[test]
    fn chrome_audio_service_rolls_up_to_the_browser() {
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        let procs = tree(&[(1, 0, r"C:\Windows\explorer.exe", 1), (10, 1, chrome, 5), (20, 10, chrome, 6)]);
        assert_eq!(root(20, &procs, WIN), 10);
    }

    #[test]
    fn webview2_rolls_up_to_its_host_app() {
        let procs = tree(&[
            (10, 1, r"C:\Users\me\AppData\Local\Teams\ms-teams.exe", 5),
            (20, 10, r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\msedgewebview2.exe", 6),
        ]);
        assert_eq!(root(20, &procs, WIN), 10);
    }

    #[test]
    fn a_helper_under_the_app_folder_rolls_up() {
        let procs = tree(&[
            (10, 1, r"C:\Program Files\App\App.exe", 5),
            (20, 10, r"C:\Program Files\App\bin\AppHelper.exe", 6),
        ]);
        assert_eq!(root(20, &procs, WIN), 10);
    }

    #[test]
    fn an_app_started_from_a_shell_or_terminal_stays_itself() {
        let procs = tree(&[
            (1, 0, r"C:\Windows\explorer.exe", 1),
            (5, 1, r"C:\Program Files\WindowsApps\Terminal\WindowsTerminal.exe", 2),
            (10, 5, r"C:\Program Files\mpv\mpv.exe", 3),
            (11, 1, r"C:\Program Files\Spotify\Spotify.exe", 3),
        ]);
        assert_eq!(root(10, &procs, WIN), 10);
        assert_eq!(root(11, &procs, WIN), 11);
    }

    #[test]
    fn an_unrelated_app_does_not_adopt_its_child() {
        let procs = tree(&[(10, 1, r"C:\Program Files\Launcher\launcher.exe", 5), (20, 10, r"D:\Games\game.exe", 6)]);
        assert_eq!(root(20, &procs, WIN), 20);
    }

    #[test]
    fn a_reused_parent_pid_is_not_a_parent() {
        let chrome = r"C:\Chrome\chrome.exe";
        // PID 10 died and was reused after the child started.
        let procs = tree(&[(10, 1, chrome, 9), (20, 10, chrome, 6)]);
        assert_eq!(root(20, &procs, WIN), 20);
    }

    #[test]
    fn system_folders_and_unknown_paths_never_roll_up() {
        let procs = tree(&[
            (10, 1, r"C:\Windows\System32\rundll32.exe", 5),
            (20, 10, r"C:\Windows\System32\helper.exe", 6),
            (30, 10, "", 6),
        ]);
        assert_eq!(root(20, &procs, WIN), 20);
        assert_eq!(root(30, &procs, WIN), 30);
    }

    #[test]
    fn loops_end() {
        let exe = r"C:\App\a.exe";
        let procs = tree(&[(10, 20, exe, 1), (20, 10, exe, 1)]);
        let r = root(10, &procs, WIN);
        assert!(r == 10 || r == 20);
        assert_eq!(root(99, &procs, WIN), 99);
    }
}
