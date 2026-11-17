// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The file actions of the tile menu and the downloads list on Linux, where
//! Windows has the Shell: GIO opens files with their default app, lists the apps
//! that can open them, and moves a file to the trash; the file manager is asked
//! to show a file through the `org.freedesktop.FileManager1` service.
//! (There is no Share sheet on Linux.)

use std::collections::HashSet;
use std::path::PathBuf;

use gtk::gio::{self, prelude::*};
use zbus::blocking::{Connection, Proxy, connection::Builder};

fn uri(path: &str) -> String {
    gio::File::for_path(path).uri().to_string()
}

/// Opens each file with the app the desktop has for it.
pub fn open(paths: &[String]) {
    for path in paths {
        let _ = gio::AppInfo::launch_default_for_uri(&uri(path), gio::AppLaunchContext::NONE);
    }
}

fn content_type(path: &str) -> Option<String> {
    gio::File::for_path(path)
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?
        .content_type()
        .map(|t| t.to_string())
}

/// A desktop file's id without its one `.desktop`: Telegram's file is
/// `org.telegram.desktop.desktop`, so stripping repeatedly would eat its name.
fn entry_name(file_id: &str) -> &str {
    file_id.strip_suffix(".desktop").unwrap_or(file_id)
}

/// (id, name, where its icon comes from) of the apps that can open every file,
/// by name, at most 40.
pub fn handlers(paths: &[String]) -> Vec<(String, String, String)> {
    let mut common: Option<Vec<(String, String, String)>> = None;
    for path in paths {
        let Some(kind) = content_type(path) else {
            return Vec::new();
        };
        let here: Vec<(String, String, String)> = gio::AppInfo::all_for_type(&kind)
            .into_iter()
            .filter(|info| info.should_show())
            .filter_map(|info| {
                let id = entry_name(&info.id()?).to_string();
                // A themed icon names itself by the first of its names the theme has.
                let icon = info
                    .icon()
                    .and_then(|icon| icon.downcast::<gio::ThemedIcon>().ok())
                    .and_then(|themed| themed.names().first().map(|name| format!("icon:{name}")))
                    .unwrap_or_default();
                Some((id, info.name().to_string(), icon))
            })
            .collect();
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

/// Opens the files with the app `id` (a desktop entry's name), if that app is one of
/// the apps that can open them.
pub fn open_with(paths: &[String], id: &str) {
    if paths.is_empty() || !handlers(paths).iter().any(|h| h.0 == id) {
        return;
    }
    let Some(app) = gio::DesktopAppInfo::new(&format!("{id}.desktop")) else {
        return;
    };
    let uris: Vec<String> = paths.iter().map(|p| uri(p)).collect();
    let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
    let _ = app.launch_uris(&refs, gio::AppLaunchContext::NONE);
}

/// Asks the file manager to show the files, selected, in their folders.
fn show_items(connection: &Connection, uris: &[String]) -> bool {
    Proxy::new(
        connection,
        "org.freedesktop.FileManager1",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
    )
    .and_then(|proxy| proxy.call::<_, _, ()>("ShowItems", &(uris, "")))
    .is_ok()
}

/// Shows the files in the file manager; where there is none to ask, their
/// folder is opened.
pub fn reveal(paths: &[String]) {
    let uris: Vec<String> = paths.iter().map(|p| uri(p)).collect();
    if let Ok(connection) = Builder::session().and_then(|b| b.build())
        && show_items(&connection, &uris)
    {
        return;
    }
    let folders: HashSet<PathBuf> = paths
        .iter()
        .filter_map(|p| std::path::Path::new(p).parent().map(PathBuf::from))
        .collect();
    for folder in folders {
        let _ = gio::AppInfo::launch_default_for_uri(
            &gio::File::for_path(folder).uri(),
            gio::AppLaunchContext::NONE,
        );
    }
}

/// Moves the file to the trash, from where it can be put back.
pub fn recycle(path: &str) -> Result<(), String> {
    gio::File::for_path(path)
        .trash(gio::Cancellable::NONE)
        .map_err(|e| e.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::testbus::PrivateBus;
    use std::sync::{Arc, Mutex};

    /// The (uris, startup id) of each call it was asked to show.
    type Calls = Arc<Mutex<Vec<(Vec<String>, String)>>>;

    struct FileManager(Calls);

    #[zbus::interface(name = "org.freedesktop.FileManager1")]
    impl FileManager {
        fn show_items(&self, uris: Vec<String>, startup: String) {
            self.0.lock().unwrap().push((uris, startup));
        }
    }

    #[test]
    fn the_file_manager_is_asked_to_show_the_files_by_uri() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipped: dbus-daemon is not installed");
            return;
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let client = bus.connect();
        // No file manager on the bus: nothing to ask.
        assert!(!show_items(&client, &["file:///a".into()]));
        let _manager = bus.serve("org.freedesktop.FileManager1", |b| {
            b.serve_at("/org/freedesktop/FileManager1", FileManager(seen.clone()))
                .unwrap()
        });
        assert!(show_items(
            &client,
            &[uri("/tmp/a b.txt"), uri("/tmp/c.txt")]
        ));
        assert_eq!(
            *seen.lock().unwrap(),
            [(
                vec![
                    "file:///tmp/a%20b.txt".to_string(),
                    "file:///tmp/c.txt".to_string()
                ],
                String::new()
            )]
        );
    }

    #[test]
    fn only_one_desktop_suffix_is_cut_from_an_apps_id() {
        assert_eq!(
            entry_name("org.telegram.desktop.desktop"),
            "org.telegram.desktop"
        );
        assert_eq!(entry_name("firefox.desktop"), "firefox");
        assert_eq!(entry_name("no-suffix"), "no-suffix");
    }

    #[test]
    fn the_apps_for_files_are_those_that_can_open_every_one() {
        let dir = std::env::temp_dir().join(format!("openbar-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        std::fs::write(&a, "x").unwrap();
        std::fs::write(&b, "y").unwrap();
        let one = handlers(&[a.to_string_lossy().into_owned()]);
        let two = handlers(&[
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
        ]);
        // Same type, same apps; sorted by name, none repeated.
        assert_eq!(one, two);
        let names: Vec<String> = one.iter().map(|h| h.1.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        let ids: HashSet<&String> = one.iter().map(|h| &h.0).collect();
        assert_eq!(ids.len(), one.len());
        // A file that is not there has no apps; nothing to open with, so nothing launches.
        assert!(handlers(&["/nonexistent/file.txt".into()]).is_empty());
        open_with(&[a.to_string_lossy().into_owned()], "no-such-app");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "moves a file to the trash of XDG_DATA_HOME: run with it set to a new folder named openbar-trash-test under the home folder"]
    fn a_file_is_moved_to_the_trash_and_a_missing_one_is_an_error() {
        // GIO trashes into the home trash only what is on the home folder's disk, and refuses
        // the temp folder (a tmpfs), so the test's own data folder lives under the home folder.
        let data = std::env::var("XDG_DATA_HOME").expect("XDG_DATA_HOME");
        assert!(
            data.contains("openbar-trash-test"),
            "refusing to use {data}: the trash must be one of the test's own"
        );
        let file = std::path::Path::new(&data).join("to-recycle.txt");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(&file, "bye").unwrap();
        recycle(&file.to_string_lossy()).unwrap();
        assert!(!file.exists());
        let trashed = std::path::Path::new(&data).join("Trash/files/to-recycle.txt");
        assert!(trashed.is_file(), "not in {}", trashed.display());
        assert!(recycle("/nonexistent/openbar/file").is_err());
    }
}
