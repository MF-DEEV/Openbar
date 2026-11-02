// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Events created in Vorssaint, independent of Windows' optional appointment store.
//! A failed read never becomes an empty file on the next save.
use crate::calendar::{Event, Shared, Snapshot};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::Manager;

pub const ID: &str = "vorssaint-local";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: Option<String>,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
    pub description: String,
    pub color: String,
}

impl Draft {
    fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.chars().count() > 200 {
            return Err("Enter an event title (up to 200 characters).".into());
        }
        if self.start < 0 || self.end <= self.start || self.end > 7_258_118_400_000 {
            return Err("Choose an end after the start, between 1970 and 2200.".into());
        }
        if self.location.chars().count() > 500 || self.description.chars().count() > 10_000 {
            return Err("The location or description is too long.".into());
        }
        if !matches!(
            self.color.as_str(),
            "#ff5c63" | "#ffb45b" | "#78cf98" | "#59a6ff" | "#b896ef"
        ) {
            return Err("Choose an event colour.".into());
        }
        Ok(())
    }

    fn event(self, id: String) -> Event {
        Event {
            id: id.clone(),
            local_id: id.clone(),
            countdown_key: id,
            title: self.title.trim().into(),
            calendar: "Vorssaint".into(),
            calendar_id: ID.into(),
            start: self.start,
            end: self.end,
            all_day: self.all_day,
            location: self.location.trim().into(),
            description: self.description.trim().into(),
            color: self.color,
            instance_start: None,
        }
    }
}

pub struct Store {
    path: Option<PathBuf>,
    gate: Mutex<()>,
}

impl Store {
    pub fn new(app: &tauri::AppHandle) -> Self {
        Self {
            path: app
                .path()
                .app_config_dir()
                .ok()
                .map(|p| p.join("calendar").join("events.json")),
            gate: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<Vec<Event>, String> {
        let path = self
            .path
            .as_ref()
            .ok_or("The calendar data folder is unavailable.")?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| format!("Cannot read saved calendar: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("Cannot read saved calendar: {e}")),
        }
    }

    pub fn load(&self) -> Result<Vec<Event>, String> {
        let _guard = self.gate.lock().unwrap();
        self.read()
    }

    fn write(&self, events: &[Event]) -> Result<(), String> {
        let path = self
            .path
            .as_ref()
            .ok_or("The calendar data folder is unavailable.")?;
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(path.parent().unwrap())?;
            let tmp = path.with_extension("json.tmp");
            let mut file = fs::File::create(&tmp)?;
            use std::io::Write;
            file.write_all(&serde_json::to_vec(events)?)?;
            file.sync_all()?;
            fs::rename(tmp, path)
        })();
        result.map_err(|e| format!("Cannot save calendar: {e}"))
    }

    fn save(&self, draft: Draft) -> Result<Event, String> {
        draft.validate()?;
        let _guard = self.gate.lock().unwrap();
        let mut events = self.read()?;
        let index = match draft.id.as_ref() {
            Some(id) => Some(
                events
                    .iter()
                    .position(|e| &e.id == id)
                    .ok_or("This event no longer exists.")?,
            ),
            None => None,
        };
        let id = match draft.id.clone() {
            Some(id) => id,
            None => {
                if events.len() >= 10_000 {
                    return Err("The local calendar is full.".into());
                }
                let mut bytes = [0u8; 16];
                getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
                format!(
                    "local-{}",
                    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                )
            }
        };
        let event = draft.event(id);
        if let Some(index) = index {
            events[index] = event.clone();
        } else {
            events.push(event.clone());
        }
        self.write(&events)?;
        Ok(event)
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        let _guard = self.gate.lock().unwrap();
        let mut events = self.read()?;
        let index = events
            .iter()
            .position(|e| e.id == id)
            .ok_or("This event no longer exists.")?;
        events.remove(index);
        self.write(&events)
    }
}

#[tauri::command]
pub async fn calendar_save_event(app: tauri::AppHandle, draft: Draft) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Store>().save(draft)?;
        Ok(app.state::<Shared>().refresh())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn calendar_delete_event(app: tauri::AppHandle, id: String) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Store>().delete(&id)?;
        Ok(app.state::<Shared>().refresh())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> Draft {
        Draft {
            id: None,
            title: " Meeting ".into(),
            start: 1_800_000_000_000,
            end: 1_800_003_600_000,
            all_day: false,
            location: " Office ".into(),
            description: " Agenda ".into(),
            color: "#ff5c63".into(),
        }
    }
    fn store(name: &str) -> Store {
        Store {
            path: Some(
                std::env::temp_dir()
                    .join(format!("vors-calendar-{}-{name}", std::process::id()))
                    .join("events.json"),
            ),
            gate: Mutex::new(()),
        }
    }
    #[test]
    fn create_update_reload_delete() {
        let store = store("crud");
        if let Some(path) = &store.path {
            let _ = fs::remove_file(path);
        }
        let event = store.save(draft()).unwrap();
        assert_eq!(event.title, "Meeting");
        let mut edit = draft();
        edit.id = Some(event.id.clone());
        edit.description = "Changed".into();
        store.save(edit).unwrap();
        // Reopen through a fresh store, as after completely exiting the app.
        let path = store.path.clone().unwrap();
        drop(store);
        let reopened = Store {
            path: Some(path.clone()),
            gate: Mutex::new(()),
        };
        assert_eq!(reopened.load().unwrap().len(), 1);
        assert_eq!(reopened.load().unwrap()[0].description, "Changed");
        reopened.delete(&event.id).unwrap();
        assert!(reopened.load().unwrap().is_empty());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn rejects_invalid_ranges_and_preserves_unreadable_data() {
        let mut invalid = draft();
        invalid.end = invalid.start;
        assert!(invalid.validate().is_err());
        invalid = draft();
        invalid.title = "  ".into();
        assert!(invalid.validate().is_err());
        let store = store("corrupt");
        let path = store.path.as_ref().unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"broken").unwrap();
        assert!(store.save(draft()).is_err());
        assert!(store.delete("missing").is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
