// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Now playing on Linux: the players that speak MPRIS on the session bus
//! (Spotify, VLC, Rhythmbox, a browser tab, ...) where Windows reads SMTC.
//! `media.rs` includes this file as `imp`, since it keeps its state private.
//!
//! The reader polls like the Windows one (four times a second while something
//! plays) and publishes only what changed. A player is named by its desktop
//! entry (`spotify`), the way the installed-players list names it, so a
//! command finds the running instance again by that name and a browser with
//! several windows is one source.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use gtk::gio::{self, prelude::*};
use tauri::{AppHandle, Emitter, Manager as _, Runtime};
use zbus::blocking::{Connection, Proxy, connection::Builder};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use super::{Command, NowPlaying, Playback, Shared, Source, is_music_app};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const PATH: &str = "/org/mpris/MediaPlayer2";
/// A player that does not answer must not hold the reader for the bus's own
/// 25 seconds.
const REPLY: Duration = Duration::from_secs(2);
/// A reading that differs from where the last one says the song should be by
/// this much is a seek, and news.
const JUMP: f64 = 1.0;
const COVER_LIMIT: usize = 4 * 1024 * 1024;

/// Players Windows' list does not know, by what their desktop entry or name
/// contains.
const MORE_MUSIC_APPS: [&str; 24] = [
    "rhythmbox",
    "lollypop",
    "amberol",
    "strawberry",
    "clementine",
    "audacious",
    "quodlibet",
    "elisa",
    "gnome-music",
    "cmus",
    "ncspot",
    "spotifyd",
    "deadbeef",
    "sayonara",
    "tauon",
    "tidal-hifi",
    "plexamp",
    "g4music",
    "cider",
    "mpd",
    "musikcube",
    "termusic",
    "youtube-music",
    "pear-desktop",
];

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn known_music_app(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    is_music_app(&name) || MORE_MUSIC_APPS.iter().any(|known| name.contains(known))
}

/// Whether a desktop file's `Categories` say music: `Music`, or an audio
/// player that is not also a video player.
fn categories_say_music(categories: &str) -> bool {
    let has = |wanted: &str| categories.split(';').any(|c| c == wanted);
    has("Music") || (has("Audio") && has("Player") && !has("Video"))
}

/// The id a player goes by: its desktop entry without `.desktop`, else the
/// bus name without the instance a browser appends (`chromium.instance42`).
fn app_id(bus_name: &str, desktop_entry: Option<&str>) -> String {
    if let Some(entry) = desktop_entry.map(str::trim).filter(|e| !e.is_empty()) {
        return entry.trim_end_matches(".desktop").to_string();
    }
    let name = bus_name.strip_prefix(PREFIX).unwrap_or(bus_name);
    match name.find(".instance") {
        Some(at) if at > 0 => name[..at].to_string(),
        _ => name.to_string(),
    }
}

// What a value on the bus holds. Players differ: a number may come as a
// signed, unsigned or floating one, and an artist as a list or a string.

fn text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.to_string()),
        Value::ObjectPath(p) => Some(p.to_string()),
        Value::Value(inner) => text(inner),
        _ => None,
    }
}

fn number(value: &Value<'_>) -> Option<f64> {
    Some(match value {
        Value::I64(n) => *n as f64,
        Value::U64(n) => *n as f64,
        Value::I32(n) => f64::from(*n),
        Value::U32(n) => f64::from(*n),
        Value::I16(n) => f64::from(*n),
        Value::U16(n) => f64::from(*n),
        Value::U8(n) => f64::from(*n),
        Value::F64(n) => *n,
        Value::Value(inner) => return number(inner),
        _ => return None,
    })
}

fn flag(value: &Value<'_>) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Value(inner) => flag(inner),
        _ => None,
    }
}

fn texts(value: &Value<'_>) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().filter_map(text).collect(),
        Value::Value(inner) => texts(inner),
        other => text(other).into_iter().collect(),
    }
}

/// What a player says about the song it is on.
#[derive(Clone, Debug, Default, PartialEq)]
struct Track {
    /// `mpris:trackid`, which `SetPosition` wants back.
    id: Option<String>,
    title: String,
    artist: String,
    album: String,
    art: Option<String>,
    /// Seconds; 0 for a stream.
    length: f64,
}

/// What a player's `Player` interface says, without the position (which
/// moves on its own and is asked for separately).
#[derive(Clone, Debug, PartialEq)]
struct State {
    playing: bool,
    track: Track,
    rate: f64,
    shuffle: Option<bool>,
    can_control: bool,
    can_play: bool,
    can_pause: bool,
    can_next: bool,
    can_previous: bool,
    can_seek: bool,
}

fn track_of(metadata: &Value<'_>) -> Track {
    let mut track = Track::default();
    let Value::Dict(entries) = metadata else {
        return track;
    };
    let mut album_artist = String::new();
    for (key, value) in entries.iter() {
        let Some(key) = text(key) else { continue };
        match key.as_str() {
            "mpris:trackid" => track.id = text(value),
            "mpris:length" => track.length = number(value).unwrap_or(0.0).max(0.0) / 1e6,
            "mpris:artUrl" => track.art = text(value).filter(|u| !u.is_empty()),
            "xesam:title" => track.title = text(value).unwrap_or_default(),
            "xesam:album" => track.album = text(value).unwrap_or_default(),
            "xesam:artist" => track.artist = texts(value).join(", "),
            "xesam:albumArtist" => album_artist = texts(value).join(", "),
            _ => {}
        }
    }
    if track.artist.is_empty() {
        track.artist = album_artist;
    }
    track
}

fn state_of(properties: &HashMap<String, OwnedValue>) -> State {
    let get = |name: &str| properties.get(name).map(|v| &**v);
    let flag = |name: &str, otherwise: bool| get(name).and_then(flag).unwrap_or(otherwise);
    State {
        playing: get("PlaybackStatus").and_then(text).as_deref() == Some("Playing"),
        track: get("Metadata").map(track_of).unwrap_or_default(),
        rate: get("Rate").and_then(number).unwrap_or(1.0),
        shuffle: get("Shuffle").and_then(self::flag),
        can_control: flag("CanControl", true),
        can_play: flag("CanPlay", true),
        can_pause: flag("CanPause", true),
        can_next: flag("CanGoNext", false),
        can_previous: flag("CanGoPrevious", false),
        can_seek: flag("CanSeek", false),
    }
}

/// A running player: where to reach it, and what it calls itself.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    app: String,
    identity: String,
    music: bool,
}

#[derive(Clone, Debug)]
struct Seen {
    entry: Entry,
    state: State,
}

impl Seen {
    fn has_track(&self) -> bool {
        !self.state.track.title.is_empty() || !self.state.track.artist.is_empty()
    }
}

static BUS: Mutex<Option<Connection>> = Mutex::new(None);

/// The session bus, connected on first use and again after it went away.
fn connection() -> Option<Connection> {
    let mut bus = BUS.lock().ok()?;
    if bus.is_none() {
        *bus = Builder::session().ok()?.method_timeout(REPLY).build().ok();
    }
    bus.clone()
}

fn forget_connection() {
    if let Ok(mut bus) = BUS.lock() {
        *bus = None;
    }
}

fn proxy<'a>(
    connection: &Connection,
    name: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    Proxy::new(connection, name, PATH, interface)
}

fn all_properties(
    connection: &Connection,
    name: &str,
    interface: &str,
) -> zbus::Result<HashMap<String, OwnedValue>> {
    proxy(connection, name, PROPERTIES)?.call("GetAll", &(interface,))
}

fn player_names(connection: &Connection) -> zbus::Result<Vec<String>> {
    let bus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let mut names: Vec<String> = bus
        .call::<_, _, Vec<String>>("ListNames", &())?
        .into_iter()
        // playerctld repeats whichever player moved last.
        .filter(|n| n.starts_with(PREFIX) && n != "org.mpris.MediaPlayer2.playerctld")
        .collect();
    names.sort();
    Ok(names)
}

fn describe(connection: &Connection, name: &str) -> Entry {
    let root = all_properties(connection, name, ROOT).unwrap_or_default();
    let property = |key: &str| root.get(key).and_then(|v| text(v));
    let desktop = property("DesktopEntry");
    let app = app_id(name, desktop.as_deref());
    let identity = property("Identity")
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| app.clone());
    let music = known_music_app(&app)
        || known_music_app(&identity)
        || gio::DesktopAppInfo::new(&format!("{app}.desktop"))
            .and_then(|info| info.string("Categories"))
            .is_some_and(|categories| categories_say_music(&categories));
    Entry {
        name: name.to_string(),
        app,
        identity,
        music,
    }
}

/// Every player on the bus and what it says now. `entries` remembers each
/// name's identity, which does not change while it runs.
fn snapshot(
    connection: &Connection,
    entries: &mut HashMap<String, Entry>,
) -> zbus::Result<Vec<Seen>> {
    let names = player_names(connection)?;
    entries.retain(|name, _| names.contains(name));
    Ok(names
        .into_iter()
        .filter_map(|name| {
            let state = state_of(&all_properties(connection, &name, PLAYER).ok()?);
            let entry = entries
                .entry(name.clone())
                .or_insert_with(|| describe(connection, &name))
                .clone();
            Some(Seen { entry, state })
        })
        .collect())
}

/// How far into the song the player is, in seconds.
fn position(connection: &Connection, name: &str) -> Option<f64> {
    let value: OwnedValue = proxy(connection, name, PROPERTIES)
        .ok()?
        .call("Get", &(PLAYER, "Position"))
        .ok()?;
    number(&value).map(|micros| (micros / 1e6).max(0.0))
}

fn sources(seen: &[Seen]) -> Vec<Source> {
    let mut sources: Vec<Source> = seen
        .iter()
        .map(|s| Source {
            id: s.entry.app.clone(),
            name: s.entry.identity.clone(),
            is_playing: s.state.playing,
            is_music_app: s.entry.music,
            has_track: s.has_track(),
            launch_path: None,
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

/// The instance a source stands for. A browser can have several players under
/// one name: the playing one wins, for the display and every command.
fn instance<'a>(seen: &'a [Seen], source: &str) -> Option<&'a Seen> {
    seen.iter()
        .filter(|s| s.entry.app == source)
        .max_by_key(|s| (s.state.playing, s.has_track()))
}

fn playback(connection: &Connection, seen: &Seen, previous: Option<&Playback>) -> Option<Playback> {
    if !seen.has_track() {
        return None;
    }
    let (state, track) = (&seen.state, &seen.state.track);
    let same_song = previous.filter(|p| {
        p.source == seen.entry.app && p.title == track.title && p.artist == track.artist
    });
    let artwork = track
        .art
        .as_deref()
        .and_then(cover)
        .or_else(|| same_song.and_then(|p| p.artwork.clone()));
    let rate = state.rate.clamp(0.0, 16.0);
    let rate = if rate == 0.0 { 1.0 } else { rate };
    let (at, read) = (position(connection, &seen.entry.name).unwrap_or(0.0), now());
    // The player's position is live and the island extrapolates it, so a
    // reading that agrees with the last one is not news; a seek is.
    let (elapsed, sampled_at) = match same_song {
        Some(p)
            if p.is_playing == state.playing
                && p.rate == rate
                && (if state.playing {
                    p.elapsed + (read - p.sampled_at) * rate
                } else {
                    p.elapsed
                } - at)
                    .abs()
                    < JUMP =>
        {
            (p.elapsed, p.sampled_at)
        }
        _ => (at, read),
    };
    Some(Playback {
        source: seen.entry.app.clone(),
        source_name: seen.entry.identity.clone(),
        is_music_app: seen.entry.music,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        artwork,
        is_playing: state.playing,
        elapsed,
        duration: track.length,
        rate,
        sampled_at,
        can_play_pause: state.can_control && (state.can_play || state.can_pause),
        can_next: state.can_control && state.can_next,
        can_previous: state.can_control && state.can_previous,
        can_seek: state.can_control && state.can_seek && track.length > 0.0,
        can_shuffle: state.can_control && state.shuffle.is_some(),
        shuffle: state.shuffle.unwrap_or(false),
    })
}

/// A cover as a data URL, which the island averages for its accent. Covers
/// come from files, `data:` URLs and HTTPS; one that is not here yet is
/// fetched on its own thread, so the reader does not wait on a slow server,
/// and the next reading has it.
mod covers {
    use super::*;

    #[derive(Default)]
    struct Cache {
        done: HashMap<String, (Option<String>, Instant)>,
        busy: HashSet<String>,
    }

    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    /// A cover that failed is tried again after this, for a server that was
    /// only briefly down.
    const RETRY: Duration = Duration::from_secs(10);

    pub fn get(url: &str) -> Option<String> {
        let mut guard = CACHE.lock().ok()?;
        let cache = guard.get_or_insert_with(Cache::default);
        if let Some((cover, at)) = cache.done.get(url)
            && (cover.is_some() || at.elapsed() < RETRY)
        {
            return cover.clone();
        }
        if cache.busy.insert(url.to_string()) {
            let owned = url.to_string();
            let spawned = thread::Builder::new().name("cover".into()).spawn(move || {
                let cover = fetch(&owned);
                if let Ok(mut guard) = CACHE.lock() {
                    let cache = guard.get_or_insert_with(Cache::default);
                    cache.busy.remove(&owned);
                    if cache.done.len() >= 16 {
                        cache.done.clear();
                    }
                    cache.done.insert(owned, (cover, Instant::now()));
                }
            });
            if spawned.is_err() {
                cache.busy.remove(url);
            }
        }
        None
    }

    /// The image type of `bytes`, from what it starts with; nothing that is
    /// not a picture is ever passed on.
    pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
        match bytes {
            [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
            [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
            [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
            [b'B', b'M', ..] => Some("image/bmp"),
            [
                b'R',
                b'I',
                b'F',
                b'F',
                _,
                _,
                _,
                _,
                b'W',
                b'E',
                b'B',
                b'P',
                ..,
            ] => Some("image/webp"),
            _ => None,
        }
    }

    pub fn data_url(bytes: &[u8]) -> Option<String> {
        if bytes.is_empty() || bytes.len() > COVER_LIMIT {
            return None;
        }
        Some(format!(
            "data:{};base64,{}",
            image_type(bytes)?,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }

    pub fn fetch(url: &str) -> Option<String> {
        if url.starts_with("data:image/") && url.len() <= COVER_LIMIT * 2 {
            return Some(url.to_string());
        }
        if url.starts_with("file://") {
            return from_file(url);
        }
        if url.starts_with("https://") {
            return from_https(url);
        }
        None
    }

    fn from_file(url: &str) -> Option<String> {
        use std::io::Read;
        let path = gio::File::for_uri(url).path()?;
        let file = std::fs::File::open(path).ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(COVER_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        data_url(&bytes)
    }

    /// One GET with no cookies kept and no redirect followed, as the Windows
    /// side asks of the covers it fetches; the page's own network stack
    /// (libsoup) does the TLS.
    fn from_https(url: &str) -> Option<String> {
        use soup::prelude::*;
        let session = soup::Session::builder()
            .timeout(5)
            .idle_timeout(5)
            .user_agent(concat!("Openbar/", env!("CARGO_PKG_VERSION")))
            .build();
        let message = soup::Message::new("GET", url).ok()?;
        message.set_flags(soup::MessageFlags::NO_REDIRECT);
        let stream = session.send(&message, gio::Cancellable::NONE).ok()?;
        if message.status() != soup::Status::Ok {
            return None;
        }
        let mut bytes = Vec::new();
        loop {
            let chunk = stream.read_bytes(16 * 1024, gio::Cancellable::NONE).ok()?;
            if chunk.is_empty() {
                break;
            }
            bytes.extend_from_slice(&chunk);
            if bytes.len() > COVER_LIMIT {
                return None;
            }
        }
        data_url(&bytes)
    }
}

use covers::get as cover;

fn preferred_player<R: Runtime>(app: &AppHandle<R>) -> String {
    app.state::<crate::prefs::Prefs>()
        .get()
        .pointer("/island/preferredPlayer")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn include_other<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.state::<crate::prefs::Prefs>()
        .get()
        .pointer("/island/includeOtherPlayers")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// Reads on its own thread and emits `music://now-playing` on change.
pub fn start<R: Runtime>(app: AppHandle<R>, shared: Shared) {
    thread::Builder::new()
        .name("now-playing".into())
        .spawn(move || {
            let mut entries = HashMap::new();
            let mut last: Option<NowPlaying> = None;
            let mut last_music: Option<String> = None;
            let preferred = preferred_player(&app);
            shared.lock().unwrap().players = installed(&preferred);
            loop {
                let mut current = NowPlaying::default();
                let (manual, revision) = {
                    let state = shared.lock().unwrap();
                    (state.selected.clone(), state.revision)
                };
                current.selected_source = manual.clone();
                current.revision = revision;
                let bus = connection();
                let seen = bus.as_ref().and_then(|bus| {
                    snapshot(bus, &mut entries)
                        .inspect_err(|_| forget_connection())
                        .ok()
                });
                if let (Some(bus), Some(seen)) = (&bus, &seen) {
                    let previous = last.as_ref().and_then(|l| l.playback.as_ref());
                    current.sources = sources(seen);
                    let selected = super::selection::choose(
                        &current.sources,
                        previous.map(|p| p.source.as_str()),
                        None,
                        manual.as_deref(),
                        include_other(&app),
                    );
                    if let Some(selected) = selected {
                        current.playback = instance(seen, &selected.id)
                            .and_then(|player| playback(bus, player, previous));
                    }
                }
                if let Some(p) = current
                    .playback
                    .as_ref()
                    .filter(|p| p.is_music_app && p.is_playing)
                {
                    last_music = Some(p.source.clone());
                }
                let mut players = current.sources.clone();
                players.extend(shared.lock().unwrap().players.clone());
                current.idle_player = super::apps::preferred(
                    &preferred_player(&app),
                    last_music.as_deref(),
                    &players,
                );
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

/// Runs `act` on the instance `source` stands for, as the commands below do.
fn with_player(source: &str, act: impl FnOnce(&Connection, &Seen) -> zbus::Result<()>) -> bool {
    connection().is_some_and(|bus| with_player_on(&bus, source, act))
}

fn with_player_on(
    bus: &Connection,
    source: &str,
    act: impl FnOnce(&Connection, &Seen) -> zbus::Result<()>,
) -> bool {
    let Ok(seen) = snapshot(bus, &mut HashMap::new()) else {
        forget_connection();
        return false;
    };
    instance(&seen, source).is_some_and(|player| act(bus, player).is_ok())
}

fn call(connection: &Connection, name: &str, method: &str) -> zbus::Result<()> {
    proxy(connection, name, PLAYER)?.call(method, &())
}

fn press(command: Command, bus: &Connection, player: &Seen) -> zbus::Result<()> {
    call(
        bus,
        &player.entry.name,
        match command {
            Command::Toggle => "PlayPause",
            Command::Next => "Next",
            Command::Previous => "Previous",
        },
    )
}

fn move_to(seconds: f64, bus: &Connection, player: &Seen) -> zbus::Result<()> {
    let micros = (seconds.max(0.0) * 1e6) as i64;
    let player_proxy = proxy(bus, &player.entry.name, PLAYER)?;
    // `SetPosition` wants the track's object path; a player that does not
    // give one is moved by how far it is from there.
    match player
        .state
        .track
        .id
        .as_deref()
        .and_then(|id| ObjectPath::try_from(id).ok())
        .filter(|id| id.as_str() != "/org/mpris/MediaPlayer2/TrackList/NoTrack")
    {
        Some(id) => player_proxy.call("SetPosition", &(id, micros)),
        None => {
            let here = position(bus, &player.entry.name).unwrap_or(0.0);
            player_proxy.call("Seek", &(micros - (here * 1e6) as i64))
        }
    }
}

fn shuffle_to(on: bool, bus: &Connection, player: &Seen) -> zbus::Result<()> {
    proxy(bus, &player.entry.name, PROPERTIES)?.call("Set", &(PLAYER, "Shuffle", Value::from(on)))
}

pub fn command(command: Command, source: &str) -> bool {
    with_player(source, |bus, player| press(command, bus, player))
}

pub fn seek(seconds: f64, source: &str) -> bool {
    with_player(source, |bus, player| move_to(seconds, bus, player))
}

pub fn set_shuffle(on: bool, source: &str) -> bool {
    with_player(source, |bus, player| shuffle_to(on, bus, player))
}

/// Ids `installed` offered, the only ones `open_player` starts: the UI names
/// a player, never a program.
static OFFERED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The music players in the applications menu, plus the one chosen as
/// preferred whatever it is. One scan on demand, never a loop.
pub fn installed(preferred: &str) -> Vec<Source> {
    let mut players = Vec::new();
    for info in gio::AppInfo::all() {
        let Some(desktop) = info.dynamic_cast_ref::<gio::DesktopAppInfo>() else {
            continue;
        };
        let Some(file) = info.id() else { continue };
        if !info.should_show() {
            continue;
        }
        let id = file.trim_end_matches(".desktop").to_string();
        let name = info.name().to_string();
        let music = known_music_app(&id)
            || known_music_app(&name)
            || desktop
                .string("Categories")
                .is_some_and(|categories| categories_say_music(&categories));
        if !id.is_empty() && (id == preferred || music) {
            players.push(Source {
                id,
                name,
                is_playing: false,
                is_music_app: music,
                has_track: false,
                launch_path: None,
            });
        }
    }
    players.sort_by(|a, b| a.name.cmp(&b.name));
    players.dedup_by(|a, b| a.id == b.id);
    if let Ok(mut offered) = OFFERED.lock() {
        *offered = Some(players.iter().map(|p| p.id.clone()).collect());
    }
    players
}

fn valid_source(source: &str) -> bool {
    !source.is_empty()
        && source.len() <= 255
        && source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// The process that owns a name on the bus.
fn owner_pid(connection: &Connection, name: &str) -> Option<u32> {
    Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .ok()?
    .call("GetConnectionUnixProcessID", &(name,))
    .ok()
}

/// Brings a running player forward: it is asked to raise itself, and a player
/// that cannot (a browser) has its window raised through the window manager.
fn raise(connection: &Connection, player: &Seen) -> bool {
    let name = player.entry.name.as_str();
    let root = all_properties(connection, name, ROOT).unwrap_or_default();
    if root.get("CanRaise").and_then(|v| flag(v)) == Some(true)
        && proxy(connection, name, ROOT)
            .and_then(|p| p.call::<_, _, ()>("Raise", &()))
            .is_ok()
    {
        return true;
    }
    owner_pid(connection, name).is_some_and(|pid| crate::linux::x11::raise(&[pid], ""))
}

/// Raises the player if it runs, else starts it, but only if the menu offered it.
pub fn open_player(source: &str) -> bool {
    if !valid_source(source) {
        return false;
    }
    if let Some(bus) = connection()
        && let Ok(seen) = snapshot(&bus, &mut HashMap::new())
        && let Some(player) = instance(&seen, source)
        && raise(&bus, player)
    {
        return true;
    }
    let offered = OFFERED
        .lock()
        .ok()
        .and_then(|offered| offered.as_ref().map(|ids| ids.contains(source)))
        .unwrap_or(false);
    offered
        && gio::DesktopAppInfo::new(&format!("{source}.desktop"))
            .is_some_and(|info| info.launch(&[], gio::AppLaunchContext::NONE).is_ok())
}

/// `--music-probe`: what the reader sees, step by step, for diagnosing a machine.
pub fn probe() -> String {
    let mut out = String::new();
    let Some(bus) = connection() else {
        return "no session bus\n".into();
    };
    let mut entries = HashMap::new();
    match snapshot(&bus, &mut entries) {
        Ok(seen) => {
            out.push_str(&format!("players: {}\n", seen.len()));
            for player in &seen {
                let track = &player.state.track;
                out.push_str(&format!(
                    "  {} ({}) as {} playing={} music={}\n    {} - {} ({:.0}/{:.0}s) art={}\n",
                    player.entry.identity,
                    player.entry.name,
                    player.entry.app,
                    player.state.playing,
                    player.entry.music,
                    track.title,
                    track.artist,
                    position(&bus, &player.entry.name).unwrap_or(0.0),
                    track.length,
                    track.art.as_deref().unwrap_or("none"),
                ));
            }
        }
        Err(e) => out.push_str(&format!("ListNames failed: {e}\n")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(v: impl Into<Value<'static>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).unwrap()
    }

    #[test]
    fn a_player_goes_by_its_desktop_entry_else_its_bus_name_without_the_instance() {
        let bus = |name: &str| format!("{PREFIX}{name}");
        assert_eq!(app_id(&bus("spotify"), Some("spotify")), "spotify");
        assert_eq!(
            app_id(&bus("x"), Some("org.gnome.Rhythmbox3.desktop")),
            "org.gnome.Rhythmbox3"
        );
        assert_eq!(app_id(&bus("vlc"), None), "vlc");
        assert_eq!(app_id(&bus("vlc"), Some("")), "vlc");
        assert_eq!(app_id(&bus("chromium.instance4242"), None), "chromium");
        assert_eq!(app_id(&bus("firefox.instance_1_72"), None), "firefox");
        assert_eq!(app_id(&bus("Lollypop"), None), "Lollypop");
    }

    #[test]
    fn categories_name_the_music_players_and_leave_the_video_ones() {
        assert!(categories_say_music("Audio;Music;Player;AudioVideo;"));
        assert!(categories_say_music("GNOME;GTK;AudioVideo;Audio;Player;"));
        assert!(!categories_say_music("AudioVideo;Audio;Video;Player;TV;"));
        assert!(!categories_say_music("Network;WebBrowser;"));
        assert!(!categories_say_music("AudioVideo;Player;Recorder;"));
        assert!(known_music_app("spotify") && known_music_app("org.gnome.Rhythmbox3"));
        assert!(!known_music_app("firefox") && !known_music_app("mpv"));
    }

    fn metadata(entries: Vec<(&str, Value<'static>)>) -> OwnedValue {
        let mut dict = zbus::zvariant::Dict::new(
            &zbus::zvariant::Signature::Str,
            &zbus::zvariant::Signature::Variant,
        );
        for (key, v) in entries {
            dict.append(Value::from(key.to_string()), Value::Value(Box::new(v)))
                .unwrap();
        }
        value(Value::Dict(dict))
    }

    #[test]
    fn the_metadata_of_different_players_reads_the_same() {
        // Spotify: track id as an object path, length as a signed, artist as a list.
        let spotify = metadata(vec![
            (
                "mpris:trackid",
                Value::from(ObjectPath::try_from("/com/spotify/track/abc").unwrap()),
            ),
            ("mpris:length", Value::from(215_000_000_i64)),
            ("mpris:artUrl", Value::from("https://i.scdn.co/image/xyz")),
            ("xesam:title", Value::from("Song")),
            ("xesam:album", Value::from("Album")),
            ("xesam:artist", Value::from(vec!["A", "B"])),
        ]);
        let track = track_of(&spotify);
        assert_eq!(track.id.as_deref(), Some("/com/spotify/track/abc"));
        assert_eq!(track.length, 215.0);
        assert_eq!(track.artist, "A, B");
        assert_eq!(track.art.as_deref(), Some("https://i.scdn.co/image/xyz"));
        // A browser: the length unsigned, the artist a plain string, the album artist as a fallback.
        let browser = metadata(vec![
            ("mpris:length", Value::from(90_000_000_u64)),
            ("xesam:title", Value::from("Tab")),
            ("xesam:artist", Value::from("Channel")),
        ]);
        let track = track_of(&browser);
        assert_eq!((track.length, track.artist.as_str()), (90.0, "Channel"));
        let fallback = metadata(vec![
            ("xesam:title", Value::from("T")),
            ("xesam:albumArtist", Value::from(vec!["Band"])),
        ]);
        assert_eq!(track_of(&fallback).artist, "Band");
        // Nothing sensible is nothing.
        assert_eq!(track_of(&value(5_i32)), Track::default());
    }

    #[test]
    fn the_state_has_defaults_for_what_a_player_leaves_out() {
        let mut properties = HashMap::new();
        properties.insert("PlaybackStatus".to_string(), value("Playing"));
        properties.insert("CanGoNext".to_string(), value(true));
        let state = state_of(&properties);
        assert!(state.playing && state.can_next);
        assert!(state.can_control && state.can_play && state.can_pause);
        assert!(!state.can_previous && !state.can_seek);
        assert_eq!((state.rate, state.shuffle), (1.0, None));
        properties.insert("PlaybackStatus".to_string(), value("Paused"));
        properties.insert("Shuffle".to_string(), value(true));
        let state = state_of(&properties);
        assert!(!state.playing);
        assert_eq!(state.shuffle, Some(true));
    }

    #[test]
    fn only_pictures_become_covers() {
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let url = covers::data_url(&png).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(
            covers::image_type(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(
            covers::image_type(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        assert!(covers::data_url(b"-----BEGIN OPENSSH PRIVATE KEY-----").is_none());
        assert!(covers::data_url(&[]).is_none());
        assert!(covers::data_url(&vec![0x89; COVER_LIMIT + 1]).is_none());
    }

    #[test]
    fn a_cover_file_is_read_but_not_a_directory_or_a_text_file() {
        let dir = std::env::temp_dir().join(format!("openbar-cover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a b.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 9]).unwrap();
        let text = dir.join("secret.png");
        std::fs::write(&text, "not a picture").unwrap();
        let uri = |p: &std::path::Path| gio::File::for_path(p).uri().to_string();
        assert!(
            covers::fetch(&uri(&png))
                .unwrap()
                .starts_with("data:image/png")
        );
        assert!(covers::fetch(&uri(&text)).is_none());
        assert!(covers::fetch(&uri(&dir)).is_none());
        assert!(covers::fetch("file:///nonexistent/cover.png").is_none());
        for url in [
            "http://example.com/a.png",
            "ftp://x/a.png",
            "/etc/passwd",
            "",
        ] {
            assert!(covers::fetch(url).is_none(), "{url}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_ui_names_a_player_never_a_program() {
        for good in ["spotify", "org.gnome.Rhythmbox3", "tidal-hifi", "a_b+c"] {
            assert!(valid_source(good), "{good}");
        }
        for bad in [
            "",
            "../x",
            "/usr/bin/x",
            "a b",
            "a;b",
            "a\nb",
            "$(x)",
            "x.desktop\0",
        ] {
            assert!(!valid_source(bad), "{bad:?}");
        }
        assert!(!open_player("not-in-the-menu-at-all"));
    }

    // The reader against a bus: a private `dbus-daemon` and players written
    // here, so the calls and the replies are the real ones.

    use std::io::{BufRead, BufReader};
    use std::process::{Child, Stdio};
    use std::sync::Arc;

    /// A bus of its own, gone with the value. `None` where `dbus-daemon` is
    /// not installed.
    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            // Tests run side by side in one process: a folder for each bus.
            static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "openbar-bus-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).ok()?;
            let config = dir.join("bus.conf");
            std::fs::write(
                &config,
                r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
                <policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>"#,
            )
            .ok()?;
            let mut child = std::process::Command::new("dbus-daemon")
                .args(["--nofork", "--print-address=1", "--config-file"])
                .arg(&config)
                .stdout(Stdio::piped())
                .spawn()
                .ok()?;
            let mut address = String::new();
            BufReader::new(child.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            std::fs::remove_dir_all(dir).ok();
            Some(Self {
                child,
                address: address.trim().to_string(),
            })
        }

        fn connect(&self) -> Connection {
            Builder::address(self.address.as_str())
                .unwrap()
                .method_timeout(REPLY)
                .build()
                .unwrap()
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            // Asked to end, the daemon removes its socket; killed, it leaves it.
            // SAFETY: a signal to a child this value started and has not reaped.
            unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
            let _ = self.child.wait();
        }
    }

    type Log = Arc<Mutex<Vec<String>>>;

    struct FakeRoot(Log, Option<&'static str>);

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl FakeRoot {
        fn raise(&self) {
            self.0.lock().unwrap().push("Raise".into());
        }
        #[zbus(property)]
        fn can_raise(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn identity(&self) -> String {
            "Fake Player".into()
        }
        #[zbus(property)]
        fn desktop_entry(&self) -> String {
            self.1.unwrap_or_default().into()
        }
    }

    struct FakePlayer {
        log: Log,
        playing: bool,
        title: &'static str,
        art: String,
        position: i64,
        shuffle: bool,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl FakePlayer {
        fn play_pause(&mut self) {
            self.log.lock().unwrap().push("PlayPause".into());
        }
        fn next(&self) {
            self.log.lock().unwrap().push("Next".into());
        }
        fn previous(&self) {
            self.log.lock().unwrap().push("Previous".into());
        }
        fn set_position(&mut self, track: ObjectPath<'_>, position: i64) {
            self.log
                .lock()
                .unwrap()
                .push(format!("SetPosition {track} {position}"));
            self.position = position;
        }
        #[zbus(property)]
        fn playback_status(&self) -> String {
            if self.playing { "Playing" } else { "Paused" }.into()
        }
        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, Value<'static>> {
            HashMap::from([
                (
                    "mpris:trackid".to_string(),
                    Value::from(ObjectPath::try_from("/track/1").unwrap()),
                ),
                ("mpris:length".to_string(), Value::from(215_000_000_i64)),
                ("mpris:artUrl".to_string(), Value::from(self.art.clone())),
                ("xesam:title".to_string(), Value::from(self.title)),
                ("xesam:artist".to_string(), Value::from(vec!["A", "B"])),
            ])
        }
        #[zbus(property)]
        fn position(&self) -> i64 {
            self.position
        }
        #[zbus(property)]
        fn rate(&self) -> f64 {
            1.0
        }
        #[zbus(property)]
        fn shuffle(&self) -> bool {
            self.shuffle
        }
        #[zbus(property)]
        fn set_shuffle(&mut self, on: bool) {
            self.shuffle = on;
        }
        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_seek(&self) -> bool {
            true
        }
    }

    /// A player on `bus` as `name`, for as long as the connection is held.
    fn player(
        bus: &PrivateBus,
        name: &str,
        desktop: Option<&'static str>,
        playing: bool,
        title: &'static str,
        art: &str,
    ) -> (Connection, Log) {
        let log = Log::default();
        let connection = Builder::address(bus.address.as_str())
            .unwrap()
            .name(format!("{PREFIX}{name}"))
            .unwrap()
            .serve_at(PATH, FakeRoot(log.clone(), desktop))
            .unwrap()
            .serve_at(
                PATH,
                FakePlayer {
                    log: log.clone(),
                    playing,
                    title,
                    art: art.into(),
                    position: 12_500_000,
                    shuffle: false,
                },
            )
            .unwrap()
            .build()
            .unwrap();
        (connection, log)
    }

    #[test]
    fn a_player_on_a_bus_is_read_and_obeyed() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let dir = std::env::temp_dir().join(format!("openbar-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("cover.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
        let art = gio::File::for_path(&png).uri().to_string();

        // Nothing plays: no source.
        let client = bus.connect();
        let mut entries = HashMap::new();
        assert!(snapshot(&client, &mut entries).unwrap().is_empty());

        let (fake, log) = player(&bus, "fake", Some("fake-player"), true, "Song", &art);
        let seen = snapshot(&client, &mut entries).unwrap();
        assert_eq!(seen.len(), 1);
        let first = &seen[0];
        assert_eq!(first.entry.app, "fake-player");
        assert_eq!(first.entry.identity, "Fake Player");
        assert!(first.state.playing && first.has_track());
        assert_eq!(
            (
                first.state.track.title.as_str(),
                first.state.track.artist.as_str()
            ),
            ("Song", "A, B")
        );
        let listed = sources(&seen);
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (
                listed[0].id.as_str(),
                listed[0].name.as_str(),
                listed[0].has_track
            ),
            ("fake-player", "Fake Player", true)
        );

        // What the island gets: the text at once, the cover when its thread is done.
        let read = playback(&client, first, None).unwrap();
        assert_eq!(
            (read.title.as_str(), read.artist.as_str()),
            ("Song", "A, B")
        );
        assert_eq!((read.elapsed, read.duration, read.rate), (12.5, 215.0, 1.0));
        assert!(read.is_playing && read.can_play_pause && read.can_next && read.can_seek);
        assert!(!read.can_previous && read.can_shuffle && !read.shuffle);
        let mut with_cover = read.clone();
        for _ in 0..100 {
            with_cover = playback(&client, first, Some(&read)).unwrap();
            if with_cover.artwork.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            with_cover
                .artwork
                .as_deref()
                .is_some_and(|a| a.starts_with("data:image/png;base64,"))
        );

        // The same song read again a moment later is not news; a seek is.
        thread::sleep(Duration::from_millis(30));
        let again = playback(&client, first, Some(&with_cover)).unwrap();
        assert_eq!(
            (again.elapsed, again.sampled_at),
            (with_cover.elapsed, with_cover.sampled_at)
        );
        assert!(with_player_on(&client, "fake-player", |bus, p| move_to(
            100.0, bus, p
        )));
        assert_eq!(
            log.lock().unwrap().last().map(String::as_str),
            Some("SetPosition /track/1 100000000")
        );
        let seen = snapshot(&client, &mut entries).unwrap();
        let sought = playback(&client, &seen[0], Some(&again)).unwrap();
        assert_eq!(sought.elapsed, 100.0);
        assert!(sought.sampled_at > again.sampled_at);

        // Commands reach the player.
        for (command, wanted) in [
            (Command::Toggle, "PlayPause"),
            (Command::Next, "Next"),
            (Command::Previous, "Previous"),
        ] {
            assert!(with_player_on(&client, "fake-player", |bus, p| press(
                command, bus, p
            )));
            assert_eq!(log.lock().unwrap().last().map(String::as_str), Some(wanted));
        }
        assert!(with_player_on(&client, "fake-player", |bus, p| shuffle_to(
            true, bus, p
        )));
        assert_eq!(
            snapshot(&client, &mut entries).unwrap()[0].state.shuffle,
            Some(true)
        );
        assert!(!with_player_on(&client, "no-such-player", |bus, p| press(
            Command::Toggle,
            bus,
            p
        )));
        let seen = snapshot(&client, &mut entries).unwrap();
        // The pid a browser's window is looked up by is the one that owns the name.
        assert_eq!(
            owner_pid(&client, &seen[0].entry.name),
            Some(std::process::id())
        );
        assert!(raise(&client, &seen[0]));
        assert_eq!(
            log.lock().unwrap().last().map(String::as_str),
            Some("Raise")
        );

        // A player that is gone is gone, and a hung one cannot be waited on forever
        // (the reply limit is set on the connection).
        drop(fake);
        // The bus may still list the name for a moment; an unreachable player is kept one more round.
        let gone = (0..100).any(|_| {
            let empty = snapshot(&client, &mut entries).unwrap().is_empty() && entries.is_empty();
            if !empty {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            empty
        });
        assert!(gone, "the player is still known: {} entries", entries.len());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_browser_with_two_players_is_one_source_and_the_playing_one_answers() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let _quiet = player(&bus, "chromium.instance10", None, false, "Paused tab", "");
        let (_loud, log) = player(&bus, "chromium.instance20", None, true, "Playing tab", "");
        let client = bus.connect();
        let seen = snapshot(&client, &mut HashMap::new()).unwrap();
        assert_eq!(seen.len(), 2);
        let listed = sources(&seen);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "chromium");
        assert!(listed[0].is_playing);
        assert_eq!(
            instance(&seen, "chromium").unwrap().state.track.title,
            "Playing tab"
        );
        assert!(with_player_on(&client, "chromium", |bus, p| press(
            Command::Toggle,
            bus,
            p
        )));
        assert_eq!(log.lock().unwrap().as_slice(), ["PlayPause"]);
    }

    #[test]
    #[ignore = "reads the players on the session bus of a desktop that has some"]
    fn the_players_on_the_session_bus_are_read_and_the_chosen_one_obeyed() {
        println!(
            "installed: {:?}",
            installed("")
                .iter()
                .map(|p| (&p.id, &p.name))
                .collect::<Vec<_>>()
        );
        let bus = connection().expect("no session bus");
        let mut entries = HashMap::new();
        let seen = snapshot(&bus, &mut entries).unwrap();
        assert!(!seen.is_empty(), "No player on the bus to read");
        for player in &seen {
            let mut read = None;
            // A cover is fetched on its own thread; give it a moment.
            for _ in 0..100 {
                read = playback(&bus, player, read.as_ref());
                if read.as_ref().is_none_or(|p| p.artwork.is_some()) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            println!(
                "{} ({}) music={} -> {:?}",
                player.entry.app,
                player.entry.name,
                player.entry.music,
                read.map(|p| (
                    p.title,
                    p.artist,
                    p.album,
                    p.elapsed,
                    p.duration,
                    p.is_playing,
                    p.can_seek,
                    p.can_next,
                    p.artwork.map(|a| a.len())
                ))
            );
        }
        // Only a player named here is touched: the point is to not pause anyone's music.
        let Ok(chosen) = std::env::var("OPENBAR_LIVE_CONTROL") else {
            return;
        };
        let state = |entries: &mut HashMap<String, Entry>| {
            let seen = snapshot(&bus, entries).unwrap();
            let player = instance(&seen, &chosen)
                .expect("the chosen player is gone")
                .clone();
            let at = position(&bus, &player.entry.name).unwrap_or(-1.0);
            (player.state.playing, at)
        };
        let (playing, _) = state(&mut entries);
        assert!(command(Command::Toggle, &chosen));
        thread::sleep(Duration::from_millis(600));
        assert_eq!(
            state(&mut entries).0,
            !playing,
            "toggle did not flip the state"
        );
        assert!(command(Command::Toggle, &chosen));
        thread::sleep(Duration::from_millis(600));
        assert_eq!(
            state(&mut entries).0,
            playing,
            "toggle did not flip it back"
        );
        assert!(seek(20.0, &chosen));
        thread::sleep(Duration::from_millis(600));
        let (_, at) = state(&mut entries);
        assert!((at - 20.0).abs() < 2.0, "seek landed at {at}");
        println!("controlled {chosen}: toggle twice and seek to 20 s, now at {at:.1} s");
        println!("open_player({chosen}) -> {}", open_player(&chosen));
    }
}
