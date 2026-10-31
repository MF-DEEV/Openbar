// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Typing into a session's console and finding its window (Windows only).
//! The island runs itself as a helper (`--console <pid> <action>`) that
//! attaches to the claude process's console, so the island never shares a
//! console, nor the Ctrl+C a person presses in it. The reply goes in as a
//! bracketed paste, which keeps line breaks as text, then Return.

use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HWND, LPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleWindow, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT,
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, WriteConsoleInputW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GW_OWNER, GetAncestor, GetWindow, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{BOOL, w};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ESC: char = '\u{1b}';

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    Type {
        text: String,
        images: Vec<String>,
    },
    /// Prints the window that shows the console.
    Window,
}

/// Runs the helper for claude process `pid` and returns what it printed.
pub fn run_helper(pid: u32, action: Action) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe)
        .args(["--console", &pid.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&serde_json::to_vec(&action).map_err(|e| e.to_string())?);
    }
    let images = match &action {
        Action::Type { images, .. } => images.len(),
        Action::Window => 0,
    };
    let deadline = Instant::now() + Duration::from_secs_f64(6.0 + 1.5 * images as f64);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let mut out = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_string(&mut out);
            }
            return if status.success() {
                Ok(out)
            } else {
                Err(format!("helper exited with {status}"))
            };
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            return Err("helper timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The helper's own entry point: `Vorssaint.exe --console <pid>`, the action as JSON on stdin.
pub fn helper_main(pid: u32) -> ! {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let code = match serde_json::from_slice::<Action>(&input) {
        Ok(action) => match helper(pid, action) {
            Ok(out) => {
                print!("{out}");
                let _ = std::io::stdout().flush();
                0
            }
            Err(_) => 1,
        },
        Err(_) => 2,
    };
    std::process::exit(code)
}

fn helper(pid: u32, action: Action) -> windows::core::Result<String> {
    unsafe {
        let _ = FreeConsole();
        AttachConsole(pid)?;
        let result = match action {
            Action::Window => {
                let console = GetConsoleWindow();
                // Under Windows Terminal the console's window is a hidden
                // pseudo-window owned by the terminal's own window.
                let root = GetAncestor(console, GA_ROOTOWNER);
                let window = if root.0.is_null() { console } else { root };
                Ok((window.0 as isize).to_string())
            }
            Action::Type { text, images } => type_reply(&text, &images).map(|_| String::new()),
        };
        let _ = FreeConsole();
        result
    }
}

/// Electron utility processes own no windows. Search the complete verified
/// ancestor chain, then prefer the window naming the deepest session folder.
pub fn raise_host_window(pids: &[u32], cwd: &str) -> bool {
    let windows = host_windows(pids);
    #[cfg(test)]
    eprintln!("Host windows: {windows:?}");
    if windows.len() == 1 {
        return raise(windows[0].0);
    }
    let hints: Vec<_> = std::path::Path::new(cwd)
        .ancestors()
        .filter_map(|p| p.file_name())
        .map(|p| p.to_string_lossy().to_lowercase())
        .collect();
    for hint in hints {
        let matches: Vec<_> = windows
            .iter()
            .filter(|(_, title)| title.contains(&hint))
            .collect();
        if matches.len() == 1 {
            return raise(matches[0].0);
        }
        if matches.len() > 1 {
            return false;
        }
    }
    false
}

/// Title fallback is confined to candidate hosts, never an arbitrary browser
/// or the island's own session title. Ambiguous titles are left alone.
pub fn raise_session_window(pids: &[u32], title: &str) -> bool {
    if title.trim().is_empty() {
        return false;
    }
    let title = title.to_lowercase();
    let windows = host_windows(pids);
    let matches: Vec<_> = windows.iter().filter(|(_, t)| t.contains(&title)).collect();
    matches.len() == 1 && raise(matches[0].0)
}

fn host_windows(pids: &[u32]) -> Vec<(isize, String)> {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    struct Search<'a> {
        pids: &'a [u32],
        windows: Vec<(isize, String)>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search<'_>) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if search.pids.contains(&owner)
            && unsafe { IsWindowVisible(hwnd) }.as_bool()
            && unsafe { GetWindow(hwnd, GW_OWNER) }.map_or(true, |o| o.0.is_null())
        {
            let mut buf = [0u16; 1024];
            let count = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
            search.windows.push((
                hwnd.0 as isize,
                String::from_utf16_lossy(&buf[..count]).to_lowercase(),
            ));
        }
        BOOL(1)
    }
    let mut search = Search {
        pids,
        windows: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search<'_> as isize));
    }
    search.windows
}

unsafe fn type_reply(text: &str, images: &[String]) -> windows::core::Result<()> {
    let input = unsafe {
        CreateFileW(
            w!("CONIN$"),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )?
    };
    let write = |records: &[INPUT_RECORD]| -> windows::core::Result<()> {
        let mut written = 0u32;
        let mut offset = 0;
        while offset < records.len() {
            unsafe { WriteConsoleInputW(input, &records[offset..], &mut written)? };
            if written == 0 {
                break;
            }
            offset += written as usize;
        }
        Ok(())
    };
    let paste = |content: &str| write(&records(&format!("{ESC}[200~{content}{ESC}[201~")));
    let result = (|| {
        if !images.is_empty() {
            // Claude Code attaches an image when a paste is only its path, and reads it a moment later.
            for image in images {
                paste(image)?;
                std::thread::sleep(Duration::from_millis(60));
            }
            std::thread::sleep(Duration::from_millis(1200 + 300 * images.len() as u64));
        }
        if !text.is_empty() {
            paste(text)?;
        }
        std::thread::sleep(Duration::from_millis(80));
        write(&key(VK_RETURN.0, '\r'))
    })();
    unsafe {
        let _ = CloseHandle(input);
    }
    result
}

/// Key down and up for every UTF-16 unit of `text`.
fn records(text: &str) -> Vec<INPUT_RECORD> {
    text.encode_utf16()
        .flat_map(|unit| key_unit(0, unit))
        .collect()
}

fn key(virtual_key: u16, ch: char) -> Vec<INPUT_RECORD> {
    key_unit(virtual_key, ch as u16).to_vec()
}

fn key_unit(virtual_key: u16, unit: u16) -> [INPUT_RECORD; 2] {
    let event = |down: bool| INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: BOOL::from(down),
                wRepeatCount: 1,
                wVirtualKeyCode: virtual_key,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                dwControlKeyState: 0,
            },
        },
    };
    [event(true), event(false)]
}

/// Restores and raises a window; allowed while the island handles the person's click.
pub fn raise(handle: isize) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, MSG, PM_NOREMOVE, PeekMessageW,
    };
    let hwnd = HWND(handle as _);
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }
        if SetForegroundWindow(hwnd).as_bool() || GetForegroundWindow() == hwnd {
            return true;
        }
        // Open runs on a worker thread. Temporarily join the foreground
        // input queue so Windows can transfer focus after the user's click.
        let current = GetCurrentThreadId();
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let target = GetWindowThreadProcessId(hwnd, None);
        let attached = current != foreground
            && foreground != 0
            && AttachThreadInput(current, foreground, true).as_bool();
        let target_attached = current != target
            && target != foreground
            && target != 0
            && AttachThreadInput(current, target, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let activated = SetForegroundWindow(hwnd).as_bool();
        #[cfg(test)]
        eprintln!(
            "Activate: current={current}, foreground={foreground}, target={target}, attached={attached}/{target_attached}, activated={activated}, foreground_window={:?}",
            GetForegroundWindow()
        );
        if target_attached {
            let _ = AttachThreadInput(current, target, false);
        }
        if attached {
            let _ = AttachThreadInput(current, foreground, false);
        }
        activated || GetForegroundWindow() == hwnd
    }
}

/// Raises the first visible top-level window of process `pid`.
pub fn raise_process_window(pid: u32) -> bool {
    struct Search {
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        let unowned = unsafe { GetWindow(hwnd, GW_OWNER) }.map_or(true, |o| o.0.is_null());
        if owner == search.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() && unowned {
            search.found = hwnd;
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut search = Search {
        pid,
        found: HWND::default(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    raise(search.found.0 as isize)
}

/// Windows Terminal's hidden console HWND cannot select a background tab.
/// Select the uniquely named tab through its native accessibility provider.
pub fn select_terminal_tab(pids: &[u32], title: &str) -> bool {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomationSelectionItemPattern, TreeScope_Descendants,
        UIA_ControlTypePropertyId, UIA_SelectionItemPatternId, UIA_TabItemControlTypeId,
    };
    let title = title.trim().to_lowercase();
    if title.is_empty() {
        return false;
    }
    let Ok(_apartment) = crate::com::Apartment::mta() else {
        return false;
    };
    let result = (|| -> windows::core::Result<bool> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
            let condition = automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &UIA_TabItemControlTypeId.0.into(),
            )?;
            let mut found = Vec::new();
            for (handle, _) in host_windows(pids) {
                let hwnd = HWND(handle as _);
                let root = automation.ElementFromHandle(hwnd)?;
                let tabs = root.FindAll(TreeScope_Descendants, &condition)?;
                for index in 0..tabs.Length()? {
                    let tab = tabs.GetElement(index)?;
                    if tab
                        .CurrentName()?
                        .to_string()
                        .to_lowercase()
                        .contains(&title)
                    {
                        found.push((handle, tab));
                    }
                }
            }
            if found.len() != 1 {
                return Ok(false);
            }
            let (handle, tab) = &found[0];
            let pattern = tab.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )?;
            pattern.Select()?;
            Ok(raise(*handle))
        }
    })();
    result.unwrap_or(false)
}
