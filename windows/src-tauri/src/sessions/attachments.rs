// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;

const LIMIT: usize = 20 * 1024 * 1024;

#[derive(Serialize)]
pub struct Attachment {
    path: String,
    preview: String,
}

fn save(bytes: &[u8]) -> Result<Attachment, String> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err("Image must be smaller than 20 MB".into());
    }
    let (mime, extension) = super::service::image_format(bytes).ok_or("Unsupported image")?;
    let folder = std::env::temp_dir().join("openbar-session-attachments");
    std::fs::create_dir_all(&folder).map_err(|_| "Cannot save image")?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "Cannot save image")?;
    let name: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let path = folder.join(format!("{name}.{extension}"));
    std::fs::write(&path, bytes).map_err(|_| "Cannot save image")?;
    Ok(Attachment {
        path: path.to_string_lossy().into_owned(),
        preview: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
    })
}

fn from_data_url(source: &str) -> Result<Attachment, String> {
    if source.len() > LIMIT * 4 / 3 + 128 {
        return Err("Image must be smaller than 20 MB".into());
    }
    let (header, data) = source.split_once(',').ok_or("Invalid image")?;
    if !header.starts_with("data:image/") || !header.ends_with(";base64") {
        return Err("Invalid image".into());
    }
    let bytes = STANDARD.decode(data).map_err(|_| "Invalid image")?;
    save(&bytes)
}

#[tauri::command]
pub async fn sessions_attach_image(data_url: String) -> Result<Attachment, String> {
    tauri::async_runtime::spawn_blocking(move || from_data_url(&data_url))
        .await
        .map_err(|_| "Cannot save image".to_string())?
}

#[tauri::command]
pub async fn sessions_clipboard_images() -> Result<Vec<Attachment>, String> {
    tauri::async_runtime::spawn_blocking(clipboard_images)
        .await
        .map_err(|_| "Cannot read clipboard".to_string())?
}

#[cfg(windows)]
fn clipboard_images() -> Result<Vec<Attachment>, String> {
    use crate::clipboard::{formats, native};

    // The clipboard may briefly be held by the screenshot tool.
    let clipboard = native::open(None).ok_or("Clipboard is busy. Try again")?;
    let png = unsafe {
        windows::Win32::System::DataExchange::RegisterClipboardFormatW(windows::core::w!("PNG"))
    };
    if let Some(data) = native::bytes(&clipboard, png, 96 * 1024 * 1024) {
        return Ok(vec![save(&data)?]);
    }
    // CF_DIBV5 and CF_DIB carry a bitmap without a BMP file header.
    for format in [17, 8] {
        if let Some(data) = native::bytes(&clipboard, format, 96 * 1024 * 1024) {
            return Ok(vec![save(&formats::dib_png(data)?)?]);
        }
    }
    // Explorer's Copy on image files stores CF_HDROP rather than bitmap data.
    image_files(
        native::hdrop_paths(&clipboard, 5)
            .map(|(_, paths)| paths)
            .unwrap_or_default(),
    )
}

/// The image files among `paths`, at most five, as attachments.
#[cfg(any(windows, target_os = "linux"))]
fn image_files(paths: Vec<String>) -> Result<Vec<Attachment>, String> {
    use std::io::Read;

    let mut images = Vec::new();
    for path in paths.into_iter().take(5) {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut data = Vec::new();
        if file.take(LIMIT as u64 + 1).read_to_end(&mut data).is_ok()
            && data.len() <= LIMIT
            && super::service::image_format(&data).is_some()
        {
            images.push(save(&data)?);
        }
    }
    Ok(images)
}

/// A picture on the clipboard, or image files copied in the file manager.
#[cfg(target_os = "linux")]
fn clipboard_images() -> Result<Vec<Attachment>, String> {
    use crate::clipboard::history::Content;

    match crate::clipboard::native::read(true) {
        Some(Content::Image { png, .. }) => Ok(vec![save(&png)?]),
        Some(Content::Files(paths)) => image_files(paths),
        _ => Ok(Vec::new()),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn clipboard_images() -> Result<Vec<Attachment>, String> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_image_keeps_original_bytes_and_rejects_text() {
        let data = [b"\x89PNG\r\n\x1a\n".as_slice(), &[1, 2, 3]].concat();
        let attachment =
            from_data_url(&format!("data:image/png;base64,{}", STANDARD.encode(&data))).unwrap();
        assert_eq!(std::fs::read(&attachment.path).unwrap(), data);
        assert!(attachment.preview.starts_with("data:image/png;base64,"));
        std::fs::remove_file(attachment.path).unwrap();
        assert!(from_data_url("data:text/plain;base64,aGVsbG8=").is_err());
        assert!(from_data_url("data:image/png;base64,aGVsbG8=").is_err());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn copied_files_give_only_their_images_and_at_most_five() {
        let folder =
            std::env::temp_dir().join(format!("openbar-image-files-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let png = [b"\x89PNG\r\n\x1a\n".as_slice(), &[9, 9]].concat();
        let mut paths = Vec::new();
        for index in 0..6 {
            let path = folder.join(format!("{index}.png"));
            std::fs::write(&path, &png).unwrap();
            paths.push(path.to_string_lossy().into_owned());
        }
        let text = folder.join("notes.txt");
        std::fs::write(&text, "hello").unwrap();
        let missing = folder.join("gone.png");

        let mixed = vec![
            text.to_string_lossy().into_owned(),
            missing.to_string_lossy().into_owned(),
            paths[0].clone(),
        ];
        let found = image_files(mixed).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(std::fs::read(&found[0].path).unwrap(), png);

        let many = image_files(paths).unwrap();
        assert_eq!(many.len(), 5);

        for attachment in found.iter().chain(many.iter()) {
            std::fs::remove_file(&attachment.path).unwrap();
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
