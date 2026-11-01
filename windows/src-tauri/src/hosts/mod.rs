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
pub mod bridge_installer;
#[cfg(windows)]
pub mod console;
pub mod process;
pub mod wezterm;

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
    /// A pane of WezTerm, by the id and socket WezTerm put in the pane's
    /// environment.
    Wezterm {
        gui_pid: u32,
        pane: u32,
        socket: String,
    },
    /// Some other terminal owns the session (no replies, Open only raises
    /// it), named so the island can say which.
    App {
        name: String,
    },
    Unknown,
}

impl Host {
    pub fn accepts_replies(&self) -> bool {
        matches!(
            self,
            Host::WindowsTerminal { .. }
                | Host::Vscode { .. }
                | Host::Conhost
                | Host::Wezterm { .. }
        )
    }

    /// The app as the person knows it, for "Open in …" and the notice
    /// that replies can't reach it.
    pub fn display_name(&self) -> Option<String> {
        match self {
            Host::WindowsTerminal { .. } => Some("Windows Terminal".into()),
            Host::Vscode { ide, .. } => Some(
                match ide.as_str() {
                    "vscode" => "Visual Studio Code",
                    "cursor" => "Cursor",
                    "antigravity" => "Antigravity",
                    "windsurf" => "Windsurf",
                    other => other,
                }
                .into(),
            ),
            Host::Wezterm { .. } => Some("WezTerm".into()),
            Host::App { name } => Some(name.clone()),
            Host::Conhost | Host::Unknown => None,
        }
    }
}

/// Terminals the island can only bring forward, by their program's name.
/// WezTerm is here too: without the variables of its pane it is one of them.
const TERMINALS: [(&str, &str); 9] = [
    ("wezterm-gui.exe", "WezTerm"),
    ("mintty.exe", "Git Bash"),
    ("alacritty.exe", "Alacritty"),
    ("warp.exe", "Warp"),
    ("tabby.exe", "Tabby"),
    ("hyper.exe", "Hyper"),
    ("conemu64.exe", "ConEmu"),
    ("conemu.exe", "ConEmu"),
    ("rio.exe", "Rio"),
];

/// A WezTerm pane, when the session's own environment names one; WezTerm
/// sets both variables in every pane it starts.
pub fn wezterm_host(
    gui_pid: u32,
    environment: &std::collections::HashMap<String, String>,
) -> Option<Host> {
    let pane = environment.get(wezterm::PANE)?.trim().parse().ok()?;
    let socket = environment
        .get(wezterm::SOCKET)
        .filter(|s| !s.trim().is_empty())?;
    Some(Host::Wezterm {
        gui_pid,
        pane,
        socket: socket.clone(),
    })
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
    let ancestors: Vec<u32> = chain
        .iter()
        .take_while(|e| {
            !matches!(
                e.name.to_ascii_lowercase().as_str(),
                "explorer.exe" | "services.exe" | "svchost.exe"
            )
        })
        .map(|e| e.pid)
        .collect();
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
        if let Some((_, terminal)) = TERMINALS.iter().find(|(exe, _)| *exe == name) {
            return Located {
                host: Host::App {
                    name: (*terminal).into(),
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
    let mut located = classify(&chain);
    // WezTerm is reached by the pane the session's environment names.
    if located.host
        == (Host::App {
            name: "WezTerm".into(),
        })
        && let Some(gui) = chain
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case("wezterm-gui.exe"))
        && let Some(host) = wezterm_host(
            gui.pid,
            &process::environment_values(pid, &[wezterm::PANE, wezterm::SOCKET]),
        )
    {
        located.host = host;
    }
    Some(located)
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
        Host::Wezterm {
            gui_pid,
            pane,
            socket,
        } => wezterm::send(*gui_pid, *pane, socket, text, images)
            .map_err(|_| DeliveryError::Unreachable),
        _ => Err(DeliveryError::Unreachable),
    }
}

/// Brings the session's window forward.
pub fn focus(pid: u32, located: &Located, cwd: &str) -> bool {
    match &located.host {
        Host::Vscode { .. } => {
            let _ = bridge::focus(&located.ancestors);
            // Electron's nearest ancestor is usually a pty/utility process,
            // not the process owning the editor's top-level windows.
            platform_raise_host(&located.ancestors, cwd)
        }
        Host::WindowsTerminal { .. } | Host::Conhost => {
            console_focus(pid) || platform_raise_host(&located.ancestors, cwd)
        }
        Host::Wezterm {
            gui_pid,
            pane,
            socket,
        } => {
            let _ = wezterm::focus(*gui_pid, *pane, socket);
            platform_raise_host(&located.ancestors, cwd)
        }
        // A terminal the island can't type into still comes forward.
        Host::App { .. } => platform_raise_host(&located.ancestors, cwd),
        Host::Unknown => false,
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
fn platform_raise_host(pids: &[u32], cwd: &str) -> bool {
    console::raise_host_window(pids, cwd)
}

/// Like the Mac's codexTerminal: find an interactive CLI in this session's
/// directory. Daemons, exec workers and another session's terminal are not
/// candidates. An explicit `resume <id>` takes precedence over directory.
pub fn focus_codex(session: &str, cwd: &str, thread_name: &str, others: &[String]) -> bool {
    let terminals: Vec<CodexTerminal> = process::pids_named("codex.exe")
        .into_iter()
        .filter_map(|pid| {
            let metadata = process::metadata(pid)?;
            let args = arguments(&metadata.command_line);
            if !interactive_codex(&args) {
                return None;
            }
            let located = locate(pid).filter(|l| l.host.accepts_replies())?;
            Some(CodexTerminal {
                pid,
                located,
                cwd: metadata.cwd,
                thread: resumed_thread(&args),
            })
        })
        .collect();
    if let Some(index) = codex_terminal(session, cwd, &terminals, others) {
        let terminal = &terminals[index];
        #[cfg(windows)]
        if matches!(terminal.located.host, Host::WindowsTerminal { .. }) {
            return console::select_terminal_tab(&terminal.located.ancestors, thread_name)
                || console::raise_session_window(&terminal.located.ancestors, thread_name);
        }
        return focus(terminal.pid, &terminal.located, cwd);
    }
    #[cfg(windows)]
    return console::raise_session_window(
        &terminals
            .iter()
            .flat_map(|t| t.located.ancestors.iter().copied())
            .collect::<Vec<_>>(),
        thread_name,
    );
    #[cfg(not(windows))]
    {
        let _ = thread_name;
        false
    }
}

struct CodexTerminal {
    pid: u32,
    located: Located,
    cwd: String,
    thread: Option<String>,
}

fn normalized_folder(folder: &str) -> String {
    folder
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn codex_terminal(
    session: &str,
    cwd: &str,
    terminals: &[CodexTerminal],
    others: &[String],
) -> Option<usize> {
    let exact: Vec<_> = terminals
        .iter()
        .enumerate()
        .filter(|(_, t)| t.thread.as_deref() == Some(session))
        .collect();
    if exact.len() == 1 {
        return Some(exact[0].0);
    }
    if !exact.is_empty() {
        return None;
    }
    let wanted = normalized_folder(cwd);
    if wanted.is_empty() {
        return None;
    }
    let eligible = |t: &CodexTerminal| t.thread.is_none();
    let here: Vec<_> = terminals
        .iter()
        .enumerate()
        .filter(|(_, t)| eligible(t) && normalized_folder(&t.cwd) == wanted)
        .collect();
    if here.len() == 1 {
        return Some(here[0].0);
    }
    if !here.is_empty() || terminals.len() != 1 || !eligible(&terminals[0]) {
        return None;
    }
    let only = normalized_folder(&terminals[0].cwd);
    (!others.iter().any(|cwd| normalized_folder(cwd) == only)).then_some(0)
}

fn interactive_codex(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "--version") {
        return false;
    }
    let Some(index) = codex_command(args) else {
        return true;
    };
    !matches!(
        args[index].as_str(),
        "app-server"
            | "exec"
            | "e"
            | "review"
            | "mcp-server"
            | "mcp"
            | "login"
            | "logout"
            | "completion"
            | "sandbox"
            | "debug"
            | "apply"
            | "cloud"
            | "features"
    )
}

fn codex_command(args: &[String]) -> Option<usize> {
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        if matches!(
            arg.as_str(),
            "-c" | "--config"
                | "-m"
                | "--model"
                | "-p"
                | "--profile"
                | "-C"
                | "--cd"
                | "--add-dir"
                | "-s"
                | "--sandbox"
                | "-a"
                | "--ask-for-approval"
                | "-i"
                | "--image"
                | "--enable"
                | "--disable"
        ) {
            index += 2;
        } else if arg.starts_with('-') {
            index += 1;
        } else {
            return Some(index);
        }
    }
    None
}

fn resumed_thread(args: &[String]) -> Option<String> {
    let index = codex_command(args)?;
    // Fork creates a new thread, so its source id cannot identify the UI.
    if args[index] != "resume" {
        return None;
    }
    args.get(index + 1)
        .filter(|id| {
            !id.starts_with('-')
                && id.len() == 36
                && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        })
        .cloned()
}

#[cfg(windows)]
fn arguments(command: &str) -> Vec<String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::UI::Shell::CommandLineToArgvW;
    use windows::core::PCWSTR;
    let wide: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let mut count = 0;
    unsafe {
        let argv = CommandLineToArgvW(PCWSTR(wide.as_ptr()), &mut count);
        if argv.is_null() {
            return Vec::new();
        }
        let result = std::slice::from_raw_parts(argv, count.max(0) as usize)
            .iter()
            .map(|arg| arg.to_string().unwrap_or_default())
            .collect();
        let _ = LocalFree(Some(HLOCAL(argv.cast())));
        result
    }
}

#[cfg(not(windows))]
fn arguments(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_owned).collect()
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
fn platform_raise_host(_pids: &[u32], _cwd: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::process::Entry;
    use super::*;

    fn terminal(pid: u32, cwd: &str, thread: Option<&str>) -> CodexTerminal {
        CodexTerminal {
            pid,
            cwd: cwd.into(),
            thread: thread.map(str::to_owned),
            located: Located {
                host: Host::Conhost,
                ancestors: vec![pid],
            },
        }
    }

    #[test]
    fn codex_background_modes_are_not_interactive_terminals() {
        for mode in [
            "app-server --listen unix://",
            "app-server daemon pid-update-loop",
            "exec run",
            "mcp-server",
            "--help",
        ] {
            let args = arguments(&format!("codex.exe {mode}"));
            assert!(!interactive_codex(&args), "{mode}");
        }
        assert!(interactive_codex(&arguments("codex.exe")));
        assert!(interactive_codex(&arguments("codex.exe --model test")));
        assert!(interactive_codex(&arguments("codex.exe --model review")));
        assert!(!interactive_codex(&arguments(
            "codex.exe -C C:/work app-server"
        )));
        assert!(!interactive_codex(&[]));
    }

    #[test]
    fn codex_selects_the_session_folder_among_several_terminals() {
        let terminals = [
            terminal(1, r"C:\work\other", None),
            terminal(2, r"c:/WORK/app/", None),
        ];
        assert_eq!(
            codex_terminal("s", r"C:\work\app", &terminals, &[]),
            Some(1)
        );
        assert_eq!(codex_terminal("s", r"C:\missing", &terminals, &[]), None);
    }

    #[test]
    fn codex_does_not_guess_between_terminals_in_one_folder() {
        let terminals = [terminal(1, r"C:\work", None), terminal(2, r"C:\work", None)];
        assert_eq!(codex_terminal("s", r"C:\work", &terminals, &[]), None);
    }

    #[test]
    fn explicit_resume_identifies_the_thread_and_excludes_a_different_thread() {
        let terminals = [
            terminal(1, r"C:\work", Some("other")),
            terminal(2, r"C:\work", Some("wanted")),
        ];
        assert_eq!(
            codex_terminal("wanted", r"C:\work", &terminals, &[]),
            Some(1)
        );
        assert_eq!(codex_terminal("absent", r"C:\work", &terminals, &[]), None);
        let id = "019a1d50-1234-7890-abcd-123456789abc";
        assert_eq!(
            resumed_thread(&arguments(&format!("codex.exe resume {id}"))),
            Some(id.into())
        );
        assert_eq!(
            resumed_thread(&arguments(&format!("codex.exe fork {id}"))),
            None
        );
        assert_eq!(resumed_thread(&arguments("codex.exe resume --last")), None);
    }

    #[test]
    fn sole_terminal_fallback_is_not_borrowed_from_another_session() {
        let terminals = [terminal(1, r"C:\other", None)];
        assert_eq!(
            codex_terminal("s", r"C:\work", &terminals, &[r"c:/OTHER/".into()]),
            None
        );
        assert_eq!(codex_terminal("s", r"C:\work", &terminals, &[]), Some(0));
        assert_eq!(codex_terminal("s", "", &terminals, &[]), None);
    }

    #[test]
    #[cfg(windows)]
    fn process_parameters_read_the_real_current_directory() {
        let metadata = process::metadata(std::process::id()).expect("read own process parameters");
        assert_eq!(
            normalized_folder(&metadata.cwd),
            normalized_folder(&std::env::current_dir().unwrap().to_string_lossy())
        );
        assert!(!metadata.command_line.is_empty());
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "Raises the live session process selected with VORSSAINT_TEST_FOCUS_PID"]
    fn live_session_open_raises_its_host() {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let pid: u32 = std::env::var("VORSSAINT_TEST_FOCUS_PID")
            .expect("set the target pid")
            .parse()
            .unwrap();
        let metadata = process::metadata(pid).expect("process parameters");
        let located = locate(pid).expect("session ancestors");
        eprintln!("Host: {:?}; cwd: {}", located, metadata.cwd);
        if matches!(located.host, Host::Vscode { .. }) {
            assert!(
                bridge::focus(&located.ancestors).is_ok(),
                "IDE bridge must select the session's own terminal"
            );
        }
        assert!(
            focus(pid, &located, &metadata.cwd),
            "session host did not activate"
        );
        let mut owner = 0;
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut owner));
        }
        assert!(
            located.ancestors.contains(&owner),
            "foreground window belongs to {owner}"
        );
    }

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
    fn other_terminals_are_named_and_only_raised() {
        let l = classify(&chain(&[
            "claude.exe",
            "bash.exe",
            "mintty.exe",
            "explorer.exe",
        ]));
        assert_eq!(
            l.host,
            Host::App {
                name: "Git Bash".into()
            }
        );
        assert!(!l.host.accepts_replies());
        assert_eq!(l.host.display_name().as_deref(), Some("Git Bash"));
        let l = classify(&chain(&["claude.exe", "pwsh.exe", "Alacritty.exe"]));
        assert_eq!(
            l.host,
            Host::App {
                name: "Alacritty".into()
            }
        );
        // Git Bash inside Windows Terminal is Windows Terminal's.
        let l = classify(&chain(&[
            "claude.exe",
            "bash.exe",
            "OpenConsole.exe",
            "WindowsTerminal.exe",
        ]));
        assert!(matches!(l.host, Host::WindowsTerminal { .. }));
    }

    #[test]
    fn a_wezterm_pane_comes_from_the_session_environment() {
        let l = classify(&chain(&["claude.exe", "pwsh.exe", "wezterm-gui.exe"]));
        assert_eq!(
            l.host,
            Host::App {
                name: "WezTerm".into()
            }
        );
        let mut environment = std::collections::HashMap::new();
        assert_eq!(wezterm_host(102, &environment), None);
        environment.insert(wezterm::PANE.to_owned(), "4".to_owned());
        assert_eq!(
            wezterm_host(102, &environment),
            None,
            "a pane needs the socket of its WezTerm"
        );
        environment.insert(
            wezterm::SOCKET.to_owned(),
            r"C:\Users\me\.local\share\wezterm\gui-sock-9".to_owned(),
        );
        let host = wezterm_host(102, &environment).unwrap();
        assert!(host.accepts_replies());
        assert_eq!(host.display_name().as_deref(), Some("WezTerm"));
        environment.insert(wezterm::PANE.to_owned(), "x".to_owned());
        assert_eq!(wezterm_host(102, &environment), None);
    }

    #[test]
    fn only_the_wanted_variables_are_read_from_an_environment_block() {
        let block: Vec<u16> = "=C:=C:\\work\0Path=C:\\bin\0wezterm_pane=12\0SECRET=x\0WEZTERM_UNIX_SOCKET=s=t\0\0ignored=1\0"
            .encode_utf16()
            .collect();
        let found = process::pick_environment(&block, &[wezterm::PANE, wezterm::SOCKET]);
        assert_eq!(found.len(), 2);
        assert_eq!(found.get(wezterm::PANE).map(String::as_str), Some("12"));
        assert_eq!(found.get(wezterm::SOCKET).map(String::as_str), Some("s=t"));
    }

    #[test]
    fn a_shell_from_explorer_is_a_console_window() {
        let l = classify(&chain(&["claude.exe", "cmd.exe", "explorer.exe"]));
        assert_eq!(l.host, Host::Conhost);
        assert!(l.host.accepts_replies());
        assert_eq!(classify(&chain(&["claude.exe"])).host, Host::Unknown);
    }
}
