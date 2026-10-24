// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! New notifications in the island (spec 04-media-info §2.5). Windows hands
//! notifications only to an app with a package identity (the sparse package)
//! that the person allowed. The reader polls once a second, diffs by id,
//! and only ever shows banners that arrive after it started: what was
//! already in Notification Center is never imported. The inbox lives in
//! memory, newest first, at most 50.

// The reader itself is Windows only; on a Mac only the commands answer.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::Serialize;

const INBOX_LIMIT: usize = 50;

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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u32,
    pub app: String,
    pub aumid: String,
    /// Epoch seconds.
    pub time: f64,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inbox {
    pub access: Access,
    pub items: Vec<Item>,
}

pub struct State {
    pub access: Access,
    pub items: Vec<Item>,
    seen: HashSet<u32>,
    primed: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            access: Access::Unavailable,
            items: Vec::new(),
            seen: HashSet::new(),
            primed: false,
        }
    }
}

impl State {
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
        self.items.truncate(INBOX_LIMIT);
    }

    pub fn inbox(&self) -> Inbox {
        Inbox {
            access: self.access,
            items: self.items.clone(),
        }
    }
}

pub type Shared = Arc<Mutex<State>>;

#[cfg(windows)]
mod imp {
    use std::thread;
    use std::time::Duration;

    use base64::Engine;
    use tauri::{AppHandle, Emitter, Runtime};
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

    use super::{Access, Item, Shared};

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

    /// Shows Windows' own consent prompt; call from the UI thread.
    pub fn request_access() -> Access {
        if !has_identity() {
            return Access::Unavailable;
        }
        match UserNotificationListener::Current()
            .and_then(|l| l.RequestAccessAsync())
            .and_then(|op| op.join())
        {
            Ok(UserNotificationListenerAccessStatus::Allowed) => Access::Allowed,
            Ok(UserNotificationListenerAccessStatus::Denied) => Access::Denied,
            _ => access(),
        }
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
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
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
        let texts: Vec<String> = binding
            .GetTextElements()
            .ok()?
            .into_iter()
            .filter_map(|t| t.Text().ok().map(|s| s.to_string()))
            .filter(|s| !s.trim().is_empty())
            .collect();
        let (title, body) = match texts.split_first() {
            Some((first, rest)) => (first.clone(), rest.join("\n")),
            None => (app.clone(), String::new()),
        };
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
                        shared.lock().unwrap().access = access;
                        let _ = app.emit("notifications://inbox", shared.lock().unwrap().inbox());
                    }
                    if access == Access::Allowed
                        && let Ok(list) = UserNotificationListener::Current()
                            .and_then(|l| l.GetNotificationsAsync(NotificationKinds::Toast))
                            .and_then(|op| op.join())
                    {
                        let notes: Vec<UserNotification> = list.into_iter().collect();
                        let ids: Vec<u32> = notes.iter().filter_map(|n| n.Id().ok()).collect();
                        let fresh = shared.lock().unwrap().diff(&ids);
                        for n in notes
                            .iter()
                            .filter(|n| n.Id().is_ok_and(|id| fresh.contains(&id)))
                        {
                            if let Some(item) = item(n) {
                                shared.lock().unwrap().add(item.clone());
                                let _ = app.emit("notifications://new", &item);
                            }
                        }
                        if !fresh.is_empty() {
                            let _ =
                                app.emit("notifications://inbox", shared.lock().unwrap().inbox());
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

#[cfg(not(windows))]
pub fn start<R: tauri::Runtime>(_app: tauri::AppHandle<R>, _shared: Shared) {}

#[tauri::command]
pub fn notifications_inbox(state: tauri::State<'_, Shared>) -> Inbox {
    state.lock().unwrap().inbox()
}

/// Asks Windows for the listener; a sync command runs on the UI thread, where the prompt belongs.
#[tauri::command]
pub fn notifications_request_access(state: tauri::State<'_, Shared>) -> Access {
    #[cfg(windows)]
    let access = imp::request_access();
    #[cfg(not(windows))]
    let access = Access::Unavailable;
    state.lock().unwrap().access = access;
    access
}

/// Removes an item from the island's inbox only; Notification Center keeps it.
#[tauri::command]
pub fn notifications_dismiss(state: tauri::State<'_, Shared>, id: u32) -> Inbox {
    let mut s = state.lock().unwrap();
    s.items.retain(|i| i.id != id);
    s.inbox()
}

#[tauri::command]
pub fn notifications_clear(state: tauri::State<'_, Shared>) -> Inbox {
    let mut s = state.lock().unwrap();
    s.items.clear();
    s.inbox()
}

/// Opens the app a notification came from (the listener cannot run the banner's own action).
#[tauri::command]
pub fn notifications_open(aumid: String) -> bool {
    crate::media::open_player(&aumid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32) -> Item {
        Item {
            id,
            app: "App".into(),
            aumid: "a".into(),
            time: 0.0,
            title: "t".into(),
            body: String::new(),
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
    fn the_inbox_keeps_fifty_newest_first() {
        let mut s = State::default();
        for id in 0..60 {
            s.add(item(id));
        }
        assert_eq!(s.items.len(), 50);
        assert_eq!(s.items[0].id, 59);
        s.add(item(30));
        assert_eq!(s.items[0].id, 30);
        assert_eq!(s.items.len(), 50);
    }
}
