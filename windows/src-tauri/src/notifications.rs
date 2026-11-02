// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Windows notification history and new-message banners. Access requires the
//! sparse package and the user's Windows consent. Existing Windows toasts are
//! imported silently; new arrivals may show a banner. The latest 200 entries
//! are kept on disk, including source icons, sender/title, message and time.

// The reader itself is Windows only; on a Mac only the commands answer.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub(crate) const INBOX_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// No package identity: the sparse package is not registered.
    Unavailable,
    /// Windows has not asked the person yet.
    NotAsked,
    Denied,
    Allowed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u32,
    pub app: String,
    pub aumid: String,
    /// Epoch seconds.
    pub time: f64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inbox {
    pub access: Access,
    pub items: Vec<Item>,
    pub error: Option<String>,
}

pub struct State {
    pub access: Access,
    pub items: Vec<Item>,
    seen: HashSet<u32>,
    primed: bool,
    enabled: bool,
    locked: bool,
    history: Option<crate::notification_history::History>,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            access: Access::Unavailable,
            items: Vec::new(),
            seen: HashSet::new(),
            primed: false,
            enabled: true,
            locked: false,
            history: None,
            error: None,
        }
    }
}

impl State {
    fn clear_private(&mut self) {
        self.primed = false;
    }

    fn save(&mut self) {
        if let Some(history) = &self.history {
            self.error = history.save(&self.items, &self.seen).err();
        }
    }

    /// Muted arrivals are consumed, so leaving quiet mode never replays them.
    fn poll(&mut self, present: &[u32], may_interrupt: bool) -> Vec<u32> {
        let fresh = self.diff(present);
        if self.enabled && may_interrupt {
            fresh
        } else {
            Vec::new()
        }
    }

    /// Takes the ids now in Notification Center; returns the ones that are new.
    /// The first call only remembers what is there.
    pub fn diff(&mut self, present: &[u32]) -> Vec<u32> {
        let fresh: Vec<u32> = present
            .iter()
            .copied()
            .filter(|id| !self.seen.contains(id))
            .collect();
        self.seen.extend(present.iter().copied());
        if !self.primed {
            self.primed = true;
            return Vec::new();
        }
        fresh
    }

    pub fn add(&mut self, item: Item) {
        self.items.retain(|i| i.id != item.id);
        self.items.insert(0, item);
        self.items
            .sort_by(|a, b| b.time.total_cmp(&a.time).then_with(|| b.id.cmp(&a.id)));
        self.items.truncate(INBOX_LIMIT);
    }

    pub fn inbox(&self) -> Inbox {
        Inbox {
            access: self.access,
            items: if self.locked || !self.enabled {
                Vec::new()
            } else {
                self.items.clone()
            },
            error: self.error.clone(),
        }
    }
}

pub type Shared = Arc<Mutex<State>>;

/// Stop collecting messages when the island or its Notifications section is off.
pub fn set_enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, enabled: bool) {
    use tauri::{Emitter, Manager};
    let Some(shared) = app.try_state::<Shared>() else {
        return;
    };
    let mut state = shared.lock().unwrap();
    if state.enabled == enabled {
        return;
    }
    state.enabled = enabled;
    state.clear_private();
    let _ = app.emit("notifications://inbox", state.inbox());
}

#[cfg(windows)]
mod imp {
    use std::thread;
    use std::time::Duration;

    use base64::Engine;
    use tauri::{AppHandle, Emitter, Manager, Runtime};
    use windows::ApplicationModel::Package;
    use windows::Foundation::Size;
    use windows::Storage::Streams::DataReader;
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::{
        KnownNotificationBindings, NotificationKinds, UserNotification,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::UI::Shell::{
        QUERY_USER_NOTIFICATION_STATE, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY,
        QUNS_NOT_PRESENT, SHQueryUserNotificationState,
    };

    use super::{Access, Item, Shared};

    /// Generic fullscreen/busy can include the island's own overlay. Its
    /// fullscreen preference controls visibility; explicit quiet/presentation
    /// and inactive-session states still suppress interrupting banners.
    pub(super) fn may_interrupt(state: Option<QUERY_USER_NOTIFICATION_STATE>) -> bool {
        matches!(
            state,
            None | Some(QUNS_ACCEPTS_NOTIFICATIONS | QUNS_BUSY | QUNS_APP)
        )
    }

    pub fn has_identity() -> bool {
        Package::Current().is_ok()
    }

    pub fn access() -> Access {
        if !has_identity() {
            return Access::Unavailable;
        }
        match UserNotificationListener::Current().and_then(|l| l.GetAccessStatus()) {
            Ok(UserNotificationListenerAccessStatus::Allowed) => Access::Allowed,
            Ok(UserNotificationListenerAccessStatus::Denied) => Access::Denied,
            Ok(_) => Access::NotAsked,
            Err(_) => Access::Unavailable,
        }
    }

    /// Start consent on the UI thread; wait elsewhere so Windows can show its dialog.
    pub fn request_access(reply: std::sync::mpsc::Sender<Access>) {
        if !has_identity() {
            let _ = reply.send(Access::Unavailable);
            return;
        }
        match UserNotificationListener::Current().and_then(|l| l.RequestAccessAsync()) {
            Ok(operation) => {
                std::thread::spawn(move || {
                    let _apartment = crate::com::Apartment::mta();
                    let result = match operation.join() {
                        Ok(UserNotificationListenerAccessStatus::Allowed) => Access::Allowed,
                        Ok(UserNotificationListenerAccessStatus::Denied) => Access::Denied,
                        _ => access(),
                    };
                    let _ = reply.send(result);
                });
            }
            Err(_) => {
                let _ = reply.send(access());
            }
        }
    }

    pub fn probe() -> serde_json::Value {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let identity = Package::Current()
            .ok()
            .and_then(|p| p.Id().ok())
            .and_then(|id| id.Name().ok())
            .map(|name| name.to_string());
        let status = access();
        let (count, error) = if status == Access::Allowed {
            match UserNotificationListener::Current()
                .and_then(|l| l.GetNotificationsAsync(NotificationKinds::Toast))
                .and_then(|op| op.join())
            {
                Ok(list) => (list.Size().ok(), None),
                Err(error) => (None, Some(format!("{}", error))),
            }
        } else {
            (None, None)
        };
        serde_json::json!({ "identity": identity, "access": status,
            "notificationCount": count, "error": error })
    }

    fn icon(n: &UserNotification) -> Option<String> {
        let logo = n
            .AppInfo()
            .ok()?
            .DisplayInfo()
            .ok()?
            .GetLogo(Size {
                Width: 48.0,
                Height: 48.0,
            })
            .ok()?;
        let stream = logo.OpenReadAsync().ok()?.join().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > 512 * 1024 {
            return None;
        }
        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size).ok()?.join().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        let image = image::load_from_memory(&bytes).ok()?.thumbnail(48, 48);
        let mut normalized = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut normalized, image::ImageFormat::Png)
            .ok()?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(normalized.into_inner())
        ))
    }

    fn item(n: &UserNotification) -> Option<Item> {
        let id = n.Id().ok()?;
        let info = n.AppInfo().ok();
        let app = info
            .as_ref()
            .and_then(|i| i.DisplayInfo().ok())
            .and_then(|d| d.DisplayName().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let aumid = info
            .as_ref()
            .and_then(|i| i.AppUserModelId().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let time = n
            .CreationTime()
            .map(|t| t.UniversalTime as f64 / 1e7 - 11_644_473_600.0)
            .unwrap_or(0.0);
        let binding = n
            .Notification()
            .ok()?
            .Visual()
            .ok()?
            .GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?)
            .ok()?;
        let mut text = binding
            .GetTextElements()
            .ok()?
            .into_iter()
            .filter_map(|t| t.Text().ok().map(|s| s.to_string()));
        let title = text
            .next()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| app.clone())
            .chars()
            .take(512)
            .collect();
        let body = text
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .take(4096)
            .collect();
        Some(Item {
            id,
            app,
            aumid,
            time,
            title,
            body,
            icon: icon(n),
        })
    }

    pub fn start<R: Runtime>(app: AppHandle<R>, shared: Shared) {
        {
            let mut state = shared.lock().unwrap();
            let prefs = app.state::<crate::prefs::Prefs>().get();
            let flag = |path| {
                prefs
                    .pointer(path)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            };
            state.enabled = flag("/island/enabled")
                && flag("/island/sections/notifications")
                && !prefs
                    .pointer("/island/hiddenModules")
                    .and_then(|v| v.as_array())
                    .is_some_and(|v| v.iter().any(|id| id.as_str() == Some("notifications")));
            match app.path().app_config_dir() {
                Ok(path) => {
                    let (history, loaded) = crate::notification_history::History::open(
                        path.join("notifications").join("history.json"),
                    );
                    match loaded {
                        Ok(document) => {
                            state.items = document.items;
                            state.seen = document.seen;
                        }
                        Err(error) => state.error = Some(error),
                    }
                    state.history = Some(history);
                }
                Err(_) => {
                    state.error = Some("The notification history folder is unavailable.".into())
                }
            }
        }
        thread::Builder::new()
            .name("notifications".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                let mut last_access = None;
                loop {
                    let access = access();
                    if last_access != Some(access) {
                        last_access = Some(access);
                        let mut state = shared.lock().unwrap();
                        state.access = access;
                        if access != Access::Allowed {
                            state.clear_private();
                        }
                        let _ = app.emit("notifications://inbox", state.inbox());
                    }
                    let shell = unsafe { SHQueryUserNotificationState() }.ok();
                    let locked = shell == Some(QUNS_NOT_PRESENT);
                    let enabled = {
                        let mut state = shared.lock().unwrap();
                        if state.locked != locked {
                            state.locked = locked;
                            state.clear_private();
                            let _ = app.emit("notifications://inbox", state.inbox());
                        }
                        state.enabled
                    };
                    // Receive only native desktop notifications: Discord's own
                    // DND and channel mutes suppress their creation upstream.
                    // Also avoid interrupting presentations and inactive sessions.
                    if access == Access::Allowed
                        && enabled
                        && !locked
                        && let Ok(list) = UserNotificationListener::Current()
                            .and_then(|l| l.GetNotificationsAsync(NotificationKinds::Toast))
                            .and_then(|op| op.join())
                    {
                        let notes: Vec<UserNotification> = list.into_iter().collect();
                        let ids: Vec<u32> = notes.iter().filter_map(|n| n.Id().ok()).collect();
                        let (collect, fresh, seen_changed) = {
                            let mut state = shared.lock().unwrap();
                            let previous = state.seen.clone();
                            let collect: Vec<u32> = ids
                                .iter()
                                .copied()
                                .filter(|id| !state.seen.contains(id))
                                .collect();
                            let fresh = state.poll(&ids, may_interrupt(shell));
                            let history_ids: std::collections::HashSet<u32> =
                                state.items.iter().map(|item| item.id).collect();
                            state
                                .seen
                                .retain(|id| ids.contains(id) || history_ids.contains(id));
                            let changed = state.seen != previous;
                            (collect, fresh, changed)
                        };
                        for n in notes
                            .iter()
                            .filter(|n| n.Id().is_ok_and(|id| collect.contains(&id)))
                        {
                            if let Some(item) = item(n) {
                                let mut state = shared.lock().unwrap();
                                // Preferences can change while Windows resolves an icon.
                                if state.enabled && !state.locked {
                                    state.add(item.clone());
                                    if fresh.contains(&item.id) {
                                        let _ = app.emit("notifications://new", &item);
                                    }
                                }
                            }
                        }
                        if seen_changed || !collect.is_empty() {
                            let mut state = shared.lock().unwrap();
                            state.save();
                            let _ = app.emit("notifications://inbox", state.inbox());
                        }
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            })
            .expect("notifications thread");
    }
}

#[cfg(windows)]
pub use imp::start;

/// A content-free diagnostic, also usable before the main island is launched.
pub fn probe_main(path: &str) -> ! {
    #[cfg(windows)]
    let report = imp::probe();
    #[cfg(not(windows))]
    let report = serde_json::json!({ "access": "unavailable" });
    let result = std::fs::write(path, report.to_string());
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

#[cfg(not(windows))]
pub fn start<R: tauri::Runtime>(_app: tauri::AppHandle<R>, _shared: Shared) {}

#[tauri::command]
pub fn notifications_inbox(state: tauri::State<'_, Shared>) -> Inbox {
    state.lock().unwrap().inbox()
}

/// Start consent on the UI thread without blocking the app while the user answers.
#[tauri::command]
pub async fn notifications_request_access(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shared>,
) -> Result<Access, String> {
    #[cfg(windows)]
    let access = {
        let (reply, answer) = std::sync::mpsc::channel();
        if app
            .run_on_main_thread(move || imp::request_access(reply))
            .is_err()
        {
            return Ok(Access::Unavailable);
        }
        tauri::async_runtime::spawn_blocking(move || {
            answer
                .recv_timeout(std::time::Duration::from_secs(120))
                .unwrap_or(Access::NotAsked)
        })
        .await
        .unwrap_or(Access::Unavailable)
    };
    #[cfg(not(windows))]
    let access = Access::Unavailable;
    state.lock().unwrap().access = access;
    use tauri::Emitter;
    let _ = app.emit("notifications://inbox", state.lock().unwrap().inbox());
    Ok(access)
}

/// Removes an item from the island's inbox only; Notification Center keeps it.
#[tauri::command]
pub fn notifications_dismiss(
    app: tauri::AppHandle,
    state: tauri::State<'_, Shared>,
    id: u32,
) -> Inbox {
    use tauri::Emitter;
    let mut s = state.lock().unwrap();
    s.items.retain(|i| i.id != id);
    s.save();
    let inbox = s.inbox();
    let _ = app.emit("notifications://inbox", &inbox);
    inbox
}

#[tauri::command]
pub fn notifications_clear(app: tauri::AppHandle, state: tauri::State<'_, Shared>) -> Inbox {
    use tauri::Emitter;
    let mut s = state.lock().unwrap();
    s.items.clear();
    s.save();
    let inbox = s.inbox();
    let _ = app.emit("notifications://inbox", &inbox);
    inbox
}

/// Opens the app a notification came from (the listener cannot run the banner's own action).
#[tauri::command]
pub fn notifications_open(aumid: String) -> bool {
    crate::media::open_player(&aumid)
}

#[tauri::command]
pub fn notifications_open_privacy_settings() -> bool {
    tauri_plugin_opener::open_url("ms-settings:privacy-notifications", None::<&str>).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn ordinary_busy_desktop_does_not_hide_banners_but_explicit_quiet_modes_do() {
        use windows::Win32::UI::Shell::*;
        assert!(imp::may_interrupt(Some(QUNS_BUSY)));
        assert!(imp::may_interrupt(Some(QUNS_APP)));
        assert!(imp::may_interrupt(Some(QUNS_ACCEPTS_NOTIFICATIONS)));
        for state in [
            QUNS_NOT_PRESENT,
            QUNS_PRESENTATION_MODE,
            QUNS_QUIET_TIME,
            QUNS_RUNNING_D3D_FULL_SCREEN,
        ] {
            assert!(!imp::may_interrupt(Some(state)));
        }
    }

    fn item(id: u32) -> Item {
        Item {
            id,
            app: "App".into(),
            aumid: "a".into(),
            time: 0.0,
            title: "t".into(),
            body: "Message".into(),
            icon: None,
        }
    }

    #[test]
    fn the_first_scan_only_primes() {
        let mut s = State::default();
        assert!(s.diff(&[1, 2]).is_empty());
        assert_eq!(s.diff(&[1, 2, 3]), vec![3]);
        assert!(s.diff(&[2, 3]).is_empty());
    }

    #[test]
    fn the_history_keeps_two_hundred_newest_first() {
        let mut s = State::default();
        for id in 0..210 {
            s.add(item(id));
        }
        assert_eq!(s.items.len(), 200);
        assert_eq!(s.items[0].id, 209);
        let mut updated = item(30);
        updated.time = 100.0;
        s.add(updated);
        assert_eq!(s.items[0].id, 30);
        assert_eq!(s.items.len(), 200);
    }

    #[test]
    fn quiet_arrivals_are_not_replayed_after_quiet_mode() {
        let mut s = State::default();
        assert!(s.poll(&[1], true).is_empty());
        assert!(s.poll(&[1, 2], false).is_empty());
        assert!(s.poll(&[1, 2], true).is_empty());
        assert_eq!(s.poll(&[1, 2, 3], true), vec![3]);
    }

    #[test]
    fn disabled_notifications_cannot_deliver() {
        let mut s = State::default();
        s.poll(&[], true);
        s.enabled = false;
        assert!(s.poll(&[1], true).is_empty());
        s.enabled = true;
        assert!(s.poll(&[1], true).is_empty());
        assert_eq!(s.poll(&[1, 2], true), vec![2]);
    }

    #[test]
    fn locking_hides_history_without_erasing_it_and_primes_again() {
        let mut s = State::default();
        s.poll(&[], true);
        s.add(item(1));
        s.locked = true;
        s.clear_private();
        assert!(s.inbox().items.is_empty());
        assert_eq!(s.items.len(), 1);
        s.locked = false;
        assert_eq!(s.inbox().items.len(), 1);
        assert!(s.poll(&[1, 2], true).is_empty());
        assert_eq!(s.poll(&[1, 2, 3], true), vec![3]);
    }
}
