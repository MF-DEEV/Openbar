// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

mod bar;
mod hooks;
mod hosts;
mod island;
mod media;
mod notifications;
mod sessions;
mod settings;
mod tray;

use tauri::{Emitter, Manager, RunEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Ctrl+Alt+Space opens and closes the island from anywhere.
fn toggle_shortcut() -> Shortcut {
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
        .manage(notifications::Shared::default())
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_rects,
            island::island_set_capture,
            island::island_place,
            island::island_set_focusable,
            settings::open_settings,
            hooks::claude_hooks_status,
            hooks::claude_hooks_install,
            hooks::claude_hooks_remove,
            media::music_now,
            media::music_command,
            media::music_seek,
            media::music_shuffle,
            media::music_open_player,
            notifications::notifications_inbox,
            notifications::notifications_request_access,
            notifications::notifications_dismiss,
            notifications::notifications_clear,
            notifications::notifications_open,
            sessions::service::sessions_decide,
            sessions::service::sessions_answer,
            sessions::service::sessions_acknowledge,
            sessions::service::sessions_focus,
            sessions::service::sessions_set_previews,
            sessions::service::sessions_reply,
            sessions::service::sessions_open,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            bar::create(app.handle())?;
            island::create(app.handle())?;
            bar::own_island(app.handle());
            app.manage(sessions::service::Service::start(app.handle().clone()));
            media::start(
                app.handle().clone(),
                app.state::<media::Shared>().inner().clone(),
            );
            notifications::start(
                app.handle().clone(),
                app.state::<notifications::Shared>().inner().clone(),
            );
            tray::create(app.handle())?;
            // Taken by another app: the island still opens by clicking it.
            let _ = app.global_shortcut().register(toggle_shortcut());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vorssaint");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            bar::remove(app);
        }
    });
}
