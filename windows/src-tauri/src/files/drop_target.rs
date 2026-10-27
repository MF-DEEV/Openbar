// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The island's own OLE drop target (spec 05 §6.4, Open question 4). Tauri's
//! file-drop handler only reads CF_HDROP and gives no answer for text or
//! links, and one window holds one drop target, so the island window turns
//! Tauri's off and registers this one for everything instead. Like wry it
//! tells WebView2 not to take external drops itself and registers on the
//! webview's windows.
//!
//! The closed island is click-through outside its shape, which hides it
//! from OLE as well; the pointer watcher (island.rs) lifts that while the
//! pointer, dragging or not, is over the shape, so a drag over the island
//! reaches this target and one beside it reaches the window beneath.

use std::cell::Cell;
use std::fs::File;
use std::path::Path;

use tauri::{AppHandle, Manager, Runtime};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller4;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, POINTL};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IDataObject};
use windows::Win32::System::Ole::{
    CF_DIB, CF_HDROP, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_NONE, IDropTarget,
    IDropTarget_Impl, RegisterDragDrop, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_INETURLW, CLSID_DragDropHelper, IDropTargetHelper,
};
use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;
use windows::core::{BOOL, Interface, Ref, implement, w};

use super::payload;
use super::ole;
use super::store::{Dropped, Item, new_id};

/// Virtual files: at most this many per drop, each at most this big. They
/// are copied while the drop lasts (the data object lives only that long),
/// which holds the island and the source, so the ceiling stays modest.
const MAX_VIRTUAL_FILES: usize = 50;
const MAX_VIRTUAL_SIZE: u64 = 512 << 20;

enum Event<'a> {
    Enter,
    Leave,
    Drop(&'a IDataObject),
}

#[implement(IDropTarget)]
struct Target {
    hwnd: HWND,
    /// Answers whether the drag may land (Enter) or landed (Drop).
    handle: Box<dyn Fn(Event) -> bool>,
    effect: Cell<DROPEFFECT>,
    /// Draws the source's drag image over the island, as Explorer does.
    helper: Option<IDropTargetHelper>,
}

/// Files, pictures, virtual files, links or text: what a shelf can keep.
fn holds_content(data: &IDataObject) -> bool {
    [
        CF_HDROP.0,
        ole::format(CFSTR_FILEDESCRIPTORW),
        ole::format(w!("PNG")),
        CF_DIB.0,
        ole::format(CFSTR_INETURLW),
        CF_UNICODETEXT.0,
    ]
    .into_iter()
    .any(|format| ole::has(data, format))
}

/// Copy when the source offers it, else a link; never a move, which would
/// let the source delete what the shelf only points at.
fn effect_for(allowed: DROPEFFECT) -> DROPEFFECT {
    if allowed.0 & DROPEFFECT_COPY.0 != 0 {
        DROPEFFECT_COPY
    } else if allowed.0 & DROPEFFECT_LINK.0 != 0 {
        DROPEFFECT_LINK
    } else {
        DROPEFFECT_NONE
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        // Told only when the drag can land, so a Leave or Drop always follows.
        let offered = effect_for(unsafe { *effect });
        let accepted = offered != DROPEFFECT_NONE
            && data.as_ref().is_some_and(holds_content)
            && (self.handle)(Event::Enter);
        let chosen = if accepted { offered } else { DROPEFFECT_NONE };
        self.effect.set(chosen);
        unsafe {
            *effect = chosen;
            if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
                let _ = helper.DragEnter(self.hwnd, data, &POINT { x: pt.x, y: pt.y }, chosen);
            }
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let chosen = if self.effect.get() == DROPEFFECT_NONE {
            DROPEFFECT_NONE
        } else {
            effect_for(unsafe { *effect })
        };
        unsafe {
            *effect = chosen;
            if let Some(helper) = &self.helper {
                let _ = helper.DragOver(&POINT { x: pt.x, y: pt.y }, chosen);
            }
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        if let Some(helper) = &self.helper {
            let _ = unsafe { helper.DragLeave() };
        }
        if self.effect.replace(DROPEFFECT_NONE) != DROPEFFECT_NONE {
            (self.handle)(Event::Leave);
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let offered = self.effect.replace(DROPEFFECT_NONE);
        unsafe {
            if let (Some(helper), Some(data)) = (&self.helper, data.as_ref()) {
                let _ = helper.Drop(data, &POINT { x: pt.x, y: pt.y }, offered);
            }
        }
        let landed = offered != DROPEFFECT_NONE && data.as_ref().is_some_and(|d| (self.handle)(Event::Drop(d)));
        unsafe { *effect = if landed { effect_for(*effect) } else { DROPEFFECT_NONE } };
        Ok(())
    }
}

/// What a drop holds, with pictures and virtual files written to `store`.
fn read(data: &IDataObject, store: Option<&Path>, room: usize) -> Dropped {
    let wide = |format: u16| ole::get(data, format).map(|b| payload::wide_string(&b)).filter(|s| !s.trim().is_empty());
    let mut dropped = Dropped {
        url: wide(ole::format(CFSTR_INETURLW)),
        text: wide(CF_UNICODETEXT.0),
        ..Default::default()
    };
    if let Some(list) = ole::get(data, CF_HDROP.0) {
        dropped.paths = payload::dropfiles(&list)
            .into_iter()
            .map(|path| {
                let id = super::identity::of(Path::new(&path)).map(|i| i.encode());
                (path, id)
            })
            .collect();
    }
    let Some(store) = store.filter(|_| dropped.paths.is_empty()) else { return dropped };
    // Virtual files: a browser's image, a mail attachment.
    if let Some(descriptors) = ole::get(data, ole::format(CFSTR_FILEDESCRIPTORW)) {
        let contents = ole::format(CFSTR_FILECONTENTS);
        for file in payload::virtual_files(&descriptors).into_iter().take(room.min(MAX_VIRTUAL_FILES)) {
            // A file declared too big is skipped before anything is copied.
            if file.size.is_some_and(|size| size > MAX_VIRTUAL_SIZE) {
                continue;
            }
            let fill = |out: &mut File| ole::copy(data, contents, file.index, MAX_VIRTUAL_SIZE, file.size, out);
            if let Some(path) = write_payload(store, &file.name, fill) {
                dropped.stored.push(Item::file(&path, None));
            }
        }
    }
    if dropped.stored.is_empty() {
        let png = ole::get(data, ole::format(w!("PNG"))).or_else(|| ole::get(data, CF_DIB.0).and_then(|d| payload::dib_to_png(&d)));
        if let Some(png) = png
            && let Some(path) = write_payload(store, "Image.png", |file| std::io::Write::write_all(file, &png).is_ok())
        {
            dropped.stored.push(Item::image(&path));
        }
    }
    dropped
}

/// Writes one payload into a folder of its own, so it keeps its name.
fn write_payload(store: &Path, name: &str, fill: impl FnOnce(&mut File) -> bool) -> Option<String> {
    let folder = store.join(new_id());
    std::fs::create_dir_all(&folder).ok()?;
    let path = folder.join(name);
    let written = File::create_new(&path).ok().is_some_and(|mut file| fill(&mut file));
    if !written {
        let _ = std::fs::remove_dir_all(&folder);
        return None;
    }
    Some(path.to_string_lossy().into_owned())
}

/// Registers the target on the island's webview. Runs on the island's thread.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(crate::island::LABEL) else { return };
    let app = app.clone();
    let _ = window.with_webview(move |webview| unsafe {
        let controller = webview.controller();
        if let Ok(controller) = controller.cast::<ICoreWebView2Controller4>() {
            let _ = controller.SetAllowExternalDrop(false);
        }
        let mut container = HWND::default();
        if controller.ParentWindow(&mut container).is_err() || container.is_invalid() {
            return;
        }
        let mut windows = vec![container];
        let _ = EnumChildWindows(Some(container), Some(collect), LPARAM(&mut windows as *mut Vec<HWND> as isize));
        let helper: Option<IDropTargetHelper> = CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER).ok();
        for hwnd in windows {
            let app = app.clone();
            let target: IDropTarget = Target {
                hwnd,
                handle: Box::new(move |event| handle(&app, event)),
                effect: Cell::new(DROPEFFECT_NONE),
                helper: helper.clone(),
            }
            .into();
            // Windows another process owns refuse; ours take the target.
            let _ = RevokeDragDrop(hwnd);
            let _ = RegisterDragDrop(hwnd, &target);
        }
    });
}

unsafe extern "system" fn collect(hwnd: HWND, list: LPARAM) -> BOOL {
    unsafe { (*(list.0 as *mut Vec<HWND>)).push(hwnd) };
    true.into()
}

fn handle<R: Runtime>(app: &AppHandle<R>, event: Event) -> bool {
    match event {
        Event::Enter => {
            let accepts = super::accepts_drops(app);
            if accepts {
                super::drag_phase(app, "enter", false);
            }
            accepts
        }
        Event::Leave => {
            super::drag_phase(app, "leave", false);
            false
        }
        Event::Drop(data) => {
            let files = app.state::<super::Files>();
            let room = store_room(&files);
            let dropped = read(data, files.store_dir().as_deref(), room);
            let added = super::add(app, super::store::items_from_drop(dropped));
            super::drag_phase(app, "drop", added);
            added
        }
    }
}

fn store_room(files: &super::Files) -> usize {
    // Never a panic inside an OLE callback: it would end the app.
    let state = files.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    super::store::MAX_LEAVES.saturating_sub(state.shelf.leaf_count())
}
