// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Windows: finds Discord's message box through UI Automation and types with
//! SendInput. The checks themselves live in `discord_reply.rs`.

use super::{is_blank, matches_target, target};
use std::{mem::size_of, thread, time::Duration};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, TreeScope_Descendants, UIA_ControlTypePropertyId,
    UIA_EditControlTypeId, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
    VIRTUAL_KEY, VK_RETURN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

/// What the editor holds now, through ValuePattern or else TextPattern.
fn editor_text(editor: &IUIAutomationElement) -> Option<String> {
    unsafe {
        if let Ok(pattern) =
            editor.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            && let Ok(value) = pattern.CurrentValue()
        {
            return Some(value.to_string());
        }
        let pattern = editor
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()?;
        Some(pattern.DocumentRange().ok()?.GetText(-1).ok()?.to_string())
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn inject(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) } as usize;
    (sent == inputs.len())
        .then_some(())
        .ok_or_else(|| "inputRefused".into())
}

/// Types in chunks and checks before each one that Discord still has the
/// foreground, so a switch to another app mid-reply stops the rest going there.
fn type_text(window: HWND, text: &str) -> Result<(), String> {
    let mut typed = false;
    let mut flush = |inputs: &mut Vec<INPUT>| -> Result<(), String> {
        if unsafe { GetForegroundWindow() } != window {
            return Err(if typed {
                "typingInterrupted"
            } else {
                "focusLost"
            }
            .into());
        }
        inject(inputs)?;
        typed = true;
        inputs.clear();
        Ok(())
    };
    let mut inputs = Vec::with_capacity(128);
    for ch in text.chars() {
        if ch == '\n' {
            inputs.extend([
                key(VK_SHIFT, false),
                key(VK_RETURN, false),
                key(VK_RETURN, true),
                key(VK_SHIFT, true),
            ]);
        } else {
            for unit in ch.encode_utf16(&mut [0; 2]).iter().copied() {
                inputs.extend([unicode(unit, false), unicode(unit, true)]);
            }
        }
        if inputs.len() >= 128 {
            flush(&mut inputs)?;
        }
    }
    if !inputs.is_empty() {
        flush(&mut inputs)?;
    }
    Ok(())
}

fn composer(automation: &IUIAutomation, window: HWND) -> Option<IUIAutomationElement> {
    unsafe {
        let root = automation.ElementFromHandle(window).ok()?;
        let condition = automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &UIA_EditControlTypeId.0.into())
            .ok()?;
        let elements = root.FindAll(TreeScope_Descendants, &condition).ok()?;
        for index in 0..elements.Length().ok()? {
            let element = elements.GetElement(index).ok()?;
            let class = element.CurrentClassName().ok()?.to_string();
            if class.contains("slateTextArea") {
                return Some(element);
            }
        }
        None
    }
}

pub fn send(_app_id: &str, title: &str, text: &str) -> Result<(), String> {
    let target = target(title).ok_or("noChat")?;
    let text = text.trim();
    if text.is_empty() || text.chars().count() > super::LIMIT {
        return Err("tooLong".into());
    }
    if !crate::notifications::notifications_open("com.squirrel.Discord.Discord".into()) {
        return Err("openFailed".into());
    }
    let _apartment = crate::com::Apartment::mta().map_err(|_| "inspectFailed")?;
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| "inspectFailed")?;

    for _ in 0..16 {
        let window = unsafe { GetForegroundWindow() };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        let is_discord = crate::shell::process_path(pid).is_some_and(|path| {
            std::path::Path::new(&path)
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Discord.exe"))
        });
        if is_discord {
            let mut wide = [0u16; 512];
            let len = unsafe { GetWindowTextW(window, &mut wide) }.max(0) as usize;
            let window_title = String::from_utf16_lossy(&wide[..len]);
            if let Some(editor) = composer(&automation, window) {
                let editor_name = unsafe { editor.CurrentName() }
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !matches_target(&target, &editor_name, &window_title) {
                    return Err("wrongChat".into());
                }
                // Enter would send a half-written message there together with the reply.
                let existing = editor_text(&editor).ok_or("inspectFailed")?;
                if !is_blank(&existing, &editor_name) {
                    return Err("draftInDiscord".into());
                }
                unsafe { editor.SetFocus() }.map_err(|_| "focusLost")?;
                thread::sleep(Duration::from_millis(80));
                let focused = unsafe { automation.GetFocusedElement() }.map_err(|_| "focusLost")?;
                if !unsafe { automation.CompareElements(&editor, &focused) }
                    .is_ok_and(|same| same.as_bool())
                    || unsafe { GetForegroundWindow() } != window
                {
                    return Err("focusLost".into());
                }
                type_text(window, text)?;
                thread::sleep(Duration::from_millis(120));
                if unsafe { GetForegroundWindow() } != window {
                    return Err("typingInterrupted".into());
                }
                inject(&[key(VK_RETURN, false), key(VK_RETURN, true)])?;
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err("wrongChat".into())
}
