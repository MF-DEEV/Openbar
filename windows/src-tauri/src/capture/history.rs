// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Private, bounded copies of captures. Never delete the user's saved PNGs.
//! Filenames encode the date and a random ID, so history survives restarts
//! without a second index that could become inconsistent with the files.

use super::snapshot::Frame;
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const CHANGED: &str = "captures://changed";
const LIMIT: usize = 30;
const MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Default)]
pub struct Shared(Mutex<()>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    id: String,
    created_at: u64,
    thumbnail: Option<String>,
}

fn valid_id(id: &str) -> bool {
    let Some((date, random)) = id.split_once('-') else {
        return false;
    };
    !date.is_empty()
        && date.bytes().all(|b| b.is_ascii_digit())
        && date
            .parse::<u64>()
            .is_ok_and(|n| n <= 8_640_000_000_000_000)
        && random.len() == 32
        && random.bytes().all(|b| b.is_ascii_hexdigit())
}

fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !linked(&m))
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // includes junctions/reparse points
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("recent-captures");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let metadata = fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
    if linked(&metadata) || !metadata.is_dir() {
        return Err("invalid capture history directory".into());
    }
    Ok(root)
}

fn path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("invalid capture ID".into());
    }
    Ok(root.join(format!("{id}.png")))
}

fn ids(root: &Path) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if let Some(id) = name.to_str().and_then(|s| s.strip_suffix(".png"))
            && valid_id(id)
            && regular(&entry.path())
        {
            ids.push(id.to_string());
        }
    }
    ids.sort_by_key(|id| std::cmp::Reverse(date(id)));
    Ok(ids)
}

fn date(id: &str) -> u64 {
    id.split('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn list(root: &Path) -> Result<Vec<Entry>, String> {
    Ok(ids(root)?
        .into_iter()
        .take(LIMIT)
        .map(|id| {
            let thumb = root.join(format!("{id}.thumb"));
            let thumbnail = if regular(&thumb) {
                fs::File::open(&thumb)
                    .and_then(|file| {
                        let mut bytes = Vec::new();
                        file.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
                        Ok(bytes)
                    })
                    .ok()
                    .filter(|b| b.len() <= 256 * 1024)
                    .map(|b| crate::shell::png_data_url(&b))
            } else {
                None
            };
            Entry {
                created_at: date(&id),
                id,
                thumbnail,
            }
        })
        .collect())
}

fn remove(root: &Path, id: &str) -> Result<(), String> {
    let full = path(root, id)?;
    // Only known private files; no recursion and no paths from the frontend.
    for file in [root.join(format!("{id}.thumb")), full] {
        match fs::symlink_metadata(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
            Ok(m) if !m.is_file() || linked(&m) => return Err("invalid capture file".into()),
            Ok(_) => fs::remove_file(&file).map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

fn trim(root: &Path) -> Result<(), String> {
    let mut bytes = 0u64;
    for (index, id) in ids(root)?.iter().enumerate() {
        bytes = bytes.saturating_add(
            fs::metadata(path(root, id)?)
                .map_err(|e| e.to_string())?
                .len(),
        );
        if index >= LIMIT || bytes > MAX_BYTES {
            remove(root, id)?;
        }
    }
    Ok(())
}

pub fn record<R: Runtime>(app: &AppHandle<R>, frame: &Frame, png: &[u8]) -> Result<(), String> {
    let shared = app.state::<Shared>();
    let _guard = shared.0.lock().map_err(|e| e.to_string())?;
    let root = root(app)?;
    let result = store(&root, frame, png);
    let _ = app.emit(CHANGED, ());
    result
}

fn store(root: &Path, frame: &Frame, png: &[u8]) -> Result<(), String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|e| e.to_string())?;
    let random: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let id = format!(
        "{}-{random}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
    );
    if png.len() as u64 > MAX_BYTES {
        return Err("capture exceeds history size limit".into());
    }
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or("invalid capture")?;
    let scale = (208.0 / frame.width as f64)
        .min(136.0 / frame.height as f64)
        .min(1.0);
    let width = ((frame.width as f64 * scale).round() as u32).max(1);
    let height = ((frame.height as f64 * scale).round() as u32).max(1);
    let small = image::imageops::thumbnail(&image, width, height);
    let thumb = crate::shell::encode_png(small.as_raw(), small.width(), small.height())
        .ok_or("could not encode thumbnail")?;
    let full = path(root, &id)?;
    fs::write(root.join(format!("{id}.thumb")), thumb).map_err(|e| e.to_string())?;
    if let Err(e) = fs::write(&full, png) {
        let _ = remove(root, &id);
        return Err(e.to_string());
    }
    trim(root)
}

// These blocking commands run on Tauri's async worker pool rather than the UI thread.
#[tauri::command]
pub async fn captures_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Entry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let _guard = shared.0.lock().map_err(|e| e.to_string())?;
        list(&root(&app)?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn captures_open<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let _guard = shared.0.lock().map_err(|e| e.to_string())?;
        let file = path(&root(&app)?, &id)?;
        if !regular(&file) {
            return Err("This capture is no longer available".into());
        }
        let mut png = Vec::new();
        fs::File::open(file)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut png)
            .map_err(|e| e.to_string())?;
        if png.len() as u64 > MAX_BYTES {
            return Err("capture exceeds history size limit".into());
        }
        let image =
            image::ImageReader::with_format(std::io::Cursor::new(&png), image::ImageFormat::Png)
                .decode()
                .map_err(|_| "This capture is damaged")?
                .into_rgba8();
        let (_, monitor) =
            super::platform::monitor_under_cursor().ok_or("No display is available")?;
        let frame = Frame {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
        };
        // Restore with the same preview service as a new capture. The history
        // file is never passed as an automatic save, so Discard cannot recycle it.
        super::preview::open(&app, frame, png, None, false, monitor)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn captures_remove<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    change(app, Some(id)).await
}

#[tauri::command]
pub async fn captures_clear<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    change(app, None).await
}

async fn change<R: Runtime>(app: AppHandle<R>, id: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let _guard = shared.0.lock().map_err(|e| e.to_string())?;
        let root = root(&app)?;
        let result = match id {
            Some(id) => remove(&root, &id),
            None => ids(&root).and_then(|ids| {
                let mut result = Ok(());
                for id in ids {
                    if let Err(e) = remove(&root, &id) {
                        result = Err(e);
                    }
                }
                result
            }),
        };
        // Also refresh after partial failure, so removed cards disappear immediately.
        let _ = app.emit(CHANGED, ());
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "1234-0123456789abcdef0123456789abcdef";

    fn fixture() -> PathBuf {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "vorssaint-captures-{:x}",
            u64::from_le_bytes(random)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_paths_and_malformed_ids() {
        for id in [
            "../outside",
            "C:\\image",
            "",
            "1234-abc",
            "../1234-0123456789abcdef0123456789abcdef",
        ] {
            assert!(!valid_id(id));
            assert!(path(Path::new("history"), id).is_err());
        }
        assert!(valid_id(ID));
    }

    #[test]
    fn records_real_pngs_and_aspect_preserving_thumbnails() {
        let dir = fixture();
        let frame = Frame {
            width: 400,
            height: 100,
            rgba: vec![255; 400 * 100 * 4],
        };
        let png = crate::shell::encode_png(&frame.rgba, frame.width, frame.height).unwrap();
        store(&dir, &frame, &png).unwrap();
        let entries = list(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(path(&dir, &entries[0].id).unwrap()).unwrap(), png);
        let thumb = image::load_from_memory(
            &fs::read(dir.join(format!("{}.thumb", entries[0].id))).unwrap(),
        )
        .unwrap();
        assert_eq!((thumb.width(), thumb.height()), (208, 52));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_traversal_and_directories_without_touching_unrelated_files() {
        let dir = fixture();
        let outside = dir.with_extension("png");
        fs::write(&outside, b"user file").unwrap();
        assert!(remove(&dir, "../outside").is_err());
        fs::create_dir(dir.join(format!("{ID}.png"))).unwrap();
        assert!(remove(&dir, ID).is_err());
        assert!(list(&dir).unwrap().is_empty());
        assert_eq!(fs::read(&outside).unwrap(), b"user file");
        fs::remove_file(outside).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn enforces_byte_limit() {
        let dir = fixture();
        let file = fs::File::create(dir.join(format!("{ID}.png"))).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        drop(file);
        trim(&dir).unwrap();
        assert!(list(&dir).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_history_and_deletion_leave_user_files_alone() {
        let dir = fixture();
        fs::write(dir.join(format!("{ID}.png")), b"capture").unwrap();
        fs::write(dir.join(format!("{ID}.thumb")), b"preview").unwrap();
        fs::write(dir.join("Screenshot.png"), b"user file").unwrap();
        assert_eq!(list(&dir).unwrap()[0].created_at, 1234);
        assert!(list(&dir).unwrap()[0].thumbnail.is_some());
        remove(&dir, ID).unwrap();
        remove(&dir, ID).unwrap(); // missing files are harmless
        assert!(list(&dir).unwrap().is_empty());
        assert_eq!(fs::read(dir.join("Screenshot.png")).unwrap(), b"user file");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn caps_history_newest_first_and_handles_missing_previews() {
        let dir = fixture();
        for n in 1..=35 {
            fs::write(
                dir.join(format!("{n}-0123456789abcdef0123456789abcdef.png")),
                b"capture",
            )
            .unwrap();
        }
        trim(&dir).unwrap();
        let entries = list(&dir).unwrap();
        assert_eq!(entries.len(), LIMIT);
        assert_eq!(entries[0].created_at, 35);
        assert_eq!(entries.last().unwrap().created_at, 6);
        assert!(entries[0].thumbnail.is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
