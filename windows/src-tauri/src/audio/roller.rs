// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Use finer volume steps (PreciseVolumeRollerService.swift): the volume
//! keys and wheels move the output 1% at a time instead of Windows' 2%, and
//! the bursts a wheel sends are thinned the way the Mac thins them. A
//! low-level keyboard hook owns the two volume keys while this is on; with
//! Alt, Ctrl or Win held they are left to Windows, as the Mac leaves them to
//! macOS with Option, Command or Control.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

/// PreciseVolumeRollerGate: presses closer than 30 ms are one detent, and a
/// wheel that jitters backwards needs three presses in the new direction
/// within 300 ms before it turns.
#[derive(Default)]
pub struct Gate {
    last_at: Option<Instant>,
    last: Option<Direction>,
    reversal: Option<Direction>,
    reversals: u32,
}

const MINIMUM_SPACING: Duration = Duration::from_millis(30);
const REVERSAL_WINDOW: Duration = Duration::from_millis(300);
const REVERSAL_CONFIRMATIONS: u32 = 2;

impl Gate {
    pub fn accepts(&mut self, direction: Direction, at: Instant) -> bool {
        if let Some(last_at) = self.last_at
            && at.duration_since(last_at) <= MINIMUM_SPACING
        {
            return false;
        }
        if let (Some(last), Some(last_at)) = (self.last, self.last_at)
            && direction != last
            && at.duration_since(last_at) < REVERSAL_WINDOW
        {
            if self.reversal == Some(direction) {
                self.reversals += 1;
            } else {
                self.reversal = Some(direction);
                self.reversals = 1;
            }
            if self.reversals <= REVERSAL_CONFIRMATIONS {
                return false;
            }
        }
        self.last = Some(direction);
        self.last_at = Some(at);
        self.reversal = None;
        self.reversals = 0;
        true
    }
}

/// PreciseVolumeKeyOwnership: whether a press belongs to Windows is decided
/// when the key goes down and holds until it comes up, so letting go of a
/// modifier halfway doesn't hand the rest of the press over.
#[derive(Default)]
pub struct Ownership {
    left_to_system: std::collections::HashMap<u32, bool>,
}

impl Ownership {
    pub fn leaves_to_system(&mut self, key: u32, down: bool, repeat: bool, modifier: bool) -> bool {
        if !down {
            return self.left_to_system.remove(&key).unwrap_or(true);
        }
        if !repeat {
            self.left_to_system.insert(key, modifier);
        }
        *self.left_to_system.get(&key).unwrap_or(&true)
    }
}

/// One finer step from `current`, on whole percents.
pub fn stepped(current: f32, direction: Direction) -> f32 {
    let percent = (current * 100.0).round();
    let next = match direction {
        Direction::Up => percent + 1.0,
        Direction::Down => percent - 1.0,
    };
    next.clamp(0.0, 100.0) / 100.0
}

#[cfg(windows)]
pub use hook::set_enabled;

#[cfg(windows)]
mod hook {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::time::Instant;

    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_VOLUME_DOWN, VK_VOLUME_UP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
        PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
        WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
    };

    use super::{Direction, Gate, Ownership};

    /// The hook thread's id while it runs.
    static THREAD: Mutex<Option<u32>> = Mutex::new(None);

    thread_local! {
        static KEYS: RefCell<(Gate, Ownership, HashSet<u32>)> = RefCell::default();
    }

    /// Installs or removes the hook. Err when Windows refused it.
    pub fn set_enabled(on: bool) -> Result<(), String> {
        let mut thread = THREAD.lock().unwrap();
        match (on, *thread) {
            (true, None) => {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::Builder::new()
                    .name("vorssaint-volume-keys".into())
                    .spawn(move || run(tx))
                    .map_err(|e| e.to_string())?;
                let id = rx.recv().map_err(|e| e.to_string())??;
                *thread = Some(id);
            }
            (false, Some(id)) => {
                let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
                *thread = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn run(ready: std::sync::mpsc::Sender<Result<u32, String>>) {
        let module = unsafe { GetModuleHandleW(None) }.ok();
        let hook = match unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), module.map(Into::into), 0)
        } {
            Ok(hook) => hook,
            Err(e) => {
                let _ = ready.send(Err(e.message()));
                return;
            }
        };
        let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }

    fn held(key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
        (unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000) != 0
    }

    /// Answers within the hook's time limit: the level is set on the audio
    /// thread, never here.
    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let direction = match info.vkCode {
                k if k == u32::from(VK_VOLUME_UP.0) => Some(Direction::Up),
                k if k == u32::from(VK_VOLUME_DOWN.0) => Some(Direction::Down),
                _ => None,
            };
            if let Some(direction) = direction {
                let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
                let modifier = held(VK_MENU) || held(VK_CONTROL) || held(VK_LWIN) || held(VK_RWIN);
                let owned = KEYS.with(|keys| {
                    let (gate, ownership, pressed) = &mut *keys.borrow_mut();
                    let repeat = if down {
                        !pressed.insert(info.vkCode)
                    } else {
                        pressed.remove(&info.vkCode);
                        false
                    };
                    if ownership.leaves_to_system(info.vkCode, down, repeat, modifier) {
                        return false;
                    }
                    if down && gate.accepts(direction, Instant::now()) {
                        crate::audio::roller_step(direction);
                    }
                    true
                });
                if owned {
                    return LRESULT(1);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_closer_than_30ms_is_one_step() {
        let mut g = Gate::default();
        let t = Instant::now();
        assert!(g.accepts(Direction::Up, t));
        assert!(!g.accepts(Direction::Up, t + Duration::from_millis(20)));
        assert!(g.accepts(Direction::Up, t + Duration::from_millis(60)));
    }

    #[test]
    fn a_quick_reversal_needs_three_presses() {
        let mut g = Gate::default();
        let t = Instant::now();
        assert!(g.accepts(Direction::Up, t));
        let at = |ms| t + Duration::from_millis(ms);
        assert!(!g.accepts(Direction::Down, at(50)));
        assert!(!g.accepts(Direction::Down, at(100)));
        assert!(g.accepts(Direction::Down, at(150)));
        // A slow reversal turns at once.
        assert!(g.accepts(Direction::Up, at(600)));
    }

    #[test]
    fn a_press_with_a_modifier_stays_the_systems_until_released() {
        let mut o = Ownership::default();
        assert!(o.leaves_to_system(1, true, false, true));
        assert!(o.leaves_to_system(1, true, true, false));
        assert!(o.leaves_to_system(1, false, false, false));
        assert!(!o.leaves_to_system(1, true, false, false));
        assert!(!o.leaves_to_system(1, false, false, true));
        // A release never seen going down is the system's.
        assert!(o.leaves_to_system(2, false, false, false));
    }

    #[test]
    fn steps_are_whole_percents_within_range() {
        assert_eq!(stepped(0.5, Direction::Up), 0.51);
        assert_eq!(stepped(0.505, Direction::Down), 0.50);
        assert_eq!(stepped(1.0, Direction::Up), 1.0);
        assert_eq!(stepped(0.0, Direction::Down), 0.0);
    }
}
