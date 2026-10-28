// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Mic Mute (MicMuteService on the Mac): every active microphone muted at
//! once. Only the microphones this muted are unmuted again, so one the person
//! had muted already stays muted; one plugged in while muted is muted too.
//! The set survives a restart, as the Mac's `micMuteMutedDevices` does.
//! `microphone.rs` only reads mute states, so the two never fight.

use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MicMute {
    pub muted: bool,
    /// Microphones this muted, to unmute later.
    pub ours: BTreeSet<String>,
    /// Microphones present while muted; only newcomers get muted on arrival.
    #[serde(skip)]
    pub seen: BTreeSet<String>,
}

impl MicMute {
    /// Turns mute on over `devices` (id, muted now); returns the ids to mute.
    pub fn mute(&mut self, devices: &[(String, bool)]) -> Vec<String> {
        self.muted = true;
        self.seen = devices.iter().map(|(id, _)| id.clone()).collect();
        let fresh: Vec<String> = devices
            .iter()
            .filter(|(_, m)| !m)
            .map(|(id, _)| id.clone())
            .collect();
        self.ours.extend(fresh.iter().cloned());
        fresh
    }

    /// Turns mute off; returns the ids to unmute.
    pub fn unmute(&mut self) -> Vec<String> {
        self.muted = false;
        self.seen.clear();
        std::mem::take(&mut self.ours).into_iter().collect()
    }

    /// Unmuting a microphone that was unplugged fails; it stays owed and is
    /// unmuted when it returns. Returns those of `present` still owed.
    pub fn owed(&self, present: &[String]) -> Vec<String> {
        if self.muted {
            return Vec::new();
        }
        present
            .iter()
            .filter(|id| self.ours.contains(*id))
            .cloned()
            .collect()
    }

    /// The microphones that arrived since muting and are live; they get muted.
    pub fn arrivals(&mut self, devices: &[(String, bool)]) -> Vec<String> {
        if !self.muted {
            return Vec::new();
        }
        let mut fresh = Vec::new();
        for (id, muted) in devices {
            if self.seen.insert(id.clone()) && !muted {
                self.ours.insert(id.clone());
                fresh.push(id.clone());
            }
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[(&str, bool)]) -> Vec<(String, bool)> {
        items.iter().map(|(id, m)| (id.to_string(), *m)).collect()
    }

    #[test]
    fn unmutes_only_what_it_muted() {
        let mut mic = MicMute::default();
        assert_eq!(mic.mute(&list(&[("a", false), ("b", true)])), vec!["a"]);
        assert!(mic.muted);
        assert_eq!(mic.unmute(), vec!["a"]);
        assert!(!mic.muted && mic.ours.is_empty());
    }

    #[test]
    fn mutes_newcomers_once_and_leaves_a_manual_unmute_alone() {
        let mut mic = MicMute::default();
        mic.mute(&list(&[("a", false)]));
        // "a" unmuted by hand stays unmuted; "c" plugged in gets muted once.
        assert_eq!(
            mic.arrivals(&list(&[("a", false), ("c", false)])),
            vec!["c"]
        );
        assert!(
            mic.arrivals(&list(&[("a", false), ("c", false)]))
                .is_empty()
        );
        let mut back = mic.unmute();
        back.sort();
        assert_eq!(back, vec!["a", "c"]);
    }

    #[test]
    fn an_unplugged_microphone_is_unmuted_when_it_returns() {
        let mut mic = MicMute::default();
        mic.mute(&list(&[("a", false), ("b", false)]));
        let _ = mic.unmute();
        // Unmuting "b" failed: the engine keeps it owed.
        mic.ours.insert("b".into());
        assert_eq!(mic.owed(&["a".into()]), Vec::<String>::new());
        assert_eq!(mic.owed(&["a".into(), "b".into()]), vec!["b"]);
        mic.mute(&list(&[("a", false)]));
        assert!(mic.owed(&["b".into()]).is_empty());
    }

    #[test]
    fn nothing_arrives_while_unmuted() {
        let mut mic = MicMute::default();
        assert!(mic.arrivals(&list(&[("a", false)])).is_empty());
    }

    #[test]
    fn the_saved_set_round_trips_without_the_session_list() {
        let mut mic = MicMute::default();
        mic.mute(&list(&[("a", false)]));
        let text = serde_json::to_string(&mic).unwrap();
        let back: MicMute = serde_json::from_str(&text).unwrap();
        assert!(back.muted && back.ours.contains("a") && back.seen.is_empty());
    }
}
