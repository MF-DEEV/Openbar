// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--hook") {
        vorssaint_windows_lib::hook_main();
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
