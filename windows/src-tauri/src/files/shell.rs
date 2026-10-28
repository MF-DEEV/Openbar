// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The tile menu's file actions (ShelfTileView.menu, spec 05 §6.4): Open
//! with each file's default app, Open With the apps every selected file has
//! in common (SHAssocEnumHandlers, launched with IAssocHandler::Invoke),
//! Show in File Explorer (SHOpenFolderAndSelectItems), and Share
//! (IDataTransferManagerInterop with the files as StorageFiles). The shell
//! calls run on one STA thread of their own.

use std::sync::OnceLock;

use tauri::{AppHandle, Runtime};

use super::Handler;
use crate::com::Worker;

#[cfg_attr(not(windows), allow(dead_code))]
fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(|| Worker::spawn("vorssaint-files", true))
}

pub fn open(paths: Vec<String>) {
    #[cfg(windows)]
    worker().run(move || imp::open(&paths));
    #[cfg(not(windows))]
    let _ = paths;
}

/// The apps for every file, by name, at most 40 (the Mac's Open With cap).
pub fn handlers(paths: Vec<String>) -> Vec<Handler> {
    #[cfg(windows)]
    {
        let found = worker()
            .run(move || imp::handlers(&paths))
            .unwrap_or_default();
        found
            .into_iter()
            .map(|(id, name, icon_source)| Handler {
                icon: crate::shell::icon(&icon_source, 32),
                id,
                name,
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Vec::new()
    }
}

pub fn open_with(paths: Vec<String>, handler: String) {
    #[cfg(windows)]
    worker().run(move || imp::open_with(&paths, &handler));
    #[cfg(not(windows))]
    let _ = (paths, handler);
}

pub fn reveal(paths: Vec<String>) {
    #[cfg(windows)]
    worker().run(move || imp::reveal(&paths));
    #[cfg(not(windows))]
    let _ = paths;
}

/// Opens the Share sheet for the island. Answers whether it opened.
pub async fn share<R: Runtime>(app: &AppHandle<R>, paths: Vec<String>) -> bool {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let Some(window) = app.get_webview_window(crate::island::LABEL) else {
            return false;
        };
        let Ok(hwnd) = window.hwnd() else {
            return false;
        };
        let hwnd = hwnd.0 as isize;
        let (tx, rx) = std::sync::mpsc::channel();
        // The share sheet belongs to the window's own thread.
        if app
            .run_on_main_thread(move || {
                let _ = tx.send(imp::share(hwnd, paths).is_ok());
            })
            .is_err()
        {
            return false;
        }
        tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or(false))
            .await
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, paths);
        false
    }
}

#[cfg(windows)]
pub(super) use imp::data_object;

#[cfg(windows)]
mod imp {
    use std::collections::HashSet;
    use std::path::Path;

    use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::{IStorageItem, StorageFile};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoTaskMemFree, IDataObject};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        ASSOC_FILTER_RECOMMENDED, BHID_DataObject, IAssocHandler, IDataTransferManagerInterop,
        ILFree, SEE_MASK_NOASYNC, SHAssocEnumHandlers, SHCreateShellItemArrayFromIDLists,
        SHELLEXECUTEINFOW, SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, SW_SHOWNORMAL,
    };
    use windows::core::{HSTRING, Interface, PWSTR};
    use windows_collections::IIterable;

    /// Absolute shell item ids, freed when dropped.
    struct Pidls(Vec<*mut ITEMIDLIST>);
    impl Pidls {
        fn parse(paths: &[String]) -> windows::core::Result<Pidls> {
            let mut out = Pidls(Vec::new());
            for path in paths {
                let mut pidl = std::ptr::null_mut();
                unsafe {
                    SHParseDisplayName(&HSTRING::from(path.as_str()), None, &mut pidl, 0, None)?
                };
                out.0.push(pidl);
            }
            Ok(out)
        }
        fn as_const(&self) -> Vec<*const ITEMIDLIST> {
            self.0.iter().map(|p| *p as *const _).collect()
        }
    }
    impl Drop for Pidls {
        fn drop(&mut self) {
            for pidl in &self.0 {
                unsafe { ILFree(Some(*pidl)) };
            }
        }
    }

    /// The shell's own data object for these files, as Explorer drags them.
    pub fn data_object(paths: &[String]) -> windows::core::Result<IDataObject> {
        let pidls = Pidls::parse(paths)?;
        unsafe {
            let array = SHCreateShellItemArrayFromIDLists(&pidls.as_const())?;
            array.BindToHandler(None, &BHID_DataObject)
        }
    }

    pub fn open(paths: &[String]) {
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
        for path in paths {
            let file = HSTRING::from(path.as_str());
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                // This worker has no message loop to finish the launch on.
                fMask: SEE_MASK_NOASYNC,
                lpFile: windows::core::PCWSTR(file.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            unsafe {
                let _ = ShellExecuteExW(&mut info);
            }
        }
    }

    fn take(text: PWSTR) -> String {
        let out = unsafe { text.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(text.0 as _)) };
        out
    }

    /// Recommended handlers for a file's extension; folders have none.
    fn handlers_for(path: &str) -> Vec<IAssocHandler> {
        let path = Path::new(path);
        let Some(ext) = path.extension().filter(|_| !path.is_dir()) else {
            return Vec::new();
        };
        let ext = HSTRING::from(format!(".{}", ext.to_string_lossy()));
        let Ok(list) = (unsafe { SHAssocEnumHandlers(&ext, ASSOC_FILTER_RECOMMENDED) }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        loop {
            let mut slot = [None];
            let mut fetched = 0u32;
            if unsafe { list.Next(&mut slot, Some(&mut fetched)) }.is_err() || fetched == 0 {
                break;
            }
            out.extend(slot[0].take());
        }
        out
    }

    /// (id, name, where its icon comes from) for the apps all files share.
    pub fn handlers(paths: &[String]) -> Vec<(String, String, String)> {
        let mut common: Option<Vec<(String, String, String)>> = None;
        for path in paths {
            let mut here = Vec::new();
            for handler in handlers_for(path) {
                let Ok(id) = (unsafe { handler.GetName() }).map(take) else {
                    continue;
                };
                let name = unsafe { handler.GetUIName() }
                    .map(take)
                    .unwrap_or_else(|_| id.clone());
                // A desktop app is named by its exe; a Store app by its
                // AppUserModelID, whose icon the Apps folder knows.
                let icon = if Path::new(&id).is_absolute() {
                    id.clone()
                } else {
                    format!(r"shell:AppsFolder\{id}")
                };
                here.push((id, name, icon));
            }
            common = Some(match common {
                None => here,
                Some(previous) => {
                    let ids: HashSet<&String> = here.iter().map(|h| &h.0).collect();
                    previous
                        .into_iter()
                        .filter(|h| ids.contains(&h.0))
                        .collect()
                }
            });
        }
        let mut list = common.unwrap_or_default();
        let mut seen = HashSet::new();
        list.retain(|h| seen.insert(h.0.clone()));
        list.sort_by_key(|h| h.1.to_lowercase());
        list.truncate(40);
        list
    }

    pub fn open_with(paths: &[String], id: &str) {
        let Some(first) = paths.first() else { return };
        let Some(handler) = handlers_for(first).into_iter().find(|h| {
            unsafe { h.GetName() }
                .map(take)
                .is_ok_and(|name| name == id)
        }) else {
            return;
        };
        let Ok(data) = data_object(paths) else { return };
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
            let _ = handler.Invoke(&data);
        }
    }

    /// One Explorer window per folder, with that folder's files selected.
    pub fn reveal(paths: &[String]) {
        let mut folders: Vec<(String, Vec<String>)> = Vec::new();
        for path in paths {
            let parent = Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned());
            let parent = parent
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| path.clone());
            match folders.iter_mut().find(|(f, _)| *f == parent) {
                Some((_, files)) => files.push(path.clone()),
                None => folders.push((parent, vec![path.clone()])),
            }
        }
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
        for (folder, files) in folders {
            let (Ok(folder), Ok(files)) = (Pidls::parse(&[folder]), Pidls::parse(&files)) else {
                continue;
            };
            unsafe {
                let _ = SHOpenFolderAndSelectItems(folder.0[0], Some(&files.as_const()), 0);
            }
        }
    }

    thread_local! {
        /// The island's share registration; a new share replaces the last.
        static SHARE: std::cell::RefCell<Option<(DataTransferManager, i64)>> = const { std::cell::RefCell::new(None) };
    }

    /// On the island's thread. The files are read when the sheet asks for them.
    pub fn share(hwnd: isize, paths: Vec<String>) -> windows::core::Result<()> {
        let hwnd = HWND(hwnd as _);
        let interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
        let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
        SHARE.with(|slot| {
            if let Some((old, token)) = slot.borrow_mut().take() {
                let _ = old.RemoveDataRequested(token);
            }
        });
        // Titled like a pile: the first name, and how many more.
        let first = paths
            .first()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = match paths.len() {
            0 | 1 => first,
            n => format!("{first} +{}", n - 1),
        };
        let token = manager.DataRequested(&TypedEventHandler::<
            DataTransferManager,
            DataRequestedEventArgs,
        >::new(move |_, args| {
            let Some(args) = args.as_ref() else {
                return Ok(());
            };
            let data = args.Request()?.Data()?;
            data.Properties()?
                .SetTitle(&HSTRING::from(title.as_str()))?;
            let items: Vec<Option<IStorageItem>> = paths
                .iter()
                .filter_map(|p| {
                    StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_str()))
                        .ok()?
                        .join()
                        .ok()
                })
                .filter_map(|f| f.cast::<IStorageItem>().ok())
                .map(Some)
                .collect();
            data.SetStorageItemsReadOnly(&IIterable::<IStorageItem>::from(items))
        }))?;
        SHARE.with(|slot| *slot.borrow_mut() = Some((manager, token)));
        unsafe { interop.ShowShareUIForWindow(hwnd) }
    }
}
