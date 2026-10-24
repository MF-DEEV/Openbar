// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--hook") {
        vorssaint_windows_lib::hook_main();
    }
    if std::env::args().nth(1).as_deref() == Some("--music-probe") {
        let path = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "music-probe.txt".into());
        vorssaint_windows_lib::music_probe_main(&path);
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
