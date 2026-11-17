// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Use finer volume steps (PreciseVolumeRollerService.swift): the volume
//! keys and wheels move the output 1% at a time instead of Windows' 2%, and
//! the bursts a wheel sends are thinned the way the Mac thins them. While
//! this is on the two keys (alone or with Shift) are held as global
//! shortcuts, so Windows never sees them; with Alt, Ctrl or Win they are a
//! different combination and stay Windows', as the Mac leaves them to macOS
//! with Option, Command or Control. A low-level keyboard hook was tried
//! first: Windows didn't call it while the Settings window was in front.

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

/// One finer step from `current`, on whole percents.
pub fn stepped(current: f32, direction: Direction) -> f32 {
    let percent = (current * 100.0).round();
    let next = match direction {
        Direction::Up => percent + 1.0,
        Direction::Down => percent - 1.0,
    };
    next.clamp(0.0, 100.0) / 100.0
}

/// The keys held while this is on.
pub fn shortcuts() -> [(tauri_plugin_global_shortcut::Shortcut, Direction); 4] {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    [
        (Shortcut::new(None, Code::AudioVolumeUp), Direction::Up),
        (Shortcut::new(None, Code::AudioVolumeDown), Direction::Down),
        (
            Shortcut::new(Some(Modifiers::SHIFT), Code::AudioVolumeUp),
            Direction::Up,
        ),
        (
            Shortcut::new(Some(Modifiers::SHIFT), Code::AudioVolumeDown),
            Direction::Down,
        ),
    ]
}

/// Which way a held key moves the volume, if it is one of them.
pub fn direction(shortcut: &tauri_plugin_global_shortcut::Shortcut) -> Option<Direction> {
    shortcuts()
        .into_iter()
        .find(|(s, _)| s == shortcut)
        .map(|(_, d)| d)
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
    fn the_plain_and_shifted_keys_are_held_and_nothing_else() {
        use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
        assert_eq!(
            direction(&Shortcut::new(None, Code::AudioVolumeUp)),
            Some(Direction::Up)
        );
        assert_eq!(
            direction(&Shortcut::new(
                Some(Modifiers::SHIFT),
                Code::AudioVolumeDown
            )),
            Some(Direction::Down)
        );
        assert_eq!(
            direction(&Shortcut::new(Some(Modifiers::ALT), Code::AudioVolumeUp)),
            None
        );
        assert_eq!(direction(&Shortcut::new(None, Code::AudioVolumeMute)), None);
    }

    #[test]
    fn steps_are_whole_percents_within_range() {
        assert_eq!(stepped(0.5, Direction::Up), 0.51);
        assert_eq!(stepped(0.505, Direction::Down), 0.50);
        assert_eq!(stepped(1.0, Direction::Up), 1.0);
        assert_eq!(stepped(0.0, Direction::Down), 0.0);
    }
}
