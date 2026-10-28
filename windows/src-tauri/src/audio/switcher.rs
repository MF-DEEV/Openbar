// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Switch outputs with a shortcut (SoundOutputSwitcher.swift): the chosen
//! outputs in a cycle, and one global shortcut that moves the default
//! output to the next one that is connected.

use std::collections::HashSet;

/// The Mac's Control-Option-Command-S, on Windows' three modifiers.
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+KeyS";

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
}
