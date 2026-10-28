// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Observe only Discord's configured mute/deafen chords. Never registers,
//! suppresses, or forwards shortcuts; Discord continues handling its own keys.
use crate::discord_storage::VoiceState;
use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ToggleMute,
    ToggleDeafen,
    HoldMute,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub id: String,
    pub keys: Vec<u16>,
    pub action: Action,
}

pub fn parse(json: &serde_json::Value) -> Vec<Binding> {
    let Some(bindings) = json.get("_state").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    bindings
        .iter()
        .filter_map(|(id, item)| {
            if item.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
                return None;
            }
            let action = match item.get("action")?.as_str()? {
                "TOGGLE_MUTE" => Action::ToggleMute,
                "TOGGLE_DEAFEN" => Action::ToggleDeafen,
                "PUSH_TO_MUTE" => Action::HoldMute,
                _ => return None,
            };
            let mut keys = Vec::new();
            for shortcut in item.get("shortcut")?.as_array()? {
                let parts = shortcut.as_array()?;
                let device = parts.first()?.as_u64()?;
                let code = parts.get(1)?.as_u64()?;
                let key = match device {
                    0 | 2 if (1..=255).contains(&code) => code as u16,
                    1 => match code {
                        0 => 1,
                        1 => 2,
                        2 => 4,
                        3 => 5,
                        4 => 6,
                        _ => return None,
                    },
                    _ => return None,
                };
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            (!keys.is_empty()).then(|| Binding {
                id: id.clone(),
                keys,
                action,
            })
        })
        .collect()
}

#[derive(Default)]
struct Tracker {
    active: bool,
    base: Option<VoiceState>,
    before_deafen_muted: bool,
    bindings: Vec<Binding>,
    down: Vec<Option<bool>>,
    holds: HashSet<String>,
    pending_until: Option<Instant>,
}

impl Tracker {
    fn state(&self) -> Option<VoiceState> {
        self.base.map(|v| VoiceState {
            muted: v.muted || v.deafened || !self.holds.is_empty(),
            deafened: v.deafened,
        })
    }
    fn bindings(&mut self, bindings: Vec<Binding>) {
        if self.bindings != bindings {
            self.down = vec![None; bindings.len()];
            self.bindings = bindings;
            self.holds.clear();
        }
    }
    fn live(&mut self, state: VoiceState, now: Instant) -> bool {
        // Give Discord time to process the same key before reading its UI.
        // A held push-to-mute must never replace the underlying toggle state.
        if !self.holds.is_empty() {
            return false;
        }
        if self.pending_until.is_some_and(|end| now < end) && self.state() != Some(state) {
            return false;
        }
        self.pending_until = None;
        if state.deafened && self.base.is_some_and(|v| !v.deafened) {
            self.before_deafen_muted = self.base.unwrap().muted;
        }
        self.base = Some(state);
        true
    }
    fn sample(&mut self, mut pressed: impl FnMut(u16) -> bool, now: Instant) {
        if !self.active {
            return;
        }
        let mut changes = Vec::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            let chord = binding.keys.iter().all(|&key| pressed(key));
            // Extra modifiers must not turn an unrelated shortcut into mute.
            let modifiers_match = [
                ([16, 160, 161], 16),
                ([17, 162, 163], 17),
                ([18, 164, 165], 18),
            ]
            .iter()
            .all(|(family, generic)| {
                !pressed(*generic) || binding.keys.iter().any(|k| family.contains(k))
            });
            let windows_match =
                (!pressed(91) && !pressed(92)) || binding.keys.iter().any(|k| [91, 92].contains(k));
            let down = chord && modifiers_match && windows_match;
            if let Some(previous) = self.down[index]
                && previous != down
            {
                changes.push((binding.id.clone(), binding.action, down));
            }
            self.down[index] = Some(down);
        }
        for (id, action, down) in changes {
            let Some(mut base) = self.base else {
                continue;
            };
            match action {
                Action::ToggleMute if down => {
                    if base.deafened {
                        base = VoiceState {
                            muted: false,
                            deafened: false,
                        };
                    } else {
                        base.muted = !base.muted;
                    }
                }
                Action::ToggleDeafen if down => {
                    if base.deafened {
                        base = VoiceState {
                            muted: self.before_deafen_muted,
                            deafened: false,
                        };
                    } else {
                        self.before_deafen_muted = base.muted;
                        base = VoiceState {
                            muted: true,
                            deafened: true,
                        };
                    }
                }
                Action::HoldMute => {
                    if down {
                        self.holds.insert(id);
                    } else {
                        self.holds.remove(&id);
                    }
                }
                _ => continue,
            }
            self.base = Some(base);
            self.pending_until = Some(now + Duration::from_millis(350));
        }
    }
}

#[derive(Default)]
pub struct Monitor {
    shared: Arc<Mutex<Tracker>>,
    stopped: Arc<AtomicBool>,
    started: bool,
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}
impl Monitor {
    pub fn active(&mut self, on: bool) {
        let mut tracker = self.shared.lock().unwrap();
        if !on {
            *tracker = Tracker::default();
        }
        tracker.active = on;
        drop(tracker);
        #[cfg(windows)]
        if on && !self.started {
            self.started = true;
            let shared = self.shared.clone();
            let stopped = self.stopped.clone();
            std::thread::spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    shared.lock().unwrap().sample(
                        |key| unsafe {
                            (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
                                key as i32,
                            ) as u16
                                & 0x8000)
                                != 0
                        },
                        Instant::now(),
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
        }
    }
    pub fn bindings(&self, bindings: Vec<Binding>) {
        self.shared.lock().unwrap().bindings(bindings);
    }
    pub fn live(&self, state: VoiceState) -> bool {
        self.shared.lock().unwrap().live(state, Instant::now())
    }
    pub fn seed(&self, state: VoiceState) {
        let mut tracker = self.shared.lock().unwrap();
        if tracker.base.is_none() {
            tracker.base = Some(state);
        }
    }
    pub fn state(&self) -> Option<VoiceState> {
        self.shared.lock().unwrap().state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn tracker(keys: Vec<u16>, action: Action) -> Tracker {
        let mut tracker = Tracker {
            active: true,
            base: Some(VoiceState {
                muted: false,
                deafened: false,
            }),
            ..Default::default()
        };
        tracker.bindings(vec![Binding {
            id: "custom".into(),
            keys,
            action,
        }]);
        tracker.sample(|_| false, Instant::now());
        tracker
    }
    #[test]
    fn reads_user_chords_and_mouse_buttons_without_fixed_hotkeys() {
        let bindings = parse(&json!({"_state": {
            "one": {"enabled": true, "action": "TOGGLE_MUTE", "shortcut": [[0, 162, "0:0"], [0, 74, "0:0"]]},
            "two": {"enabled": true, "action": "TOGGLE_DEAFEN", "shortcut": [[1, 3, "0:0"]]},
            "disabled": {"enabled": false, "action": "TOGGLE_MUTE", "shortcut": [[0, 120, 1]]},
            "unrelated": {"action": "SAVE_CLIP", "shortcut": [[0, 119, 1]]}
        }}));
        assert_eq!(bindings.len(), 2);
        assert!(
            bindings
                .iter()
                .any(|b| b.keys == [162, 74] && b.action == Action::ToggleMute)
        );
        assert!(
            bindings
                .iter()
                .any(|b| b.keys == [5] && b.action == Action::ToggleDeafen)
        );
    }
    #[test]
    fn arbitrary_chord_toggles_once_per_press_and_survives_stale_ui_sample() {
        let now = Instant::now();
        let mut t = tracker(vec![162, 74], Action::ToggleMute);
        t.sample(|k| [17, 162, 74].contains(&k), now);
        assert!(t.state().unwrap().muted);
        t.live(
            VoiceState {
                muted: false,
                deafened: false,
            },
            now + Duration::from_millis(20),
        );
        assert!(t.state().unwrap().muted);
        t.sample(|k| [17, 162, 74].contains(&k), now);
        assert!(t.state().unwrap().muted);
        t.sample(|_| false, now);
        t.sample(|k| [17, 162, 74].contains(&k), now);
        assert!(!t.state().unwrap().muted);
    }
    #[test]
    fn deafen_restores_the_previous_mic_state_and_hold_mute_restores_on_release() {
        let now = Instant::now();
        let mut t = tracker(vec![120], Action::ToggleDeafen);
        t.sample(|k| k == 120, now);
        assert_eq!(
            t.state(),
            Some(VoiceState {
                muted: true,
                deafened: true
            })
        );
        t.sample(|_| false, now);
        t.sample(|k| k == 120, now);
        assert_eq!(
            t.state(),
            Some(VoiceState {
                muted: false,
                deafened: false
            })
        );
        let mut t = tracker(vec![5], Action::HoldMute);
        t.sample(|k| k == 5, now);
        assert!(t.state().unwrap().muted);
        t.live(
            VoiceState {
                muted: true,
                deafened: false,
            },
            now,
        );
        t.sample(|_| false, now);
        assert!(!t.state().unwrap().muted);
    }
    #[test]
    fn extra_modifiers_and_initially_held_keys_do_not_trigger() {
        let now = Instant::now();
        let mut t = tracker(vec![74], Action::ToggleMute);
        t.sample(|k| [17, 162, 74].contains(&k), now);
        assert!(!t.state().unwrap().muted);
        t.bindings(vec![Binding {
            id: "new".into(),
            keys: vec![75],
            action: Action::ToggleMute,
        }]);
        t.sample(|k| k == 75, now);
        assert!(!t.state().unwrap().muted);
    }
}
