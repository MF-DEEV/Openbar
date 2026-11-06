// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The per-user folders the app keeps files in: `%APPDATA%` and `%LOCALAPPDATA%`
//! on Windows, the XDG folders elsewhere (where Tauri puts its own config folder too).

use std::path::PathBuf;

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// `%APPDATA%`, or `$XDG_CONFIG_HOME` (`~/.config`).
pub fn roaming() -> Option<PathBuf> {
    #[cfg(windows)]
    return env_dir("APPDATA");
    #[cfg(not(windows))]
    return xdg("XDG_CONFIG_HOME", ".config");
}

/// `%LOCALAPPDATA%`, or `$XDG_CACHE_HOME` (`~/.cache`): for what is safe to delete.
pub fn cache() -> Option<PathBuf> {
    #[cfg(windows)]
    return env_dir("LOCALAPPDATA");
    #[cfg(not(windows))]
    return xdg("XDG_CACHE_HOME", ".cache");
}

#[cfg(not(windows))]
fn xdg(variable: &str, fallback: &str) -> Option<PathBuf> {
    env_dir(variable).or_else(|| Some(env_dir("HOME")?.join(fallback)))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn an_xdg_folder_must_be_absolute_and_falls_back_to_home() {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        // Only the fallback rule is checked: the process environment is shared between tests.
        assert_eq!(
            xdg("OPENBAR_NO_SUCH_VARIABLE", ".config"),
            home.map(|h| h.join(".config"))
        );
    }
}
