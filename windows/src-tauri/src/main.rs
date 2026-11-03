// The island must never own a console, including locally installed debug builds.
// Closing a console owned by this process would also terminate the island.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--music-queue-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "music-queue-probe.json".into());
        let title = std::env::args().nth(3).unwrap_or_default();
        openbar_windows_lib::music_queue_probe_main(&path, &title);
    }
    if std::env::args().nth(1).as_deref() == Some("--microphone-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "microphone-probe.json".into());
        openbar_windows_lib::microphone_probe_main(&path);
    }
    if std::env::args().nth(1).as_deref() == Some("--notifications-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "notifications-probe.json".into());
        openbar_windows_lib::notifications_probe_main(&path);
    }
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--watch-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "watch-probe.txt".into());
        let title = std::env::args().nth(3).unwrap_or_default();
        openbar_windows_lib::watch_probe_main(&path, &title);
    }
    if std::env::args().nth(1).as_deref() == Some("--agents-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "agents-probe.json".into());
        let limits = std::env::args().any(|a| a == "--limits");
        openbar_windows_lib::agents_probe_main(&path, limits);
    }
    if std::env::args().nth(1).as_deref() == Some("--hook") {
        openbar_windows_lib::hook_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--music-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "music-probe.txt".into());
        openbar_windows_lib::music_probe_main(&path);
    }
    if std::env::args().nth(1).as_deref() == Some("--voice-probe") {
        let wav = std::env::args().nth(2).unwrap_or_default();
        let out = std::env::args()
            .nth(3)
            .unwrap_or_else(|| "voice-probe.txt".into());
        let language = std::env::args().nth(4).unwrap_or_else(|| "en".into());
        openbar_windows_lib::voice_probe_main(&wav, &out, &language);
    }
    if std::env::args().nth(1).as_deref() == Some("--speed-test-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "speed-test-probe.txt".into());
        openbar_windows_lib::speed_test_probe_main(&path);
    }
    if let Some(arg) = std::env::args().nth(1) {
        openbar_windows_lib::network_helper_main(&arg);
    }
    if std::env::args().nth(1).as_deref() == Some("--migrate-legacy") {
        openbar_windows_lib::migrate_legacy_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--remove-hooks") {
        openbar_windows_lib::remove_hooks_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--remove-ide-bridge") {
        openbar_windows_lib::remove_ide_bridge_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--codex-probe") {
        openbar_windows_lib::codex_probe_main();
    }
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--console") {
        let pid = std::env::args()
            .nth(2)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        openbar_windows_lib::console_main(pid);
    }
    openbar_windows_lib::run()
}
