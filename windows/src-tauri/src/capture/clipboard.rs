// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! A minimal clipboard writer for the capture tools and the scratchpad's
//! Copy all: text, or a picture as `CF_DIB` plus the registered "PNG"
//! format (spec 05 §4.2), which keeps transparency for the apps that read
//! it. Kept on its own so it can fold into the Clipboard section's writer.

/// The clipboard as Windows numbers it (winuser.h).
#[cfg(any(windows, test))]
const CF_DIB: u32 = 8;
#[cfg(windows)]
const CF_UNICODETEXT: u32 = 13;

/// A bottom-up 32-bit `BITMAPINFOHEADER` + BGRA pixels, the `CF_DIB` every
/// reader understands (a negative, top-down height trips older apps).
#[cfg(any(windows, test))]
fn dib(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = width as usize * 4;
    let mut out = Vec::with_capacity(40 + rgba.len());
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(rgba.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution and palette: none
    for y in (0..height as usize).rev() {
        for px in rgba[y * row..(y + 1) * row].as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

pub fn set_text(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes =
            unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
        imp::put(&[(CF_UNICODETEXT, bytes)])
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err("clipboard needs Windows".into())
    }
}

pub fn set_image(rgba: &[u8], width: u32, height: u32, png: &[u8]) -> Result<(), String> {
    #[cfg(windows)]
    {
        let dib = dib(rgba, width, height);
        let png_format = unsafe {
            windows::Win32::System::DataExchange::RegisterClipboardFormatW(windows::core::w!("PNG"))
        };
        imp::put(&[(CF_DIB, &dib), (png_format, png)])
    }
    #[cfg(not(windows))]
    {
        let _ = (rgba, width, height, png);
        Err("clipboard needs Windows".into())
    }
}

#[cfg(windows)]
mod imp {
    use std::thread::sleep;
    use std::time::Duration;
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    };
    use windows::core::w;

    /// Replaces the clipboard with `formats`. A message-only window owns it:
    /// with no owner, `EmptyClipboard` leaves `SetClipboardData` failing.
    pub fn put(formats: &[(u32, &[u8])]) -> Result<(), String> {
        unsafe {
            let owner = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!(""),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
            let result = put_owned(owner, formats);
            let _ = DestroyWindow(owner);
            result
        }
    }

    unsafe fn put_owned(owner: HWND, formats: &[(u32, &[u8])]) -> Result<(), String> {
        // Another app may hold the clipboard for a moment (a clipboard manager reading it).
        let mut opened = false;
        for _ in 0..10 {
            if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(20));
        }
        if !opened {
            return Err("the clipboard is busy".into());
        }
        let result = (|| unsafe {
            EmptyClipboard().map_err(|e| e.to_string())?;
            for &(format, bytes) in formats {
                let memory =
                    GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|e| e.to_string())?;
                let target = GlobalLock(memory).cast::<u8>();
                if target.is_null() {
                    let _ = GlobalFree(Some(memory));
                    return Err("out of memory".into());
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
                let _ = GlobalUnlock(memory);
                // The clipboard owns the memory once this succeeds.
                if let Err(e) = SetClipboardData(format, Some(HANDLE(memory.0))) {
                    let _ = GlobalFree(Some(memory));
                    return Err(e.to_string());
                }
            }
            Ok(())
        })();
        unsafe {
            let _ = CloseClipboard();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dib_is_bottom_up_bgra_behind_a_40_byte_header() {
        // 1×2: red on top, blue below.
        let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
        let out = dib(&rgba, 1, 2);
        assert_eq!(out.len(), 40 + 8);
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(out[8..12].try_into().unwrap()), 2);
        assert_eq!(
            &out[40..44],
            &[255, 0, 0, 255],
            "the bottom row (blue) comes first, as BGRA"
        );
        assert_eq!(&out[44..48], &[0, 0, 255, 255]);
        let _ = CF_DIB;
    }
}
