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
    let hwnd = HWND(handle as _);
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
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

/// Raises the first visible top-level window whose title contains `text`
/// (Windows Terminal titles its window after the active tab).
pub fn raise_window_titled(text: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    struct Search {
        text: Vec<u16>,
        found: HWND,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let mut buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
        let title = &buf[..len];
        if !search.text.is_empty()
            && title
                .windows(search.text.len())
                .any(|w| w == search.text.as_slice())
        {
            search.found = hwnd;
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut search = Search {
        text: text.encode_utf16().collect(),
        found: HWND::default(),
    };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    raise(search.found.0 as isize)
}
