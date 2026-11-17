// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The folders the downloads list watches on Linux: the desktop's Downloads
//! folder and the one each browser is set to save in, and the History file of
//! each Chromium profile, where the browser keeps what it is downloading.
//! Windows finds the same places through the Known Folders API and
//! `%LOCALAPPDATA%`.

use std::path::{Path, PathBuf};

use gtk::glib::{self, UserDirectory};

/// Chromium based browsers keep one folder per profile under their config
/// folder, each with a `Preferences` file; the second name is how the list
/// says where a download comes from.
const CHROMIUM: [(&str, &str); 9] = [
    ("google-chrome", "Chrome"),
    ("google-chrome-beta", "Chrome"),
    ("google-chrome-unstable", "Chrome"),
    ("chromium", "Chromium"),
    ("BraveSoftware/Brave-Browser", "Brave"),
    ("microsoft-edge", "Edge"),
    ("vivaldi", "Vivaldi"),
    ("opera", "Opera"),
    ("opera-gx", "Opera GX"),
];

/// The save folder a Chromium profile is set to, from its `Preferences` file.
fn chromium_folder(preferences: &[u8]) -> Option<PathBuf> {
    let value: serde_json::Value = serde_json::from_slice(preferences).ok()?;
    Some(
        value
            .pointer("/download/default_directory")?
            .as_str()?
            .into(),
    )
}

/// The save folder in a Firefox `prefs.js`, when it was moved off the default.
fn firefox_folder(prefs: &str) -> Option<PathBuf> {
    prefs
        .lines()
        .filter(|line| line.starts_with("user_pref(\"browser.download.dir\","))
        .find_map(|line| {
            let (_, value) = line.split_once(',')?;
            serde_json::from_str::<String>(value.trim().trim_end_matches(");"))
                .ok()
                .map(PathBuf::from)
        })
}

/// The profile folders of a browser: its own folder and each one inside it.
fn profiles(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root) {
        found.extend(
            entries
                .flatten()
                .take(128)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| entry.path()),
        );
    }
    found
}

/// What the Chromium browsers under `config` (`~/.config`) save to.
fn chromium_folders(config: &Path) -> Vec<PathBuf> {
    CHROMIUM
        .iter()
        .flat_map(|(dir, _)| profiles(&config.join(dir)))
        .filter_map(|profile| chromium_folder(&std::fs::read(profile.join("Preferences")).ok()?))
        .collect()
}

/// The History file of each Chromium profile under `config`, with its browser's name.
fn chromium_histories(config: &Path) -> Vec<(PathBuf, String)> {
    CHROMIUM
        .iter()
        .flat_map(|(dir, name)| {
            profiles(&config.join(dir))
                .into_iter()
                .filter(|profile| profile.join("Preferences").is_file())
                .map(|profile| profile.join("History"))
                .filter(|history| history.is_file())
                .map(move |history| (history, (*name).to_string()))
        })
        .collect()
}

/// What the Firefox profiles under `roots` save to. A profile that never moved
/// it has no entry and saves to the desktop's Downloads folder.
fn firefox_folders(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| profiles(root))
        .filter_map(|profile| {
            firefox_folder(&std::fs::read_to_string(profile.join("prefs.js")).ok()?)
        })
        .collect()
}

/// The places downloads land, in the order they are worth listing. The caller
/// keeps the ones that exist.
pub fn folders() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut found: Vec<PathBuf> = glib::user_special_dir(UserDirectory::Downloads)
        .into_iter()
        .collect();
    found.extend(home.as_ref().map(|home| home.join("Downloads")));
    if let Some(config) = super::paths::roaming() {
        found.extend(chromium_folders(&config));
        // Firefox moved its profiles from `~/.mozilla` into the config folder.
        let mut roots = vec![config.join("mozilla/firefox")];
        roots.extend(home.map(|home| home.join(".mozilla/firefox")));
        found.extend(firefox_folders(&roots));
    }
    found
}

/// The databases the downloads of the Chromium browsers are read from, as Windows
/// finds them under `%LOCALAPPDATA%`.
pub fn histories() -> Vec<(PathBuf, String)> {
    super::paths::roaming()
        .map(|config| chromium_histories(&config))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openbar-downloads-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_chromium_profile_names_the_folder_it_saves_to() {
        let moved =
            br#"{"download":{"default_directory":"/home/me/Fetched","prompt_for_download":false}}"#;
        assert_eq!(
            chromium_folder(moved),
            Some(PathBuf::from("/home/me/Fetched"))
        );
        assert_eq!(chromium_folder(br#"{"download":{}}"#), None);
        assert_eq!(chromium_folder(b"not json"), None);
    }

    #[test]
    fn firefox_names_its_folder_only_when_it_was_moved() {
        let prefs = "user_pref(\"browser.download.dir\", \"/home/me/Fetched\");\n\
                     user_pref(\"browser.download.folderList\", 2);\n";
        assert_eq!(
            firefox_folder(prefs),
            Some(PathBuf::from("/home/me/Fetched"))
        );
        assert_eq!(
            firefox_folder("user_pref(\"browser.download.folderList\", 1);"),
            None
        );
        // A quote inside the path is escaped the way JSON escapes it.
        let quoted = r#"user_pref("browser.download.dir", "/home/me/a \"b\"");"#;
        assert_eq!(
            firefox_folder(quoted),
            Some(PathBuf::from("/home/me/a \"b\""))
        );
    }

    #[test]
    fn every_profile_of_every_browser_is_read() {
        let config = scratch("chromium");
        let chrome = config.join("google-chrome");
        for (profile, folder) in [("Default", "/data/one"), ("Profile 2", "/data/two")] {
            std::fs::create_dir_all(chrome.join(profile)).unwrap();
            std::fs::write(
                chrome.join(profile).join("Preferences"),
                format!(r#"{{"download":{{"default_directory":"{folder}"}}}}"#),
            )
            .unwrap();
        }
        // A profile without the setting and a folder that is no profile add nothing.
        std::fs::create_dir_all(chrome.join("Guest Profile")).unwrap();
        std::fs::write(chrome.join("Guest Profile/Preferences"), "{}").unwrap();
        std::fs::create_dir_all(chrome.join("ShaderCache")).unwrap();

        let mut found = chromium_folders(&config);
        found.sort();
        assert_eq!(
            found,
            [PathBuf::from("/data/one"), PathBuf::from("/data/two")]
        );
        let _ = std::fs::remove_dir_all(&config);
    }

    #[test]
    fn the_history_of_every_chromium_profile_is_found_under_its_browsers_name() {
        let config = scratch("histories");
        for (dir, profile, history) in [
            ("google-chrome", "Default", true),
            ("google-chrome", "Profile 2", true),
            // A profile that never had a download has no History yet.
            ("google-chrome", "Profile 3", false),
            ("BraveSoftware/Brave-Browser", "Default", true),
            // Opera keeps its profile in its own folder.
            ("opera", "", true),
        ] {
            let folder = config.join(dir).join(profile);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("Preferences"), "{}").unwrap();
            if history {
                std::fs::write(folder.join("History"), "").unwrap();
            }
        }
        // A History without Preferences is no profile of a browser, nor is a folder like ShaderCache.
        std::fs::create_dir_all(config.join("chromium/Stray")).unwrap();
        std::fs::write(config.join("chromium/Stray/History"), "").unwrap();
        std::fs::create_dir_all(config.join("google-chrome/ShaderCache")).unwrap();

        let mut found = chromium_histories(&config);
        found.sort();
        let at = |path: &str, name: &str| (config.join(path), name.to_string());
        assert_eq!(
            found,
            [
                at("BraveSoftware/Brave-Browser/Default/History", "Brave"),
                at("google-chrome/Default/History", "Chrome"),
                at("google-chrome/Profile 2/History", "Chrome"),
                at("opera/History", "Opera"),
            ]
        );
        assert!(chromium_histories(&config.join("none")).is_empty());
        let _ = std::fs::remove_dir_all(&config);
    }

    #[test]
    fn firefox_profiles_are_read_from_either_place() {
        let roots = [scratch("ff-config"), scratch("ff-home")];
        for (root, folder) in roots.iter().zip(["/data/one", "/data/two"]) {
            let profile = root.join("abcd.default-release");
            std::fs::create_dir_all(&profile).unwrap();
            std::fs::write(
                profile.join("prefs.js"),
                format!("user_pref(\"browser.download.dir\", \"{folder}\");\n"),
            )
            .unwrap();
        }
        assert_eq!(
            firefox_folders(&roots),
            [PathBuf::from("/data/one"), PathBuf::from("/data/two")]
        );
        // A root that does not exist is not an error.
        assert!(firefox_folders(&[PathBuf::from("/nonexistent/openbar")]).is_empty());
        for root in roots {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn the_desktops_downloads_folder_comes_first() {
        if let Some(dir) = glib::user_special_dir(UserDirectory::Downloads) {
            assert_eq!(folders().first(), Some(&dir));
        }
    }
}
