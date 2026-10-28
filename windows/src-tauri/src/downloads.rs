// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
// Local file-transfer monitoring. Never reads file contents or browsing URLs.
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub received: u64,
    pub total: Option<u64>,
    pub speed: u64,
    pub status: String,
}
#[derive(Clone, Default, PartialEq, Serialize)]
pub struct Snapshot {
    pub items: Vec<Item>,
    pub folders: Vec<String>,
}
pub type Shared = Arc<Mutex<Snapshot>>;

#[derive(Clone)]
struct Sample {
    path: PathBuf,
    target: PathBuf,
    size: u64,
    total: Option<u64>,
    source: String,
    partial: bool,
    modified: SystemTime,
}
struct Entry {
    sample: Sample,
    changed: u64,
    missing: Option<u64>,
    finished: Option<u64>,
    item: Item,
}
#[derive(Default)]
struct Tracker {
    observed: HashMap<PathBuf, u64>,
    entries: HashMap<PathBuf, Entry>,
    primed: bool,
}
impl Tracker {
    fn update(&mut self, samples: Vec<Sample>, now: u64) -> Vec<Item> {
        let current: HashMap<_, _> = samples.into_iter().map(|s| (s.path.clone(), s)).collect();
        for (path, entry) in &mut self.entries {
            if entry.finished.is_some() {
                continue;
            }
            if let Some(sample) = current.get(path) {
                entry.missing = None;
                // A busy browser database must not replace its received-byte
                // counter with a possibly preallocated temporary file length.
                if entry.sample.source != "Files" && sample.source == "Files" && sample.partial {
                    entry.item.speed = 0;
                    continue;
                }
                let delta = sample.size.saturating_sub(entry.sample.size);
                if sample.size != entry.sample.size {
                    entry.changed = now;
                }
                entry.item.received = sample.size;
                entry.item.total = sample.total.or(entry.item.total);
                entry.item.speed = delta;
                entry.item.source = sample.source.clone();
                entry.sample = sample.clone();
                // Temporary files stay active even when paused. Plain transfers
                // finish only after their writer closed the file and size settled.
                if !sample.partial && now.saturating_sub(entry.changed) >= 3 && closed(path) {
                    entry.finished = Some(now);
                    entry.item.status = "completed".into();
                    entry.item.total = Some(sample.size);
                    entry.item.speed = 0;
                }
            } else {
                let missing = *entry.missing.get_or_insert(now);
                if entry.sample.target.is_file() || now.saturating_sub(missing) >= 3 {
                    let complete = entry.sample.target.is_file();
                    entry.item.status = if complete { "completed" } else { "interrupted" }.into();
                    if complete {
                        let size = std::fs::metadata(&entry.sample.target)
                            .map(|m| m.len())
                            .unwrap_or(entry.item.received);
                        entry.item.received = size;
                        entry.item.total = Some(size);
                    }
                    entry.item.speed = 0;
                    entry.finished = Some(now);
                }
            }
        }
        let targets: HashSet<_> = self
            .entries
            .values()
            .filter(|e| {
                e.sample.partial
                    && e.sample.target != e.sample.path
                    && e.finished.is_none_or(|t| now.saturating_sub(t) < 5)
            })
            .map(|e| e.sample.target.clone())
            .collect();
        for (path, sample) in &current {
            if self.entries.get(path).is_some_and(|e| e.finished.is_none())
                || targets.contains(path)
            {
                continue;
            }
            let changed = self
                .observed
                .get(path)
                .is_some_and(|size| *size != sample.size);
            let recent = SystemTime::now()
                .duration_since(sample.modified)
                .unwrap_or_default()
                < Duration::from_secs(15);
            let fresh = !self.observed.contains_key(path) && self.primed && recent;
            let resume = sample.partial && !self.primed && recent;
            if !(changed || fresh || resume) {
                continue;
            }
            let target = sample.target.to_string_lossy().into_owned();
            self.entries.insert(
                path.clone(),
                Entry {
                    sample: sample.clone(),
                    changed: now,
                    missing: None,
                    finished: None,
                    item: Item {
                        id: path.to_string_lossy().into_owned(),
                        name: sample
                            .target
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: target,
                        source: sample.source.clone(),
                        received: sample.size,
                        total: sample.total,
                        speed: 0,
                        status: "downloading".into(),
                    },
                },
            );
        }
        self.observed = current.into_iter().map(|(p, s)| (p, s.size)).collect();
        self.primed = true;
        self.entries
            .retain(|_, e| e.finished.is_none_or(|t| now.saturating_sub(t) < 300));
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by_key(|e| (e.finished.is_some(), std::cmp::Reverse(e.changed)));
        entries
            .into_iter()
            .take(64)
            .map(|e| e.item.clone())
            .collect()
    }
}
fn closed(path: &Path) -> bool {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path).is_ok()
}
fn sample(path: PathBuf, source: &str) -> Option<Sample> {
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let partial = matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "crdownload" | "part" | "partial" | "download"
    );
    let target = if partial {
        path.with_extension("")
    } else {
        path.clone()
    };
    Some(Sample {
        path,
        target,
        size: meta.len(),
        total: None,
        source: source.into(),
        partial,
        modified: meta.modified().ok()?,
    })
}

#[derive(Default)]
struct Locations {
    folders: Vec<PathBuf>,
    profiles: Vec<(PathBuf, String)>,
}
fn add_folder(folders: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_absolute()
        && path.is_dir()
        && !folders.iter().any(|p| {
            p.to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        })
    {
        folders.push(path);
    }
}
fn locations(extra: &[PathBuf]) -> Locations {
    let mut out = Locations::default();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Shell::{
            FOLDERID_Downloads, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
        };
        if let Ok(path) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            if let Ok(text) = path.to_string() {
                add_folder(&mut out.folders, PathBuf::from(text));
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(path.0.cast()));
        }
    }
    if out.folders.is_empty() {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            add_folder(&mut out.folders, PathBuf::from(home).join("Downloads"));
        }
    }
    for folder in extra {
        add_folder(&mut out.folders, folder.clone());
    }
    for (base, relative, name) in [
        (
            "LOCALAPPDATA",
            r"BraveSoftware\Brave-Browser\User Data",
            "Brave",
        ),
        ("LOCALAPPDATA", r"Google\Chrome\User Data", "Chrome"),
        ("LOCALAPPDATA", r"Microsoft\Edge\User Data", "Edge"),
        ("LOCALAPPDATA", r"Vivaldi\User Data", "Vivaldi"),
        ("APPDATA", r"Opera Software\Opera Stable", "Opera"),
        ("APPDATA", r"Opera Software\Opera GX Stable", "Opera GX"),
    ] {
        let Some(base) = std::env::var_os(base) else {
            continue;
        };
        let root = PathBuf::from(base).join(relative);
        let mut profiles = vec![root.clone()];
        if let Ok(dirs) = std::fs::read_dir(&root) {
            profiles.extend(
                dirs.flatten()
                    .take(128)
                    .filter(|d| d.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|d| d.path()),
            );
        }
        for profile in profiles {
            if !profile.join("Preferences").is_file() {
                continue;
            }
            if let Ok(bytes) = std::fs::read(profile.join("Preferences")) {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if let Some(path) = value
                        .pointer("/download/default_directory")
                        .and_then(|v| v.as_str())
                    {
                        add_folder(&mut out.folders, path.into());
                    }
                }
            }
            if profile.join("History").is_file() {
                out.profiles.push((profile.join("History"), name.into()));
            }
        }
    }
    if let Some(base) = std::env::var_os("APPDATA") {
        if let Ok(profiles) =
            std::fs::read_dir(PathBuf::from(base).join(r"Mozilla\Firefox\Profiles"))
        {
            for profile in profiles.flatten().take(64) {
                if let Ok(prefs) = std::fs::read_to_string(profile.path().join("prefs.js")) {
                    for line in prefs
                        .lines()
                        .filter(|l| l.starts_with("user_pref(\"browser.download.dir\","))
                    {
                        if let Some((_, value)) = line.split_once(',') {
                            if let Ok(path) =
                                serde_json::from_str::<String>(value.trim().trim_end_matches(");"))
                            {
                                add_folder(&mut out.folders, path.into());
                            }
                        }
                    }
                }
            }
        }
    }
    out
}
fn scan(locations: &Locations) -> Vec<Sample> {
    let mut found = HashMap::new();
    for folder in &locations.folders {
        if let Ok(files) = std::fs::read_dir(folder) {
            for file in files.flatten().take(8192) {
                // Do not follow directory junctions or inspect nested personal folders.
                if !file.file_type().is_ok_and(|t| t.is_file()) {
                    continue;
                }
                if let Some(s) = sample(file.path(), "Files") {
                    found.insert(s.path.clone(), s);
                }
            }
        }
    }
    #[cfg(windows)]
    for (path, source) in &locations.profiles {
        for s in browser::active(path, source) {
            found.insert(s.path.clone(), s);
        }
    }
    found.into_values().collect()
}

#[tauri::command]
pub fn downloads_snapshot(state: tauri::State<'_, Shared>) -> Snapshot {
    state.lock().unwrap().clone()
}

pub fn start<R: Runtime>(app: AppHandle<R>, state: Shared) {
    std::thread::spawn(move || {
        let mut tracker = Tracker::default();
        let mut places = Locations::default();
        let mut extra = Vec::new();
        let mut tick = 0u64;
        loop {
            let prefs = app.state::<crate::prefs::Prefs>().get();
            let on = prefs
                .pointer("/island/downloadProgress")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
                && prefs
                    .pointer("/island/enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                && prefs
                    .pointer("/island/sections/downloads")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
                && !prefs
                    .pointer("/island/hiddenModules")
                    .and_then(|v| v.as_array())
                    .is_some_and(|v| v.iter().any(|id| id == "downloads"));
            let folders: Vec<PathBuf> = prefs
                .pointer("/island/downloadFolders")
                .and_then(|v| v.as_array())
                .map(|v| {
                    v.iter()
                        .filter_map(|s| s.as_str().map(PathBuf::from))
                        .collect()
                })
                .unwrap_or_default();
            if on && (tick % 30 == 0 || folders != extra || places.folders.is_empty()) {
                places = locations(&folders);
                extra = folders;
            }
            let next = Snapshot {
                items: if on {
                    tracker.update(scan(&places), tick)
                } else {
                    tracker = Tracker::default();
                    vec![]
                },
                folders: places
                    .folders
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
            };
            let mut previous = state.lock().unwrap();
            if *previous != next {
                *previous = next.clone();
                let _ = app.emit("downloads://snapshot", &next);
            }
            drop(previous);
            tick += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(windows)]
mod browser {
    use super::*;
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    #[link(name = "winsqlite3")]
    unsafe extern "C" {
        fn sqlite3_open_v2(
            path: *const c_char,
            db: *mut *mut c_void,
            flags: c_int,
            vfs: *const c_char,
        ) -> c_int;
        fn sqlite3_close(db: *mut c_void) -> c_int;
        fn sqlite3_busy_timeout(db: *mut c_void, ms: c_int) -> c_int;
        fn sqlite3_prepare_v2(
            db: *mut c_void,
            sql: *const c_char,
            len: c_int,
            statement: *mut *mut c_void,
            tail: *mut *const c_char,
        ) -> c_int;
        fn sqlite3_step(statement: *mut c_void) -> c_int;
        fn sqlite3_finalize(statement: *mut c_void) -> c_int;
        fn sqlite3_column_text(statement: *mut c_void, column: c_int) -> *const c_char;
        fn sqlite3_column_int64(statement: *mut c_void, column: c_int) -> i64;
    }
    struct Db(*mut c_void);
    impl Drop for Db {
        fn drop(&mut self) {
            unsafe {
                sqlite3_close(self.0);
            }
        }
    }
    struct Statement(*mut c_void);
    impl Drop for Statement {
        fn drop(&mut self) {
            unsafe {
                sqlite3_finalize(self.0);
            }
        }
    }
    pub fn active(path: &Path, source: &str) -> Vec<Sample> {
        let Ok(path) = CString::new(path.to_string_lossy().as_bytes()) else {
            return vec![];
        };
        let mut pointer = std::ptr::null_mut();
        unsafe {
            // READONLY: no schema changes, no URLs or page-history queries.
            let result = sqlite3_open_v2(path.as_ptr(), &mut pointer, 1, std::ptr::null());
            if pointer.is_null() {
                return vec![];
            }
            let db = Db(pointer);
            if result != 0 {
                return vec![];
            }
            sqlite3_busy_timeout(db.0, 30);
            let sql = c"SELECT current_path,target_path,total_bytes,received_bytes FROM downloads WHERE state=0 ORDER BY start_time DESC LIMIT 64";
            let mut pointer = std::ptr::null_mut();
            let result =
                sqlite3_prepare_v2(db.0, sql.as_ptr(), -1, &mut pointer, std::ptr::null_mut());
            if pointer.is_null() {
                return vec![];
            }
            let statement = Statement(pointer);
            if result != 0 {
                return vec![];
            }
            let mut rows = Vec::new();
            while sqlite3_step(statement.0) == 100 {
                let path = sqlite3_column_text(statement.0, 0);
                let target = sqlite3_column_text(statement.0, 1);
                if path.is_null() || target.is_null() {
                    continue;
                }
                let path = PathBuf::from(CStr::from_ptr(path).to_string_lossy().into_owned());
                if let Some(mut row) = sample(path, source) {
                    let total = sqlite3_column_int64(statement.0, 2);
                    row.total = (total > 0).then_some(total as u64);
                    row.size = sqlite3_column_int64(statement.0, 3).max(0) as u64;
                    row.target =
                        PathBuf::from(CStr::from_ptr(target).to_string_lossy().into_owned());
                    row.partial = true;
                    rows.push(row);
                }
            }
            rows
        }
    }
    #[test]
    fn reads_only_active_downloads_and_uses_received_bytes_instead_of_preallocation() {
        let root = std::env::temp_dir().join(format!(
            "vorssaint-browser-download-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("fixture.bin.crdownload");
        std::fs::write(&file, [0u8; 100]).unwrap();
        let history = root.join("History");
        let path = CString::new(history.to_string_lossy().as_bytes()).unwrap();
        let mut pointer = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                sqlite3_open_v2(path.as_ptr(), &mut pointer, 6, std::ptr::null()),
                0
            );
            let db = Db(pointer);
            let quoted = file.to_string_lossy().replace('\'', "''");
            for text in [
                "CREATE TABLE downloads(current_path TEXT,target_path TEXT,total_bytes INTEGER,received_bytes INTEGER,state INTEGER,start_time INTEGER)".to_string(),
                format!("INSERT INTO downloads VALUES('{quoted}','{quoted}',100,32,0,1)"),
                format!("INSERT INTO downloads VALUES('{quoted}','{quoted}',100,100,1,2)"),
            ] {
                let sql = CString::new(text).unwrap();
                let mut stmt = std::ptr::null_mut();
                assert_eq!(sqlite3_prepare_v2(db.0, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()), 0);
                let stmt = Statement(stmt);
                assert_eq!(sqlite3_step(stmt.0), 101);
            }
        }
        let original = std::fs::read(&history).unwrap();
        let rows = active(&history, "Fixture browser");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].size, 32);
        assert_eq!(rows[0].total, Some(100));
        assert_eq!(original, std::fs::read(&history).unwrap());
        std::fs::remove_file(file).unwrap();
        std::fs::remove_file(history).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(path: &Path, size: u64) -> Sample {
        Sample {
            path: path.into(),
            target: path.with_extension(""),
            size,
            total: Some(100),
            source: "Browser".into(),
            partial: true,
            modified: SystemTime::now(),
        }
    }
    #[test]
    fn ignores_existing_files_and_tracks_growth_without_fabricated_totals() {
        let path = PathBuf::from("test.zip.crdownload");
        let mut tracker = Tracker::default();
        let mut old = file(&path, 10);
        old.modified = SystemTime::UNIX_EPOCH;
        assert!(tracker.update(vec![old.clone()], 0).is_empty());
        old.size = 30;
        old.total = None;
        let items = tracker.update(vec![old], 1);
        assert_eq!(items[0].received, 30);
        assert_eq!(items[0].total, None);
        assert_eq!(tracker.update(vec![], 2)[0].status, "downloading");
        assert_eq!(tracker.update(vec![], 5)[0].status, "interrupted");
    }
    #[test]
    fn paused_partial_stays_active_and_rename_is_completed_only_when_target_exists() {
        let root =
            std::env::temp_dir().join(format!("vorssaint-download-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.zip.part");
        let mut tracker = Tracker::default();
        tracker.update(vec![], 0);
        let row = file(&path, 30);
        tracker.update(vec![row.clone()], 1);
        assert_eq!(tracker.update(vec![row], 120)[0].status, "downloading");
        std::fs::write(path.with_extension(""), [1, 2, 3]).unwrap();
        assert_eq!(tracker.update(vec![], 121)[0].status, "completed");
        std::fs::remove_file(path.with_extension("")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
