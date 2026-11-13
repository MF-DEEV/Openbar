// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The first-run window (#168), as the Mac's onboarding: it opens once on a
//! clean install and introduces Openbar through the agents' sessions. An
//! install that already had settings counts as onboarded, so an update
//! never puts it in front of someone who knows the app.

use serde_json::json;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::prefs::Prefs;

pub const LABEL: &str = "onboarding";

/// Whether the first-run window should open at launch: never once it was
/// finished, and not for settings written before onboarding existed.
pub fn due(onboarded: bool, settings_existed: bool) -> bool {
    !onboarded && !settings_existed
}

/// At launch: opens the window on a clean install (or when `--onboarding`
/// asks), and marks an existing install as already onboarded.
pub fn start<R: Runtime>(app: &AppHandle<R>) {
    let prefs = app.state::<Prefs>();
    let onboarded = prefs.get().get("onboarded").and_then(|v| v.as_bool()) == Some(true);
    let forced = std::env::args().any(|a| a == "--onboarding");
    if forced || due(onboarded, prefs.existed()) {
        open(app);
    } else if !onboarded {
        let _ = prefs.set(&json!({ "onboarded": true }));
    }
}

/// Opens (or brings forward) the window. Built on a worker, as the settings
/// window is, since a webview built on the event loop's thread never loads.
pub fn open<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { show(&app) });
}

fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("onboarding.html".into()))
        .title("Openbar")
        .inner_size(560.0, 640.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .focused(true)
        .build();
}

#[tauri::command]
pub async fn onboarding_open<R: Runtime>(app: AppHandle<R>) {
    show(&app);
}

/// Done or skipped: never shown at launch again, and the window closes.
#[tauri::command]
pub fn onboarding_finish<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    app.state::<Prefs>().set(&json!({ "onboarded": true }))?;
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
    Ok(())
}

/// Whether Claude Code is on this PC: its command, or the folder it keeps.
#[tauri::command]
pub fn onboarding_claude_present() -> bool {
    crate::sessions::background::executable().is_some()
        || crate::hooks::installer::home_dir().is_some_and(|home| home.join(".claude").is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_on_a_clean_install() {
        assert!(due(false, false));
        assert!(!due(true, false));
        assert!(
            !due(false, true),
            "an update keeps the person's settings and skips the tour"
        );
        assert!(!due(true, true));
    }
}
