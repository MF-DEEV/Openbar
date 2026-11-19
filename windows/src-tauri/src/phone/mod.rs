// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Phone approvals through a Telegram bot (#214): Settings › Sessions ›
//! Phone approvals. Windows only for now; elsewhere the section says it is
//! not available and nothing here runs.

pub mod answers;
// Runs only on Windows; elsewhere it is built for its tests.
#[cfg_attr(not(windows), allow(dead_code))]
pub mod bridge;
#[cfg_attr(not(windows), allow(dead_code))]
pub mod messages;
#[cfg(windows)]
mod secret;
#[cfg(windows)]
mod telegram;

use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tauri::{AppHandle, Wry};

use crate::sessions::models::Session;
use bridge::{Bridge, Status};
use messages::Settings;

static BRIDGE: OnceLock<Arc<Bridge>> = OnceLock::new();
/// The choices as the prefs last said, also before the bridge starts.
static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

/// The sessions as the island now shows them.
pub fn publish(sessions: &[Session]) {
    if let Some(bridge) = BRIDGE.get() {
        bridge.publish(sessions);
    }
}

/// Prefs changed (`/sessions/phone*`).
pub fn apply(value: &Value) {
    let settings = Settings::from_prefs(value);
    *SETTINGS.lock().unwrap() = Some(settings.clone());
    if let Some(bridge) = BRIDGE.get() {
        bridge.set_settings(settings);
    }
}

#[cfg(windows)]
mod host {
    use std::path::PathBuf;

    use tauri::{AppHandle, Emitter, Manager, Wry};

    use super::bridge::{Host, Status, Stored};
    use crate::sessions::hook_message::Decision;

    pub struct AppHost {
        pub app: AppHandle<Wry>,
        pub identifier: String,
        pub store: Option<PathBuf>,
    }

    impl Host for AppHost {
        fn decide(&self, session: &str, request: &str, decision: Decision, revision: Option<&str>) -> bool {
            self.app
                .try_state::<crate::sessions::service::Shared>()
                .is_some_and(|s| s.phone_decide(session, request, decision, revision))
        }
        fn reply(&self, session: &str, text: &str) -> Option<&'static str> {
            match self.app.try_state::<crate::sessions::service::Shared>() {
                Some(s) => s.phone_reply(session, text),
                None => Some("unreachable"),
            }
        }
        fn auto_allow(&self, session: &str) {
            let _ = self.app.emit("phone://auto-allow", session);
        }
        fn locked(&self) -> bool {
            crate::session_lock::screen_away()
        }
        fn idle(&self) -> f64 {
            crate::session_lock::session_idle_seconds().unwrap_or(0.0)
        }
        fn minute(&self) -> u32 {
            use chrono::Timelike;
            let now = chrono::Local::now();
            now.hour() * 60 + now.minute()
        }
        fn now(&self) -> f64 {
            crate::sessions::service::now()
        }
        fn computer(&self) -> String {
            std::env::var("COMPUTERNAME").ok().filter(|n| !n.is_empty()).unwrap_or_else(|| "this PC".into())
        }
        fn changed(&self, status: &Status) {
            let _ = self.app.emit("phone://status", status);
        }
        fn save(&self, stored: &Stored) {
            let Some(path) = &self.store else { return };
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(text) = serde_json::to_string_pretty(stored) {
                let _ = std::fs::write(path, text);
            }
        }
        fn save_token(&self, token: Option<&str>) -> bool {
            match token {
                Some(token) => super::secret::write(&self.identifier, token),
                None => super::secret::delete(&self.identifier),
            }
        }
        fn qr(&self, text: &str) -> Option<String> {
            let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
            Some(
                code.render::<qrcode::render::svg::Color>()
                    .min_dimensions(200, 200)
                    .dark_color(qrcode::render::svg::Color("#000000"))
                    .light_color(qrcode::render::svg::Color("#ffffff"))
                    .build(),
            )
        }
    }
}

/// Starts with the app, once the sessions and the prefs are there.
pub fn start(app: AppHandle<Wry>) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let identifier = app.config().identifier.clone();
        let store = app.path().app_data_dir().ok().map(|d| d.join("phone.json"));
        let stored: bridge::Stored = store
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let token = secret::read(&identifier);
        let settings = SETTINGS.lock().unwrap().clone().unwrap_or_default();
        let host = Arc::new(host::AppHost { app, identifier, store });
        let bridge = Bridge::new(host, Arc::new(telegram::Telegram), stored, token, settings);
        if BRIDGE.set(bridge.clone()).is_ok() {
            bridge.start();
        }
    }
    #[cfg(not(windows))]
    let _ = app;
}

/// Runs a step that may make or drop the HTTP client off the async runtime,
/// where the blocking client must not be dropped.
async fn blocking(step: impl FnOnce(&Bridge) -> Status + Send + 'static) -> Status {
    let Some(bridge) = BRIDGE.get().cloned() else { return Status::default() };
    tauri::async_runtime::spawn_blocking(move || step(&bridge)).await.unwrap_or_default()
}

#[tauri::command]
pub fn phone_status() -> Status {
    BRIDGE.get().map(|b| b.status()).unwrap_or_default()
}

#[tauri::command]
pub async fn phone_use_token(token: String) -> Status {
    blocking(move |b| b.use_token(&token)).await
}

#[tauri::command]
pub async fn phone_start_link() -> Status {
    blocking(|b| b.start_linking()).await
}

#[tauri::command]
pub async fn phone_cancel_link() -> Status {
    blocking(|b| b.cancel_linking()).await
}

#[tauri::command]
pub async fn phone_unlink() -> Status {
    blocking(|b| b.unlink()).await
}

#[tauri::command]
pub fn phone_test() {
    if let Some(bridge) = BRIDGE.get() {
        bridge.send_test();
    }
}
