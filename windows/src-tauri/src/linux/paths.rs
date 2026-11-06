// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The per-user folders on Linux: the XDG ones, where Tauri puts its own config folder too.
//! Windows keeps the same things under `%APPDATA%` and `%LOCALAPPDATA%`.

use std::path::PathBuf;

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// `$XDG_CONFIG_HOME` (`~/.config`), where Windows has `%APPDATA%`.
pub fn roaming() -> Option<PathBuf> {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `$XDG_CACHE_HOME` (`~/.cache`), where Windows has `%LOCALAPPDATA%`: for what is safe to delete.
pub fn cache() -> Option<PathBuf> {
    xdg("XDG_CACHE_HOME", ".cache")
}

/// `$XDG_DATA_HOME` (`~/.local/share`).
pub fn data() -> Option<PathBuf> {
    xdg("XDG_DATA_HOME", ".local/share")
}

fn xdg(variable: &str, fallback: &str) -> Option<PathBuf> {
    env_dir(variable).or_else(|| Some(env_dir("HOME")?.join(fallback)))
}

#[cfg(test)]
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
