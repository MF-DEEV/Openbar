// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Claude Code hooks: the transport between a hook and the island, and the
//! settings.json entries that run it.

pub mod installer;
pub mod transport;

/// The app id in the hook's marker and endpoint, so a second build can install beside this one.
pub const APP_ID: &str = "com.openbar.island";

use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksStatus {
    installed: bool,
    /// Entries of this app pointing at an older path: installing replaces them.
    stale: bool,
    error: Option<String>,
    /// How many Claude accounts are shown, and how many hold the hooks (#216).
    accounts: usize,
    installed_in: usize,
}

/// The program every agent's hook runs.
pub fn current_program() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    return Ok(crate::linux::hooks::program(exe));
    #[cfg(not(target_os = "linux"))]
    Ok(exe)
}

fn current_command() -> Result<String, String> {
    Ok(installer::command(&current_program()?, APP_ID))
}

/// Every Claude account's settings file, each once: two folders whose
/// settings are one linked file share it (#216). `shown` leaves hidden
/// accounts out, as installing does; removing covers them all.
fn account_files(shown: bool) -> Vec<(crate::sessions::accounts::Account, PathBuf)> {
    let Some(home) = installer::home_dir() else {
        return vec![];
    };
    let mut accounts = crate::sessions::accounts::current(&home);
    // The app's own CLAUDE_CONFIG_DIR, or ~/.claude, even before it exists.
    if let Some(path) = installer::settings_path()
        && let Some(folder) = path.parent()
        && !accounts
            .iter()
            .any(|a| a.id == crate::sessions::accounts::canonical(folder))
    {
        accounts.insert(
            0,
            crate::sessions::accounts::Account {
                id: crate::sessions::accounts::canonical(folder),
                path: folder.to_string_lossy().into_owned(),
                name: crate::sessions::accounts::name_for(folder, &home),
                named: false,
                manual: false,
                hidden: false,
            },
        );
    }
    let mut seen = std::collections::HashSet::new();
    accounts
        .into_iter()
        .filter(|a| !shown || !a.hidden)
        .filter_map(|a| {
            let file = a.settings_path();
            let key = std::fs::canonicalize(&file)
                .unwrap_or_else(|_| file.clone())
                .to_string_lossy()
                .to_lowercase();
            seen.insert(key).then_some((a, file))
        })
        .collect()
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum AccountHooks {
    Installed,
    /// An older hook of this app, as one pointing at an old path.
    Outdated,
    Missing,
    /// The settings are not JSON Claude Code could read either; left alone.
    Unreadable,
}

fn account_hooks(path: &std::path::Path, command: &str) -> AccountHooks {
    match installer::read(path) {
        Err(_) => AccountHooks::Unreadable,
        Ok(settings) if installer::is_installed(&settings, command) => AccountHooks::Installed,
        Ok(settings) if installer::has_any(&settings, APP_ID) => AccountHooks::Outdated,
        Ok(_) => AccountHooks::Missing,
    }
}

/// Whether every account shown runs this app's current hook, for the
/// sessions snapshot.
pub fn claude_hooks_status_installed() -> bool {
    let Ok(command) = current_command() else {
        return false;
    };
    let files = account_files(true);
    !files.is_empty()
        && files
            .iter()
            .all(|(_, path)| account_hooks(path, &command) == AccountHooks::Installed)
}

#[tauri::command]
pub fn claude_hooks_status() -> HooksStatus {
    let command = match current_command() {
        Ok(command) => command,
        Err(e) => {
            return HooksStatus {
                installed: false,
                stale: false,
                error: Some(e),
                accounts: 0,
                installed_in: 0,
            };
        }
    };
    let files = account_files(true);
    if files.is_empty() {
        return HooksStatus {
            installed: false,
            stale: false,
            error: Some("no home folder".into()),
            accounts: 0,
            installed_in: 0,
        };
    }
    let states: Vec<AccountHooks> = files
        .iter()
        .map(|(_, p)| account_hooks(p, &command))
        .collect();
    let installed_in = states
        .iter()
        .filter(|s| **s == AccountHooks::Installed)
        .count();
    let unreadable = states.iter().any(|s| *s == AccountHooks::Unreadable);
    HooksStatus {
        installed: installed_in == states.len(),
        stale: installed_in < states.len() && states.contains(&AccountHooks::Outdated),
        error: unreadable.then(|| installer::Failure::Unreadable.to_string()),
        accounts: states.len(),
        installed_in,
    }
}

/// Into every account shown; why any failed, by account name.
#[tauri::command]
pub fn claude_hooks_install() -> Result<(), String> {
    let command = current_command()?;
    let files = account_files(true);
    if files.is_empty() {
        return Err("no home folder".into());
    }
    let failures: Vec<String> = files
        .iter()
        .filter_map(|(account, path)| {
            installer::update(path, |s| installer::installing(s, &command, APP_ID))
                .err()
                .map(|e| format!("{}: {e}", account.name))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Out of every account, hidden ones too; a settings file that is not there
/// is not made.
#[tauri::command]
pub fn claude_hooks_remove() -> Result<(), String> {
    let failures: Vec<String> = account_files(false)
        .iter()
        .filter(|(_, path)| path.exists())
        .filter_map(|(account, path)| {
            installer::update(path, |s| installer::removing(s, APP_ID))
                .err()
                .map(|e| format!("{}: {e}", account.name))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    #[serde(flatten)]
    account: crate::sessions::accounts::Account,
    hooks: AccountHooks,
}

/// The Claude accounts on this PC and whether each holds the hooks (#216).
#[tauri::command]
pub fn claude_accounts_status() -> Vec<AccountStatus> {
    let command = current_command().unwrap_or_default();
    let Some(home) = installer::home_dir() else {
        return vec![];
    };
    crate::sessions::accounts::current(&home)
        .into_iter()
        .map(|account| {
            let hooks = account_hooks(&account.settings_path(), &command);
            AccountStatus { account, hooks }
        })
        .collect()
}

/// The hooks into one account, as "install here too".
#[tauri::command]
pub fn claude_hooks_install_account(path: String) -> Result<(), String> {
    let command = current_command()?;
    let folder = PathBuf::from(&path);
    if !folder.is_dir() {
        return Err("not a folder".into());
    }
    installer::update(&folder.join("settings.json"), |s| {
        installer::installing(s, &command, APP_ID)
    })
    .map_err(|e| e.to_string())
}

/// Whether a folder can be added by hand: one that exists and is not listed.
#[tauri::command]
pub fn claude_accounts_can_add(path: String) -> bool {
    let folder = PathBuf::from(&path);
    let Some(home) = installer::home_dir() else {
        return false;
    };
    folder.is_absolute()
        && folder.is_dir()
        && !crate::sessions::accounts::current(&home)
            .iter()
            .any(|a| a.id == crate::sessions::accounts::canonical(&folder))
}
