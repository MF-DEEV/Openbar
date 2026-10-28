// The island must never own a console, including locally installed debug builds.
// Closing a console owned by this process would also terminate the island.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--microphone-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "microphone-probe.json".into());
        vorssaint_windows_lib::microphone_probe_main(&path);
    }
    if std::env::args().nth(1).as_deref() == Some("--notifications-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "notifications-probe.json".into());
        vorssaint_windows_lib::notifications_probe_main(&path);
    }
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--watch-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "watch-probe.txt".into());
        let title = std::env::args().nth(3).unwrap_or_default();
        vorssaint_windows_lib::watch_probe_main(&path, &title);
    }
    if std::env::args().nth(1).as_deref() == Some("--agents-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "agents-probe.json".into());
        let limits = std::env::args().any(|a| a == "--limits");
        vorssaint_windows_lib::agents_probe_main(&path, limits);
    }
    if std::env::args().nth(1).as_deref() == Some("--hook") {
        vorssaint_windows_lib::hook_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--music-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "music-probe.txt".into());
        vorssaint_windows_lib::music_probe_main(&path);
    }
    if std::env::args().nth(1).as_deref() == Some("--speed-test-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "speed-test-probe.txt".into());
        vorssaint_windows_lib::speed_test_probe_main(&path);
    }
    if std::env::args().nth(1).as_deref() == Some("--remove-hooks") {
        vorssaint_windows_lib::remove_hooks_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--codex-probe") {
        vorssaint_windows_lib::codex_probe_main();
    }
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--console") {
        let pid = std::env::args()
            .nth(2)
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        vorssaint_windows_lib::console_main(pid);
    }
    vorssaint_windows_lib::run()
}
