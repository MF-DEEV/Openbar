// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Names, icons and thumbnails from the Windows shell, as the island's pages
//! show them: an app's display name (its FileDescription), an exe or file
//! icon, or a file's thumbnail, encoded as a PNG data URL. All shell calls
//! run on one STA thread; results are cached by path and size.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use base64::Engine;

use crate::com::Worker;

fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(|| Worker::spawn("vorssaint-shell", true))
}

const CACHE_LIMIT: usize = 256;

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(key: String, make: impl FnOnce() -> Option<String> + Send + 'static) -> Option<String> {
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return hit.clone();
    }
    let value = worker().run(make).flatten();
    let mut map = cache().lock().unwrap();
    if map.len() >= CACHE_LIMIT {
        map.clear();
    }
    map.insert(key, value.clone());
    value
}

/// PNG bytes as a data URL for the webview.
pub fn png_data_url(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// Straight RGBA from the shell's premultiplied BGRA, top-down.
pub fn bgra_premultiplied_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len());
    for px in bgra.as_chunks::<4>().0 {
        let (b, g, r, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
        let un = |c: u32| (c * 255 + a / 2).checked_div(a).map_or(0, |v| v.min(255) as u8);
        out.extend_from_slice(&[un(r), un(g), un(b), a as u8]);
    }
    out
}

pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The icon of a file, folder or exe, `size` physical pixels square.
pub fn icon(path: &str, size: u32) -> Option<String> {
    let owned = path.to_string();
    cached(format!("icon|{size}|{path}"), move || {
        imp::image(&owned, size, size, true).map(|png| png_data_url(&png))
    })
}

/// A file's thumbnail fit into `width` × `height`, or its icon when it has none.
pub fn thumbnail(path: &str, width: u32, height: u32) -> Option<String> {
    let owned = path.to_string();
    cached(format!("thumb|{width}x{height}|{path}"), move || {
        imp::image(&owned, width, height, false).map(|png| png_data_url(&png))
    })
}

/// An app's name as Windows shows it: the exe's FileDescription, else its file name.
pub fn app_name(exe: &str) -> String {
    imp::file_description(exe).unwrap_or_else(|| {
        std::path::Path::new(exe)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

/// The full path of a running process's exe.
pub fn process_path(pid: u32) -> Option<String> {
    imp::process_path(pid)
}

/// The file's type as Explorer names it ("PNG File", "Application").
pub fn type_name(path: &str) -> Option<String> {
    let owned = path.to_string();
    worker().run(move || imp::type_name(&owned)).flatten()
}

#[tauri::command]
pub async fn shell_icon(path: String, size: u32, thumbnail: bool) -> Option<String> {
    let size = size.clamp(8, 512);
    tauri::async_runtime::spawn_blocking(move || {
        if thumbnail {
            self::thumbnail(&path, size, size)
        } else {
            icon(&path, size)
        }
    })
    .await
    .ok()
    .flatten()
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, SIZE};
    use windows::Win32::Graphics::Gdi::{
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
        GetObjectW, HGDIOBJ, ReleaseDC,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAGS_AND_ATTRIBUTES, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SHFILEINFOW, SHGFI_TYPENAME, SHGetFileInfoW,
        SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR};

    pub fn image(path: &str, width: u32, height: u32, icon_only: bool) -> Option<Vec<u8>> {
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
            let flags = if icon_only {
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK
            } else {
                SIIGBF_RESIZETOFIT
            };
            let bitmap = factory
                .GetImage(SIZE { cx: width as i32, cy: height as i32 }, flags)
                .ok()?;
            let png = bitmap_png(bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            png
        }
    }

    unsafe fn bitmap_png(bitmap: windows::Win32::Graphics::Gdi::HBITMAP) -> Option<Vec<u8>> {
        unsafe {
            let mut info = BITMAP::default();
            if GetObjectW(
                HGDIOBJ(bitmap.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut info as *mut _ as *mut _),
            ) == 0
            {
                return None;
            }
            let (w, h) = (info.bmWidth, info.bmHeight.abs());
            if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
                return None;
            }
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            let dc = GetDC(None);
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                Some(bgra.as_mut_ptr() as *mut _),
                &mut header,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            if lines == 0 {
                return None;
            }
            // Bitmaps without alpha (some thumbnails) come back with alpha 0 everywhere.
            if bgra.chunks_exact(4).all(|px| px[3] == 0) {
                for px in bgra.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }
            let rgba = super::bgra_premultiplied_to_rgba(&bgra);
            super::encode_png(&rgba, w as u32, h as u32)
        }
    }

    pub fn file_description(exe: &str) -> Option<String> {
        unsafe {
            let name = HSTRING::from(exe);
            let size = GetFileVersionInfoSizeW(&name, None);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(&name, None, size, data.as_mut_ptr() as *mut _).ok()?;
            // The first language/code page pair names the string table.
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            let mut table = "040904B0".to_string();
            if VerQueryValueW(
                data.as_ptr() as *const _,
                &HSTRING::from("\\VarFileInfo\\Translation"),
                &mut ptr,
                &mut len,
            )
            .as_bool()
                && len >= 4
            {
                let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
                table = format!("{:04X}{:04X}", pair[0], pair[1]);
            }
            let key = HSTRING::from(format!("\\StringFileInfo\\{table}\\FileDescription"));
            if !VerQueryValueW(data.as_ptr() as *const _, &key, &mut ptr, &mut len).as_bool() || len == 0 {
                return None;
            }
            let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let text = String::from_utf16_lossy(text);
            let text = text.trim_end_matches('\0').trim();
            (!text.is_empty()).then(|| text.to_string())
        }
    }

    pub fn process_path(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 32768];
            let mut len = buffer.len() as u32;
            let read = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(process);
            read.ok()?;
            Some(String::from_utf16_lossy(&buffer[..len as usize]))
        }
    }

    pub fn type_name(path: &str) -> Option<String> {
        unsafe {
            let mut info = SHFILEINFOW::default();
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let ok = SHGetFileInfoW(
                PCWSTR(wide.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_TYPENAME,
            );
            if ok == 0 {
                return None;
            }
            let end = info.szTypeName.iter().position(|&c| c == 0).unwrap_or(0);
            (end > 0).then(|| String::from_utf16_lossy(&info.szTypeName[..end]))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn image(_path: &str, _w: u32, _h: u32, _icon_only: bool) -> Option<Vec<u8>> {
        None
    }
    pub fn file_description(_exe: &str) -> Option<String> {
        None
    }
    pub fn process_path(_pid: u32) -> Option<String> {
        None
    }
    pub fn type_name(_path: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_pixels_are_straightened() {
        // Half-transparent pure red, premultiplied: B=0 G=0 R=128 A=128.
        assert_eq!(bgra_premultiplied_to_rgba(&[0, 0, 128, 128]), vec![255, 0, 0, 128]);
        assert_eq!(bgra_premultiplied_to_rgba(&[10, 20, 30, 0]), vec![0, 0, 0, 0]);
        assert_eq!(bgra_premultiplied_to_rgba(&[1, 2, 3, 255]), vec![3, 2, 1, 255]);
    }

    #[test]
    fn encodes_a_png_with_a_data_url() {
        let png = encode_png(&[255, 0, 0, 255, 0, 255, 0, 255], 2, 1).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png_data_url(&png).starts_with("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn an_unknown_exe_is_named_after_its_file() {
        assert_eq!(app_name(r"C:\nowhere\Some App.exe").replace('\\', "/").rsplit('/').next().unwrap(), "Some App");
    }
}
