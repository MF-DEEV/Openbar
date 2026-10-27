// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The Files section's shelf (spec 05 §6): what was dropped onto the island,
//! kept across launches, and the file actions on it. This side owns the
//! items, their pins and the ZIP job; the page owns selection and which
//! piles are open. Every change is saved and sent as `files://shelf`.
//!
//! Windows parts: the island's own drop target (`drop_target`, files, text
//! and links), dragging tiles out (`drag_out`), and the shell actions
//! (`shell`: Open, Open With, Show in File Explorer, Share).

pub mod archive;
mod identity;
mod payload;
mod shell;
pub mod store;
#[cfg(windows)]
mod drag_out;
#[cfg(windows)]
mod drop_target;
#[cfg(windows)]
mod ole;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use archive::Failure;
use store::{Item, Kind, Load, Shelf};

pub const SHELF_EVENT: &str = "files://shelf";
pub const ZIP_EVENT: &str = "files://zip";
#[cfg_attr(not(windows), allow(dead_code))]
/// A drag over the island: `enter`, `leave`, or `drop` with whether it landed.
pub const DRAG_EVENT: &str = "files://drag";
/// Every file an action reached for is gone (ShelfService.handleDeadDrag).
pub const MISSING_EVENT: &str = "files://missing";

/// Pasted pictures and virtual files stay this long after their item leaves,
/// so an app still reading a drop has time (ShelfService.retireOwnedPayloads).
const RETIRE_AFTER: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipStatus {
    pub running: bool,
    pub completed: usize,
    pub total: usize,
    pub outputs: Vec<String>,
    pub failure: Option<Failure>,
    pub cancelled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub items: Vec<Item>,
    pub zip: ZipStatus,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DragEvent {
    phase: &'static str,
    added: bool,
}

#[derive(Default)]
struct State {
    shelf: Shelf,
    zip: ZipStatus,
    cancel: Option<Arc<AtomicBool>>,
    /// A tile is being dragged out: the island's own drop target refuses it.
    dragging_out: bool,
}

pub struct Files {
    /// `%APPDATA%\com.vorssaint.island\shelf`.
    dir: Option<PathBuf>,
    state: Mutex<State>,
}

impl Files {
    pub fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        let dir = app.path().app_config_dir().ok().map(|d| d.join("shelf"));
        let mut shelf = Shelf::default();
        if let Some(dir) = &dir {
            let file = dir.join("items.json");
            match store::load(std::fs::read(&file).ok().as_deref()) {
                Load::Items { items, complete } => {
                    let exists = |p: &str| Path::new(p).exists();
                    let refind = |i: &Item| identity::refind(i.path.as_deref()?, i.file_id.as_deref()?);
                    shelf.items = store::sanitized(items, &exists, &refind);
                    // Only a store read whole may sweep files nothing points at.
                    if complete {
                        sweep(&dir.join("files"), &store::paths(&shelf.items));
                    }
                }
                // Moved aside, never overwritten (prefs.rs does the same).
                Load::Unreadable => {
                    let _ = std::fs::rename(&file, dir.join("items.json.broken"));
                }
            }
        }
        let files = Files { dir, state: Mutex::new(State { shelf, ..Default::default() }) };
        files.save(&files.state.lock().unwrap().shelf);
        files
    }

    /// Where dropped pictures and virtual files are written.
    pub fn store_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join("files"))
    }

    fn save(&self, shelf: &Shelf) {
        let Some(dir) = &self.dir else { return };
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join("items.json.tmp");
            std::fs::write(&tmp, store::encode(&shelf.items))?;
            std::fs::rename(&tmp, dir.join("items.json"))
        };
        let _ = write();
    }

    fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap();
        Snapshot { items: state.shelf.items.clone(), zip: state.zip.clone() }
    }
}

/// Removes payload folders under `store` that no item references.
fn sweep(store: &Path, referenced: &[String]) {
    let Ok(entries) = std::fs::read_dir(store) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !referenced.iter().any(|r| Path::new(r).starts_with(&path)) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// Files shows only while its section is on (Settings › Content).
pub fn enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    let prefs = app.state::<crate::prefs::Prefs>().get();
    let flag = |p: &str| prefs.pointer(p).and_then(|v| v.as_bool()).unwrap_or(true);
    flag("/island/enabled")
        && flag("/island/sections/files")
        && !prefs
            .pointer("/island/hiddenModules")
            .and_then(|v| v.as_array())
            .is_some_and(|v| v.iter().any(|id| id == "files"))
}

fn pref<R: Runtime>(app: &AppHandle<R>, key: &str, default: bool) -> bool {
    app.state::<crate::prefs::Prefs>()
        .get()
        .pointer(&format!("/island/{key}"))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

/// Changes the shelf, saves it, tells the page, and retires what left.
fn change<R: Runtime, T>(app: &AppHandle<R>, f: impl FnOnce(&mut Shelf) -> (T, Vec<Item>)) -> T {
    let files = app.state::<Files>();
    let (result, removed) = {
        let mut state = files.state.lock().unwrap();
        let before = state.shelf.clone();
        let out = f(&mut state.shelf);
        if state.shelf != before {
            files.save(&state.shelf);
        }
        out
    };
    retire(app, removed);
    let _ = app.emit_to(crate::island::LABEL, SHELF_EVENT, files.snapshot());
    result
}

/// Deletes the shelf's own payload files that left with their items, once
/// nothing points at them any more.
fn retire<R: Runtime>(app: &AppHandle<R>, removed: Vec<Item>) {
    let Some(store) = app.state::<Files>().store_dir() else { return };
    let owned: Vec<String> = store::paths(&removed)
        .into_iter()
        .filter(|p| store::is_owned(p, &store))
        .collect();
    if owned.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(RETIRE_AFTER);
        let live = store::paths(&app.state::<Files>().state.lock().unwrap().shelf.items);
        for path in owned.iter().filter(|p| !live.contains(p)) {
            // Each payload sits in a folder of its own, named by an id.
            if let Some(folder) = Path::new(path).parent().filter(|f| f.starts_with(&store) && *f != store) {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    });
}

/// Adds what was dropped onto the island; false when the shelf is off or full.
pub fn add<R: Runtime>(app: &AppHandle<R>, items: Vec<Item>) -> bool {
    if items.is_empty() || !enabled(app) {
        retire_now(app, &items);
        return false;
    }
    let refused = items.clone();
    let added = change(app, |shelf| (shelf.add(items).is_some(), Vec::new()));
    if !added {
        retire_now(app, &refused);
    }
    added
}

/// Payloads written for a drop the shelf could not take.
fn retire_now<R: Runtime>(app: &AppHandle<R>, items: &[Item]) {
    let Some(store) = app.state::<Files>().store_dir() else { return };
    for path in store::paths(items).iter().filter(|p| store::is_owned(p, &store)) {
        if let Some(folder) = Path::new(path).parent().filter(|f| *f != store) {
            let _ = std::fs::remove_dir_all(folder);
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
/// The island's drop target reports here (on the island's thread).
pub fn drag_phase<R: Runtime>(app: &AppHandle<R>, phase: &'static str, added: bool) {
    let _ = app.emit_to(crate::island::LABEL, DRAG_EVENT, DragEvent { phase, added });
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Whether a drag entering the island may land on the shelf.
pub fn accepts_drops<R: Runtime>(app: &AppHandle<R>) -> bool {
    enabled(app) && !app.state::<Files>().state.lock().unwrap().dragging_out
}

/// The file leaves under `ids`, found again when moved. When every file is
/// gone the person is told and the dead tiles leave, except files on a drive
/// that is simply not connected now.
fn living_files<R: Runtime>(app: &AppHandle<R>, ids: &[String]) -> Vec<(String, String)> {
    let leaves = app.state::<Files>().state.lock().unwrap().shelf.leaves(ids);
    let mut living = Vec::new();
    let mut dead = Vec::new();
    for leaf in leaves.iter().filter(|l| l.kind == Kind::File) {
        let Some(path) = leaf.path.clone() else { continue };
        if Path::new(&path).exists() {
            living.push((leaf.id.clone(), path));
        } else if let Some(found) = leaf.file_id.as_deref().and_then(|id| identity::refind(&path, id)) {
            change(app, |shelf| (shelf.heal(&leaf.id, &found), Vec::new()));
            living.push((leaf.id.clone(), found));
        } else {
            dead.push(leaf);
        }
    }
    if living.is_empty() && !dead.is_empty() {
        let _ = app.emit_to(crate::island::LABEL, MISSING_EVENT, ());
        let gone: HashSet<String> = dead
            .iter()
            .filter(|l| drive_present(l.path.as_deref().unwrap_or_default()))
            .map(|l| l.id.clone())
            .collect();
        if !gone.is_empty() {
            change(app, |shelf| ((), shelf.remove(&gone)));
        }
    }
    living
}

fn drive_present(path: &str) -> bool {
    Path::new(path).ancestors().last().is_some_and(|root| root.exists())
}

fn living_paths<R: Runtime>(app: &AppHandle<R>, ids: &[String]) -> Vec<String> {
    living_files(app, ids).into_iter().map(|(_, p)| p).collect()
}

// MARK: Commands

#[tauri::command]
pub fn files_snapshot(files: tauri::State<'_, Files>) -> Snapshot {
    files.snapshot()
}

#[tauri::command]
pub fn files_remove<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) {
    let ids: HashSet<String> = ids.into_iter().collect();
    change(&app, |shelf| ((), shelf.remove(&ids)));
}

#[tauri::command]
pub fn files_clear<R: Runtime>(app: AppHandle<R>) {
    change(&app, |shelf| ((), shelf.clear()));
}

#[tauri::command]
pub fn files_toggle_pin<R: Runtime>(app: AppHandle<R>, id: String) {
    change(&app, |shelf| {
        shelf.toggle_pin(&id);
        ((), Vec::new())
    });
}

#[tauri::command]
pub async fn files_open<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) {
    let paths = living_paths(&app, &ids);
    let _ = tauri::async_runtime::spawn_blocking(move || shell::open(paths)).await;
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Handler {
    /// What identifies the handler again for `files_open_with`.
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

/// The apps that open every selected file, up to 40, by name.
#[tauri::command]
pub async fn files_open_with_apps<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) -> Vec<Handler> {
    let paths = living_paths(&app, &ids);
    tauri::async_runtime::spawn_blocking(move || shell::handlers(paths))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn files_open_with<R: Runtime>(app: AppHandle<R>, ids: Vec<String>, handler: String) {
    let paths = living_paths(&app, &ids);
    let _ = tauri::async_runtime::spawn_blocking(move || shell::open_with(paths, handler)).await;
}

#[tauri::command]
pub async fn files_reveal<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) {
    let paths = living_paths(&app, &ids);
    let _ = tauri::async_runtime::spawn_blocking(move || shell::reveal(paths)).await;
}

/// "Show in File Explorer" after a ZIP: selects the archives it made.
#[tauri::command]
pub async fn files_reveal_outputs(files: tauri::State<'_, Files>) -> Result<(), ()> {
    let outputs = files.state.lock().unwrap().zip.outputs.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || shell::reveal(outputs)).await;
    Ok(())
}

#[tauri::command]
pub async fn files_share<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) -> bool {
    let paths = living_paths(&app, &ids);
    !paths.is_empty() && shell::share(&app, paths).await
}

/// Drags these tiles out of the island (ShelfTileView.beginItemDrag). Answers
/// whether a destination accepted them; accepted tiles leave the shelf unless
/// pinned or the person keeps them (shelfRemoveAfterDrop).
#[tauri::command]
pub async fn files_drag_out<R: Runtime>(app: AppHandle<R>, ids: Vec<String>) -> bool {
    let leaves = app.state::<Files>().state.lock().unwrap().shelf.leaves(&ids);
    // Dead files would make every destination refuse the whole drag.
    let living: HashMap<String, String> = living_files(&app, &ids).into_iter().collect();
    let mut dragged = Vec::new();
    let mut paths = Vec::new();
    let mut texts = Vec::new();
    for leaf in &leaves {
        match leaf.kind {
            Kind::File => {
                if let Some(path) = living.get(&leaf.id) {
                    paths.push(path.clone());
                    dragged.push(leaf.id.clone());
                }
            }
            Kind::Text => {
                texts.push(leaf.text.clone().unwrap_or_default());
                dragged.push(leaf.id.clone());
            }
            Kind::Link => {
                texts.push(leaf.url.clone().unwrap_or_default());
                dragged.push(leaf.id.clone());
            }
            Kind::Batch => {}
        }
    }
    if dragged.is_empty() {
        return false;
    }
    let remove_after = pref(&app, "shelfRemoveAfterDrop", true);
    let protected = app.state::<Files>().state.lock().unwrap().shelf.protected_ids();
    // A kept item must not be moved away from where the shelf remembers it.
    let allow_move = remove_after && !dragged.iter().any(|id| protected.contains(id));
    app.state::<Files>().state.lock().unwrap().dragging_out = true;
    let accepted = run_drag(&app, paths, texts, allow_move).await;
    app.state::<Files>().state.lock().unwrap().dragging_out = false;
    if accepted && remove_after {
        let removable: HashSet<String> = dragged.into_iter().filter(|id| !protected.contains(id)).collect();
        if !removable.is_empty() {
            change(&app, |shelf| ((), shelf.remove(&removable)));
        }
    }
    accepted
}

#[cfg(windows)]
async fn run_drag<R: Runtime>(app: &AppHandle<R>, paths: Vec<String>, texts: Vec<String>, allow_move: bool) -> bool {
    let Some(window) = app.get_webview_window(crate::island::LABEL) else { return false };
    let Ok(hwnd) = window.hwnd() else { return false };
    let hwnd = hwnd.0 as isize;
    let (tx, rx) = std::sync::mpsc::channel();
    let island = app.clone();
    let on_island: drag_out::OnIsland =
        Box::new(move |x, y| island.state::<crate::island::IslandState>().contains(x, y));
    // On the island's own thread: OLE drags start where the button went down.
    let started = app.run_on_main_thread(move || {
        let _ = tx.send(drag_out::run(hwnd, &paths, &texts, allow_move, on_island));
    });
    if started.is_err() {
        return false;
    }
    tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or(false))
        .await
        .unwrap_or(false)
}

#[cfg(not(windows))]
async fn run_drag<R: Runtime>(_app: &AppHandle<R>, _paths: Vec<String>, _texts: Vec<String>, _allow_move: bool) -> bool {
    false
}

/// Create ZIP (NotchFilesView.chooseArchiveDestination): one input asks
/// where to save `<name>.zip`, several ask for a folder and each becomes its
/// own ZIP. The dialog belongs to the island, so it opens above it. Answers
/// whether a job started; progress comes as `files://zip`.
#[tauri::command]
pub async fn files_zip<R: Runtime>(app: AppHandle<R>, ids: Vec<String>, title: String) -> bool {
    if app.state::<Files>().state.lock().unwrap().zip.running {
        return false;
    }
    let inputs: Vec<PathBuf> = living_paths(&app, &ids).into_iter().map(PathBuf::from).collect();
    let Some(first) = inputs.first().cloned() else { return false };
    let directory = inputs.len() > 1;
    let dialog = app.clone();
    let destination = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let folder = first.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut builder = dialog.dialog().file().set_title(title).set_directory(&folder);
        if let Some(window) = dialog.get_webview_window(crate::island::LABEL) {
            builder = builder.set_parent(&window);
        }
        let chosen = if directory {
            builder.blocking_pick_folder()
        } else {
            let name = first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let suggested = archive::unique_zip_path(&folder, &name, &HashSet::new());
            builder
                .set_file_name(suggested.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                .add_filter("ZIP", &["zip"])
                .blocking_save_file()
        };
        chosen.and_then(|p| p.into_path().ok())
    })
    .await
    .ok()
    .flatten();
    let Some(destination) = destination else { return false };
    if !enabled(&app) {
        return false;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let files = app.state::<Files>();
        let mut state = files.state.lock().unwrap();
        if state.zip.running {
            return false;
        }
        state.zip = ZipStatus { running: true, total: inputs.len(), ..Default::default() };
        state.cancel = Some(cancel.clone());
    }
    emit_zip(&app);
    let worker = app.clone();
    std::thread::spawn(move || run_zip(&worker, inputs, destination, directory, &cancel));
    true
}

fn run_zip<R: Runtime>(app: &AppHandle<R>, inputs: Vec<PathBuf>, destination: PathBuf, directory: bool, cancel: &AtomicBool) {
    let mut failure = None;
    let mut cancelled = false;
    if archive::inside_inputs(&destination, &inputs) {
        failure = Some(Failure::Inside);
    } else {
        let mut reserved = HashSet::new();
        for input in &inputs {
            let output = if directory {
                let name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let output = archive::unique_zip_path(&destination, &name, &reserved);
                reserved.insert(output.clone());
                output
            } else {
                destination.clone()
            };
            match archive::archive(input, &output, cancel) {
                Ok(()) => {
                    let path = output.to_string_lossy().into_owned();
                    {
                        let files = app.state::<Files>();
                        let mut state = files.state.lock().unwrap();
                        state.zip.completed += 1;
                        state.zip.outputs.push(path.clone());
                    }
                    let id = identity::of(&output).map(|i| i.encode());
                    add(app, vec![Item::file(&path, id)]);
                    emit_zip(app);
                }
                Err(Failure::Cancelled) => {
                    cancelled = true;
                    break;
                }
                Err(other) => {
                    failure = Some(other);
                    break;
                }
            }
        }
    }
    {
        let files = app.state::<Files>();
        let mut state = files.state.lock().unwrap();
        state.zip.running = false;
        state.zip.failure = failure;
        state.zip.cancelled = cancelled;
        state.cancel = None;
    }
    emit_zip(app);
}

fn emit_zip<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit_to(crate::island::LABEL, ZIP_EVENT, app.state::<Files>().snapshot().zip);
}

#[tauri::command]
pub fn files_zip_cancel(files: tauri::State<'_, Files>) {
    let state = files.state.lock().unwrap();
    if let Some(cancel) = &state.cancel {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// Puts the drop target on the island once its webview exists.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    #[cfg(windows)]
    drop_target::install(app);
    #[cfg(not(windows))]
    let _ = app;
}
