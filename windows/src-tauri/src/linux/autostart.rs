// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Launch at login on Linux: a desktop entry in `~/.config/autostart`.

use std::path::{Path, PathBuf};

const FILE: &str = "openbar.desktop";

fn entry() -> Option<PathBuf> {
    Some(super::paths::roaming()?.join("autostart").join(FILE))
}

/// The program to start: the AppImage itself, not the folder it unpacks to.
fn program() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

/// A desktop entry's Exec value quotes a path with spaces and escapes its quotes.
fn exec(program: &Path) -> String {
    let text = program.to_string_lossy();
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

fn contents(program: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Openbar\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec(program)
    )
}

pub fn enabled() -> bool {
    entry().is_some_and(|path| path.is_file())
}

pub fn set(on: bool) -> Result<(), String> {
    let path = entry().ok_or("no config folder")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let program = program().ok_or("cannot find the program")?;
    std::fs::create_dir_all(path.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    std::fs::write(&path, contents(&program)).map_err(|e| e.to_string())
}

pub fn legacy_present() -> bool {
    false
}

pub fn remove_legacy() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exec_line_quotes_what_a_desktop_entry_reads_specially() {
        assert_eq!(
            exec(Path::new("/opt/Open bar/openbar")),
            "\"/opt/Open bar/openbar\""
        );
        assert_eq!(exec(Path::new("/a\"b$c%d")), "\"/a\\\"b\\$c%%d\"");
    }

    #[test]
    fn the_entry_starts_the_program_without_a_terminal() {
        let text = contents(Path::new("/usr/bin/openbar"));
        assert!(text.starts_with("[Desktop Entry]\nType=Application\n"));
        assert!(text.contains("\nExec=\"/usr/bin/openbar\"\n"));
        assert!(text.contains("\nTerminal=false\n"));
    }
}
