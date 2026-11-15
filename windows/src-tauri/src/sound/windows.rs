// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-player gain, never the endpoint or the process-wide audio mixer. SMTC is
//! disabled so short sounds cannot replace music in the island or intercept keys.
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use windows::Media::Core::MediaSource;
use windows::Media::Playback::{MediaPlayer, MediaPlayerAudioCategory};
use windows::Storage::StorageFile;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

#[derive(Default)]
struct Player {
    generation: u64,
    player: Option<MediaPlayer>,
}
fn state() -> &'static Mutex<Player> {
    static PLAYER: OnceLock<Mutex<Player>> = OnceLock::new();
    PLAYER.get_or_init(Default::default)
}

pub(super) fn play(path: &Path, volume: f64, ticket: u64) -> bool {
    let generation = {
        let mut state = state().lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        if let Some(player) = state.player.take() {
            let _ = player.Close();
        }
        state.generation
    };
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let result = (|| -> windows::core::Result<bool> {
        let path = windows::core::HSTRING::from(path.as_os_str());
        let file = tauri::async_runtime::block_on(async {
            StorageFile::GetFileFromPathAsync(&path)?.await
        })?;
        let source = MediaSource::CreateFromStorageFile(&file)?;
        let player = MediaPlayer::new()?;
        player.CommandManager()?.SetIsEnabled(false)?;
        player.SystemMediaTransportControls()?.SetIsEnabled(false)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::SoundEffects)?;
        player.SetVolume(volume)?;
        player.SetSource(&source)?;
        let mut state = state().lock().unwrap();
        if state.generation != generation || !crate::sound::session_request_current(ticket) {
            let _ = player.Close();
            return Ok(false);
        }
        player.Play()?;
        state.player = Some(player);
        Ok(true)
    })()
    .unwrap_or(false);
    if initialized {
        unsafe { RoUninitialize() };
    }
    result
}

pub(super) fn stop() {
    let mut state = state().lock().unwrap();
    state.generation = state.generation.wrapping_add(1);
    if let Some(player) = state.player.take() {
        let _ = player.Close();
    }
}
