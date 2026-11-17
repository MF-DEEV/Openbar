// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Desktop entry names. A program names its own entry without the `.desktop`
//! extension, as the specifications say, but a few add it, and an entry may
//! itself end in `.desktop` (`org.telegram.desktop`), so the extension is not
//! simply cut: what is installed decides.

use gtk::gio;

/// `entry` as the installed entries know it: the name as given, unless it
/// ends in `.desktop` and only the name without it is installed.
fn choose(entry: &str, known: impl Fn(&str) -> bool) -> String {
    match entry.strip_suffix(".desktop") {
        Some(stripped) if !known(entry) && known(stripped) => stripped.to_string(),
        _ => entry.to_string(),
    }
}

pub fn entry_id(entry: &str) -> String {
    choose(entry.trim(), |id| {
        gio::DesktopAppInfo::new(&format!("{id}.desktop")).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_cut_only_when_the_name_without_it_is_the_one_installed() {
        let installed = |names: &'static [&'static str]| move |id: &str| names.contains(&id);
        // A program that adds the extension.
        assert_eq!(
            choose("rhythmbox.desktop", installed(&["rhythmbox"])),
            "rhythmbox"
        );
        // A name that ends in it, as Telegram's does.
        assert_eq!(
            choose("org.telegram.desktop", installed(&["org.telegram.desktop"])),
            "org.telegram.desktop"
        );
        // Both unknown: as given.
        assert_eq!(choose("thing.desktop", installed(&[])), "thing.desktop");
        assert_eq!(choose("plain", installed(&["plain"])), "plain");
        // Never more than one extension.
        assert_eq!(
            choose(
                "org.telegram.desktop.desktop",
                installed(&["org.telegram.desktop"])
            ),
            "org.telegram.desktop"
        );
        assert_eq!(choose("", installed(&[])), "");
    }
}
