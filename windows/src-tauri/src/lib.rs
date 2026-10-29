// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

mod agent_usage;
mod audio;
mod bar;
mod brightness;
mod calendar;
mod capture;
mod clipboard;
mod com;
mod discord_keys;
mod discord_storage;
mod downloads;
mod files;
mod fullscreen;
mod hooks;
mod hosts;
mod island;
mod keep_awake;
mod media;
mod media_volume;
mod microphone;
mod notifications;
mod ocr;
mod power_saver;
mod prefs;
mod scratchpad;
mod sessions;
mod settings;
mod shell;
mod sound;
mod speed_test;
mod system_monitor;
mod timer;
mod toggles;
mod tray;
mod watch;

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

/// `Vorssaint.exe --watch-probe <file> <title>`: times Watch's capture steps for a window.
#[cfg(windows)]
pub fn watch_probe_main(path: &str, title: &str) -> ! {
    watch::probe(path, title);
    std::process::exit(0)
}

/// `Vorssaint.exe --agents-probe <file> [--limits]`: what the AI page would read.
pub fn agents_probe_main(path: &str, limits: bool) -> ! {
    agent_usage::probe_main(path, limits)
}

pub fn notifications_probe_main(path: &str) -> ! {
    notifications::probe_main(path)
}

/// `Vorssaint.exe --speed-test-probe <file>`: runs the network detail's speed test.
pub fn speed_test_probe_main(path: &str) -> ! {
    speed_test::probe_main(path)
}

/// `Vorssaint.exe --network-helper`, `--install-network-helper` or
/// `--remove-network-helper`: the per-app network list's elevated helper
/// and the steps that approve and remove it. Returns for anything else.
pub fn network_helper_main(arg: &str) {
    system_monitor::network_helper_main(arg)
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if audio::roller_key(shortcut, event.state() == ShortcutState::Pressed) {
                        return;
                    }
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &toggle_shortcut() {
                        let _ = app.emit_to(island::LABEL, "island://toggle", ());
                    } else if audio::is_switcher_shortcut(shortcut) {
                        audio::switch_to_next_output();
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
        .manage(speed_test::Shared::default())
        .manage(calendar::Shared::default())
        .manage(audio::Shared::default())
        .manage(keep_awake::Shared::default())
        .manage(capture::Shared::default())
        .manage(clipboard::Shared::default())
        .manage(agent_usage::State::default())
        .manage(watch::Shared::default())
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
            shell::shell_open_task_manager,
            sound::sound_play,
            sound::sound_alarm,
            timer::timer_now_ms,
            timer::timer_wake,
            system_monitor::system_monitor_watch,
            system_monitor::system_monitor_snapshot,
            system_monitor::system_network_approve,
            speed_test::speed_test_state,
            speed_test::speed_test_start,
            calendar::calendar_snapshot,
            calendar::calendar_show_range,
            calendar::calendar_request_access,
            calendar::calendar_open,
            calendar::calendar_open_privacy_settings,
            audio::audio_state,
            audio::audio_set_output,
            audio::audio_set_input,
            audio::audio_set_default,
            audio::audio_set_app,
            audio::audio_watch_apps,
            audio::mic_mute_set,
            audio::audio_priority_set,
            audio::audio_switcher_recording,
            keep_awake::keep_awake_state,
            keep_awake::keep_awake_set,
            keep_awake::keep_awake_toggle,
            power_saver::power_saver_on,
            brightness::brightness_displays,
            brightness::brightness_set,
            capture::capture_run,
            capture::capture_session,
            capture::capture_pixels,
            capture::capture_ready,
            capture::capture_commit,
            capture::tools_copy_text,
            toggles::toggles_state,
            toggles::toggles_run,
            scratchpad::scratchpad_load,
            scratchpad::scratchpad_save,
            scratchpad::scratchpad_export,
            clipboard::clipboard_snapshot,
            clipboard::clipboard_copy,
            clipboard::clipboard_paste,
            clipboard::clipboard_toggle_pin,
            clipboard::clipboard_remove,
            clipboard::clipboard_move,
            clipboard::clipboard_clear_recent,
            clipboard::clipboard_copy_text,
            clipboard::clipboard_copy_image,
            clipboard::clipboard_beep,
            files::files_snapshot,
            files::files_remove,
            files::files_clear,
            files::files_toggle_pin,
            files::files_open,
            files::files_open_with_apps,
            files::files_open_with,
            files::files_reveal,
            files::files_reveal_outputs,
            files::files_share,
            files::files_drag_out,
            files::files_zip,
            files::files_zip_cancel,
            files::files_kind,
            agent_usage::agents_snapshot,
            agent_usage::agents_page_visible,
            agent_usage::agents_resets_refresh,
            agent_usage::agents_resets_redeem,
            agent_usage::agents_codex_refresh,
            agent_usage::agents_codex_discover,
            agent_usage::agents_codex_choose_folder,
            watch::watch_snapshot,
            watch::watch_choose,
            watch::watch_stop,
            watch::watch_again,
            watch::watch_set_rule,
            watch::watch_pin_line,
            watch::watch_page_visible,
            watch::watch_notify,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            // Loaded before the island exists, so its first prefs_get finds them.
            app.manage(prefs::Prefs::load(app.handle()));
            app.manage(scratchpad::Store::new(app.handle()));
            app.manage(files::Files::load(app.handle()));
            // Followed before the island exists, so its first power_saver_on has it.
            power_saver::start(app.handle().clone());
            island::create(app.handle())?;
            files::install(app.handle());
            fullscreen::watch(app.handle());
            microphone::start(
                app.handle().clone(),
                app.state::<microphone::Shared>().inner().clone(),
            );
            downloads::start(
                app.handle().clone(),
                app.state::<downloads::Shared>().inner().clone(),
            );
            app.manage(sessions::service::Service::start(app.handle().clone()));
            media::start(
                app.handle().clone(),
                app.state::<media::Shared>().inner().clone(),
            );
            notifications::start(
                app.handle().clone(),
                app.state::<notifications::Shared>().inner().clone(),
            );
            system_monitor::start(
                app.handle().clone(),
                app.state::<system_monitor::Shared>().inner().clone(),
            );
            calendar::start(
                app.handle().clone(),
                app.state::<calendar::Shared>().inner().clone(),
            );
            audio::start(
                app.handle().clone(),
                app.state::<audio::Shared>().inner().clone(),
            );
            clipboard::start(
                app.handle().clone(),
                app.state::<clipboard::Shared>().inner().clone(),
            );
            agent_usage::start(
                app.handle().clone(),
                app.state::<agent_usage::State>().inner().clone(),
            );
            let prefs = app.state::<prefs::Prefs>().get();
            prefs::apply(app.handle(), None, &prefs);
            tray::create(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Vorssaint");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            system_monitor::stop(app.state::<system_monitor::Shared>().inner());
            fullscreen::stop(app);
            audio::stop();
        }
    });
}
