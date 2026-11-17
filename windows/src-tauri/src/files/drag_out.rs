// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Dragging tiles out of the island (ShelfTileView.beginItemDrag; spec 05
//! §6.4, Open question 5): a native DoDragDrop with the shell's own data
//! object for the files (as Explorer drags them) and plain text for notes
//! and links. It runs on the island's thread, where the button went down.
//!
//! While OLE runs its drag loop on that thread, the pointer watcher's
//! click-through changes may wait behind it, so the source keeps the
//! island's hit area right itself as well: the window is hit-testable over
//! the island's shape (a drop back on the island goes nowhere) and
//! click-through everywhere else, so every other window under the pointer
//! can take the drop.
//!
//! Explorer's optimized move reports no effect even after moving the file;
//! the real result is in "Performed DropEffect", or in the file being gone.

use std::path::Path;

use windows::Win32::Foundation::{
    COLORREF, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND, POINT, RECT,
    S_OK, SIZE,
};
use windows::Win32::Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW, HGDIOBJ};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject};
use windows::Win32::System::Ole::{
    CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE, DoDragDrop,
    IDropSource, IDropSource_Impl,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    CFSTR_INETURLW, CFSTR_PERFORMEDDROPEFFECT, CLSID_DragDropHelper, IDragSourceHelper,
    IShellItemImageFactory, SHCreateDataObject, SHCreateItemFromParsingName, SHDRAGIMAGE,
    SIIGBF_RESIZETOFIT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, WS_EX_LAYERED,
    WS_EX_TRANSPARENT,
};
use windows::core::{BOOL, HRESULT, HSTRING, implement};

use super::{ole, payload, store};

/// Whether a point in the island window's CSS pixels is on the island.
pub type OnIsland = Box<dyn Fn(f64, f64) -> bool + Send>;

#[implement(IDropSource)]
struct Source {
    hwnd: HWND,
    on_island: OnIsland,
}

impl Source {
    /// Hit-testable over the island, click-through elsewhere (as tao's
    /// ignore-cursor-events sets it).
    fn follow_pointer(&self) {
        unsafe {
            let mut pointer = POINT::default();
            let mut frame = RECT::default();
            if GetCursorPos(&mut pointer).is_err() || GetWindowRect(self.hwnd, &mut frame).is_err()
            {
                return;
            }
            let scale = match GetDpiForWindow(self.hwnd) {
                0 => 1.0,
                dpi => dpi as f64 / 96.0,
            };
            let inside = (self.on_island)(
                (pointer.x - frame.left) as f64 / scale,
                (pointer.y - frame.top) as f64 / scale,
            );
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let through = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize;
            let wanted = if inside {
                style & !through
            } else {
                style | through
            };
            if wanted != style {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, wanted);
            }
        }
    }
}

#[allow(non_snake_case)]
impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        self.follow_pointer();
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Drags `paths` (files) and `texts` (notes and links). Answers whether a
/// destination took them.
pub fn run(
    hwnd: isize,
    paths: &[String],
    texts: &[String],
    allow_move: bool,
    on_island: OnIsland,
) -> bool {
    let hwnd = HWND(hwnd as _);
    let data: windows::core::Result<IDataObject> = if paths.is_empty() {
        unsafe { SHCreateDataObject(None, None, None::<&IDataObject>) }
    } else {
        super::shell::data_object(paths)
    };
    let Ok(data) = data else { return false };
    if !texts.is_empty() {
        let _ = ole::set(
            &data,
            CF_UNICODETEXT.0,
            &payload::wide_bytes(&texts.join("\r\n")),
        );
        if let [only] = texts
            && store::is_link(only)
        {
            let _ = ole::set(
                &data,
                ole::format(CFSTR_INETURLW),
                &payload::wide_bytes(only),
            );
        }
    }
    if let Some(first) = paths.first() {
        drag_image(&data, first);
    }
    let source = Source { hwnd, on_island };
    let allowed = if allow_move {
        DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0)
    } else {
        DROPEFFECT_COPY
    };
    let source: IDropSource = source.into();
    let mut effect = DROPEFFECT_NONE;
    let result = unsafe { DoDragDrop(&data, &source, allowed, &mut effect) };
    if result != DRAGDROP_S_DROP {
        return false;
    }
    if effect != DROPEFFECT_NONE {
        return true;
    }
    let performed = ole::get(&data, ole::format(CFSTR_PERFORMEDDROPEFFECT))
        .and_then(|b| {
            b.get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        })
        .unwrap_or(0);
    performed != DROPEFFECT_NONE.0 || paths.iter().any(|p| !Path::new(p).exists())
}

/// The first file's thumbnail under the pointer while it travels.
fn drag_image(data: &IDataObject, path: &str) {
    unsafe {
        let Ok(helper) = CoCreateInstance::<_, IDragSourceHelper>(
            &CLSID_DragDropHelper,
            None,
            CLSCTX_INPROC_SERVER,
        ) else {
            return;
        };
        let Ok(factory) =
            SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(&HSTRING::from(path), None)
        else {
            return;
        };
        let Ok(bitmap) = factory.GetImage(SIZE { cx: 96, cy: 96 }, SIIGBF_RESIZETOFIT) else {
            return;
        };
        let mut info = BITMAP::default();
        GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut info as *mut _ as *mut _),
        );
        let image = SHDRAGIMAGE {
            sizeDragImage: SIZE {
                cx: info.bmWidth,
                cy: info.bmHeight.abs(),
            },
            ptOffset: POINT {
                x: info.bmWidth / 2,
                y: info.bmHeight.abs() / 2,
            },
            hbmpDragImage: bitmap,
            crColorKey: COLORREF(u32::MAX),
        };
        // On success the helper owns the bitmap.
        if helper.InitializeFromBitmap(&image, data).is_err() {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
    }
}
