// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Carries an install over from the time the island shipped as Vorssaint
//! (`com.vorssaint.island`, `%LOCALAPPDATA%\Vorssaint\Vorssaint.exe`). Openbar
//! has its own id now, so its settings folder, hooks and Run value are new:
//! the old ones are brought over once, by the installer through
//! `Openbar.exe --migrate-legacy` or else on the first start.
//!
//! Hooks are what can break: the old ones run `Vorssaint.exe`, which the
//! installer takes away. Wherever the old app had hooks, Openbar's go in, and
//! the old ones come out once the old exe is gone.

use std::path::{Path, PathBuf};

use crate::hooks::{self, installer as claude};
use crate::sessions::hook_agents::installer as agents;
use crate::sessions::models::Provider;

pub const LEGACY_APP_ID: &str = "com.vorssaint.island";

/// Written into the new settings folder once the migration has run.
const MIGRATED: &str = ".legacy-migrated";

pub fn is_legacy(app: &str) -> bool {
    app.starts_with("com.vorssaint.")
}

/// `%LOCALAPPDATA%\Vorssaint\Vorssaint.exe`, where the old installer put it.
fn legacy_exe() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("Vorssaint").join("Vorssaint.exe"))
}

/// Copies `<appdata>\com.vorssaint.island` to `<appdata>\com.openbar.island`
/// when the new one does not exist yet. Built beside it and renamed in whole,
/// so a copy cut short never looks finished.
pub fn migrate_data(appdata: &Path, current: &str) -> bool {
    let source = appdata.join(LEGACY_APP_ID);
    let target = appdata.join(current);
    if !source.is_dir() || target.exists() {
        return false;
    }
    let staging = appdata.join(format!(".{current}-migrating-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let copied = copy_tree(&source, &staging).is_ok() && std::fs::rename(&staging, &target).is_ok();
    let _ = std::fs::remove_dir_all(&staging);
    copied
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let destination = to.join(entry.file_name());
        if kind.is_symlink() {
            continue;
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &destination)?;
        }
    }
    Ok(())
}

/// What the hook migration changed, for the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub claude: bool,
    pub agents: Vec<Provider>,
}

/// Puts Openbar's hooks wherever the old app had them, and with
/// `remove_legacy` takes the old ones out.
pub fn migrate_hooks(
    settings: &Path,
    home: &Path,
    exe: &Path,
    endpoint: &str,
    current: &str,
    remove_legacy: bool,
) -> Moved {
    let mut moved = Moved::default();
    if claude::read(settings).is_ok_and(|s| claude::has_any(&s, LEGACY_APP_ID)) {
        let command = claude::command(exe, current);
        moved.claude = claude::update(settings, |s| {
            let mut s = if remove_legacy {
                claude::removing(s, LEGACY_APP_ID)
            } else {
                s.clone()
            };
            if !claude::is_installed(&s, &command) {
                s = claude::installing(&s, &command, current);
            }
            s
        })
        .is_ok();
    }
    for provider in Provider::HOOK_AGENTS {
        let old = agents::state(provider, home, exe, endpoint, LEGACY_APP_ID);
        if !matches!(old, agents::State::Installed | agents::State::Outdated) {
            continue;
        }
        if agents::state(provider, home, exe, endpoint, current) != agents::State::Installed
            && agents::install(provider, home, exe, endpoint, current).is_err()
        {
            continue;
        }
        if remove_legacy {
            let _ = agents::remove(provider, home, LEGACY_APP_ID);
        }
        moved.agents.push(provider);
    }
    moved
}

/// Everything, for this exe and this user. `remove_legacy` is for when the
/// old app is on its way out (the installer) or already gone.
fn migrate(remove_legacy: bool) {
    let current = hooks::APP_ID;
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        migrate_data(&appdata, current);
        let _ = std::fs::create_dir_all(appdata.join(current));
        let _ = std::fs::write(appdata.join(current).join(MIGRATED), b"");
    }
    if let (Some(settings), Some(home), Ok(exe)) = (
        claude::settings_path(),
        claude::home_dir(),
        std::env::current_exe(),
    ) {
        migrate_hooks(
            &settings,
            &home,
            &exe,
            &hooks::transport::endpoint(current),
            current,
            remove_legacy,
        );
    }
    if crate::prefs::legacy_launch_at_login() {
        let _ = crate::prefs::launch_at_login_set(true);
        if remove_legacy {
            crate::prefs::remove_legacy_launch_at_login();
        }
    }
    if remove_legacy {
        crate::hosts::bridge_installer::replace_legacy();
    }
}

/// `Openbar.exe --migrate-legacy`: run by the installer before it removes the
/// old app, whose uninstaller then finds nothing of its own left to take.
pub fn migrate_main() -> ! {
    migrate(true);
    std::process::exit(0)
}

/// On the first start, for an Openbar put in place without the installer.
pub fn migrate_at_start() {
    let done = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(hooks::APP_ID).join(MIGRATED))
        .is_some_and(|marker| marker.exists());
    if done {
        return;
    }
    let gone = legacy_exe().is_none_or(|exe| !exe.exists());
    migrate(gone);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openbar-legacy-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_old_settings_folder_is_copied_once() {
        let appdata = temp("data");
        std::fs::create_dir_all(appdata.join(LEGACY_APP_ID).join("shelf")).unwrap();
        std::fs::write(
            appdata.join(LEGACY_APP_ID).join("settings.json"),
            b"{\"a\":1}",
        )
        .unwrap();
        std::fs::write(
            appdata.join(LEGACY_APP_ID).join("shelf").join("x.png"),
            b"x",
        )
        .unwrap();
        assert!(migrate_data(&appdata, "com.openbar.island"));
        let new = appdata.join("com.openbar.island");
        assert_eq!(
            std::fs::read(new.join("settings.json")).unwrap(),
            b"{\"a\":1}"
        );
        assert_eq!(
            std::fs::read(new.join("shelf").join("x.png")).unwrap(),
            b"x"
        );
        assert!(appdata.join(LEGACY_APP_ID).join("settings.json").exists());
        std::fs::write(appdata.join(LEGACY_APP_ID).join("later.json"), b"").unwrap();
        assert!(!migrate_data(&appdata, "com.openbar.island"));
        assert!(!new.join("later.json").exists());
        let _ = std::fs::remove_dir_all(&appdata);
    }

    #[test]
    fn old_hooks_are_found_by_their_old_marker() {
        assert_eq!(
            claude::marker(LEGACY_APP_ID),
            "--marker vorssaint-hook --app com.vorssaint.island"
        );
        assert_eq!(
            claude::marker("com.openbar.island"),
            "--marker openbar-hook --app com.openbar.island"
        );
        assert_eq!(
            agents::own_name(LEGACY_APP_ID),
            "vorssaint-com.vorssaint.island"
        );
    }

    #[test]
    fn claude_hooks_move_to_openbar() {
        let home = temp("hooks");
        let settings = home.join("settings.json");
        let old_exe = Path::new("C:/Users/x/AppData/Local/Vorssaint/Vorssaint.exe");
        let new_exe = Path::new("C:/Users/x/AppData/Local/Openbar/Openbar.exe");
        let old = claude::installing(
            &serde_json::json!({"model": "opus"}),
            &claude::command(old_exe, LEGACY_APP_ID),
            LEGACY_APP_ID,
        );
        std::fs::write(&settings, serde_json::to_vec(&old).unwrap()).unwrap();
        let moved = migrate_hooks(
            &settings,
            &home,
            new_exe,
            "pipe",
            "com.openbar.island",
            false,
        );
        let read = claude::read(&settings).unwrap();
        assert!(moved.claude);
        assert!(claude::is_installed(
            &read,
            &claude::command(new_exe, "com.openbar.island")
        ));
        assert!(
            claude::has_any(&read, LEGACY_APP_ID),
            "kept while the old app is there"
        );
        migrate_hooks(
            &settings,
            &home,
            new_exe,
            "pipe",
            "com.openbar.island",
            true,
        );
        let read = claude::read(&settings).unwrap();
        assert!(!claude::has_any(&read, LEGACY_APP_ID));
        assert!(claude::is_installed(
            &read,
            &claude::command(new_exe, "com.openbar.island")
        ));
        assert_eq!(read["model"], "opus");
        let _ = std::fs::remove_dir_all(&home);
    }
}
