// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Editors on Linux: found by their command on PATH, with their data in the
//! home folder. Included by `hosts/bridge_installer.rs`, which keeps the types
//! it works with private; Windows looks under Programs instead.

use super::{Editor, Found, Places};

/// The command and the data folder of an editor as its Linux build names them.
fn names(id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
        "vscode" => ("code", ".vscode"),
        "vscode-insiders" => ("code-insiders", ".vscode-insiders"),
        "cursor" => ("cursor", ".cursor"),
        "antigravity" => ("antigravity", ".antigravity"),
        "windsurf" => ("windsurf", ".windsurf"),
        _ => return None,
    })
}

/// An editor installed system-wide: its command on PATH, its data in the home folder.
pub fn find_on_path<'a>(editor: &'a Editor, places: &Places) -> Option<Found<'a>> {
    let (command, data_folder) = names(editor.id)?;
    let cli = places
        .path
        .iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(command))
        .find(|cli| cli.is_file())?;
    Some(Found {
        editor,
        extensions: places.home.join(data_folder).join("extensions"),
        cli,
    })
}
