// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Reading and writing formats on an OLE data object, for the island's drop
//! target and for tiles dragged out.

use std::io::Write;
use std::mem::ManuallyDrop;

use windows::Win32::Foundation::{GlobalFree, HGLOBAL};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTREAM};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::core::PCWSTR;

/// A registered format's number ("UniformResourceLocatorW", "PNG"…).
pub fn format(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

fn etc(format: u16, index: i32, tymed: u32) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed,
    }
}

pub fn has(data: &IDataObject, format: u16) -> bool {
    let tymed = (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32;
    unsafe { data.QueryGetData(&etc(format, -1, tymed)) }.is_ok()
}

/// Copies a format's contents into `out`, up to `limit` bytes. False when
/// the format is missing, unreadable or larger than the limit.
pub fn copy(data: &IDataObject, format: u16, index: i32, limit: u64, out: &mut dyn Write) -> bool {
    let tymed = (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32;
    let Ok(mut medium) = (unsafe { data.GetData(&etc(format, index, tymed)) }) else {
        return false;
    };
    let ok = unsafe {
        if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            let global = medium.u.hGlobal;
            let size = GlobalSize(global);
            let start = GlobalLock(global) as *const u8;
            let ok = !start.is_null()
                && size as u64 <= limit
                && out.write_all(std::slice::from_raw_parts(start, size)).is_ok();
            let _ = GlobalUnlock(global);
            ok
        } else if medium.tymed == TYMED_ISTREAM.0 as u32 {
            match (*medium.u.pstm).clone() {
                Some(stream) => {
                    let mut buffer = vec![0u8; 1 << 20];
                    let mut total = 0u64;
                    loop {
                        let mut read = 0u32;
                        let hr = stream.Read(buffer.as_mut_ptr() as *mut _, buffer.len() as u32, Some(&mut read));
                        if hr.is_err() {
                            break false;
                        }
                        if read == 0 {
                            break true;
                        }
                        total += read as u64;
                        if total > limit || out.write_all(&buffer[..read as usize]).is_err() {
                            break false;
                        }
                    }
                }
                None => false,
            }
        } else {
            false
        }
    };
    unsafe { ReleaseStgMedium(&mut medium) };
    ok
}

/// A small format's bytes (text, links, file lists, descriptors).
pub fn get(data: &IDataObject, format: u16) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    copy(data, format, -1, 64 << 20, &mut bytes).then_some(bytes)
}

/// Puts `bytes` on the data object as an HGLOBAL it then owns.
pub fn set(data: &IDataObject, format: u16, bytes: &[u8]) -> windows::core::Result<()> {
    unsafe {
        let global: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let start = GlobalLock(global) as *mut u8;
        if !start.is_null() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), start, bytes.len());
        }
        let _ = GlobalUnlock(global);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: global },
            pUnkForRelease: ManuallyDrop::new(None),
        };
        let result = data.SetData(&etc(format, -1, TYMED_HGLOBAL.0 as u32), &medium, true);
        if result.is_err() {
            let _ = GlobalFree(Some(global));
        }
        result
    }
}
