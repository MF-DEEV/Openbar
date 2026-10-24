// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The settings window, opened from the tray or the island's gear.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "settings";

pub fn open<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("Vorssaint Settings")
        .inner_size(860.0, 620.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
}

#[tauri::command]
pub fn open_settings<R: Runtime>(app: AppHandle<R>) {
    open(&app);
}
