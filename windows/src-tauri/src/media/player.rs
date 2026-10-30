// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! SMTC sources include desktop image names as well as packaged app IDs.
//! Opening every source through Explorer can silently open Documents instead.

fn valid_source(source: &str) -> bool {
    !source.trim().is_empty()
        && !source.chars().any(char::is_control)
        && !source.contains(['"', '&', '|', '<', '>', '/', '\\', ':'])
}

fn desktop_image(source: &str) -> Option<String> {
    if !valid_source(source) || source.contains('!') {
        return None;
    }
    Some(if source.to_ascii_lowercase().ends_with(".exe") {
        source.to_string()
    } else {
        format!("{source}.exe")
    })
}

#[cfg(windows)]
pub fn open_player(source: &str) -> bool {
    use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance};
    use windows::Win32::UI::Shell::{
        AO_NOERRORUI, ApplicationActivationManager, IApplicationActivationManager,
    };
    use windows::core::PCWSTR;

    if !valid_source(source) {
        return false;
    }
    if let Some(image) = desktop_image(source) {
        let pids = crate::hosts::process::pids_named(&image);
        // Restore a minimized player or focus its existing main window.
        if pids
            .iter()
            .any(|&pid| crate::hosts::console::raise_process_window(pid))
        {
            return true;
        }
        // A tray-only player may have no visible window. Ask its existing
        // executable to show itself; never execute a path supplied by the UI.
        for pid in pids {
            if let Some(path) = crate::shell::process_path(pid)
                && std::path::Path::new(&path)
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&image))
                && std::process::Command::new(path).spawn().is_ok()
            {
                return true;
            }
        }
    }
    // Store apps such as Apple Music / packaged Spotify have an AUMID, not
    // an executable name. Activation reports failure instead of opening a folder.
    let Ok(_apartment) = crate::com::Apartment::sta() else {
        return false;
    };
    let id: Vec<u16> = source.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let Ok(activation): Result<IApplicationActivationManager, _> =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
        else {
            return false;
        };
        activation
            .ActivateApplication(PCWSTR(id.as_ptr()), PCWSTR::null(), AO_NOERRORUI)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_sources_resolve_to_exact_process_names() {
        assert_eq!(desktop_image("Spotify.exe").as_deref(), Some("Spotify.exe"));
        assert_eq!(desktop_image("SPOTIFY.EXE").as_deref(), Some("SPOTIFY.EXE"));
        assert_eq!(desktop_image("MSEdge").as_deref(), Some("MSEdge.exe"));
        assert_eq!(desktop_image("Brave").as_deref(), Some("Brave.exe"));
        assert_eq!(desktop_image("vlc.exe").as_deref(), Some("vlc.exe"));
    }

    #[test]
    fn store_ids_use_activation_instead_of_process_name_guessing() {
        for source in [
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App",
        ] {
            assert!(valid_source(source));
            assert!(desktop_image(source).is_none());
        }
    }

    #[test]
    fn paths_arguments_and_control_characters_are_rejected() {
        for source in [
            "",
            "  ",
            "C:\\Spotify.exe",
            "../Spotify.exe",
            "spotify:album:x",
            "Spotify.exe\0",
            "Spotify.exe\n",
            "\"Spotify.exe\"",
            "Spotify.exe & calc.exe",
        ] {
            assert!(!valid_source(source));
            assert!(desktop_image(source).is_none());
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires a running Spotify desktop player and an interactive desktop"]
    fn focuses_running_spotify() {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        assert!(!open_player("Vorssaint.MissingPlayer_0000000000000!App"));
        assert!(!crate::hosts::process::pids_named("Spotify.exe").is_empty());
        assert!(open_player("Spotify.exe"));
        for _ in 0..40 {
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
            }
            if crate::shell::process_path(pid)
                .is_some_and(|p| p.to_ascii_lowercase().ends_with("\\spotify.exe"))
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("Spotify did not become the foreground application");
    }
}
