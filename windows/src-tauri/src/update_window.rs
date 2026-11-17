// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The update prompt (#213), the Mac's UpdatePreviewView: the new version,
//! what changed, and Install and relaunch / Remind me later / Skip.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "update";

fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("update.html".into()))
        .title("Openbar")
        .inner_size(520.0, 480.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .center()
        .focused(true)
        .build();
}

/// Built on a worker, as the settings window is: a webview built on the
/// event loop's thread never loads.
#[tauri::command]
pub async fn update_window_open<R: Runtime>(app: AppHandle<R>) {
    show(&app);
}
