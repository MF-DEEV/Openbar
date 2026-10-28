// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Music page's player volume control, one of the audio module's
//! session readers.

/// Match SMTC's application identity to its WASAPI process, including packaged players.
fn matches_player(source: &str, executable: &str) -> bool {
    let source = source.to_ascii_lowercase();
    let executable = executable.to_ascii_lowercase();
    let known = [
        ("spotify", "spotify"),
        ("msedge", "msedge"),
        ("chrome", "chrome"),
        ("firefox", "firefox"),
        ("brave", "brave"),
        ("opera", "opera"),
        ("vivaldi", "vivaldi"),
        ("zunemusic", "music.ui"),
        ("applemusic", "applemusic"),
        ("vlc", "vlc"),
        ("itunes", "itunes"),
        ("tidal", "tidal"),
        ("deezer", "deezer"),
        ("foobar2000", "foobar2000"),
        ("musicbee", "musicbee"),
    ];
    if let Some((_, process)) = known.iter().find(|(identity, _)| source.contains(identity)) {
        return executable == *process;
    }
    source.trim_end_matches(".exe") == executable
}

/// The player's volume through the audio module's session list, set first
/// when `level` is given (the loudest matching session wins).
pub fn volume(source: &str, level: Option<f32>) -> Result<f32, String> {
    crate::audio::player_volume(matches_player, source.to_string(), level)
}

#[tauri::command]
pub async fn music_volume(source: String, level: Option<f32>) -> Result<f32, String> {
    tauri::async_runtime::spawn_blocking(move || volume(&source, level))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_levels_before_accessing_audio_sessions() {
        for level in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert_eq!(
                volume("Spotify.exe", Some(level)),
                Err("Invalid volume level".into())
            );
        }
    }
    #[test]
    fn matches_packaged_and_desktop_players_without_matching_other_apps() {
        assert!(matches_player(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            "Spotify"
        ));
        assert!(matches_player("Chrome", "chrome"));
        assert!(matches_player(
            "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
            "Music.UI"
        ));
        assert!(matches_player("custom-player.exe", "custom-player"));
        assert!(!matches_player("Spotify.exe", "chrome"));
        assert!(!matches_player("Chrome", "chromedriver"));
    }
}
