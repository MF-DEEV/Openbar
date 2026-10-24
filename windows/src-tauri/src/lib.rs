// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

mod island;
mod settings;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| settings::open(app)))
        .plugin(tauri_plugin_opener::init())
        .manage(island::IslandState::default())
        .invoke_handler(tauri::generate_handler![
            island::island_set_hit_rects,
            island::island_set_capture,
            island::island_place,
            settings::open_settings,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            island::create(app.handle())?;
            tray::create(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Vorssaint");
}
