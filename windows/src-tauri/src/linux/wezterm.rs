// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! WezTerm on Linux: its command line is `wezterm` beside the GUI program that
//! runs the pane, and runs the way tmux's and kitty's do. `hosts/wezterm.rs`
//! hands over to this where Windows has `wezterm.exe` and its own runner.

use std::path::PathBuf;

/// `wezterm` beside the `wezterm-gui` that runs the pane, so the command line
/// always matches that WezTerm.
pub fn cli(gui_pid: u32) -> Option<PathBuf> {
    let gui = super::process::image_path(gui_pid)?;
    let cli = gui.parent()?.join("wezterm");
    cli.is_file().then_some(cli)
}

/// `wezterm cli <arguments>` against the WezTerm that owns `socket`.
pub fn run(cli: &PathBuf, socket: &str, arguments: &[&str], input: Option<&str>) -> Option<String> {
    let mut command = vec!["cli"];
    command.extend_from_slice(arguments);
    super::command::run(
        cli,
        &[(crate::hosts::wezterm::SOCKET, socket)],
        &command,
        input,
    )
}
