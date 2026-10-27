// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Windows clipboard (spec 05 §3.4, §15.1): a message-only window that
//! hears every change through `AddClipboardFormatListener`, reads the
//! formats the history keeps, writes an entry back, and pastes it into the
//! app the person came from with `SendInput`.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RETURN,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
};
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW,
    GetWindowThreadProcessId, HWND_MESSAGE, IsWindow, MB_OK, MSG, RegisterClassW, SetForegroundWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_CLIPBOARDUPDATE, WNDCLASSW,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::formats;
use super::history::{Content, MAX_FILES, MAX_IMAGE_BYTES, MAX_RAW_IMAGE_BYTES};

const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_DIBV5: u32 = 17;
/// Four bytes per UTF-16 unit is far beyond a 1,000,000-character text.
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// Rides along a write from the history, so the listener never records it
/// again (the sequence number below covers the same write a second way).
const HISTORY_MARK: &str = "Vorssaint.HistoryCopy";

/// A change worth reading, with who made it as far as Windows can tell.
pub struct Update {
    /// Vorssaint's own write (an OCR result, a color) or a copy made inside
    /// the island: recorded, with no app named.
    pub own: bool,
    /// The exe the copy came from: the clipboard's owner, else the window in front.
    pub exe: Option<String>,
    /// Every exe that held the front since the last change, the owner and the
    /// one in front now: the copy is left out when any of them is ignored.
    pub candidates: Vec<String>,
}

static ON_UPDATE: OnceLock<Box<dyn Fn(Update) + Send + Sync>> = OnceLock::new();
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static OWN_SEQUENCE: AtomicU32 = AtomicU32::new(0);
/// The last window in front that isn't Vorssaint's, for a paste when the
/// island never recorded where it came from (opened by hover, then clicked).
static LAST_FOREIGN: AtomicIsize = AtomicIsize::new(0);
static FRONT: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
}

fn available(format: u32) -> bool {
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0;
    if !hwnd.0.is_null() {
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    }
    pid
}

// MARK: Listening

/// Starts the listener thread once; `on_update` runs on it for each change.
pub fn listen(on_update: impl Fn(Update) + Send + Sync + 'static) {
    if ON_UPDATE.set(Box::new(on_update)).is_err() {
        return;
    }
    let _ = std::thread::Builder::new().name("vorssaint-clipboard".into()).spawn(|| unsafe {
        let instance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let class = w!("VorssaintClipboardListener");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        ) else {
            return;
        };
        WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = AddClipboardFormatListener(hwnd);
        // Out-of-context hooks call back on this thread, from its message loop.
        let _hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        foreground_changed(HWINEVENTHOOK::default(), 0, GetForegroundWindow(), 0, 0, 0, 0);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        changed();
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let pid = pid_of(hwnd);
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return;
    }
    LAST_FOREIGN.store(hwnd.0 as isize, Ordering::SeqCst);
    if let Some(exe) = crate::shell::process_path(pid) {
        let mut front = FRONT.lock().unwrap();
        if !front.contains(&exe) && front.len() < 32 {
            front.push(exe);
        }
    }
}

/// WM_CLIPBOARDUPDATE: who wrote it is read now, while that app (often a
/// password manager about to hide) is still in front; the content is read
/// by the caller's worker.
fn changed() {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    if sequence == OWN_SEQUENCE.load(Ordering::SeqCst) || available(format(HISTORY_MARK)) {
        return;
    }
    let me = unsafe { GetCurrentProcessId() };
    let owner = unsafe { GetClipboardOwner() }.map(pid_of).unwrap_or(0);
    let front = pid_of(unsafe { GetForegroundWindow() });
    let exe_of = |pid: u32| (pid != 0 && pid != me).then(|| crate::shell::process_path(pid)).flatten();
    let owner_exe = exe_of(owner);
    let front_exe = exe_of(front);
    let mut candidates = std::mem::take(&mut *FRONT.lock().unwrap());
    for exe in owner_exe.iter().chain(front_exe.iter()) {
        if !candidates.contains(exe) {
            candidates.push(exe.clone());
        }
    }
    // The next change's window starts with whoever is in front now.
    *FRONT.lock().unwrap() = front_exe.iter().cloned().collect();
    let own = owner == me || front == me;
    let update = Update {
        own,
        exe: if own { None } else { owner_exe.or(front_exe) },
        candidates,
    };
    if let Some(on_update) = ON_UPDATE.get() {
        on_update(update);
    }
}

// MARK: Reading

/// The clipboard, open until dropped.
pub struct Opened(());

impl Drop for Opened {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Other apps hold the clipboard for a moment while they write it: 5 tries, 20 ms apart.
pub fn open(owner: Option<HWND>) -> Option<Opened> {
    for attempt in 0..5 {
        if unsafe { OpenClipboard(owner) }.is_ok() {
            return Some(Opened(()));
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    None
}

/// A format's bytes, if present and no larger than `max`.
pub fn bytes(_open: &Opened, format: u32, max: usize) -> Option<Vec<u8>> {
    unsafe {
        let handle = HGLOBAL(GetClipboardData(format).ok()?.0);
        let size = GlobalSize(handle);
        if size == 0 || size > max {
            return None;
        }
        let pointer = GlobalLock(handle);
        if pointer.is_null() {
            return None;
        }
        let copy = std::slice::from_raw_parts(pointer.cast::<u8>(), size).to_vec();
        let _ = GlobalUnlock(handle);
        Some(copy)
    }
}

/// CF_HDROP: how many files there are, and the first `take` paths.
pub fn hdrop_paths(_open: &Opened, take: usize) -> Option<(usize, Vec<String>)> {
    unsafe {
        let drop = HDROP(GetClipboardData(CF_HDROP).ok()?.0);
        let count = DragQueryFileW(drop, u32::MAX, None) as usize;
        let paths = (0..count.min(take) as u32)
            .filter_map(|index| {
                let length = DragQueryFileW(drop, index, None) as usize;
                let mut path = vec![0u16; length + 1];
                DragQueryFileW(drop, index, Some(&mut path));
                (length > 0).then(|| String::from_utf16_lossy(&path[..length]))
            })
            .collect();
        Some((count, paths))
    }
}

/// What the person asked to keep out of history: password managers and
/// Windows' own Win+V honor the same three marks (spec 05 §3.4).
fn private(open: &Opened) -> bool {
    available(format("ExcludeClipboardContentFromMonitorProcessing"))
        || available(format("Clipboard Viewer Ignore"))
        || bytes(open, format("CanIncludeInClipboardHistory"), 64)
            .is_some_and(|b| b.len() >= 4 && u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == 0)
}

/// The content the history keeps, richest first: files, then text, then a
/// picture. Text comes before a picture on Windows because Office and
/// browsers put a rendered bitmap beside copied cells and text.
pub fn read(include_images_files: bool) -> Option<Content> {
    let open = open(None)?;
    if private(&open) {
        return None;
    }
    if include_images_files {
        match hdrop_paths(&open, MAX_FILES) {
            Some((count, paths)) if count <= MAX_FILES && !paths.is_empty() => return Some(Content::Files(paths)),
            Some(_) => {}
            // A remote desktop's file copy carries descriptors, not paths (§15.1).
            None if available(format("FileGroupDescriptorW")) || available(format("FileGroupDescriptor")) => {
                return None;
            }
            None => {}
        }
    }
    if available(CF_UNICODETEXT) {
        let raw = bytes(&open, CF_UNICODETEXT, MAX_TEXT_BYTES)?;
        let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let text = formats::text_from_utf16(&units);
        if !text.trim().is_empty() || !include_images_files {
            return Some(Content::Text(text));
        }
    }
    if !include_images_files {
        return None;
    }
    if let Some(png) = bytes(&open, format("PNG"), MAX_IMAGE_BYTES)
        && let Some((width, height)) = formats::png_dimensions(&png)
    {
        return Some(Content::Image { hash: formats::sha256_hex(&png), png, width, height });
    }
    for dib_format in [CF_DIBV5, CF_DIB] {
        if let Some(dib) = bytes(&open, dib_format, MAX_RAW_IMAGE_BYTES) {
            drop(open);
            let png = formats::dib_png(dib).ok().filter(|png| png.len() <= MAX_IMAGE_BYTES)?;
            let (width, height) = formats::png_dimensions(&png)?;
            return Some(Content::Image { hash: formats::sha256_hex(&png), png, width, height });
        }
    }
    None
}

// MARK: Writing

pub enum Payload {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
    Files(Vec<String>),
}

/// Puts `payload` on the clipboard. A write from the history is marked and
/// its sequence number kept, so the listener doesn't record it again.
pub fn write(payload: &Payload, from_history: bool) -> bool {
    let mut items: Vec<(u32, Vec<u8>)> = match payload {
        Payload::Text(text) => vec![(CF_UNICODETEXT, formats::utf16z(text))],
        Payload::Image(png) => {
            let mut items = Vec::new();
            if let Some(dib) = formats::png_to_dib(png) {
                items.push((CF_DIB, dib));
            }
            items.push((format("PNG"), png.clone()));
            items
        }
        // An Explorer cut leaves MOVE behind; a paste from history copies (§15.1).
        Payload::Files(paths) => vec![
            (CF_HDROP, formats::dropfiles(paths)),
            (format("Preferred DropEffect"), 1u32.to_le_bytes().to_vec()),
        ],
    };
    if from_history {
        items.push((format(HISTORY_MARK), vec![0; 4]));
    }
    // EmptyClipboard needs a window to own the clipboard, or SetClipboardData fails.
    let window = HWND(WINDOW.load(Ordering::SeqCst) as _);
    if window.0.is_null() {
        return false;
    }
    let Some(open) = open(Some(window)) else { return false };
    if unsafe { EmptyClipboard() }.is_err() {
        return false;
    }
    let written = items.iter().all(|(format, data)| set(*format, data));
    drop(open);
    if from_history {
        OWN_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::SeqCst);
    }
    written
}

fn set(format: u32, data: &[u8]) -> bool {
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) else { return false };
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<u8>(), data.len());
        let _ = GlobalUnlock(memory);
        // Once set, the memory belongs to the clipboard.
        if SetClipboardData(format, Some(HANDLE(memory.0))).is_ok() {
            true
        } else {
            let _ = GlobalFree(Some(memory));
            false
        }
    }
}

// MARK: Pasting

/// Where a paste goes: the window the island took the keyboard from, else
/// the last other app in front. Never one of Vorssaint's own windows.
pub fn paste_target(island_previous: isize) -> Option<isize> {
    let me = unsafe { GetCurrentProcessId() };
    [island_previous, LAST_FOREIGN.load(Ordering::SeqCst)].into_iter().find(|&raw| {
        let hwnd = HWND(raw as _);
        raw != 0 && unsafe { IsWindow(Some(hwnd)) }.as_bool() && pid_of(hwnd) != me
    })
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0;
        let read = GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        let _ = CloseHandle(token);
        read.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

/// UIPI drops keys sent into an elevated window from a normal one, and says
/// nothing. A target whose elevation can't be read counts as elevated.
pub fn blocks_input(target: isize) -> bool {
    if token_elevated(unsafe { GetCurrentProcess() }) == Some(true) {
        return false;
    }
    let pid = pid_of(HWND(target as _));
    match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(process) => {
            let elevated = token_elevated(process);
            unsafe {
                let _ = CloseHandle(process);
            }
            elevated != Some(false)
        }
        Err(_) => true,
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let extended = [VK_RCONTROL, VK_RMENU, VK_LWIN, VK_RWIN].contains(&vk);
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

/// Brings `target` forward, waits for it, then types Ctrl+V. Keys still held
/// (Shift, Alt, Enter from Ctrl+n or Return) are let go first, after Ctrl
/// is down so letting go of Alt or Win doesn't open a menu.
pub fn send_paste(target: isize) -> bool {
    let hwnd = HWND(target as _);
    let pid = pid_of(hwnd);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let deadline = Instant::now() + Duration::from_millis(500);
        while pid_of(GetForegroundWindow()) != pid {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(50));
        let mut inputs = vec![key(VK_CONTROL, false)];
        for held in [VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN, VK_RETURN] {
            if GetAsyncKeyState(held.0 as i32) as u16 & 0x8000 != 0 {
                inputs.push(key(held, true));
            }
        }
        inputs.extend([key(VK_V, false), key(VK_V, true), key(VK_CONTROL, true)]);
        SendInput(&inputs, size_of::<INPUT>() as i32) as usize == inputs.len()
    }
}

/// The empty slot's answer to Ctrl+n (the Mac's NSSound.beep()).
pub fn beep() {
    unsafe {
        let _ = MessageBeep(MB_OK);
    }
}
