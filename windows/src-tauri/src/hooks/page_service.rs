// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The Hooks page's commands, and what keeps its rows true while the app
//! runs (#236; ClaudeSessionService.swift): lost hooks put back at launch and
//! when a hook file changes on disk, said once in a notification.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Wry};

use super::page::{self, Env, Place, Target};
use crate::sessions::models::Provider;

/// Told to the Settings window when the rows change.
pub const CHANGED: &str = "hooks://changed";
/// How often the hook files are looked at for a change.
const WATCH_EVERY: Duration = Duration::from_secs(5);
/// How often one row is put back in a run; a file something keeps emptying
/// is left alone after that.
const REPAIRS_PER_ROW: u32 = 3;

static APP: OnceLock<AppHandle<Wry>> = OnceLock::new();
/// Why the last Reinstall, Remove or add of a row failed, by row id.
static FAILURES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
static REPAIRED: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);
static WATCH: Mutex<Option<Watch>> = Mutex::new(None);
static CHECKING: AtomicBool = AtomicBool::new(false);
/// One change at a time: a repair and a button may meet.
static CHANGING: Mutex<()> = Mutex::new(());

struct Watch {
    last: Instant,
    stamps: BTreeMap<PathBuf, Option<SystemTime>>,
}

pub fn set_app(app: AppHandle<Wry>) {
    let _ = APP.set(app);
}

fn changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit(CHANGED, ());
    }
}

pub fn env() -> Option<Env> {
    Some(Env {
        home: super::installer::home_dir()?,
        exe: super::current_program().ok()?,
        endpoint: super::transport::endpoint(super::APP_ID),
        app: super::APP_ID.to_owned(),
    })
}

pub fn context(env: &Env) -> page::Context {
    let settings = page::settings();
    let mut found: HashSet<Provider> = Provider::HOOK_AGENTS
        .into_iter()
        .filter(|p| crate::sessions::hook_agents::installer::is_present(*p, &env.home))
        .collect();
    if crate::agent_usage::codex_executable().is_some() {
        found.insert(Provider::Codex);
    }
    page::Context {
        home: env.home.clone(),
        exe: env.exe.clone(),
        endpoint: env.endpoint.clone(),
        app: env.app.clone(),
        accounts: super::account_files(false)
            .into_iter()
            .map(|(a, _)| a)
            .collect(),
        hidden: settings.hidden,
        found,
        folders: settings.folders,
        tools: settings.tools,
        opt_out: page::opt_out(),
        records: crate::diagnostics::hook_records::all(),
    }
}

/// The rows as they are now, each with why its last change failed.
/// `settled`: the files are as the island leaves them, so a change seen
/// before now needs no second look; otherwise only new files are noted.
fn rows(env: &Env, settled: bool) -> Vec<Target> {
    let mut targets = page::list(&context(env));
    let failures = FAILURES.lock().unwrap().clone();
    for target in &mut targets {
        target.failure = failures.get(&target.id).cloned();
    }
    watch(&targets, settled);
    targets
}

fn stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Remembers how the hook files look now, so only a later change counts.
/// A look that is not `settled` keeps what was noted of each file, so a
/// change it happens to see first is still put right by the next tick.
fn watch(targets: &[Target], settled: bool) {
    let mut watch = WATCH.lock().unwrap();
    let Some(watch) = watch.as_mut() else {
        // Nothing is watched before the first check at launch.
        return;
    };
    let before = std::mem::take(&mut watch.stamps);
    watch.stamps = targets
        .iter()
        .flat_map(|t| {
            let mut files: Vec<PathBuf> = t.file.iter().cloned().collect();
            // Factory Droid's hooks.json beside its settings decides where its entries go (#242).
            if t.provider == Provider::Droid
                && let Some(folder) = t.file.as_ref().and_then(|f| f.parent())
            {
                files.push(folder.join("hooks.json"));
                files.push(folder.join("hooks").join("hooks.json"));
            }
            files
        })
        .map(|f| {
            let s = match before.get(&f) {
                Some(s) if !settled => *s,
                _ => stamp(&f),
            };
            (f, s)
        })
        .collect();
}

/// Counts one putting back of a row; false once a row came back often
/// enough this run that something must keep taking it out.
pub fn note_repair(id: &str) -> bool {
    let mut repaired = REPAIRED.lock().unwrap();
    let counts = repaired.get_or_insert_with(HashMap::new);
    let count = counts.entry(id.to_owned()).or_default();
    if *count >= REPAIRS_PER_ROW {
        return false;
    }
    *count += 1;
    true
}

/// Says once that lost hooks were put back.
pub fn announce(names: &[String]) {
    if names.is_empty() {
        return;
    }
    let list = names.join(", ");
    crate::diagnostics::log::info(
        "hooks",
        &format!("told the person the hooks came back in {list}"),
    );
    crate::watch::toast(
        "Openbar put its hooks back",
        &format!("They had gone missing from {list}. You can turn this off in Settings › Hooks."),
    );
}

/// Reads every row again; with `repair`, puts back the hooks that were lost
/// first, as the person allows.
pub fn check(repair: bool) -> Vec<Target> {
    let Some(env) = env() else {
        return Vec::new();
    };
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut targets = rows(&env, repair);
    if !repair {
        return targets;
    }
    let records = crate::diagnostics::hook_records::all();
    let due: Vec<Target> = page::repairs(&targets, &records, page::settings().auto_repair)
        .into_iter()
        .cloned()
        .collect();
    let mut put_back = Vec::new();
    for target in &due {
        if !note_repair(&target.id) {
            continue;
        }
        let path = target
            .file
            .as_deref()
            .map(|f| f.display().to_string())
            .unwrap_or_default();
        match page::install(target, &env) {
            Ok(()) => {
                crate::diagnostics::log::info("hooks", &format!("put back the hooks in {path}"));
                put_back.push(if target.provider == Provider::Claude {
                    format!("Claude Code ({})", target.name)
                } else {
                    target.name.clone()
                });
            }
            Err(e) => crate::diagnostics::log::error(
                "hooks",
                &format!("could not put back the hooks in {path}: {e}"),
            ),
        }
    }
    if !due.is_empty() {
        targets = rows(&env, true);
    }
    if !put_back.is_empty() {
        announce(&put_back);
        changed();
    }
    targets
}

/// From the sessions ticker: at launch, and whenever a hook file changed on
/// disk, the agents' hooks are kept in step and lost ones put back.
pub fn tick() {
    let due = {
        let mut watch = WATCH.lock().unwrap();
        match watch.as_mut() {
            None => true,
            Some(w) if w.last.elapsed() < WATCH_EVERY => false,
            Some(w) => {
                w.last = Instant::now();
                w.stamps.iter().any(|(f, s)| stamp(f) != *s)
            }
        }
    };
    if !due || CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    {
        let mut watch = WATCH.lock().unwrap();
        if watch.is_none() {
            *watch = Some(Watch {
                last: Instant::now(),
                stamps: BTreeMap::new(),
            });
        }
    }
    std::thread::spawn(|| {
        crate::sessions::service::sync_agent_hooks(&page::settings().hidden);
        check(true);
        changed();
        CHECKING.store(false, Ordering::SeqCst);
    });
}

fn find(env: &Env, id: &str) -> Result<Target, String> {
    page::list(&context(env))
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| "That row is no longer on the page.".to_owned())
}

fn note_failure(id: &str, result: &Result<(), String>) {
    let mut failures = FAILURES.lock().unwrap();
    match result {
        Ok(()) => failures.remove(id),
        Err(e) => failures.insert(id.to_owned(), e.clone()),
    };
}

/// Reinstall or Remove on one row, a copy of the file kept first.
fn change(id: &str, install: bool) -> Result<(), String> {
    let env = env().ok_or("no home folder")?;
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let target = find(&env, id)?;
    let result = if install {
        page::reinstall(&target, &env)
    } else {
        page::take_out(&target, &env)
    };
    let path = target
        .file
        .as_deref()
        .map(|f| f.display().to_string())
        .unwrap_or_else(|| target.id.clone());
    let verb = if install { "reinstall" } else { "remove" };
    match &result {
        Ok(()) => crate::diagnostics::log::info("hooks", &format!("{verb}: the hooks in {path}")),
        Err(e) => crate::diagnostics::log::error(
            "hooks",
            &format!("could not {verb} the hooks in {path}: {e}"),
        ),
    }
    note_failure(id, &result);
    result
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let result = tauri::async_runtime::spawn_blocking(work).await;
    match result {
        Ok(value) => value,
        Err(e) => panic!("hooks page work failed: {e}"),
    }
}

fn done<T>(result: T) -> T {
    changed();
    result
}

#[tauri::command]
pub async fn hooks_page_status() -> Vec<Target> {
    blocking(|| check(false)).await
}

/// "Put lost hooks back", `/hooks/autoRepair` in prefs, on unless turned off.
#[tauri::command]
pub fn hooks_page_auto_repair() -> bool {
    page::settings().auto_repair
}

/// Turns "Put lost hooks back" on or off; turned on, the hooks lost
/// meanwhile come back at once.
#[tauri::command]
pub async fn hooks_page_set_auto_repair(app: AppHandle<Wry>, on: bool) -> Result<(), String> {
    blocking(move || {
        save_prefs(&app, json!({ "hooks": { "autoRepair": on } }))?;
        if on {
            crate::sessions::service::sync_agent_hooks(&page::settings().hidden);
            check(true);
        }
        done(Ok(()))
    })
    .await
}

#[tauri::command]
pub async fn hooks_page_reinstall(id: String) -> Result<(), String> {
    blocking(move || done(change(&id, true))).await
}

#[tauri::command]
pub async fn hooks_page_remove(id: String) -> Result<(), String> {
    blocking(move || done(change(&id, false))).await
}

/// Takes the island out of every file it wrote into; how many rows failed.
#[tauri::command]
pub async fn hooks_page_remove_all() -> usize {
    blocking(|| {
        let Some(env) = env() else {
            return 0;
        };
        let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
        let targets = page::list(&context(&env));
        let failures = page::remove_everything(&targets, &env);
        crate::diagnostics::log::info(
            "hooks",
            &format!("removed every hook, {} failed", failures.len()),
        );
        *FAILURES.lock().unwrap() = failures.clone();
        done(failures.len())
    })
    .await
}

/// Shows a row's file in Explorer or the file manager: only paths from the
/// rows themselves.
#[tauri::command]
pub async fn hooks_page_reveal(id: String) -> bool {
    blocking(move || {
        let Some(env) = env() else {
            return false;
        };
        let Ok(target) = find(&env, &id) else {
            return false;
        };
        let path = match target.file.filter(|f| f.exists()) {
            Some(file) => file,
            None if target.folder.is_dir() => target.folder,
            None => return false,
        };
        crate::files::shell::reveal(vec![path.to_string_lossy().into_owned()]);
        true
    })
    .await
}

fn save_prefs(app: &AppHandle<Wry>, patch: Value) -> Result<(), String> {
    crate::prefs::save_patch(app, patch).map(|_| ())
}

fn add_and_install(id: &str) {
    let Some(env) = env() else {
        return;
    };
    let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
    let result = find(&env, id).and_then(|t| page::reinstall(&t, &env));
    note_failure(id, &result);
}

/// Adds a config folder for an agent and puts its hooks in; the reason it
/// could not be added, as a code the page words.
#[tauri::command]
pub async fn hooks_page_add_folder(
    app: AppHandle<Wry>,
    agent: String,
    path: String,
) -> Result<(), String> {
    blocking(move || {
        let provider = Provider::from_key(&agent)
            .filter(|p| crate::sessions::hook_agents::installer::FOLDERS_BY_HAND.contains(p))
            .ok_or("notFolder")?;
        let home = super::installer::home_dir().ok_or("notFolder")?;
        let own = crate::sessions::hook_agents::installer::agent_folder(provider, &home);
        let existing = page::settings().folders;
        let setting =
            page::add_folder(provider, Path::new(&path), own.as_deref(), &home, &existing)
                .map_err(|f| f.code().to_owned())?;
        let mut folders = existing;
        folders.push(setting.clone());
        save_prefs(&app, json!({ "sessions": { "agentFolders": folders } }))?;
        add_and_install(&setting.target_id());
        done(Ok(()))
    })
    .await
}

/// Adds a tool with Claude Code style hooks and puts its hooks in.
#[tauri::command]
pub async fn hooks_page_add_tool(
    app: AppHandle<Wry>,
    name: String,
    path: String,
) -> Result<(), String> {
    blocking(move || {
        let claude_files: Vec<PathBuf> = super::account_files(false)
            .into_iter()
            .map(|(_, f)| f)
            .collect();
        let existing = page::settings().tools;
        let tool = page::add_tool(&name, Path::new(path.trim()), &claude_files, &existing)
            .map_err(|f| f.code().to_owned())?;
        let mut tools = existing;
        tools.push(tool.clone());
        save_prefs(&app, json!({ "sessions": { "customHookTools": tools } }))?;
        add_and_install(&tool.target_id());
        done(Ok(()))
    })
    .await
}

/// Picks a tool's settings file.
#[tauri::command]
pub async fn hooks_page_pick_file(app: AppHandle<Wry>) -> Option<String> {
    blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let mut builder = app.dialog().file();
        if let Some(window) = app.get_webview_window(crate::settings::LABEL) {
            builder = builder.set_parent(&window);
        }
        builder
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
}

/// Takes a folder, tool or Claude account added by hand off the page, its
/// hooks out first and its file as before the island.
#[tauri::command]
pub async fn hooks_page_forget(app: AppHandle<Wry>, id: String) -> Result<(), String> {
    blocking(move || {
        let env = env().ok_or("no home folder")?;
        let _one = CHANGING.lock().unwrap_or_else(|e| e.into_inner());
        let target = find(&env, &id)?;
        if !target.manual {
            return Err("That row can't be taken off the page.".into());
        }
        let result = page::forget(&target, &env);
        note_failure(&id, &result);
        result?;
        let prefs = app.state::<crate::prefs::Prefs>().get();
        let patch = match &target.place {
            Place::Agent(_, Some(_)) => {
                let folders: Vec<_> = page::settings()
                    .folders
                    .into_iter()
                    .filter(|f| f.target_id() != target.id)
                    .collect();
                json!({ "sessions": { "agentFolders": folders } })
            }
            Place::Custom(tool) => {
                let tools: Vec<_> = page::settings()
                    .tools
                    .into_iter()
                    .filter(|t| t.id != tool.id)
                    .collect();
                json!({ "sessions": { "customHookTools": tools } })
            }
            Place::Claude(_) => {
                let id = crate::sessions::accounts::canonical(&target.folder);
                let accounts: Vec<Value> = prefs
                    .pointer("/sessions/claudeAccounts")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|a| {
                        a.get("path").and_then(Value::as_str).is_none_or(|p| {
                            crate::sessions::accounts::canonical(Path::new(p)) != id
                        })
                    })
                    .collect();
                json!({ "sessions": { "claudeAccounts": accounts } })
            }
            Place::Agent(_, None) => return Ok(()),
        };
        save_prefs(&app, patch)?;
        done(Ok(()))
    })
    .await
}

/// `Openbar --remove-hooks`: everything the island wrote, in every file the
/// page lists, comes out, each file as it was before Openbar where nothing
/// else changed in it. The prefs and records are read from the app's config
/// folder, found without a running app.
pub fn remove_everything_for_uninstall() -> bool {
    if let Some(folder) = config_dir() {
        crate::diagnostics::hook_records::init(&folder);
        page::init(&folder);
        if let Some(prefs) = std::fs::read(folder.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        {
            crate::sessions::accounts::set_settings(
                crate::sessions::accounts::settings_from_prefs(&prefs),
            );
            page::set_settings(page::settings_from_prefs(&prefs));
        }
    }
    let Some(env) = env() else {
        return false;
    };
    let targets = page::list(&context(&env));
    page::remove_everything(&targets, &env).is_empty()
}

/// The app's config folder, as Tauri names it: `%APPDATA%\<id>` on Windows,
/// `$XDG_CONFIG_HOME/<id>` (`~/.config/<id>`) elsewhere.
pub fn config_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)?
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| super::installer::home_dir().map(|h| h.join(".config")))?
    };
    Some(base.join(super::APP_ID))
}
