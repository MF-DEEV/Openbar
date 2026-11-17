// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Spotify's exposed queue, read on demand through Windows accessibility.
//! No account token, browser history, network request or playback command.
use serde::Serialize;
pub mod artwork;
#[cfg(any(windows, test))]
#[path = "music_queue/selection.rs"]
mod selection;

#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub source: String,
    pub title: String,
    pub status: &'static str,
    pub items: Vec<Item>,
    pub prefetch: Vec<Item>,
}

pub fn read(source: &str, title: &str, open: bool) -> Snapshot {
    let mut snapshot = Snapshot {
        source: source.into(),
        title: title.into(),
        status: "unsupported",
        items: vec![],
        prefetch: vec![],
    };
    if !source.to_ascii_lowercase().contains("spotify") {
        return snapshot;
    }
    snapshot.status = "unavailable";
    #[cfg(windows)]
    if let Some(mut items) = native::read(title, open) {
        if items.len() > 5 {
            snapshot.prefetch = items.split_off(5);
        }
        snapshot.items = items;
        snapshot.status = "ready";
    }
    #[cfg(not(windows))]
    let _ = open;
    snapshot
}

#[tauri::command]
pub async fn music_queue(
    source: String,
    title: String,
    open: bool,
    state: tauri::State<'_, crate::media::Shared>,
) -> Result<Snapshot, String> {
    fn matches(state: &crate::media::Shared, source: &str, title: &str) -> bool {
        state
            .lock()
            .unwrap()
            .now
            .as_ref()
            .and_then(|now| now.playback.as_ref())
            .is_some_and(|p| p.source == source && p.title == title)
    }
    let state = state.inner().clone();
    if !matches(&state, &source, &title) {
        return Err("Player changed".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        static READING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = READING.try_lock().map_err(|_| "Queue read in progress")?;
        let snapshot = read(&source, &title, open);
        if !matches(&state, &source, &title) {
            return Err("Player changed");
        }
        Ok(snapshot)
    })
    .await
    .map_err(|e| e.to_string())?;
    result.map_err(String::from)
}

#[cfg(windows)]
mod native {
    use super::Item;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETOBJECT,
    };
    use windows::core::{BOOL, Interface};

    struct Windows {
        pids: Vec<u32>,
        handles: Vec<HWND>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, context: LPARAM) -> BOOL {
        unsafe {
            let context = &mut *(context.0 as *mut Windows);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if context.pids.contains(&pid) {
                context.handles.push(hwnd);
            }
        }
        true.into()
    }

    pub fn read(title: &str, open: bool) -> Option<Vec<Item>> {
        let _apartment = crate::com::Apartment::mta().ok()?;
        let mut context = Windows {
            pids: crate::hosts::process::pids_named("Spotify.exe"),
            handles: vec![],
        };
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut context as *mut Windows as isize));
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
            if let Ok(v2) = automation.cast::<IUIAutomation2>() {
                let _ = v2.SetConnectionTimeout(300);
                let _ = v2.SetTransactionTimeout(300);
            }
            for hwnd in context.handles {
                // Chromium documents this accessibility handshake. It also works
                // while Spotify is minimized; never restore or focus its window.
                let mut result = 0;
                let _ = SendMessageTimeoutW(
                    hwnd,
                    WM_GETOBJECT,
                    WPARAM(0),
                    LPARAM(1),
                    SMTO_ABORTIFHUNG,
                    300,
                    Some(&mut result),
                );
                // Request the documented MSAA client object as well: some
                // Chromium builds initially expose only the window chrome to
                // UIA until a client asks for the document's accessible object.
                let mut accessible = std::ptr::null_mut();
                if AccessibleObjectFromWindow(
                    hwnd,
                    (-4_i32) as u32, // OBJID_CLIENT
                    &IAccessible::IID,
                    &mut accessible,
                )
                .is_ok()
                    && !accessible.is_null()
                {
                    drop(IAccessible::from_raw(accessible));
                }
                if open {
                    std::thread::sleep(std::time::Duration::from_millis(160));
                }
                let Ok(root) = automation.ElementFromHandle(hwnd) else {
                    continue;
                };
                if let Some(items) = panel(&automation, &root, title) {
                    return Some(items);
                }
                if !open {
                    continue;
                }
                let pane_condition = automation
                    .CreatePropertyCondition(UIA_AutomationIdPropertyId, &"queue-panel".into())
                    .ok()?;
                // Never toggle an already open queue closed just because its
                // cached song is too old or ambiguous to align safely.
                if root
                    .FindFirst(TreeScope_Descendants, &pane_condition)
                    .is_ok()
                {
                    continue;
                }
                let buttons = automation
                    .CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &UIA_ButtonControlTypeId.0.into(),
                    )
                    .ok()?;
                for name in ["Queue", "Kolejka"] {
                    let named = automation
                        .CreatePropertyCondition(UIA_NamePropertyId, &name.into())
                        .ok()?;
                    let condition = automation.CreateAndCondition(&buttons, &named).ok()?;
                    let Ok(button) = root.FindFirst(TreeScope_Descendants, &condition) else {
                        continue;
                    };
                    let invoked = button
                        .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                        .and_then(|p| p.Invoke())
                        .is_ok()
                        || button
                            .GetCurrentPatternAs::<IUIAutomationLegacyIAccessiblePattern>(
                                UIA_LegacyIAccessiblePatternId,
                            )
                            .and_then(|p| p.DoDefaultAction())
                            .is_ok();
                    if invoked {
                        for _ in 0..5 {
                            std::thread::sleep(std::time::Duration::from_millis(160));
                            if let Some(items) = panel(&automation, &root, title) {
                                return Some(items);
                            }
                        }
                        return None;
                    }
                }
            }
        }
        None
    }

    unsafe fn panel(
        automation: &IUIAutomation,
        root: &IUIAutomationElement,
        title: &str,
    ) -> Option<Vec<Item>> {
        unsafe {
            let condition = automation
                .CreatePropertyCondition(UIA_AutomationIdPropertyId, &"queue-panel".into())
                .ok()?;
            let pane = root.FindFirst(TreeScope_Descendants, &condition).ok()?;
            let grid_condition = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &UIA_DataGridControlTypeId.0.into(),
                )
                .ok()?;
            let grids = pane.FindAll(TreeScope_Descendants, &grid_condition).ok()?;
            if grids.Length().ok()? == 0 {
                return None;
            }
            let rows_condition = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &UIA_TreeItemControlTypeId.0.into(),
                )
                .ok()?;
            let current = grids
                .GetElement(0)
                .ok()?
                .FindFirst(TreeScope_Descendants, &rows_condition)
                .ok()?
                .CurrentName()
                .ok()?
                .to_string();
            // A snapshot belongs to the currently selected song, never to an
            // old queue, another source or the playlist behind the side panel.
            let links = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &UIA_HyperlinkControlTypeId.0.into(),
                )
                .ok()?;
            let mut items = vec![Item {
                title: current,
                artist: String::new(),
            }];
            // Usually the first row already matches SMTC: only inspect the
            // five visible songs plus five covers prepared ahead of a skip.
            let limit = if items[0].title.trim() == title.trim() {
                11
            } else {
                25
            };
            for grid in 1..grids.Length().ok()?.min(8) {
                let rows = grids
                    .GetElement(grid)
                    .ok()?
                    .FindAll(TreeScope_Descendants, &rows_condition)
                    .ok()?;
                for row in 0..rows.Length().ok()?.min(24) {
                    let row = rows.GetElement(row).ok()?;
                    let title = row.CurrentName().ok()?.to_string();
                    if title.trim().is_empty() {
                        return None;
                    }
                    let artists = row.FindAll(TreeScope_Descendants, &links).ok()?;
                    let artist = (0..artists.Length().ok()?.min(12))
                        .filter_map(|i| {
                            artists
                                .GetElement(i)
                                .ok()?
                                .CurrentName()
                                .ok()
                                .map(|s| s.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    items.push(Item {
                        title: title.chars().take(1024).collect(),
                        artist: artist.chars().take(1024).collect(),
                    });
                    if items.len() == limit {
                        break;
                    }
                }
                if items.len() == limit {
                    break;
                }
            }
            let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
            let range = super::selection::upcoming_range(&titles, title)?;
            let end = items.len().min(range.start + 10);
            Some(items[range.start..end].to_vec())
        }
    }
}
