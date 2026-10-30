// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Now playing (spec 04-media-info §1.1, §1.7): the island reads whatever
//! Windows' media controls (SMTC) know — Spotify, a browser tab, Media
//! Player — and sends it play/pause, skips, seeks and shuffle. One reader
//! thread polls the sessions, four times a second while something plays,
//! and publishes only when what it read changed. On a Mac (designing only)
//! nothing plays.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
mod apps;
mod selection;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    /// The player, as Windows names it (an AUMID such as `Spotify.exe`).
    pub source: String,
    pub source_name: String,
    pub is_music_app: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The cover as a data URL; the island averages it for the accent tint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artwork: Option<String>,
    pub is_playing: bool,
    /// Seconds, as of `sampled_at` (epoch seconds); the island extrapolates.
    pub elapsed: f64,
    pub duration: f64,
    pub rate: f64,
    pub sampled_at: f64,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    pub can_shuffle: bool,
    pub shuffle: bool,
}

/// What the island receives: the playback, or none while nothing plays.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub idle_player: Option<Source>,
    pub selected_source: Option<String>,
    pub revision: u64,
    pub playback: Option<Playback>,
    /// Every player Windows lists, for the source menu.
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub name: String,
    pub is_playing: bool,
    pub is_music_app: bool,
    pub has_track: bool,
    /// Trusted Apps-folder executable; never accepted from a frontend command.
    #[serde(skip)]
    pub launch_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Toggle,
    Next,
    Previous,
}

/// Known music players; Windows has no app category to tell them apart.
#[cfg_attr(not(windows), allow(dead_code))]
const MUSIC_APPS: [&str; 13] = [
    "spotify",
    "applemusic",
    "zunemusic",
    "itunes",
    "foobar2000",
    "musicbee",
    "tidal",
    "deezer",
    "amazonmusic",
    "aimp",
    "winamp",
    "youtube music",
    "vlc",
];

#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_music_app(aumid: &str) -> bool {
    let id = aumid.to_ascii_lowercase();
    MUSIC_APPS.iter().any(|m| id.contains(m))
}

#[cfg_attr(not(windows), allow(dead_code))]
/// A readable name for an AUMID: `Spotify.exe` → Spotify, packaged ids by their app part.
pub fn source_name(aumid: &str) -> String {
    let lower = aumid.to_ascii_lowercase();
    let known = [
        ("spotify", "Spotify"),
        ("zunemusic", "Media Player"),
        ("applemusic", "Apple Music"),
        ("msedge", "Microsoft Edge"),
        ("chrome", "Google Chrome"),
        ("firefox", "Firefox"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vivaldi", "Vivaldi"),
        ("vlc", "VLC"),
        ("itunes", "iTunes"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("foobar2000", "foobar2000"),
        ("musicbee", "MusicBee"),
    ];
    if let Some((_, name)) = known.iter().find(|(k, _)| lower.contains(k)) {
        return (*name).to_string();
    }
    let app = aumid.rsplit('!').next().unwrap_or(aumid);
    let app = app.trim_end_matches(".exe").trim_end_matches(".EXE");
    let app = app.rsplit(['.', '\\', '/']).next().unwrap_or(app);
    if app.is_empty() {
        aumid.to_string()
    } else {
        app.to_string()
    }
}

#[derive(Default)]
pub struct MediaState {
    now: Option<NowPlaying>,
    selected: Option<String>,
    revision: u64,
    players: Vec<Source>,
}
pub type Shared = Arc<Mutex<MediaState>>;

#[cfg(windows)]
pub use imp::{command, probe, seek, set_shuffle, start};

#[cfg(any(windows, test))]
mod player;
#[cfg(windows)]
pub use player::open_player;

#[cfg(not(windows))]
pub fn probe() -> String {
    "SMTC is Windows only\n".into()
}

#[cfg(windows)]
mod imp {
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use base64::Engine;
    use tauri::{AppHandle, Emitter, Manager as _, Runtime};
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Storage::Streams::DataReader;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    use super::{Command, NowPlaying, Playback, Shared, Source, is_music_app, source_name};

    const ARTWORK_LIMIT: u32 = 4 * 1024 * 1024;

    fn now() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }

    fn manager() -> Option<Manager> {
        Manager::RequestAsync().ok()?.join().ok()
    }

    /// The session the island follows: a playing music app, else any playing
    /// player, else the one Windows calls current.
    fn include_other<R: Runtime>(app: &AppHandle<R>) -> bool {
        app.state::<crate::prefs::Prefs>()
            .get()
            .pointer("/island/includeOtherPlayers")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    }

    fn artwork(session: &Session) -> Option<String> {
        let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > ARTWORK_LIMIT {
            return None;
        }
        let kind = stream
            .ContentType()
            .map(|t| t.to_string())
            .unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size).ok()?.join().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        let kind = if kind.starts_with("image/") {
            kind
        } else {
            "image/png".into()
        };
        Some(format!(
            "data:{kind};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }

    fn read(session: &Session, previous: Option<&Playback>) -> Option<Playback> {
        let source = session.SourceAppUserModelId().ok()?.to_string();
        let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let info = session.GetPlaybackInfo().ok()?;
        let controls = info.Controls().ok()?;
        let timeline = session.GetTimelineProperties().ok()?;
        let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
        let artist = props.Artist().map(|t| t.to_string()).unwrap_or_default();
        let album = props
            .AlbumTitle()
            .map(|t| t.to_string())
            .unwrap_or_default();
        if title.is_empty() && artist.is_empty() {
            return None;
        }
        // TimeSpan and DateTime are 100 ns ticks; DateTime counts from 1601.
        let ticks = |t: i64| t as f64 / 1e7;
        let start = timeline
            .StartTime()
            .map(|t| ticks(t.Duration))
            .unwrap_or(0.0);
        let end = timeline.EndTime().map(|t| ticks(t.Duration)).unwrap_or(0.0);
        let position = timeline
            .Position()
            .map(|t| ticks(t.Duration))
            .unwrap_or(0.0);
        let updated = timeline
            .LastUpdatedTime()
            .map(|t| ticks(t.UniversalTime) - 11_644_473_600.0)
            .ok()
            .filter(|t| *t > 0.0)
            .unwrap_or_else(now);
        let is_playing = info.PlaybackStatus().ok() == Some(Status::Playing);
        let rate = info
            .PlaybackRate()
            .ok()
            .and_then(|r| r.Value().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 16.0);
        let same_song =
            previous.is_some_and(|p| p.source == source && p.title == title && p.artist == artist);
        let artwork = if same_song {
            previous.and_then(|p| p.artwork.clone())
        } else {
            None
        };
        Some(Playback {
            source_name: source_name(&source),
            is_music_app: is_music_app(&source),
            source,
            title,
            artist,
            album,
            artwork: artwork.or_else(|| self::artwork(session)),
            is_playing,
            elapsed: (position - start).max(0.0),
            duration: (end - start).max(0.0),
            rate: if rate == 0.0 { 1.0 } else { rate },
            sampled_at: updated,
            can_play_pause: controls.IsPlayPauseToggleEnabled().unwrap_or(false)
                || controls.IsPlayEnabled().unwrap_or(false)
                || controls.IsPauseEnabled().unwrap_or(false),
            can_next: controls.IsNextEnabled().unwrap_or(false),
            can_previous: controls.IsPreviousEnabled().unwrap_or(false),
            can_seek: controls.IsPlaybackPositionEnabled().unwrap_or(false) && end > start,
            can_shuffle: controls.IsShuffleEnabled().unwrap_or(false),
            shuffle: info
                .IsShuffleActive()
                .ok()
                .and_then(|s| s.Value().ok())
                .unwrap_or(false),
        })
    }

    fn sources(manager: &Manager) -> Vec<Source> {
        let Ok(list) = manager.GetSessions() else {
            return Vec::new();
        };
        let mut sources: Vec<Source> = list
            .into_iter()
            .filter_map(|s| {
                let id = s.SourceAppUserModelId().ok()?.to_string();
                let is_playing = s
                    .GetPlaybackInfo()
                    .ok()
                    .and_then(|i| i.PlaybackStatus().ok())
                    == Some(Status::Playing);
                Some(Source {
                    name: source_name(&id),
                    is_music_app: is_music_app(&id),
                    has_track: s
                        .TryGetMediaPropertiesAsync()
                        .ok()
                        .and_then(|p| p.join().ok())
                        .is_some_and(|p| {
                            !p.Title().unwrap_or_default().is_empty()
                                || !p.Artist().unwrap_or_default().is_empty()
                        }),
                    id,
                    is_playing,
                    launch_path: None,
                })
            })
            .collect();
        sources.sort_by(|a, b| {
            a.id.cmp(&b.id)
                .then(b.has_track.cmp(&a.has_track))
                .then(b.is_playing.cmp(&a.is_playing))
        });
        sources.dedup_by(|a, b| a.id == b.id);
        sources
    }

    /// Reads on its own thread (SMTC wants an MTA) and emits `music://now-playing` on change.
    pub fn start<R: Runtime>(app: AppHandle<R>, shared: Shared) {
        thread::Builder::new()
            .name("now-playing".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                let mut manager = None;
                let mut last: Option<NowPlaying> = None;
                let preferred = app
                    .state::<crate::prefs::Prefs>()
                    .get()
                    .pointer("/island/preferredPlayer")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let installed = super::apps::installed(&preferred);
                shared.lock().unwrap().players = installed;
                let mut last_music: Option<String> = None;
                loop {
                    if manager.is_none() {
                        manager = self::manager();
                    }
                    let mut current = NowPlaying::default();
                    let (manual, revision) = {
                        let state = shared.lock().unwrap();
                        (state.selected.clone(), state.revision)
                    };
                    current.selected_source = manual.clone();
                    current.revision = revision;
                    if let Some(m) = &manager {
                        let previous = last.as_ref().and_then(|l| l.playback.as_ref());
                        current.sources = sources(m);
                        let system = m
                            .GetCurrentSession()
                            .ok()
                            .and_then(|s| s.SourceAppUserModelId().ok())
                            .map(|s| s.to_string());
                        let selected = super::selection::choose(
                            &current.sources,
                            previous.map(|p| p.source.as_str()),
                            system.as_deref(),
                            manual.as_deref(),
                            include_other(&app),
                        );
                        if let Some(selected) = selected {
                            current.playback =
                                matching_session(m, &selected.id).and_then(|s| read(&s, previous));
                        }
                    }
                    if let Some(p) = current
                        .playback
                        .as_ref()
                        .filter(|p| p.is_music_app && p.is_playing)
                    {
                        last_music = Some(p.source.clone());
                    }
                    let preferred = app
                        .state::<crate::prefs::Prefs>()
                        .get()
                        .pointer("/island/preferredPlayer")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let mut players = current.sources.clone();
                    players.extend(shared.lock().unwrap().players.clone());
                    current.idle_player =
                        super::apps::preferred(&preferred, last_music.as_deref(), &players);
                    // Position moves on its own; only a jump or a new reading counts as news.
                    if last.as_ref() != Some(&current) {
                        let mut state = shared.lock().unwrap();
                        if state.revision != revision {
                            continue;
                        }
                        state.now = Some(current.clone());
                        let _ = app.emit("music://now-playing", &current);
                        last = Some(current);
                    }
                    let playing = last
                        .as_ref()
                        .and_then(|l| l.playback.as_ref())
                        .is_some_and(|p| p.is_playing);
                    thread::sleep(Duration::from_millis(if playing { 250 } else { 1000 }));
                }
            })
            .expect("now-playing thread");
    }

    /// `--music-probe`: what the reader sees, step by step, for diagnosing a machine.
    pub fn probe() -> String {
        let mut out = String::new();
        unsafe {
            let r = CoInitializeEx(None, COINIT_MULTITHREADED);
            out.push_str(&format!("CoInitializeEx: {r:?}\n"));
        }
        let manager = match Manager::RequestAsync().and_then(|op| op.join()) {
            Ok(m) => m,
            Err(e) => return out + &format!("RequestAsync failed: {e}\n"),
        };
        match manager.GetSessions() {
            Ok(list) => {
                out.push_str(&format!("sessions: {}\n", list.Size().unwrap_or(0)));
                for s in list {
                    let id = s
                        .SourceAppUserModelId()
                        .map(|h| h.to_string())
                        .unwrap_or_default();
                    let status = s
                        .GetPlaybackInfo()
                        .and_then(|i| i.PlaybackStatus())
                        .map(|v| v.0);
                    out.push_str(&format!("  {id} status={status:?}\n"));
                    match read(&s, None) {
                        Some(p) => out.push_str(&format!(
                            "    {} - {} ({:.0}/{:.0}s) art={}\n",
                            p.title,
                            p.artist,
                            p.elapsed,
                            p.duration,
                            p.artwork.as_ref().map_or(0, |a| a.len())
                        )),
                        None => out.push_str("    read: none\n"),
                    }
                }
            }
            Err(e) => out.push_str(&format!("GetSessions failed: {e}\n")),
        }
        out
    }

    fn matching_session(manager: &Manager, source: &str) -> Option<Session> {
        let sessions: Vec<_> = manager
            .GetSessions()
            .ok()?
            .into_iter()
            .filter(|s| s.SourceAppUserModelId().ok().is_some_and(|id| id == source))
            .collect();
        // Browsers can expose multiple sessions under the same application ID.
        // Prefer its playing session, for both the display and every command.
        sessions
            .iter()
            .find(|s| {
                s.GetPlaybackInfo()
                    .ok()
                    .and_then(|i| i.PlaybackStatus().ok())
                    == Some(Status::Playing)
            })
            .or_else(|| sessions.first())
            .cloned()
    }

    fn with_session<T>(
        source: &str,
        f: impl FnOnce(&Session) -> windows::core::Result<T>,
    ) -> Option<T> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let manager = manager()?;
        let session = matching_session(&manager, source)?;
        f(&session).ok()
    }

    pub fn command(command: Command, source: &str) -> bool {
        with_session(source, |s| {
            let op = match command {
                Command::Toggle => s.TryTogglePlayPauseAsync()?,
                Command::Next => s.TrySkipNextAsync()?,
                Command::Previous => s.TrySkipPreviousAsync()?,
            };
            op.join()
        })
        .unwrap_or(false)
    }

    pub fn seek(seconds: f64, source: &str) -> bool {
        with_session(source, |s| {
            let start = s.GetTimelineProperties()?.StartTime()?.Duration;
            s.TryChangePlaybackPositionAsync(start + (seconds.max(0.0) * 1e7) as i64)?
                .join()
        })
        .unwrap_or(false)
    }

    pub fn set_shuffle(on: bool, source: &str) -> bool {
        with_session(source, |s| s.TryChangeShuffleActiveAsync(on)?.join()).unwrap_or(false)
    }

    #[cfg(test)]
    mod live_tests {
        use super::*;
        #[test]
        #[ignore = "reads media sessions on an interactive Windows desktop"]
        fn selected_sources_resolve_to_their_real_control_sessions() {
            let _apartment = crate::com::Apartment::mta().unwrap();
            let manager = manager().unwrap();
            let sources = sources(&manager);
            let active: Vec<_> = sources.iter().filter(|s| s.has_track).collect();
            assert!(!active.is_empty(), "No active media sessions to verify");
            for source in active {
                let chosen =
                    super::super::selection::choose(&sources, None, None, Some(&source.id), false)
                        .unwrap();
                assert_eq!(chosen.id, source.id);
                let actual =
                    with_session(&source.id, |session| session.SourceAppUserModelId()).unwrap();
                assert_eq!(actual, source.id);
                let playback =
                    read(&matching_session(&manager, &source.id).unwrap(), None).unwrap();
                assert_eq!(playback.source, source.id);
                println!("Verified display and command session: {}", source.id);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn start<R: tauri::Runtime>(_app: tauri::AppHandle<R>, _shared: Shared) {}
#[cfg(not(windows))]
pub fn command(_command: Command, _source: &str) -> bool {
    false
}
#[cfg(not(windows))]
pub fn seek(_seconds: f64, _source: &str) -> bool {
    false
}
#[cfg(not(windows))]
pub fn set_shuffle(_on: bool, _source: &str) -> bool {
    false
}
#[cfg(not(windows))]
pub fn open_player(_aumid: &str) -> bool {
    false
}

#[tauri::command]
pub fn music_now(state: tauri::State<'_, Shared>) -> Option<NowPlaying> {
    state.lock().unwrap().now.clone()
}

#[tauri::command]
pub async fn music_players<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, Shared>,
) -> Result<Vec<Source>, String> {
    use tauri::Manager;
    let preferred = app
        .state::<crate::prefs::Prefs>()
        .get()
        .pointer("/island/preferredPlayer")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let installed = apps::installed(&preferred);
        #[cfg(not(windows))]
        let installed: Vec<Source> = vec![];
        let mut state = state.lock().unwrap();
        state.players = installed.clone();
        let mut players = state
            .now
            .as_ref()
            .map(|n| n.sources.clone())
            .unwrap_or_default();
        for player in installed {
            if !players.iter().any(|s| s.id == player.id) {
                players.push(player);
            }
        }
        players.sort_by(|a, b| a.name.cmp(&b.name));
        players
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn music_select_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, Shared>,
    source: Option<String>,
) -> Result<(), String> {
    use tauri::Emitter;
    let mut state = state.lock().unwrap();
    if source == state.selected {
        return Ok(());
    }
    if let Some(id) = &source
        && !state
            .now
            .as_ref()
            .is_some_and(|n| n.sources.iter().any(|s| &s.id == id))
    {
        return Err("This player is no longer available".into());
    }
    state.selected = source.clone();
    state.revision += 1;
    let revision = state.revision;
    if let Some(now) = state.now.as_mut() {
        now.playback = None;
        now.selected_source = source;
        now.revision = revision;
        let _ = app.emit("music://now-playing", now.clone());
    }
    Ok(())
}

fn command_source(state: &Shared, expected: Option<&str>) -> Option<String> {
    let state = state.lock().unwrap();
    let source = state.now.as_ref()?.playback.as_ref()?.source.as_str();
    if expected.is_some_and(|expected| expected != source) {
        return None;
    }
    Some(source.into())
}

/// Commands wait on the player, so they run off the main thread.
#[tauri::command]
pub async fn music_command(
    command: Command,
    source: Option<String>,
    state: tauri::State<'_, Shared>,
) -> Result<bool, String> {
    let Some(source) = command_source(&state, source.as_deref()) else {
        return Ok(false);
    };
    tauri::async_runtime::spawn_blocking(move || self::command(command, &source))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn music_seek(
    seconds: f64,
    source: Option<String>,
    state: tauri::State<'_, Shared>,
) -> Result<bool, String> {
    let Some(source) = command_source(&state, source.as_deref()) else {
        return Ok(false);
    };
    tauri::async_runtime::spawn_blocking(move || seek(seconds, &source))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn music_shuffle(
    on: bool,
    source: Option<String>,
    state: tauri::State<'_, Shared>,
) -> Result<bool, String> {
    let Some(source) = command_source(&state, source.as_deref()) else {
        return Ok(false);
    };
    tauri::async_runtime::spawn_blocking(move || set_shuffle(on, &source))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn music_open_player(
    source: String,
    state: tauri::State<'_, Shared>,
) -> Result<bool, String> {
    let path = state
        .lock()
        .unwrap()
        .players
        .iter()
        .find(|p| p.id == source)
        .and_then(|p| p.launch_path.clone());
    tauri::async_runtime::spawn_blocking(move || {
        // Preserve the native focus/activation behavior for running players.
        if open_player(&source) {
            return true;
        }
        if let Some(path) = path {
            // Only a path obtained from Windows' installed application catalog.
            return std::process::Command::new(path).spawn().is_ok();
        }
        false
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_cannot_borrow_a_different_players_context() {
        let state = Shared::default();
        state.lock().unwrap().now = Some(NowPlaying {
            playback: Some(Playback {
                source: "chosen".into(),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(
            command_source(&state, Some("chosen")).as_deref(),
            Some("chosen")
        );
        assert!(command_source(&state, Some("previous")).is_none());
        state.lock().unwrap().now.as_mut().unwrap().playback = None;
        assert!(command_source(&state, Some("chosen")).is_none());
        assert!(command_source(&state, None).is_none());
    }

    #[test]
    fn players_get_readable_names() {
        assert_eq!(source_name("Spotify.exe"), "Spotify");
        assert_eq!(
            source_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"),
            "Media Player"
        );
        assert_eq!(source_name("MSEdge"), "Microsoft Edge");
        assert_eq!(source_name("Contoso.Player_abc!App"), "App");
        assert!(is_music_app("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!is_music_app("MSEdge"));
    }
}
