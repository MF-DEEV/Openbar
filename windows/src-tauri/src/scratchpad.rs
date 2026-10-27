// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The scratchpad's pads on disk (ScratchpadStore / ScratchpadDocument,
//! ScratchpadSupport.swift:57-160; spec 05 §11): one small JSON document in
//! `%APPDATA%\com.vorssaint.island\scratchpad\Scratchpad.json`, written
//! atomically. The page keeps the model; this reads it back sanitized
//! (at most 12 pads, unique ids, names of 40 characters at most, a valid
//! selection) and never overwrites a file it couldn't read.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

pub const MAX_PADS: usize = 12;
pub const MAX_NAME: usize = 40;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pad {
    pub id: String,
    pub name: String,
    pub text: String,
    /// Milliseconds since 1970, the last edit; none while empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub pads: Vec<Pad>,
    #[serde(alias = "selectedID")]
    pub selected_id: String,
}

/// Words joined by single spaces, at most 40 characters.
pub fn sanitized_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_NAME).collect()
}

impl Document {
    /// None when nothing usable is left (the page then starts a first pad).
    pub fn sanitized(self) -> Option<Document> {
        let mut seen = std::collections::HashSet::new();
        let mut pads: Vec<Pad> = Vec::new();
        for pad in self.pads {
            if pads.len() == MAX_PADS || pad.id.is_empty() || !seen.insert(pad.id.clone()) {
                continue;
            }
            let name = sanitized_name(&pad.name);
            let name = if name.is_empty() { format!("Scratchpad {}", pads.len() + 1) } else { name };
            let modified_at = if pad.text.is_empty() { None } else { pad.modified_at.filter(|t| t.is_finite()) };
            pads.push(Pad { name, modified_at, ..pad });
        }
        let first = pads.first()?.id.clone();
        let selected_id = if pads.iter().any(|p| p.id == self.selected_id) { self.selected_id } else { first };
        Some(Document { pads, selected_id })
    }
}

pub struct Store {
    path: Option<PathBuf>,
    /// The file is there but unreadable: saving would destroy it.
    blocked: Mutex<bool>,
}

impl Store {
    pub fn new<R: Runtime>(app: &AppHandle<R>) -> Self {
        let path = app.path().app_config_dir().ok().map(|d| d.join("scratchpad").join("Scratchpad.json"));
        Store { path, blocked: Mutex::new(false) }
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Store { path: Some(path), blocked: Mutex::new(false) }
    }

    /// Ok(None) for no file yet; Err for one that can't be read.
    pub fn load(&self) -> Result<Option<Document>, String> {
        let Some(path) = &self.path else { return Ok(None) };
        let result = read(path);
        *self.blocked.lock().unwrap() = result.is_err();
        result
    }

    pub fn save(&self, document: &Document) -> Result<(), String> {
        if *self.blocked.lock().unwrap() {
            return Err("the scratchpad file couldn't be read, so it isn't overwritten".into());
        }
        let Some(path) = &self.path else { return Ok(()) };
        write(path, document).map_err(|e| e.to_string())
    }
}

fn read(path: &Path) -> Result<Option<Document>, String> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let document: Document = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    Ok(document.sanitized())
}

/// A temporary file renamed over the old one: a crash leaves either.
fn write(path: &Path, document: &Document) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(document)?)?;
    fs::rename(&tmp, path)
}

#[tauri::command]
pub fn scratchpad_load(store: tauri::State<'_, Store>) -> Result<Option<Document>, String> {
    store.load()
}

#[tauri::command]
pub fn scratchpad_save(store: tauri::State<'_, Store>, document: Document) -> Result<(), String> {
    store.save(&document)
}

/// "Save as file": a save dialog over the island, then the text as UTF-8.
/// Ok(None) when the person cancels.
#[tauri::command]
pub async fn scratchpad_export<R: Runtime>(app: AppHandle<R>, name: String, text: String) -> Result<Option<String>, String> {
    let owner = app.get_webview_window(crate::island::LABEL).and_then(|w| platform::handle(&w));
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = platform::save_dialog(owner, &name) else { return Ok(None) };
        fs::write(&path, text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
mod platform {
    use std::path::PathBuf;
    use tauri::{Runtime, WebviewWindow};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Controls::Dialogs::{
        GetSaveFileNameW, OFN_EXPLORER, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows::core::{PCWSTR, PWSTR, w};

    pub fn handle<R: Runtime>(window: &WebviewWindow<R>) -> Option<isize> {
        window.hwnd().ok().map(|h| h.0 as isize)
    }

    pub fn save_dialog(owner: Option<isize>, name: &str) -> Option<PathBuf> {
        let mut file = [0u16; 1024];
        for (slot, unit) in file.iter_mut().zip(name.encode_utf16().take(1000)) {
            *slot = unit;
        }
        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: HWND(owner.unwrap_or(0) as _),
            lpstrFilter: w!("Text\0*.txt\0\0"),
            lpstrFile: PWSTR(file.as_mut_ptr()),
            nMaxFile: file.len() as u32,
            lpstrDefExt: w!("txt"),
            Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            lpstrTitle: PCWSTR::null(),
            ..Default::default()
        };
        if !unsafe { GetSaveFileNameW(&mut dialog) }.as_bool() {
            return None;
        }
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::PathBuf;
    use tauri::{Runtime, WebviewWindow};

    pub fn handle<R: Runtime>(_window: &WebviewWindow<R>) -> Option<isize> {
        None
    }

    /// The Mac build is for designing: no dialog, the Desktop.
    pub fn save_dialog(_owner: Option<isize>, name: &str) -> Option<PathBuf> {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop").join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(id: &str, name: &str, text: &str) -> Pad {
        Pad { id: id.into(), name: name.into(), text: text.into(), modified_at: Some(1_700_000_000_000.0) }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vors-scratchpad-{}-{name}", std::process::id())).join("Scratchpad.json")
    }

    #[test]
    fn a_document_survives_a_round_trip() {
        let path = temp("round-trip");
        let store = Store::at(path.clone());
        assert_eq!(store.load(), Ok(None), "no file yet");
        let document = Document {
            pads: vec![pad("a", "Scratchpad 1", "- [ ] milk\nñ ✓"), pad("b", "Ideas", "")],
            selected_id: "b".into(),
        };
        store.save(&document).unwrap();
        let back = store.load().unwrap().unwrap();
        assert_eq!(back.pads[0], document.pads[0]);
        assert_eq!(back.pads[1].modified_at, None, "an empty pad has no edit date");
        assert_eq!(back.selected_id, "b");
        assert!(!path.with_extension("json.tmp").exists(), "the temporary file is renamed away");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn an_unreadable_file_is_never_overwritten() {
        let path = temp("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        let store = Store::at(path.clone());
        assert!(store.load().is_err());
        assert!(store.save(&Document { pads: vec![pad("a", "A", "x")], selected_id: "a".into() }).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn loading_sanitizes_ids_names_count_and_selection() {
        let mut pads: Vec<Pad> = (0..14).map(|i| pad(&format!("p{i}"), "  many   spaces  ", "")).collect();
        pads.insert(1, pad("p0", "duplicate", "x"));
        pads[2].name = "n".repeat(60);
        let document = Document { pads, selected_id: "gone".into() }.sanitized().unwrap();
        assert_eq!(document.pads.len(), MAX_PADS);
        assert_eq!(document.pads[0].name, "many spaces");
        assert_eq!(document.pads[1].id, "p1", "the duplicate id dropped");
        assert_eq!(document.pads[1].name.chars().count(), MAX_NAME);
        assert_eq!(document.selected_id, "p0");
        assert_eq!(Document { pads: vec![], selected_id: String::new() }.sanitized(), None);
    }

    #[test]
    fn the_mac_spelling_of_the_selection_reads_too() {
        let json = r#"{"pads":[{"id":"a","name":"","text":"hi"}],"selectedID":"a"}"#;
        let document: Document = serde_json::from_str(json).unwrap();
        let document = document.sanitized().unwrap();
        assert_eq!(document.pads[0].name, "Scratchpad 1");
        assert_eq!(document.selected_id, "a");
    }
}
