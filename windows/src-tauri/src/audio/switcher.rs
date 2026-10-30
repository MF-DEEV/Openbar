// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Switch outputs with a shortcut (SoundOutputSwitcher.swift): the chosen
//! outputs in a cycle, and one global shortcut that moves the default
//! output to the next one that is connected.

use std::collections::HashSet;

/// The Mac's Control-Option-Command-S, on Windows' three modifiers.
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+KeyS";

/// Shortcuts the island already uses: its toggle, and the keys a Sessions
/// approval takes while it shows (sessions/globalPermissionKeys.ts).
pub fn reserved(shortcut: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    [Code::KeyY, Code::KeyN, Code::KeyT]
        .into_iter()
        .any(|key| *shortcut == Shortcut::new(Some(Modifiers::CONTROL), key))
}

/// ShortcutRecorderButton's rule: Ctrl, Alt or Win with the key, so a hand-
/// edited "Escape" can't take a key from every app.
pub fn has_modifier(shortcut: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    use tauri_plugin_global_shortcut::Modifiers;
    shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
}

/// MixerRoutingSupport.nextSelectedOutputDeviceUID: the chosen output after
/// the current one, wrapping; the first one when the current isn't chosen;
/// None when nothing chosen is connected or the only one already plays.
pub fn next<'a>(
    current: Option<&str>,
    selected: &'a [String],
    available: &HashSet<String>,
) -> Option<&'a str> {
    let mut seen = HashSet::new();
    let candidates: Vec<&str> = selected
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && available.contains(*id) && seen.insert(*id))
        .collect();
    let first = *candidates.first()?;
    let Some(index) = current.and_then(|c| candidates.iter().position(|id| *id == c)) else {
        return Some(first);
    };
    if candidates.len() == 1 {
        return None;
    }
    Some(candidates[(index + 1) % candidates.len()])
}

/// A press that changed nothing still worked when the only chosen output
/// that is connected is already playing.
pub fn nothing_to_do_is_fine(selected: &[String], available: &HashSet<String>) -> bool {
    selected.iter().any(|id| available.contains(id.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_islands_own_keys_are_reserved_and_a_modifier_is_required() {
        let parse = |s: &str| s.parse::<tauri_plugin_global_shortcut::Shortcut>().unwrap();
        assert!(reserved(&parse("Ctrl+KeyY")));
        assert!(reserved(&parse("Ctrl+KeyT")));
        assert!(reserved(&parse("Ctrl+KeyN")));
        assert!(!reserved(&parse("Ctrl+Shift+KeyY")));
        assert!(!reserved(&parse(DEFAULT_SHORTCUT)));
        assert!(has_modifier(&parse("Alt+F5")));
        assert!(!has_modifier(&parse("Shift+F5")));
        assert!(!has_modifier(&parse("Escape")));
    }

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cycles_through_the_connected_choices() {
        let available = set(&["a", "b", "c"]);
        let chosen = ids(&["a", "x", "c", "a"]);
        assert_eq!(next(Some("a"), &chosen, &available), Some("c"));
        assert_eq!(next(Some("c"), &chosen, &available), Some("a"));
        assert_eq!(next(Some("b"), &chosen, &available), Some("a"));
        assert_eq!(next(None, &chosen, &available), Some("a"));
    }

    #[test]
    fn one_connected_choice_already_playing_is_nothing_to_do() {
        let available = set(&["a", "b"]);
        assert_eq!(next(Some("a"), &ids(&["a", "x"]), &available), None);
        assert!(nothing_to_do_is_fine(&ids(&["a", "x"]), &available));
        assert_eq!(next(Some("a"), &ids(&["x"]), &available), None);
        assert!(!nothing_to_do_is_fine(&ids(&["x"]), &available));
        assert!(!nothing_to_do_is_fine(&[], &available));
    }

    #[test]
    fn the_default_shortcut_parses() {
        let parsed: Result<tauri_plugin_global_shortcut::Shortcut, _> = DEFAULT_SHORTCUT.parse();
        assert!(parsed.is_ok());
    }

    /// What src/shared/shortcut.ts records: the modifiers, then the key's code.
    #[test]
    fn every_kind_of_recorded_shortcut_parses() {
        use tauri_plugin_global_shortcut::Shortcut;
        for accelerator in [
            "Ctrl+Alt+Shift+KeyS",
            "Super+F24",
            "Alt+Digit2",
            "Ctrl+Numpad0",
            "Ctrl+Backquote",
            "Ctrl+Backslash",
            "Ctrl+BracketLeft",
            "Ctrl+Quote",
            "Ctrl+Space",
            "Ctrl+Enter",
            "Ctrl+PageUp",
            "Ctrl+PageDown",
            "Ctrl+ArrowUp",
            "Ctrl+PrintScreen",
            "Ctrl+Pause",
        ] {
            assert!(
                accelerator.parse::<Shortcut>().is_ok(),
                "{accelerator} does not parse"
            );
        }
    }

    #[test]
    fn padded_choices_still_count() {
        let available = set(&["a", "b"]);
        assert_eq!(
            next(Some("a"), &ids(&[" a ", "b", "", "b"]), &available),
            Some("b")
        );
        assert!(nothing_to_do_is_fine(&ids(&[" a"]), &available));
    }
}
