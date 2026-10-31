// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The settings window, opened from the tray or the island's gear.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "settings";

/// Opens (or brings forward) the settings window. On Windows a webview
/// built on the thread that runs the event loop never loads (Tauri's
/// synchronous-command deadlock), so the building happens on a worker.
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
    let _ = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title(format!(
            "Vorssaint — {}",
            crate::locale::text(app, "settings").trim_end_matches('…')
        ))
        .inner_size(860.0, 620.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
}

#[tauri::command]
pub async fn open_settings<R: Runtime>(app: AppHandle<R>) {
    show(&app);
}
