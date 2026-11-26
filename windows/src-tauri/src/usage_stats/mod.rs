// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The anonymous daily usage ping (#333), as on the Mac
//! (`Sources/Openbar/Services/UsageStats`): a minute after launch when the
//! last one is more than 20 hours old, then about every 24 hours while the
//! app runs, and 6 hours after a failure. It carries a random install id,
//! the app and system versions, and counts of sessions per agent, terminals
//! and island sections; docs/PRIVACY.md lists every field. Nothing is
//! counted or sent with Share anonymous usage stats off, with
//! `DO_NOT_TRACK=1`, or from a debug build. The id lives in settings.json
//! (`statsInstallId`), the counts in `usage-stats.json` beside it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use crate::sessions::models::{HostKind, Session};

#[cfg(windows)]
mod http;

#[cfg(target_os = "linux")]
#[path = "../linux/usage_stats.rs"]
mod http;

pub const ENDPOINT: &str = "https://openbar-stats.vercel.app/api/v1/ping";
pub const LAUNCH_DELAY: Duration = Duration::from_secs(60);
pub const CHECK_EVERY: Duration = Duration::from_secs(3600);
pub const LAUNCH_THRESHOLD: f64 = 20.0 * 3600.0;
pub const RUNNING_THRESHOLD: f64 = 24.0 * 3600.0;
pub const RETRY: f64 = 6.0 * 3600.0;
pub const SEEN_LIFETIME: f64 = 3.0 * 86_400.0;
pub const SEEN_LIMIT: usize = 2_000;
pub const MAX_BYTES: usize = 4_096;
pub const MAX_COUNT: u32 = 100_000;
/// The settings.json key of the id, and the setting itself.
pub const ID_KEY: &str = "statsInstallId";
pub const SETTING: &str = "/island/shareUsageStats";
/// Plain http is accepted only for these hosts: this PC, and the Mac as a
/// QEMU or Lima test machine sees it.
const LOCAL_HOSTS: [&str; 5] = [
    "localhost",
    "127.0.0.1",
    "[::1]",
    "10.0.2.2",
    "host.lima.internal",
];
/// A debug build is the developer build: it never sends anything.
const DEVELOPER: bool = cfg!(debug_assertions);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub at: f64,
    pub terminal: bool,
}

/// Counts since the last delivered ping, the sessions already counted, and
/// when it last tried.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub sessions: BTreeMap<String, u32>,
    pub terminals: BTreeMap<String, u32>,
    pub opened: BTreeMap<String, u32>,
    pub seen: BTreeMap<String, Seen>,
    pub last_success: Option<f64>,
    pub last_failure: Option<f64>,
}

/// A session the ping may count; its id never leaves the PC.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: String,
    pub provider: String,
    pub terminal: Option<String>,
}

/// Read right before a send.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub platform: &'static str,
    pub os_version: String,
    pub arch: String,
    pub app_version: String,
    pub hooks: BTreeMap<String, bool>,
    pub enabled: Vec<String>,
}

/// Where a ping goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Stored, over the day's limit or refused: a new day starts either way.
    Delivered,
    /// Network or server trouble: the counts stay, and it tries in 6 hours.
    Failed,
}

pub fn is_key(value: &str) -> bool {
    let mut chars = value.chars();
    value.len() <= 32
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `DO_NOT_TRACK=1` (or true/yes) turns the ping off whatever the setting says.
pub fn do_not_track(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// `OPENBAR_STATS_URL` for tests, the dashboard otherwise. An override that
/// isn't https, or plain http to a local address, sends nothing at all.
pub fn endpoint(override_url: Option<&str>) -> Option<Target> {
    let raw = override_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(ENDPOINT);
    let (secure, rest) = match raw.strip_prefix("https://") {
        Some(rest) => (true, rest),
        None => (false, raw.strip_prefix("http://")?),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() || authority.contains('@') || path.contains(char::is_whitespace) {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') || authority.starts_with('[') => {
            if host.is_empty() {
                return None;
            }
            (host, port.parse::<u16>().ok()?)
        }
        _ => (authority, if secure { 443 } else { 80 }),
    };
    let host = host.to_ascii_lowercase();
    if !secure && !LOCAL_HOSTS.contains(&host.as_str()) {
        return None;
    }
    Some(Target {
        secure,
        host,
        port,
        path: path.to_string(),
    })
}

pub fn allowed(setting_on: bool, developer: bool, dnt: bool) -> bool {
    setting_on && !developer && !dnt
}

/// A minute after launch it goes when the last one is 20 hours old, later
/// every 24 hours; a failure waits 6 hours. A date in the future counts as never.
pub fn is_due(now: f64, state: &State, at_launch: bool) -> bool {
    let threshold = if at_launch {
        LAUNCH_THRESHOLD
    } else {
        RUNNING_THRESHOLD
    };
    if let Some(success) = state.last_success
        && success <= now
        && now - success < threshold
    {
        return false;
    }
    if let Some(failure) = state.last_failure
        && failure <= now
        && now - failure < RETRY
    {
        return false;
    }
    true
}

fn bump(counts: &mut BTreeMap<String, u32>, key: &str) {
    let count = counts.entry(key.to_string()).or_insert(0);
    *count = (*count + 1).min(MAX_COUNT);
}

pub fn prune(state: &mut State, now: f64) {
    state
        .seen
        .retain(|_, seen| seen.at <= now && now - seen.at < SEEN_LIFETIME);
}

/// Counts each session once, and its terminal once when first known. A
/// session counted in the last three days isn't counted again after a restart.
pub fn record(state: &mut State, notes: &[Note], now: f64) {
    prune(state, now);
    for note in notes {
        if note.id.is_empty() || note.id == crate::sessions::sample::ID || !is_key(&note.provider) {
            continue;
        }
        if !state.seen.contains_key(&note.id) {
            bump(&mut state.sessions, &note.provider);
            state.seen.insert(
                note.id.clone(),
                Seen {
                    at: now,
                    terminal: false,
                },
            );
        }
        if let Some(terminal) = note.terminal.as_deref().filter(|t| is_key(t))
            && let Some(seen) = state.seen.get_mut(&note.id)
            && !seen.terminal
        {
            seen.terminal = true;
            bump(&mut state.terminals, terminal);
        }
    }
    if state.seen.len() > SEEN_LIMIT {
        let mut oldest: Vec<(String, f64)> = state
            .seen
            .iter()
            .map(|(id, s)| (id.clone(), s.at))
            .collect();
        oldest.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (id, _) in oldest.into_iter().take(state.seen.len() - SEEN_LIMIT) {
            state.seen.remove(&id);
        }
    }
}

pub fn record_open(state: &mut State, section: &str) {
    if is_key(section) {
        bump(&mut state.opened, section);
    }
}

/// Schema v1, exactly the fields docs/PRIVACY.md lists.
pub fn payload(install_id: &str, context: &Context, state: &State) -> Value {
    let mut agents = serde_json::Map::new();
    let providers: std::collections::BTreeSet<&String> =
        state.sessions.keys().chain(context.hooks.keys()).collect();
    for provider in providers.into_iter().filter(|p| is_key(p)) {
        let sessions = state.sessions.get(provider).copied().unwrap_or(0);
        let hooks = context.hooks.get(provider).copied().unwrap_or(false);
        if sessions > 0 || hooks {
            agents.insert(
                provider.clone(),
                json!({ "sessions": sessions, "hooks": hooks }),
            );
        }
    }
    let positive = |counts: &BTreeMap<String, u32>| -> BTreeMap<String, u32> {
        counts
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(k, n)| (k.clone(), *n))
            .collect()
    };
    json!({
        "v": 1,
        "installId": install_id,
        "platform": context.platform,
        "osVersion": context.os_version,
        "arch": context.arch,
        "appVersion": context.app_version,
        "agents": agents,
        "terminals": positive(&state.terminals),
        "features": {
            "enabled": context.enabled.iter().filter(|s| is_key(s)).collect::<Vec<_>>(),
            "opened": positive(&state.opened),
        },
    })
}

/// The body as sent, at most 4 KB: the smallest terminal counts go first,
/// then the smallest section counts.
pub fn encode(mut payload: Value) -> Option<Vec<u8>> {
    loop {
        let bytes = serde_json::to_vec(&payload).ok()?;
        if bytes.len() <= MAX_BYTES {
            return Some(bytes);
        }
        let smallest = |map: &serde_json::Map<String, Value>| {
            map.iter()
                .min_by(|a, b| a.1.as_u64().cmp(&b.1.as_u64()).then(a.0.cmp(b.0)))
                .map(|(k, _)| k.clone())
        };
        if let Some(terminals) = payload.get_mut("terminals").and_then(Value::as_object_mut)
            && let Some(key) = smallest(terminals)
        {
            terminals.remove(&key);
            continue;
        }
        let opened = payload
            .pointer_mut("/features/opened")
            .and_then(Value::as_object_mut)?;
        let key = smallest(opened)?;
        opened.remove(&key);
    }
}

pub fn outcome(status: Option<u16>) -> Outcome {
    match status {
        Some(200..=299 | 400 | 413 | 422 | 429) => Outcome::Delivered,
        _ => Outcome::Failed,
    }
}

/// After a delivery, takes away what was sent, so counts that came in while
/// the ping was on its way are kept for the next one.
pub fn settle(state: &mut State, sent: &State, outcome: Outcome, now: f64) {
    match outcome {
        Outcome::Delivered => {
            for (counts, gone) in [
                (&mut state.sessions, &sent.sessions),
                (&mut state.terminals, &sent.terminals),
                (&mut state.opened, &sent.opened),
            ] {
                for (key, n) in gone {
                    let left = counts.get(key).copied().unwrap_or(0).saturating_sub(*n);
                    if left > 0 {
                        counts.insert(key.clone(), left);
                    } else {
                        counts.remove(key);
                    }
                }
            }
            state.last_success = Some(now);
            state.last_failure = None;
        }
        Outcome::Failed => state.last_failure = Some(now),
    }
}

/// The terminal a session runs in, as a fixed id; app names never leave the
/// PC, so anything unlisted is "other". None while it isn't known yet.
pub fn terminal_id(host: HostKind, name: Option<&str>) -> Option<&'static str> {
    Some(match host {
        HostKind::WindowsTerminal => "windows-terminal",
        HostKind::Conhost => "conhost",
        HostKind::Wezterm => "wezterm",
        HostKind::Tmux => "tmux",
        HostKind::Zellij => "zellij",
        HostKind::Kitty => "kitty",
        HostKind::Background => "background",
        HostKind::Remote => "remote",
        HostKind::Vscode => match name {
            Some("Cursor") => "cursor",
            Some("Windsurf") => "windsurf",
            Some("Antigravity") => "antigravity",
            Some("VSCodium") => "vscodium",
            _ => "vscode",
        },
        HostKind::App => match name {
            Some("WezTerm") => "wezterm",
            Some("Git Bash") => "git-bash",
            Some("Alacritty") => "alacritty",
            Some("Warp") => "warp",
            Some("Tabby") => "tabby",
            Some("Hyper") => "hyper",
            Some("Rio") => "rio",
            Some("kitty") => "kitty",
            Some("Ghostty") => "ghostty",
            Some("GNOME Terminal") => "gnome-terminal",
            Some("Konsole") => "konsole",
            Some("Xfce Terminal") => "xfce-terminal",
            Some("Tilix") => "tilix",
            Some("Terminator") => "terminator",
            Some("foot") => "foot",
            Some("ZCode") => "zcode",
            _ => "other",
        },
        HostKind::Unknown => return None,
    })
}

pub fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x86_64",
    }
}

/// The OS as the dashboard reads it: `10.0.<build>` on Windows, the
/// distribution's name and version on Linux, only in plain characters.
pub fn os_version(name: &str, version: &str, build: Option<&str>) -> String {
    if cfg!(windows) {
        let build = build
            .and_then(|b| b.split('.').next())
            .filter(|b| b.chars().all(|c| c.is_ascii_digit()));
        return match build {
            Some(b) if !b.is_empty() => format!("10.0.{b}"),
            _ => "10.0".into(),
        };
    }
    let text = format!("{name} {version}");
    let clean: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " ._()/-".contains(c) {
                c
            } else {
                ' '
            }
        })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = clean.chars().take(48).collect();
    if clean.trim().is_empty() {
        "Linux".into()
    } else {
        clean.trim().to_string()
    }
}

/// The island sections that are on, in their order (`island/modules.ts`).
pub fn enabled_sections(prefs: &Value) -> Vec<String> {
    const MODULES: [&str; 17] = [
        "controls",
        "mixer",
        "music",
        "clipboard",
        "captures",
        "files",
        "system",
        "tools",
        "calendar",
        "notifications",
        "timer",
        "camera",
        "downloads",
        "scratchpad",
        "agents",
        "sessions",
        "watch",
    ];
    if prefs.pointer("/island/enabled").and_then(Value::as_bool) == Some(false) {
        return Vec::new();
    }
    let list = |key: &str| -> Vec<String> {
        prefs
            .pointer(&format!("/island/{key}"))
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let hidden = list("hiddenModules");
    let mut order: Vec<String> = Vec::new();
    for id in list("moduleOrder")
        .into_iter()
        .chain(MODULES.iter().map(|m| m.to_string()))
    {
        if MODULES.contains(&id.as_str()) && !order.contains(&id) {
            order.push(id);
        }
    }
    order
        .into_iter()
        .filter(|id| !hidden.contains(id))
        .filter(|id| {
            prefs
                .pointer(&format!("/island/sections/{id}"))
                .and_then(Value::as_bool)
                .unwrap_or(true)
        })
        .collect()
}

pub fn setting_on(prefs: &Value) -> bool {
    prefs
        .pointer(SETTING)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// A random UUID (v4); None rather than a predictable one when the system
/// has no randomness to give.
pub fn new_id() -> Option<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).ok()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The counts on disk, written whenever they change.
pub struct Store {
    path: Option<PathBuf>,
    state: Mutex<State>,
    sending: AtomicBool,
    /// Whether counting is allowed right now (setting, DNT, build).
    active: AtomicBool,
}

impl Store {
    pub fn open(path: Option<PathBuf>) -> Store {
        let state = path
            .as_deref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Store {
            path,
            state: Mutex::new(state),
            sending: AtomicBool::new(false),
            active: AtomicBool::new(false),
        }
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::SeqCst);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn change(&self, edit: impl FnOnce(&mut State)) {
        let mut state = self.state.lock().unwrap();
        let before = state.clone();
        edit(&mut state);
        if *state != before {
            save(self.path.as_deref(), &state);
        }
    }

    pub fn note(&self, notes: &[Note], now: f64) {
        if self.is_active() && !notes.is_empty() {
            self.change(|state| record(state, notes, now));
        }
    }

    pub fn note_open(&self, section: &str) {
        if self.is_active() {
            self.change(|state| record_open(state, section));
        }
    }

    /// Forgets everything counted.
    pub fn clear(&self) {
        *self.state.lock().unwrap() = State::default();
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn has_data(&self) -> bool {
        self.path.as_deref().is_some_and(Path::exists)
            || *self.state.lock().unwrap() != State::default()
    }

    /// Sends the ping when due; None when nothing was sent. `install_id`
    /// gives the id, making one when needed; `send` posts the body and gives
    /// the status code.
    pub fn send_if_due(
        &self,
        at_launch: bool,
        now: f64,
        context: &Context,
        install_id: &mut dyn FnMut() -> Option<String>,
        send: &mut dyn FnMut(&[u8]) -> Option<u16>,
    ) -> Option<Outcome> {
        if !self.is_active() || self.sending.swap(true, Ordering::SeqCst) {
            return None;
        }
        let prepared = (|| {
            let mut sent = self.state.lock().unwrap().clone();
            if !is_due(now, &sent, at_launch) {
                return None;
            }
            prune(&mut sent, now);
            let id = install_id()?;
            Some((encode(payload(&id, context, &sent))?, sent))
        })();
        let Some((body, sent)) = prepared else {
            self.sending.store(false, Ordering::SeqCst);
            return None;
        };
        let outcome = outcome(send(&body));
        // Switched off while it was on its way: keep nothing.
        if self.is_active() {
            self.change(|state| settle(state, &sent, outcome, now));
        }
        self.sending.store(false, Ordering::SeqCst);
        Some(outcome)
    }
}

fn save(path: Option<&Path>, state: &State) {
    let Some(path) = path else { return };
    let Ok(bytes) = serde_json::to_vec(state) else {
        return;
    };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let temporary = path.with_extension("json.tmp");
    if std::fs::write(&temporary, bytes).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

static STORE: OnceLock<Store> = OnceLock::new();

fn dnt() -> bool {
    do_not_track(std::env::var("DO_NOT_TRACK").ok().as_deref())
}

/// At launch, before the preferences are applied: opens the counts and
/// starts the daily timer. A debug build never starts it.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let path = app
        .path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("usage-stats.json"));
    let _ = STORE.set(Store::open(path));
    if DEVELOPER {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(LAUNCH_DELAY);
        check(&app, true);
        loop {
            std::thread::sleep(CHECK_EVERY);
            check(&app, false);
        }
    });
}

/// From `prefs::apply`: the switch decides whether anything is counted, and
/// switching it off deletes the id and the counts at once.
pub fn apply_prefs<R: Runtime>(app: &AppHandle<R>, value: &Value) {
    let on = setting_on(value);
    if let Some(store) = STORE.get() {
        store.set_active(allowed(on, DEVELOPER, dnt()));
    }
    if !on && (value.get(ID_KEY).is_some() || STORE.get().is_some_and(Store::has_data)) {
        opt_out(app);
    }
}

fn opt_out<R: Runtime>(app: &AppHandle<R>) {
    if let Some(store) = STORE.get() {
        store.clear();
    }
    let app = app.clone();
    // Off the caller's stack: prefs::apply runs inside a settings save.
    std::thread::spawn(move || {
        let prefs = app.state::<crate::prefs::Prefs>();
        if prefs.get().get(ID_KEY).is_some() {
            let _ = prefs.set(&json!({ ID_KEY: null }));
        }
    });
}

/// From the sessions service whenever its list is published.
pub fn note_sessions(sessions: &[Session]) {
    let Some(store) = STORE.get() else { return };
    if !store.is_active() {
        return;
    }
    let notes: Vec<Note> = sessions
        .iter()
        .map(|s| Note {
            id: s.id.clone(),
            provider: s.provider.key().to_string(),
            terminal: terminal_id(s.host, s.host_name.as_deref()).map(String::from),
        })
        .collect();
    store.note(&notes, now());
}

/// The person opened a section of the island (island/store.ts).
#[tauri::command]
pub fn stats_section_opened(section: String) {
    if let Some(store) = STORE.get() {
        store.note_open(&section);
    }
}

fn check<R: Runtime>(app: &AppHandle<R>, at_launch: bool) {
    let Some(store) = STORE.get() else { return };
    let prefs = app.state::<crate::prefs::Prefs>();
    let value = prefs.get();
    store.set_active(allowed(setting_on(&value), DEVELOPER, dnt()));
    let Some(target) = endpoint(std::env::var("OPENBAR_STATS_URL").ok().as_deref()) else {
        return;
    };
    let context = context(app, &value);
    let mut install_id = || {
        if let Some(id) = prefs
            .get()
            .get(ID_KEY)
            .and_then(Value::as_str)
            .filter(|s| s.len() == 36)
        {
            return Some(id.to_string());
        }
        let id = new_id()?;
        prefs.set(&json!({ ID_KEY: id })).ok()?;
        Some(id)
    };
    let agent = format!("Openbar/{}", context.app_version);
    let mut send = |body: &[u8]| post(&target, &agent, body);
    store.send_if_due(at_launch, now(), &context, &mut install_id, &mut send);
}

#[cfg(any(windows, target_os = "linux"))]
fn post(target: &Target, agent: &str, body: &[u8]) -> Option<u16> {
    http::post(target, agent, body)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn post(_target: &Target, _agent: &str, _body: &[u8]) -> Option<u16> {
    None
}

fn context<R: Runtime>(app: &AppHandle<R>, prefs: &Value) -> Context {
    let os = crate::diagnostics::os();
    Context {
        platform: if cfg!(windows) { "windows" } else { "linux" },
        os_version: os_version(&os.name, &os.version, os.build.as_deref()),
        arch: arch().into(),
        app_version: app.package_info().version.to_string(),
        hooks: hooks(),
        enabled: enabled_sections(prefs),
    }
}

/// Whether each agent holds this app's hooks.
fn hooks() -> BTreeMap<String, bool> {
    use crate::sessions::hook_agents::installer;
    use crate::sessions::models::Provider;
    let mut hooks = BTreeMap::new();
    hooks.insert(
        Provider::Claude.key().to_string(),
        crate::hooks::claude_hooks_status_installed(),
    );
    let (Some(home), Ok(exe)) = (
        crate::hooks::installer::home_dir(),
        crate::hooks::current_program(),
    ) else {
        return hooks;
    };
    let endpoint = crate::hooks::transport::endpoint(crate::hooks::APP_ID);
    for provider in Provider::INSTALLED_AGENTS {
        if provider == Provider::Claude {
            continue;
        }
        let state = installer::state(provider, &home, &exe, &endpoint, crate::hooks::APP_ID);
        hooks.insert(
            provider.key().to_string(),
            matches!(state, installer::State::Installed),
        );
    }
    hooks
}

#[cfg(test)]
mod tests;
