// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
//! The same precedence as NotchPlaybackSource.preferred on macOS.
use super::Source;

pub fn choose<'a>(
    sources: &'a [Source],
    previous: Option<&str>,
    system: Option<&str>,
    manual: Option<&str>,
    include_other: bool,
) -> Option<&'a Source> {
    let available: Vec<_> = sources.iter().filter(|s| s.has_track).collect();
    if let Some(selected) = available.iter().find(|s| Some(s.id.as_str()) == manual) {
        return Some(selected);
    }
    for group in 0..4 {
        let mut candidates: Vec<_> = available
            .iter()
            .copied()
            .filter(|s| match group {
                0 => s.is_music_app && s.is_playing,
                1 => include_other && !s.is_music_app && s.is_playing,
                2 => include_other && !s.is_music_app && Some(s.id.as_str()) == previous,
                _ => s.is_music_app,
            })
            .collect();
        candidates.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(chosen) = candidates
            .iter()
            .find(|s| Some(s.id.as_str()) == previous)
            .or_else(|| candidates.iter().find(|s| Some(s.id.as_str()) == system))
            .or_else(|| candidates.first())
        {
            return Some(chosen);
        }
    }
    include_other
        .then(|| {
            available
                .into_iter()
                .find(|s| Some(s.id.as_str()) == system)
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(id: &str, music: bool, playing: bool) -> Source {
        Source {
            id: id.into(),
            name: id.into(),
            is_music_app: music,
            is_playing: playing,
            has_track: true,
            launch_path: None,
        }
    }
    fn picked(
        s: &[Source],
        prev: Option<&str>,
        system: Option<&str>,
        manual: Option<&str>,
        others: bool,
    ) -> Option<String> {
        choose(s, prev, system, manual, others).map(|s| s.id.clone())
    }
    #[test]
    fn manual_overrides_playback_and_other_players_filter() {
        let s = [source("music", true, true), source("browser", false, false)];
        assert_eq!(
            picked(&s, None, None, Some("browser"), false).as_deref(),
            Some("browser")
        );
    }
    #[test]
    fn missing_or_trackless_manual_source_falls_back_and_can_return() {
        let mut s = [source("music", true, false), source("browser", false, true)];
        s[1].has_track = false;
        assert_eq!(
            picked(&s, None, None, Some("browser"), true).as_deref(),
            Some("music")
        );
        s[1].has_track = true;
        assert_eq!(
            picked(&s, None, None, Some("browser"), true).as_deref(),
            Some("browser")
        );
        assert_eq!(
            picked(&s, None, None, Some("closed"), true).as_deref(),
            Some("browser")
        );
    }
    #[test]
    fn music_wins_playing_ties_and_paused_browser_keeps_its_place() {
        let mut s = [source("music", true, true), source("browser", false, true)];
        assert_eq!(
            picked(&s, Some("browser"), Some("browser"), None, true).as_deref(),
            Some("music")
        );
        s[0].is_playing = false;
        s[1].is_playing = false;
        assert_eq!(
            picked(&s, Some("browser"), Some("music"), None, true).as_deref(),
            Some("browser")
        );
        assert_eq!(
            picked(&s, Some("browser"), Some("browser"), None, false).as_deref(),
            Some("music")
        );
    }
    #[test]
    fn non_music_sources_can_be_excluded_without_hiding_them() {
        let s = [source("browser", false, true)];
        assert!(picked(&s, None, Some("browser"), None, false).is_none());
        assert_eq!(
            picked(&s, None, None, None, true).as_deref(),
            Some("browser")
        );
    }
    #[test]
    fn ties_keep_previous_then_system_then_stable_id() {
        let s = [source("z", true, true), source("a", true, true)];
        assert_eq!(
            picked(&s, Some("z"), Some("a"), None, true).as_deref(),
            Some("z")
        );
        assert_eq!(
            picked(&s, None, Some("z"), None, true).as_deref(),
            Some("z")
        );
        assert_eq!(picked(&s, None, None, None, true).as_deref(), Some("a"));
    }
}
