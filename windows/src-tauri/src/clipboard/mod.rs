// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Clipboard history (ClipboardHistoryService.swift; spec 05 §3): every copy
//! while `clipboardHistoryEnabled` is on, kept in
//! `%APPDATA%\com.vorssaint.island\clipboard\history.json` with pictures in
//! `images\`, and the commands the island's Clipboard page and the Tools
//! section use to copy, paste, pin, move and delete. Reads run on a worker
//! and are given up after 5 s, as on the Mac: an app that renders its
//! clipboard late (Office) can stall them (§15.1).
// The Mac build only designs the page: there is no clipboard to read there.
#![cfg_attr(not(windows), allow(dead_code))]

pub mod formats;
pub mod history;
#[cfg(windows)]
pub mod native;
mod sensitive;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use history::{Content, Entry, History, Kind, Source};

pub const EVENT: &str = "clipboard://history";
#[cfg(windows)]
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// The Mac trims to the limit and retention hourly (CHS:658-664).
const TRIM_INTERVAL: Duration = Duration::from_secs(3_600);

/// The preferences the backend reads (spec 05 §3.2), with the Mac's defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub limit: i64,
    pub retention_days: i64,
    pub include_images_files: bool,
    pub filter_sensitive: bool,
    /// Exe names, lowercase ("keepass.exe").
    pub ignored: Vec<String>,
}

impl Config {
    pub fn from_prefs(prefs: &Value) -> Self {
        let island = |key: &str| prefs.pointer(&format!("/island/{key}"));
        let flag = |key: &str, default: bool| island(key).and_then(Value::as_bool).unwrap_or(default);
        let number = |key: &str| island(key).and_then(Value::as_i64).unwrap_or(-1);
        Config {
            enabled: flag("clipboardHistoryEnabled", false),
            limit: history::sanitized_limit(number("clipboardHistoryLimit")),
            retention_days: history::sanitized_retention(number("clipboardHistoryRetentionDays")),
            include_images_files: flag("clipboardHistoryIncludeImagesFiles", true),
            filter_sensitive: flag("clipboardFilterSensitiveText", false),
            ignored: island("clipboardIgnoredApps")
                .and_then(Value::as_array)
                .map(|apps| apps.iter().filter_map(Value::as_str).map(exe_name).filter(|n| !n.is_empty()).collect())
                .unwrap_or_default(),
        }
    }

    /// Whether a copy made while any of `candidates` held the front is left out.
    pub fn ignores(&self, candidates: &[String]) -> bool {
        candidates.iter().any(|exe| self.ignored.contains(&exe_name(exe)))
    }
}

/// "C:\Program Files\KeePass\KeePass.exe" and "KeePass.exe" both read "keepass.exe".
fn exe_name(path: &str) -> String {
    path.trim().rsplit(['\\', '/']).next().unwrap_or("").to_lowercase()
}

fn config<R: Runtime>(app: &AppHandle<R>) -> Config {
    Config::from_prefs(&app.state::<crate::prefs::Prefs>().get())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The history and where it lives on disk.
#[derive(Default)]
pub struct Store {
    history: History,
    dir: Option<PathBuf>,
    /// The saved history couldn't be read (another program held it): nothing
    /// is saved or deleted, so the file and its pictures survive.
    blocked: bool,
    /// The saved history was damaged and set aside: its pictures stay, since
    /// the entries naming them may yet be recovered from it.
    keeps_images: bool,
}

pub type Shared = Arc<Mutex<Store>>;

impl Store {
    fn images(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("images"))
    }

    fn load(dir: PathBuf) -> Self {
        let file = dir.join("history.json");
        let mut store = Store { dir: Some(dir), ..Store::default() };
        match std::fs::read(&file) {
            Ok(bytes) => match history::decode(&bytes) {
                Some(entries) => store.history.entries = entries,
                None => {
                    // Kept aside rather than overwritten by the next save.
                    store.keeps_images = true;
                    store.blocked = std::fs::rename(&file, file.with_extension("json.broken")).is_err();
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => store.blocked = true,
        }
        store
    }

    /// Written to a temporary file, then moved over the old one.
    fn save(&self) {
        let Some(dir) = self.dir.as_ref().filter(|_| !self.blocked) else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let file = dir.join("history.json");
        let tmp = dir.join("history.json.tmp");
        if std::fs::write(&tmp, history::encode(&self.history.entries)).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }

    /// Deletes pictures no entry uses any more.
    fn clean_images(&self) {
        if self.blocked || self.keeps_images {
            return;
        }
        let Some(images) = self.images() else { return };
        let Ok(files) = std::fs::read_dir(&images) else { return };
        let used: std::collections::HashSet<&str> = self.history.image_files().collect();
        for file in files.flatten() {
            let name = file.file_name();
            if name.to_str().is_some_and(|n| n.ends_with(".png") && !used.contains(n)) {
                let _ = std::fs::remove_file(file.path());
            }
        }
    }

    fn store_image(images: Option<&Path>, png: &[u8]) -> Option<String> {
        let images = images?;
        std::fs::create_dir_all(images).ok()?;
        let name = format!("{}.png", history::new_id());
        std::fs::write(images.join(&name), png).ok()?;
        Some(name)
    }

    fn snapshot(&self, captured: Option<String>) -> Snapshot {
        let images = self.images();
        Snapshot {
            entries: self.history.entries.iter().map(|e| View::of(e, images.as_deref())).collect(),
            captured,
        }
    }

    /// The bytes an entry puts back on the clipboard.
    #[cfg(windows)]
    fn payload(&self, id: &str) -> Option<native::Payload> {
        let entry = self.history.entries.iter().find(|e| e.id == id)?;
        Some(match entry.kind {
            Kind::Text => native::Payload::Text(entry.text.clone()),
            Kind::Files => native::Payload::Files(entry.file_paths.clone()),
            Kind::Image => native::Payload::Image(std::fs::read(self.images()?.join(entry.image_file.as_ref()?)).ok()?),
        })
    }
}

/// An entry as the page draws it: labels derived here, the text cut for search.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    id: String,
    kind: Kind,
    text: String,
    preview: String,
    card_preview: String,
    copied_at: i64,
    pinned: bool,
    file_paths: Vec<String>,
    image_path: Option<String>,
    image_width: Option<u32>,
    image_height: Option<u32>,
    source_exe: Option<String>,
    source_name: Option<String>,
}

impl View {
    fn of(e: &Entry, images: Option<&Path>) -> Self {
        View {
            id: e.id.clone(),
            kind: e.kind,
            text: e.text.chars().take(history::SEARCH_CHARACTERS).collect(),
            preview: e.preview(),
            card_preview: e.card_preview(),
            copied_at: e.copied_at,
            pinned: e.is_pinned(),
            file_paths: e.file_paths.clone(),
            image_path: images
                .zip(e.image_file.as_deref())
                .map(|(dir, name)| dir.join(name).to_string_lossy().into_owned()),
            image_width: e.image_width,
            image_height: e.image_height,
            source_exe: e.source_exe.clone(),
            source_name: e.source_name.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    entries: Vec<View>,
    /// The entry a new copy just added or refreshed, for the "Copied" notice.
    captured: Option<String>,
}

fn emit<R: Runtime>(app: &AppHandle<R>, snapshot: &Snapshot) {
    let _ = app.emit_to(crate::island::LABEL, EVENT, snapshot);
}

/// Runs `change` on the history; when it changed anything, trims, saves and tells the page.
fn mutate<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut History) -> bool) -> bool {
    let Some(shared) = app.try_state::<Shared>() else { return false };
    let config = config(app);
    let mut store = shared.lock().unwrap();
    if !change(&mut store.history) {
        return false;
    }
    store.history.trim(config.limit, config.retention_days, now_ms());
    store.save();
    store.clean_images();
    emit(app, &store.snapshot(None));
    true
}

/// Applies the limit and retention now; saves and tells the page only when
/// something was dropped.
fn trim_now<R: Runtime>(app: &AppHandle<R>) {
    let Some(shared) = app.try_state::<Shared>() else { return };
    let config = config(app);
    let mut store = shared.lock().unwrap();
    if store.history.trim(config.limit, config.retention_days, now_ms()) {
        store.save();
        store.clean_images();
        emit(app, &store.snapshot(None));
    }
}

/// Loads the history, then listens for copies and trims it every hour.
pub fn start<R: Runtime>(app: AppHandle<R>, shared: Shared) {
    if let Ok(dir) = app.path().app_config_dir() {
        let mut store = Store::load(dir.join("clipboard"));
        let config = config(&app);
        store.history.trim(config.limit, config.retention_days, now_ms());
        store.clean_images();
        *shared.lock().unwrap() = store;
    }
    #[cfg(windows)]
    {
        let (tx, rx) = std::sync::mpsc::channel::<native::Update>();
        native::listen(move |update| {
            let _ = tx.send(update);
        });
        let (app, shared) = (app.clone(), shared.clone());
        std::thread::spawn(move || capture(app, shared, rx));
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TRIM_INTERVAL);
            trim_now(&app);
        }
    });
}

/// One read at a time. Changes that pile up behind a slow read are taken
/// as one, the newest content, left out if any of them came from an
/// ignored app.
#[cfg(windows)]
fn capture<R: Runtime>(app: AppHandle<R>, shared: Shared, rx: std::sync::mpsc::Receiver<native::Update>) {
    while let Ok(mut update) = rx.recv() {
        while let Ok(next) = rx.try_recv() {
            let mut candidates = std::mem::take(&mut update.candidates);
            candidates.extend(next.candidates.iter().cloned());
            update = native::Update { candidates, ..next };
        }
        let config = config(&app);
        if !config.enabled || (!update.own && config.ignores(&update.candidates)) {
            continue;
        }
        let (tx, result) = std::sync::mpsc::channel();
        let include = config.include_images_files;
        std::thread::spawn(move || {
            let _ = tx.send(native::read(include));
        });
        let Ok(Some(content)) = result.recv_timeout(READ_TIMEOUT) else { continue };
        let source = Source { name: update.exe.as_deref().map(crate::shell::app_name), exe: update.exe };
        record(&app, &shared, &config, content, source);
    }
}

/// A copy the history keeps: text within the bounds and, with the filter
/// on, not looking like a secret (CHS:924-946).
fn admitted(config: &Config, content: Content) -> Option<Content> {
    match content {
        Content::Text(raw) => {
            let text = history::storable_text(&raw)?;
            (!(config.filter_sensitive && sensitive::looks_sensitive(&text))).then_some(Content::Text(text))
        }
        other => Some(other),
    }
}

fn record<R: Runtime>(app: &AppHandle<R>, shared: &Shared, config: &Config, content: Content, source: Source) {
    let Some(content) = admitted(config, content) else { return };
    let mut store = shared.lock().unwrap();
    let images = store.images();
    let Some(id) = store.history.promote(content, source, now_ms(), |png| Store::store_image(images.as_deref(), png))
    else {
        return;
    };
    store.history.trim(config.limit, config.retention_days, now_ms());
    store.save();
    store.clean_images();
    emit(app, &store.snapshot(Some(id)));
}

/// Called with each preferences change: a new limit or retention trims now.
pub fn apply_prefs<R: Runtime>(app: &AppHandle<R>, before: Option<&Value>, value: &Value) {
    let next = Config::from_prefs(value);
    if before.is_some_and(|b| {
        let old = Config::from_prefs(b);
        (old.limit, old.retention_days) == (next.limit, next.retention_days)
    }) {
        return;
    }
    trim_now(app);
}

// MARK: Commands

#[tauri::command]
pub fn clipboard_snapshot(state: tauri::State<'_, Shared>) -> Snapshot {
    state.lock().unwrap().snapshot(None)
}

/// Copy: the entry back on the clipboard, and on top of the recent ones.
#[tauri::command]
pub async fn clipboard_copy<R: Runtime>(app: AppHandle<R>, id: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || put_back(&app, &id)).await.unwrap_or(false)
}

#[cfg(windows)]
fn put_back<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    let Some(shared) = app.try_state::<Shared>() else { return false };
    let Some(payload) = shared.lock().unwrap().payload(id) else { return false };
    native::write(&payload, true) && {
        mutate(app, |h| h.touch(id, now_ms()));
        true
    }
}

#[cfg(not(windows))]
fn put_back<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    mutate(app, |h| h.touch(id, now_ms()))
}

/// Paste: puts the entry back, then types Ctrl+V into the app the island
/// came from. "pasting" lets the page close the island; "copied" means the
/// target is elevated (or gone) and the entry only went on the clipboard.
#[tauri::command]
pub async fn clipboard_paste<R: Runtime>(app: AppHandle<R>, id: String) -> &'static str {
    #[cfg(windows)]
    let previous = crate::island::previous_foreground(&app);
    tauri::async_runtime::spawn_blocking(move || {
        if !put_back(&app, &id) {
            return "failed";
        }
        #[cfg(windows)]
        if let Some(target) = native::paste_target(previous).filter(|&t| !native::blocks_input(t)) {
            // The entry stays on the clipboard either way; a target that
            // never came forward gets nothing typed, and says so.
            std::thread::spawn(move || {
                if !native::send_paste(target) {
                    native::beep();
                }
            });
            return "pasting";
        }
        "copied"
    })
    .await
    .unwrap_or("failed")
}

#[tauri::command]
pub fn clipboard_toggle_pin<R: Runtime>(app: AppHandle<R>, id: String) {
    mutate(&app, |h| h.toggle_pin(&id, now_ms()));
}

#[tauri::command]
pub fn clipboard_remove<R: Runtime>(app: AppHandle<R>, id: String) {
    mutate(&app, |h| h.remove(&id));
}

#[tauri::command]
pub fn clipboard_move<R: Runtime>(app: AppHandle<R>, id: String, up: bool) {
    mutate(&app, |h| h.move_entry(&id, up));
}

/// `confirmed`: each unpinned entry the confirmation counted, with its copy time.
#[tauri::command]
pub fn clipboard_clear_recent<R: Runtime>(app: AppHandle<R>, confirmed: HashMap<String, i64>) {
    mutate(&app, |h| h.clear_recent(&confirmed));
}

/// Copies text Vorssaint made (OCR, a color). It joins the history as a
/// copy with no app named, as on the Mac.
#[tauri::command]
pub async fn clipboard_copy_text(text: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || write_own_text(text)).await.unwrap_or(false)
}

/// Copies a PNG Vorssaint made (a screenshot), as CF_DIB and "PNG".
#[tauri::command]
pub async fn clipboard_copy_image(data_url: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || formats::png_from_data_url(&data_url).is_some_and(write_own_png))
        .await
        .unwrap_or(false)
}

#[cfg(windows)]
fn write_own_text(text: String) -> bool {
    native::write(&native::Payload::Text(text), false)
}

#[cfg(windows)]
fn write_own_png(png: Vec<u8>) -> bool {
    native::write(&native::Payload::Image(png), false)
}

#[cfg(not(windows))]
fn write_own_text(_text: String) -> bool {
    false
}

#[cfg(not(windows))]
fn write_own_png(_png: Vec<u8>) -> bool {
    false
}

#[tauri::command]
pub fn clipboard_beep() {
    #[cfg(windows)]
    native::beep();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preferences_read_with_mac_defaults_and_sanitized_choices() {
        let defaults = Config::from_prefs(&json!({}));
        assert_eq!(
            defaults,
            Config {
                enabled: false,
                limit: 50,
                retention_days: 0,
                include_images_files: true,
                filter_sensitive: false,
                ignored: vec![],
            }
        );
        let set = Config::from_prefs(&json!({"island": {
            "clipboardHistoryEnabled": true,
            "clipboardHistoryLimit": 10000,
            "clipboardHistoryRetentionDays": 7,
            "clipboardHistoryIncludeImagesFiles": false,
            "clipboardFilterSensitiveText": true,
            "clipboardIgnoredApps": ["KeePass.exe", r"C:\Apps\1Password.EXE", "", 3],
        }}));
        assert_eq!((set.enabled, set.limit, set.retention_days), (true, 10000, 7));
        assert!(!set.include_images_files && set.filter_sensitive);
        assert_eq!(set.ignored, ["keepass.exe", "1password.exe"]);
        let odd = Config::from_prefs(&json!({"island": {"clipboardHistoryLimit": 7, "clipboardHistoryRetentionDays": "x"}}));
        assert_eq!((odd.limit, odd.retention_days), (50, 0));
    }

    #[test]
    fn ignored_apps_match_by_exe_name_whatever_the_folder_or_case() {
        let config = Config { ignored: vec!["keepass.exe".into()], ..Config::from_prefs(&json!({})) };
        assert!(config.ignores(&[r"C:\Tools\notepad.exe".into(), r"D:\KeePass\KEEPASS.EXE".into()]));
        assert!(!config.ignores(&[r"C:\Tools\notkeepass.exe".into()]));
        assert!(!config.ignores(&[]));
        assert_eq!(exe_name("  C:\\x\\App.EXE "), "app.exe");
        assert_eq!(exe_name("  "), "");
    }

    #[test]
    fn admitted_text_is_trimmed_and_the_sensitive_filter_applies_only_when_on() {
        let mut config = Config::from_prefs(&json!({}));
        assert_eq!(admitted(&config, Content::Text("  hi \n".into())), Some(Content::Text("hi".into())));
        assert_eq!(admitted(&config, Content::Text("   ".into())), None);
        let secret = Content::Text("password=hunter2".into());
        assert!(admitted(&config, secret.clone()).is_some());
        config.filter_sensitive = true;
        assert_eq!(admitted(&config, secret), None);
        let files = Content::Files(vec!["C:\\a".into()]);
        assert_eq!(admitted(&config, files.clone()), Some(files));
    }

    #[test]
    fn the_store_saves_atomically_reloads_and_cleans_unused_pictures() {
        let dir = std::env::temp_dir().join(format!("vorssaint-clipboard-test-{}", history::new_id()));
        let mut store = Store { dir: Some(dir.clone()), ..Store::default() };
        let images = store.images();
        let png = vec![1, 2, 3];
        let image = Content::Image { png: png.clone(), hash: "h".into(), width: 1, height: 1 };
        store.history.promote(image, Source::default(), 1, |p| Store::store_image(images.as_deref(), p));
        store.history.promote(Content::Text("hello".into()), Source::default(), 2, |_| None);
        std::fs::write(images.as_ref().unwrap().join("orphan.png"), b"x").unwrap();
        store.save();
        store.clean_images();
        assert!(!dir.join("history.json.tmp").exists());
        let names: Vec<_> = std::fs::read_dir(images.as_ref().unwrap()).unwrap().flatten().map(|f| f.file_name()).collect();
        assert_eq!(names.len(), 1);
        let loaded = Store::load(dir.clone());
        assert_eq!(loaded.history.entries, store.history.entries);
        let snapshot = loaded.snapshot(None);
        let view = &snapshot.entries[1];
        assert_eq!(std::fs::read(view.image_path.as_ref().unwrap()).unwrap(), png);
        // A damaged file is set aside, never overwritten, and its pictures stay.
        std::fs::write(dir.join("history.json"), b"{oops").unwrap();
        let damaged = Store::load(dir.clone());
        assert!(damaged.history.entries.is_empty());
        assert!(dir.join("history.json.broken").exists());
        damaged.clean_images();
        assert_eq!(std::fs::read_dir(images.as_ref().unwrap()).unwrap().count(), 1);
        // No file yet is an empty history that saves normally.
        std::fs::remove_file(dir.join("history.json.broken")).unwrap();
        let fresh = Store::load(dir.clone());
        assert!(!fresh.blocked && !fresh.keeps_images && fresh.history.entries.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn views_cut_long_text_for_search_but_keep_labels() {
        let mut h = History::default();
        let long = "x".repeat(history::SEARCH_CHARACTERS + 10);
        h.promote(Content::Text(long), Source::default(), 1, |_| None);
        let view = View::of(&h.entries[0], None);
        assert_eq!(view.text.chars().count(), history::SEARCH_CHARACTERS);
        assert!(view.preview.ends_with('…'));
        assert_eq!(view.image_path, None);
    }
}
