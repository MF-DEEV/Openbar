// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

mod bar;
mod hooks;
mod hosts;
mod island;
mod settings;
mod tray;

use tauri::{Emitter, RunEvent};
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
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_rects,
            island::island_set_capture,
            island::island_place,
            island::island_set_focusable,
            settings::open_settings,
            hooks::claude_hooks_status,
            hooks::claude_hooks_install,
            hooks::claude_hooks_remove,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            bar::create(app.handle())?;
            island::create(app.handle())?;
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
