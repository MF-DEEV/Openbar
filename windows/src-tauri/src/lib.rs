// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

mod bar;
mod com;
mod downloads;
mod discord_storage;
mod discord_keys;
mod fullscreen;
mod hooks;
mod hosts;
mod island;
mod media;
mod media_volume;
mod microphone;
mod notifications;
mod prefs;
mod sessions;
mod settings;
mod shell;
mod sound;
mod timer;
mod system_monitor;
mod tray;

use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

/// Ctrl+Alt+Space opens and closes the island from anywhere.
pub(crate) fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
}

/// `Vorssaint.exe --hook`: Claude Code runs this for every hook event. It
/// never starts the app, only hands the event to the running island.
pub fn hook_main() -> ! {
    hooks::transport::run_hook(hooks::APP_ID)
}

/// `Vorssaint.exe --music-probe <file>`: writes what the media reader sees.
pub fn music_probe_main(path: &str) -> ! {
    let _ = std::fs::write(path, media::probe());
    std::process::exit(0)
}

pub fn microphone_probe_main(path: &str) -> ! {
    microphone::probe_main(path)
}

pub fn notifications_probe_main(path: &str) -> ! {
    notifications::probe_main(path)
}

/// `Vorssaint.exe --remove-hooks`: the uninstaller takes Vorssaint's hooks out
/// of Claude Code's settings, so Claude doesn't call a program that's gone.
pub fn remove_hooks_main() -> ! {
    std::process::exit(if hooks::claude_hooks_remove().is_ok() {
        0
    } else {
        1
    })
}

/// `Vorssaint.exe --codex-probe`: prints what Codex's app-server tells the island.
pub fn codex_probe_main() -> ! {
    sessions::codex::client::probe()
}

/// `Vorssaint.exe --console <pid>`: the helper that types into a session's console.
#[cfg(windows)]
pub fn console_main(pid: u32) -> ! {
    hosts::console::helper_main(pid)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            settings::open(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if shortcut == &toggle_shortcut() && event.state() == ShortcutState::Pressed {
                        let _ = app.emit_to(island::LABEL, "island://toggle", ());
                    }
                })
                .build(),
        )
        .manage(island::IslandState::default())
        .manage(media::Shared::default())
        .manage(microphone::Shared::default())
        .manage(downloads::Shared::default())
        .manage(notifications::Shared::default())
        .manage(system_monitor::Shared::default())
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_rects,
            island::island_set_capture,
            island::island_set_focusable,
            fullscreen::attention_context,
            fullscreen::attention_compact_sessions,
            settings::open_settings,
            hooks::claude_hooks_status,
            hooks::claude_hooks_install,
            hooks::claude_hooks_remove,
            media::music_now,
            microphone::microphone_snapshot,
            downloads::downloads_snapshot,
            media_volume::music_volume,
            media::music_command,
            media::music_seek,
            media::music_shuffle,
            media::music_open_player,
            notifications::notifications_inbox,
            notifications::notifications_request_access,
            notifications::notifications_dismiss,
            notifications::notifications_clear,
            notifications::notifications_open,
            prefs::prefs_get,
            prefs::prefs_set,
            prefs::launch_at_login_get,
            prefs::launch_at_login_set,
            sessions::service::sessions_decide,
            sessions::service::sessions_answer,
            sessions::service::sessions_acknowledge,
            sessions::service::sessions_focus,
            sessions::service::sessions_codex_status,
            prefs::shortcut_registered,
            sessions::service::sessions_reply,
            sessions::service::sessions_open,
            sessions::service::sessions_image,
            sessions::attachments::sessions_attach_image,
            sessions::attachments::sessions_clipboard_images,
            shell::shell_icon,
            sound::sound_play,
            sound::sound_alarm,
            timer::timer_now_ms,
            timer::timer_wake,
            system_monitor::system_monitor_watch,
            system_monitor::system_monitor_snapshot,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            // Loaded before the island exists, so its first prefs_get finds them.
            app.manage(prefs::Prefs::load(app.handle()));
            island::create(app.handle())?;
            fullscreen::watch(app.handle());
            microphone::start(app.handle().clone(), app.state::<microphone::Shared>().inner().clone());
            downloads::start(app.handle().clone(), app.state::<downloads::Shared>().inner().clone());
            app.manage(sessions::service::Service::start(app.handle().clone()));
            media::start(
                app.handle().clone(),
                app.state::<media::Shared>().inner().clone(),
            );
            notifications::start(
                app.handle().clone(),
                app.state::<notifications::Shared>().inner().clone(),
            );
            system_monitor::start(app.handle().clone(), app.state::<system_monitor::Shared>().inner().clone());
            let prefs = app.state::<prefs::Prefs>().get();
            prefs::apply(app.handle(), None, &prefs);
            tray::create(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vorssaint");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            fullscreen::stop(app);
        }
    });
}
