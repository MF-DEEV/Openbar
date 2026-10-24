// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Claude Code hooks: the transport between a hook and the island, and the
//! settings.json entries that run it.

pub mod installer;
pub mod transport;

/// The app id in the hook's marker and endpoint, so a second build can install beside this one.
pub const APP_ID: &str = "com.vorssaint.island";

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksStatus {
    installed: bool,
    /// Entries of this app pointing at an older path: installing replaces them.
    stale: bool,
    error: Option<String>,
}

fn current_command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(installer::command(&exe, APP_ID))
}

/// Whether every event runs this app's current hook, for the sessions snapshot.
pub fn claude_hooks_status_installed() -> bool {
    let (Some(path), Ok(command)) = (installer::settings_path(), current_command()) else {
        return false;
    };
    installer::read(&path).is_ok_and(|settings| installer::is_installed(&settings, &command))
}

#[tauri::command]
pub fn claude_hooks_status() -> HooksStatus {
    let Some(path) = installer::settings_path() else {
        return HooksStatus {
            installed: false,
            stale: false,
            error: Some("no home folder".into()),
        };
    };
    match (installer::read(&path), current_command()) {
        (Ok(settings), Ok(command)) => {
            let installed = installer::is_installed(&settings, &command);
            HooksStatus {
                installed,
                stale: !installed && installer::has_any(&settings, APP_ID),
                error: None,
            }
        }
        (Err(e), _) => HooksStatus {
            installed: false,
            stale: false,
            error: Some(e.to_string()),
        },
        (_, Err(e)) => HooksStatus {
            installed: false,
            stale: false,
            error: Some(e),
        },
    }
}

#[tauri::command]
pub fn claude_hooks_install() -> Result<(), String> {
    let path = installer::settings_path().ok_or("no home folder")?;
    let command = current_command()?;
    installer::update(&path, |s| installer::installing(s, &command, APP_ID))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn claude_hooks_remove() -> Result<(), String> {
    let path = installer::settings_path().ok_or("no home folder")?;
    installer::update(&path, |s| installer::removing(s, APP_ID)).map_err(|e| e.to_string())
}
