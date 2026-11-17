// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Hooks on Linux: where the hook socket lives, who may talk to it, and which
//! program a hook runs. `hooks/transport.rs` and `hooks/mod.rs` hand over to
//! this where Windows has a named pipe and `current_exe`.

use std::ffi::OsString;
use std::path::PathBuf;

/// `~/.openbar`, unless the socket in it would not fit a socket address
/// (about 108 bytes): then a folder of the same name in the temp folder.
pub fn private_dir() -> PathBuf {
    private_dir_in(std::env::var_os("HOME").map(PathBuf::from))
}

fn private_dir_in(home: Option<PathBuf>) -> PathBuf {
    // Room for "/com.openbar.island.sock" and the app ids a second build may use.
    const ROOM: usize = 72;
    let beside = |dir: PathBuf| dir.join(".openbar");
    match home {
        Some(home) if beside(home.clone()).as_os_str().len() <= ROOM => beside(home),
        _ => {
            let user = std::env::var("USER").unwrap_or_default();
            std::env::temp_dir().join(format!("openbar-{user}"))
        }
    }
}

/// The socket is 0600 in a 0700 folder; the kernel also names the caller.
pub fn same_user(stream: &interprocess::local_socket::Stream) -> bool {
    use interprocess::local_socket::traits::StreamCommon;
    stream
        .peer_creds()
        .ok()
        .and_then(|c| c.euid())
        .is_none_or(|uid| uid == unsafe { libc::geteuid() })
}

/// The program a hook should run: an AppImage's own file, since the
/// executable inside it lives in a mount that is gone when the app closes.
pub fn program(current: PathBuf) -> PathBuf {
    hook_program(std::env::var_os("APPIMAGE"), current)
}

fn hook_program(appimage: Option<OsString>, current: PathBuf) -> PathBuf {
    appimage
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_home_folder_moves_the_socket_to_the_temp_folder() {
        assert_eq!(
            private_dir_in(Some(PathBuf::from("/home/me"))),
            PathBuf::from("/home/me/.openbar")
        );
        let long = PathBuf::from(format!("/home/{}", "x".repeat(80)));
        let moved = private_dir_in(Some(long));
        assert!(moved.starts_with(std::env::temp_dir()));
        assert!(private_dir_in(None).starts_with(std::env::temp_dir()));
    }

    #[test]
    fn an_appimage_is_run_by_its_own_file_not_its_mount() {
        let file = std::env::temp_dir().join(format!("openbar-appimage-{}", std::process::id()));
        std::fs::write(&file, b"").unwrap();
        let mounted = PathBuf::from("/tmp/.mount_Openbar1/usr/bin/openbar");
        assert_eq!(
            hook_program(Some(file.clone().into_os_string()), mounted.clone()),
            file
        );
        // A variable naming nothing is ignored.
        assert_eq!(
            hook_program(
                Some("/nonexistent/Openbar.AppImage".into()),
                mounted.clone()
            ),
            mounted
        );
        assert_eq!(hook_program(None, mounted.clone()), mounted);
        let _ = std::fs::remove_file(file);
    }
}
