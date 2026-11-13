// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Installs an update in place (#213). The page picks the release from the
//! GitHub feed (updates/releases.ts); this reads that release's `latest.json`,
//! downloads the installer, checks its minisign signature against the public
//! key in tauri.conf.json, and runs it passively. The installer replaces the
//! app and starts it again. A download whose signature does not match is
//! refused and deleted; nothing runs.
//!
//! Settings, hooks and the rest live outside the install folder (prefs.rs,
//! Claude's own settings), and the NSIS hooks keep them on an update
//! (installer-hooks.nsh). The one thing to carry over is which window was
//! open: Settings comes back if it was.

use serde::Serialize;
use serde_json::json;
#[cfg(windows)]
use tauri::Emitter;
use tauri::{AppHandle, Manager, Runtime};

use crate::prefs::Prefs;

/// Where releases live. `OPENBAR_UPDATE_SERVER` points both at a local
/// server for testing; the signature is still required, so it cannot get an
/// unsigned build installed.
const GITHUB_FEED: &str = "https://api.github.com/repos/MF-DEEV/Openbar/releases?per_page=30";
const GITHUB_DOWNLOADS: &str = "https://github.com/MF-DEEV/Openbar/releases/download";
const TEST_SERVER: &str = "OPENBAR_UPDATE_SERVER";
const REOPEN_SETTINGS: &str = "reopenSettingsAfterUpdate";

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Endpoints {
    /// The releases list the page reads.
    pub feed: String,
    /// Where a release's files are, `<downloads>/<tag>/<file>`.
    pub downloads: String,
}

fn endpoints_for(server: Option<&str>) -> Endpoints {
    match server
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
    {
        Some(server) => Endpoints {
            feed: format!("{server}/releases.json"),
            downloads: format!("{server}/download"),
        },
        None => Endpoints {
            feed: GITHUB_FEED.into(),
            downloads: GITHUB_DOWNLOADS.into(),
        },
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn test_server() -> Option<String> {
    std::env::var(TEST_SERVER)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

#[tauri::command]
pub fn updates_endpoints() -> Endpoints {
    endpoints_for(test_server().as_deref())
}

/// The `latest.json` of the release tagged `tag`. Only tags this platform
/// publishes are accepted, so the page cannot be steered to any URL.
#[cfg_attr(not(windows), allow(dead_code))]
fn manifest_url(downloads: &str, tag: &str) -> Result<String, String> {
    let version = tag
        .strip_prefix("windows-v")
        .ok_or_else(|| "not a Windows release".to_string())?;
    let valid = !version.is_empty()
        && version.len() <= 40
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !valid {
        return Err("not a Windows release".into());
    }
    Ok(format!("{downloads}/{tag}/latest.json"))
}

#[cfg(windows)]
#[derive(Serialize, Clone)]
struct Progress {
    received: u64,
    total: Option<u64>,
}

/// Short, untranslated reasons the page maps to its own words; "signature"
/// is the refused-download message.
#[cfg(windows)]
fn reason(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_) | Error::SignatureUtf8(_) | Error::Base64(_) => "signature".into(),
        Error::ReleaseNotFound => "not found".into(),
        other => other.to_string(),
    }
}

/// Downloads, verifies and installs the release tagged `tag`, emitting
/// `updates://progress` on the way. On success the process exits while the
/// installer runs, so this only ever returns an error.
#[cfg(not(windows))]
#[tauri::command]
pub async fn updates_install<R: Runtime>(_app: AppHandle<R>, _tag: String) -> Result<(), String> {
    Err("unsupported".into())
}

#[cfg(windows)]
#[tauri::command]
pub async fn updates_install<R: Runtime>(app: AppHandle<R>, tag: String) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    let server = test_server();
    let url = manifest_url(&updates_endpoints().downloads, &tag)?;
    let url = url.parse().map_err(|_| "bad url".to_string())?;
    let mut builder = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?;
    if server.is_none() {
        builder = builder.timeout(std::time::Duration::from_secs(120));
    }
    let reopen = app.clone();
    let updater = builder
        .on_before_exit(move || remember_open_windows(&reopen))
        .build()
        .map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| reason(&e))?
        .ok_or_else(|| "up to date".to_string())?;
    let mut received = 0u64;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                received += chunk as u64;
                let _ = progress.emit("updates://progress", Progress { received, total });
            },
            || {},
        )
        .await
        .map_err(|e| reason(&e))
}

#[cfg(windows)]
fn remember_open_windows<R: Runtime>(app: &AppHandle<R>) {
    let open = app
        .get_webview_window(crate::settings::LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let _ = app.state::<Prefs>().set(&json!({ REOPEN_SETTINGS: open }));
}

/// At launch: Settings comes back if an update closed the app while it was open.
pub fn restore_windows<R: Runtime>(app: &AppHandle<R>) {
    let prefs = app.state::<Prefs>();
    if prefs.get().get(REOPEN_SETTINGS).and_then(|v| v.as_bool()) != Some(true) {
        return;
    }
    let _ = prefs.set(&json!({ REOPEN_SETTINGS: false }));
    crate::settings::open(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_default_to_github() {
        let e = endpoints_for(None);
        assert_eq!(e.feed, GITHUB_FEED);
        assert_eq!(e.downloads, GITHUB_DOWNLOADS);
        assert_eq!(endpoints_for(Some("  ")), e);
    }

    #[test]
    fn a_test_server_serves_both() {
        let e = endpoints_for(Some("http://10.0.2.2:8765/"));
        assert_eq!(e.feed, "http://10.0.2.2:8765/releases.json");
        assert_eq!(e.downloads, "http://10.0.2.2:8765/download");
    }

    #[test]
    fn only_windows_tags_name_a_manifest() {
        assert_eq!(
            manifest_url(GITHUB_DOWNLOADS, "windows-v0.4.0").unwrap(),
            "https://github.com/MF-DEEV/Openbar/releases/download/windows-v0.4.0/latest.json"
        );
        assert!(manifest_url(GITHUB_DOWNLOADS, "windows-v0.4.0-beta.1").is_ok());
        for bad in [
            "v3.6.0",
            "linux-v0.3.0",
            "windows-v",
            "windows-v../../x",
            "windows-v1.0/../a",
            "windows-v1 0",
        ] {
            assert!(manifest_url(GITHUB_DOWNLOADS, bad).is_err(), "{bad}");
        }
    }
}
