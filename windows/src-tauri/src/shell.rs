// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Names, icons and thumbnails from the Windows shell, as the island's pages
//! show them: an app's display name (its FileDescription), an exe or file
//! icon, or a file's thumbnail, encoded as a PNG data URL. All shell calls
//! run on one STA thread; results are cached by path and size. It also opens
//! Task Manager, the Mac's "Open Activity Monitor".

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
        let un = |c: u32| {
            (c * 255 + a / 2)
                .checked_div(a)
                .map_or(0, |v| v.min(255) as u8)
        };
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

/// Version resources can overstate a string's length and include the next
/// metadata field after its NUL terminator. Only the first string is a name.
#[cfg_attr(not(windows), allow(dead_code))]
fn version_description(units: &[u16]) -> Option<String> {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let text = String::from_utf16(&units[..end]).ok()?;
    let text = text.trim();
    (!text.is_empty() && !text.chars().any(char::is_control)).then(|| text.to_string())
}

/// A returned version value must stay inside its owning resource buffer.
#[cfg_attr(not(windows), allow(dead_code))]
fn version_bytes(data: &[u8], value: *const std::ffi::c_void, len: usize) -> Option<&[u8]> {
    let offset = (value as usize).checked_sub(data.as_ptr() as usize)?;
    data.get(offset..offset.checked_add(len)?)
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

/// Task Manager, as the System detail's process list opens it; false when
/// it couldn't be started.
#[tauri::command]
pub async fn shell_open_task_manager() -> bool {
    tauri::async_runtime::spawn_blocking(|| worker().run(imp::open_task_manager).unwrap_or(false))
        .await
        .unwrap_or(false)
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, SIZE};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
        GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAGS_AND_ATTRIBUTES, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SEE_MASK_DOENVSUBST, SEE_MASK_NOASYNC, SHCreateItemFromParsingName,
        SHELLEXECUTEINFOW, SHFILEINFOW, SHGFI_TYPENAME, SHGetFileInfoW, SIIGBF_BIGGERSIZEOK,
        SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT, ShellExecuteExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR, w};

    /// By its System32 path: a bare name is looked for in the current
    /// directory first. A running Task Manager comes to the front.
    pub fn open_task_manager() -> bool {
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // This worker has no message loop to finish the launch on.
            fMask: SEE_MASK_NOASYNC | SEE_MASK_DOENVSUBST,
            lpFile: w!(r"%SystemRoot%\System32\Taskmgr.exe"),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        unsafe { ShellExecuteExW(&mut info) }.is_ok()
    }

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
                .GetImage(
                    SIZE {
                        cx: width as i32,
                        cy: height as i32,
                    },
                    flags,
                )
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
            if bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
                for px in bgra.as_chunks_mut::<4>().0 {
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
                let pair = super::version_bytes(&data, ptr, 4)?;
                let language = u16::from_le_bytes([pair[0], pair[1]]);
                let code_page = u16::from_le_bytes([pair[2], pair[3]]);
                table = format!("{language:04X}{code_page:04X}");
            }
            let key = HSTRING::from(format!("\\StringFileInfo\\{table}\\FileDescription"));
            if !VerQueryValueW(data.as_ptr() as *const _, &key, &mut ptr, &mut len).as_bool()
                || len == 0
            {
                return None;
            }
            let bytes = super::version_bytes(&data, ptr, (len as usize).checked_mul(2)?)?;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            super::version_description(&units)
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
    pub fn open_task_manager() -> bool {
        false
    }
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
    fn version_description_stops_before_neighboring_metadata() {
        let units: Vec<u16> = "NVIDIA App\0\u{8}\u{1}FileVersion\0"
            .encode_utf16()
            .collect();
        assert_eq!(version_description(&units).as_deref(), Some("NVIDIA App"));
        let unicode: Vec<_> = "  Aplikacja Żółć 🦊  \0".encode_utf16().collect();
        assert_eq!(
            version_description(&unicode).as_deref(),
            Some("Aplikacja Żółć 🦊")
        );
        assert_eq!(version_description(&[0, 65]), None);
        assert_eq!(version_description(&[0xD800]), None);
        assert_eq!(version_description(&[65, 8, 66]), None);
    }

    #[test]
    fn version_values_cannot_read_past_the_resource() {
        let data = [0u8; 16];
        let start = data.as_ptr().cast();
        assert_eq!(version_bytes(&data, start, 16), Some(data.as_slice()));
        assert!(version_bytes(&data, start, 17).is_none());
        assert!(version_bytes(&data, std::ptr::null(), 4).is_none());
        assert!(version_bytes(&data, data[12..].as_ptr().cast(), 8).is_none());
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "checks version resources of NVIDIA software installed on this host"]
    fn live_nvidia_display_names() {
        let directory = std::path::Path::new(r"C:\Program Files\NVIDIA Corporation");
        let mut checked = 0;
        for relative in [
            r"NVIDIA App\CEF\NVIDIA Overlay.exe",
            r"NVIDIA App\NVIDIA app.exe",
            r"NvContainer\nvcontainer.exe",
        ] {
            let path = directory.join(relative);
            if !path.is_file() {
                continue;
            }
            let name = app_name(path.to_str().unwrap());
            eprintln!("{}: {name}", path.display());
            assert!(!name.is_empty());
            assert!(!name.chars().any(char::is_control));
            assert!(!name.contains("FileVer"));
            checked += 1;
        }
        assert!(
            checked > 0,
            "no NVIDIA executables found for host validation"
        );
    }

    #[test]
    fn premultiplied_pixels_are_straightened() {
        // Half-transparent pure red, premultiplied: B=0 G=0 R=128 A=128.
        assert_eq!(
            bgra_premultiplied_to_rgba(&[0, 0, 128, 128]),
            vec![255, 0, 0, 128]
        );
        assert_eq!(
            bgra_premultiplied_to_rgba(&[10, 20, 30, 0]),
            vec![0, 0, 0, 0]
        );
        assert_eq!(
            bgra_premultiplied_to_rgba(&[1, 2, 3, 255]),
            vec![3, 2, 1, 255]
        );
    }

    #[test]
    fn encodes_a_png_with_a_data_url() {
        let png = encode_png(&[255, 0, 0, 255, 0, 255, 0, 255], 2, 1).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png_data_url(&png).starts_with("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn an_unknown_exe_is_named_after_its_file() {
        assert_eq!(
            app_name(r"C:\nowhere\Some App.exe")
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .unwrap(),
            "Some App"
        );
    }
}
