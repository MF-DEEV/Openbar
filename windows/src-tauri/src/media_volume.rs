// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/// Match SMTC's application identity to its WASAPI process, including packaged players.
fn matches_player(source: &str, executable: &str) -> bool {
    let source = source.to_ascii_lowercase();
    let executable = executable.to_ascii_lowercase();
    let known = [
        ("spotify", "spotify"),
        ("msedge", "msedge"),
        ("chrome", "chrome"),
        ("firefox", "firefox"),
        ("brave", "brave"),
        ("opera", "opera"),
        ("vivaldi", "vivaldi"),
        ("zunemusic", "music.ui"),
        ("applemusic", "applemusic"),
        ("vlc", "vlc"),
        ("itunes", "itunes"),
        ("tidal", "tidal"),
        ("deezer", "deezer"),
        ("foobar2000", "foobar2000"),
        ("musicbee", "musicbee"),
    ];
    if let Some((_, process)) = known.iter().find(|(identity, _)| source.contains(identity)) {
        return executable == *process;
    }
    source.trim_end_matches(".exe") == executable
}

#[cfg(windows)]
pub fn volume(source: &str, level: Option<f32>) -> Result<f32, String> {
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Media::Audio::{
        DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    if level.is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v)) {
        return Err("Invalid volume level".into());
    }
    // Every interface is created, used and released on this MTA thread.
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
    }
    let _com = ComGuard;
    let result = (|| -> windows::core::Result<Option<f32>> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let mut result: Option<f32> = None;
            let mut processes = std::collections::HashMap::<u32, String>::new();
            for device_index in 0..devices.GetCount()? {
                let device = devices.Item(device_index)?;
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                    continue;
                };
                let sessions = manager.GetSessionEnumerator()?;
                for session_index in 0..sessions.GetCount()? {
                    let control = sessions.GetSession(session_index)?;
                    let control: IAudioSessionControl2 = control.cast()?;
                    let pid = control.GetProcessId()?;
                    let executable = processes.entry(pid).or_insert_with(|| {
                        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                        else {
                            return String::new();
                        };
                        let mut buffer = [0u16; 32768];
                        let mut len = buffer.len() as u32;
                        let read = QueryFullProcessImageNameW(
                            process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len,
                        );
                        let _ = CloseHandle(process);
                        if read.is_err() {
                            return String::new();
                        }
                        let path = String::from_utf16_lossy(&buffer[..len as usize]);
                        std::path::Path::new(&path)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    });
                    if executable.is_empty() || !matches_player(source, executable) {
                        continue;
                    }
                    let volume: ISimpleAudioVolume = control.cast()?;
                    if let Some(level) = level {
                        volume.SetMasterVolume(level, std::ptr::null())?;
                        if level > 0.0 {
                            volume.SetMute(false, std::ptr::null())?;
                        }
                    }
                    let value = if volume.GetMute()?.as_bool() {
                        0.0
                    } else {
                        volume.GetMasterVolume()?
                    };
                    result = Some(result.map_or(value, |current| current.max(value)));
                }
            }
            Ok(result)
        }
    })();
    result
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No audio session for this player".into())
}

#[cfg(not(windows))]
pub fn volume(_source: &str, _level: Option<f32>) -> Result<f32, String> {
    Err("Application volume is available on Windows".into())
}

#[tauri::command]
pub async fn music_volume(source: String, level: Option<f32>) -> Result<f32, String> {
    tauri::async_runtime::spawn_blocking(move || volume(&source, level))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn rejects_invalid_levels_before_accessing_audio_sessions() {
        for level in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert_eq!(volume("Spotify.exe", Some(level)), Err("Invalid volume level".into()));
        }
    }
    #[test]
    fn matches_packaged_and_desktop_players_without_matching_other_apps() {
        assert!(matches_player("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", "Spotify"));
        assert!(matches_player("Chrome", "chrome"));
        assert!(matches_player("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic", "Music.UI"));
        assert!(matches_player("custom-player.exe", "custom-player"));
        assert!(!matches_player("Spotify.exe", "chrome"));
        assert!(!matches_player("Chrome", "chromedriver"));
    }
}
