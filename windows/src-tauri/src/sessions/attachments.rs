// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

use std::io::{Cursor, Read};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;

const LIMIT: usize = 20 * 1024 * 1024;

#[derive(Serialize)]
pub struct Attachment {
    path: String,
    preview: String,
}

fn save(bytes: &[u8]) -> Result<Attachment, String> {
    if bytes.is_empty() || bytes.len() > LIMIT { return Err("Image must be smaller than 20 MB".into()); }
    let (mime, extension) = super::service::image_format(bytes).ok_or("Unsupported image")?;
    let folder = std::env::temp_dir().join("vorssaint-session-attachments");
    std::fs::create_dir_all(&folder).map_err(|_| "Cannot save image")?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "Cannot save image")?;
    let name: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let path = folder.join(format!("{name}.{extension}"));
    std::fs::write(&path, bytes).map_err(|_| "Cannot save image")?;
    Ok(Attachment { path: path.to_string_lossy().into_owned(), preview: format!("data:{mime};base64,{}", STANDARD.encode(bytes)) })
}

fn from_data_url(source: &str) -> Result<Attachment, String> {
    if source.len() > LIMIT * 4 / 3 + 128 { return Err("Image must be smaller than 20 MB".into()); }
    let (header, data) = source.split_once(',').ok_or("Invalid image")?;
    if !header.starts_with("data:image/") || !header.ends_with(";base64") { return Err("Invalid image".into()); }
    let bytes = STANDARD.decode(data).map_err(|_| "Invalid image")?;
    save(&bytes)
}

#[tauri::command]
pub async fn sessions_attach_image(data_url: String) -> Result<Attachment, String> {
    tauri::async_runtime::spawn_blocking(move || from_data_url(&data_url)).await.map_err(|_| "Cannot save image".to_string())?
}

#[tauri::command]
pub async fn sessions_clipboard_images() -> Result<Vec<Attachment>, String> {
    tauri::async_runtime::spawn_blocking(clipboard_images).await.map_err(|_| "Cannot read clipboard".to_string())?
}

fn dib_png(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    use image::ImageDecoder;
    let decoder = image::codecs::bmp::BmpDecoder::new_without_file_header(Cursor::new(bytes)).map_err(|_| "Unsupported clipboard image")?;
    let (width, height) = decoder.dimensions();
    if width as u64 * height as u64 > 40_000_000 { return Err("Clipboard image is too large".into()); }
    let image = image::DynamicImage::from_decoder(decoder).map_err(|_| "Cannot read clipboard image")?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).map_err(|_| "Cannot read clipboard image")?;
    Ok(png.into_inner())
}

#[cfg(windows)]
fn clipboard_images() -> Result<Vec<Attachment>, String> {
    use windows::core::w;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    struct Clipboard;
    impl Drop for Clipboard { fn drop(&mut self) { unsafe { let _ = CloseClipboard(); } } }
    unsafe fn bytes(format: u32) -> Option<Vec<u8>> {
        let handle = HGLOBAL(unsafe { GetClipboardData(format).ok()? }.0);
        let size = unsafe { GlobalSize(handle) };
        if size == 0 || size > 96 * 1024 * 1024 { return None; }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() { return None; }
        let copy = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
        unsafe { let _ = GlobalUnlock(handle); }
        Some(copy)
    }

    // The clipboard may briefly be held by the screenshot tool.
    let mut opened = false;
    for _ in 0..5 {
        if unsafe { OpenClipboard(None) }.is_ok() { opened = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !opened { return Err("Clipboard is busy. Try again".into()); }
    let _clipboard = Clipboard;
    let png = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    if let Some(data) = unsafe { bytes(png) } { return Ok(vec![save(&data)?]); }
    // CF_DIBV5 and CF_DIB carry a bitmap without a BMP file header.
    for format in [17, 8] {
        if let Some(data) = unsafe { bytes(format) } { return Ok(vec![save(&dib_png(data)?)?]); }
    }
    // Explorer's Copy on image files stores CF_HDROP rather than bitmap data.
    let mut images = Vec::new();
    if let Ok(handle) = unsafe { GetClipboardData(15) } {
        let drop = HDROP(handle.0);
        let count = unsafe { DragQueryFileW(drop, u32::MAX, None) }.min(5);
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop, index, None) } as usize;
            let mut path = vec![0u16; length + 1];
            unsafe { DragQueryFileW(drop, index, Some(&mut path)); }
            let path = String::from_utf16_lossy(&path[..length]);
            let Ok(file) = std::fs::File::open(&path) else { continue };
            let mut data = Vec::new();
            if file.take(LIMIT as u64 + 1).read_to_end(&mut data).is_ok()
                && data.len() <= LIMIT && super::service::image_format(&data).is_some() {
                images.push(save(&data)?);
            }
        }
    }
    Ok(images)
}

#[cfg(not(windows))]
fn clipboard_images() -> Result<Vec<Attachment>, String> { Ok(Vec::new()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_dib_is_encoded_as_png_without_resizing() {
        // A bottom-up 2 x 1, 24-bit DIB with one red and one green pixel.
        let mut dib = vec![0u8; 48];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        dib[40..46].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
        let png = dib_png(dib).unwrap();
        let decoded = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 255, 0]);
    }

    #[test]
    fn pasted_image_keeps_original_bytes_and_rejects_text() {
        let data = [b"\x89PNG\r\n\x1a\n".as_slice(), &[1, 2, 3]].concat();
        let attachment = from_data_url(&format!("data:image/png;base64,{}", STANDARD.encode(&data))).unwrap();
        assert_eq!(std::fs::read(&attachment.path).unwrap(), data);
        assert!(attachment.preview.starts_with("data:image/png;base64,"));
        std::fs::remove_file(attachment.path).unwrap();
        assert!(from_data_url("data:text/plain;base64,aGVsbG8=").is_err());
        assert!(from_data_url("data:image/png;base64,aGVsbG8=").is_err());
    }
}
