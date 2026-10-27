// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Keep Awake (KeepAwakeManager on the Mac; spec 04 §7, 05 §1.2): a power
//! request that keeps the PC and its display from idling to sleep, for the
//! duration picked in Settings or until turned off. `PowerSetRequest` names
//! the reason in `powercfg /requests`. Like the Mac's idle assertions, it
//! doesn't stop a closed lid or the power button from sleeping the PC.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

/// The Mac's choices (`Defaults.allowedDurations`); 0 keeps it on until turned off.
pub const DURATIONS: [u32; 7] = [0, 15, 30, 60, 120, 240, 480];

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub active: bool,
    /// Milliseconds since 1970; None while it runs until turned off.
    pub ends_at: Option<u64>,
}

#[derive(Default)]
struct Inner {
    state: State,
    request: Option<Request>,
    /// Bumped by every start and stop, so an old timer ends nothing.
    generation: u64,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Inner>>);

pub fn sanitized(minutes: u32) -> u32 {
    if DURATIONS.contains(&minutes) { minutes } else { 0 }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn ends_at(now: u64, minutes: u32) -> Option<u64> {
    (minutes > 0).then(|| now + u64::from(minutes) * 60_000)
}

/// The held power request; dropping it lets the PC sleep again.
struct Request(#[cfg_attr(not(windows), allow(dead_code))] isize);

impl Request {
    #[cfg(windows)]
    fn create() -> Result<Self, String> {
        use windows::Win32::System::Power::{PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest};
        use windows::Win32::System::Threading::{POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0};
        use windows::core::PWSTR;
        let mut reason: Vec<u16> = "Vorssaint: Keep awake is on".encode_utf16().chain(Some(0)).collect();
        let context = REASON_CONTEXT {
            Version: 0, // POWER_REQUEST_CONTEXT_VERSION
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(reason.as_mut_ptr()) },
        };
        unsafe {
            let handle = PowerCreateRequest(&context).map_err(|e| e.message())?;
            let request = Request(handle.0 as isize);
            PowerSetRequest(handle, PowerRequestSystemRequired).map_err(|e| e.message())?;
            PowerSetRequest(handle, PowerRequestDisplayRequired).map_err(|e| e.message())?;
            Ok(request)
        }
    }

    #[cfg(not(windows))]
    fn create() -> Result<Self, String> {
        Ok(Request(0))
    }
}

#[cfg(windows)]
impl Drop for Request {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::Power::{PowerClearRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired};
        let handle = HANDLE(self.0 as *mut _);
        unsafe {
            let _ = PowerClearRequest(handle, PowerRequestDisplayRequired);
            let _ = PowerClearRequest(handle, PowerRequestSystemRequired);
            let _ = CloseHandle(handle);
        }
    }
}

fn emit(app: &AppHandle, state: &State) {
    let _ = app.emit_to(crate::island::LABEL, "keepawake://state", state);
}

/// Starts a session of `minutes` (0 = until turned off), replacing any running one.
fn start(app: &AppHandle, shared: &Shared, minutes: u32) -> Result<State, String> {
    let minutes = sanitized(minutes);
    // The new request is held before the old one goes, so a failure changes nothing.
    let request = Request::create()?;
    let state = {
        let mut inner = shared.0.lock().unwrap();
        inner.request = Some(request);
        inner.generation += 1;
        inner.state = State { active: true, ends_at: ends_at(now_ms(), minutes) };
        if let Some(end) = inner.state.ends_at {
            watch_end(app.clone(), shared.clone(), inner.generation, end);
        }
        inner.state.clone()
    };
    emit(app, &state);
    Ok(state)
}

fn stop(app: &AppHandle, shared: &Shared) -> State {
    stop_if(app, shared, None).unwrap_or_default()
}

/// Ends the session, or with `generation` only the session it names, under
/// one lock so a session started meanwhile isn't ended by an old timer.
fn stop_if(app: &AppHandle, shared: &Shared, generation: Option<u64>) -> Option<State> {
    let state = {
        let mut inner = shared.0.lock().unwrap();
        if generation.is_some_and(|g| g != inner.generation) {
            return None;
        }
        inner.request = None;
        inner.generation += 1;
        inner.state = State::default();
        inner.state.clone()
    };
    emit(app, &state);
    Some(state)
}

/// Ends the session at `end` by the wall clock, so time asleep with the lid
/// closed counts as it does on the Mac.
fn watch_end(app: AppHandle, shared: Shared, generation: u64, end: u64) {
    std::thread::spawn(move || {
        loop {
            let left = end.saturating_sub(now_ms());
            if left == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(left.min(30_000)));
            if shared.0.lock().unwrap().generation != generation {
                return;
            }
        }
        stop_if(&app, &shared, Some(generation));
    });
}

#[tauri::command]
pub fn keep_awake_state(state: tauri::State<'_, Shared>) -> State {
    state.0.lock().unwrap().state.clone()
}

/// `minutes` starts a session (0 = until turned off); None ends it.
#[tauri::command]
pub fn keep_awake_set(app: AppHandle, state: tauri::State<'_, Shared>, minutes: Option<u32>) -> Result<State, String> {
    match minutes {
        Some(m) => start(&app, &state, m),
        None => Ok(stop(&app, &state)),
    }
}

/// The tile: off when on, else on for the duration picked in Settings.
#[tauri::command]
pub fn keep_awake_toggle(app: AppHandle, state: tauri::State<'_, Shared>) -> Result<State, String> {
    if state.0.lock().unwrap().state.active {
        return Ok(stop(&app, &state));
    }
    let minutes = app
        .state::<crate::prefs::Prefs>()
        .get()
        .pointer("/island/keepAwakeDuration")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    start(&app, &state, u32::try_from(minutes).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_offered_durations_run() {
        assert_eq!(sanitized(30), 30);
        assert_eq!(sanitized(0), 0);
        assert_eq!(sanitized(45), 0);
        assert_eq!(sanitized(u32::MAX), 0);
    }

    #[test]
    fn a_timed_session_ends_after_its_minutes() {
        assert_eq!(ends_at(1_000, 15), Some(1_000 + 15 * 60_000));
        assert_eq!(ends_at(1_000, 0), None);
    }

    #[test]
    fn the_state_names_its_end_for_the_island() {
        let json = serde_json::to_value(State { active: true, ends_at: None }).unwrap();
        assert_eq!(json["active"], true);
        assert!(json["endsAt"].is_null());
    }
}
