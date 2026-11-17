// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The quick toggles on Linux, as far as every desktop agrees: lock the
//! screen, turn the display off, start the screensaver, empty the trash and
//! (where the desktop keeps its dark style in the settings database) dark
//! mode. What differs per desktop and per file manager (hidden files, icons on
//! the desktop, ejecting disks) is not offered: its row stays hidden, as it
//! does on Windows when the state is not known.

use gtk::gio::{self, prelude::*};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::dpms::{self, ConnectionExt as _};

use crate::toggles::{Action, State, empty_recycle_bin};

fn run(program: &str, arguments: &[&str]) -> Option<String> {
    super::command::run(program, &[("LC_ALL", "C")], arguments, None)
}

/// The first of `attempts` (program and arguments) that succeeds.
fn first_that_works(attempts: &[(&str, &[&str])]) -> bool {
    attempts
        .iter()
        .any(|(program, arguments)| run(program, arguments).is_some())
}

/// Locks the screen the way the desktop does, else asks the session to lock
/// (a locker that listens for it, as `xss-lock` does, takes it from there).
fn lock_screen() -> Result<(), String> {
    first_that_works(&[
        ("xdg-screensaver", &["lock"]),
        ("loginctl", &["lock-session"]),
    ])
    .then_some(())
    .ok_or_else(|| "Nothing on this desktop could lock the screen".into())
}

fn screensaver() -> Result<(), String> {
    first_that_works(&[("xdg-screensaver", &["activate"])])
        .then_some(())
        .ok_or_else(|| "No screensaver is running to start".into())
}

/// Turns the display off through the X server's power management (DPMS),
/// which turns it on again at the next key or mouse move.
fn display_off() -> Result<(), String> {
    let (connection, _) = x11rb::connect(None).map_err(|_| "There is no X display to turn off")?;
    connection
        .extension_information(dpms::X11_EXTENSION_NAME)
        .map_err(|e| e.to_string())?
        .ok_or("This X server has no power management for displays")?;
    connection.dpms_enable().map_err(|e| e.to_string())?;
    connection
        .dpms_force_level(dpms::DPMSMode::OFF)
        .map_err(|e| e.to_string())?;
    connection.flush().map_err(|e| e.to_string())
}

fn trash_items() -> Option<i64> {
    let info = gio::File::for_uri("trash:///")
        .query_info(
            "trash::item-count",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?;
    Some(i64::from(info.attribute_uint32("trash::item-count")))
}

/// `gio trash --empty`, which empties every trash the person has (their home
/// folder's and a drive's), directories included.
fn empty_trash_with(gio_program: &str) -> Result<(), String> {
    empty_recycle_bin(trash_items, || {
        run(gio_program, &["trash", "--empty"])
            .map(|_| ())
            .ok_or_else(|| "The trash could not be emptied".into())
    })
}

/// Desktops that keep the dark style in the settings database (the color
/// scheme apps and the portal read). Others keep it in a theme name, which
/// differs by toolkit and is not guessed.
fn keeps_color_scheme(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|d| ["GNOME", "Unity", "Pantheon", "Budgie", "X-Cinnamon"].contains(&d))
}

fn dark_mode_with(gsettings: &str, desktop: &str) -> Option<bool> {
    if !keeps_color_scheme(desktop) {
        return None;
    }
    let scheme = run(
        gsettings,
        &["get", "org.gnome.desktop.interface", "color-scheme"],
    )?;
    Some(scheme.trim().trim_matches('\'') == "prefer-dark")
}

fn toggle_dark_mode_with(gsettings: &str, desktop: &str) -> Result<(), String> {
    let dark = dark_mode_with(gsettings, desktop)
        .ok_or("This desktop does not keep dark mode in its settings")?;
    run(
        gsettings,
        &[
            "set",
            "org.gnome.desktop.interface",
            "color-scheme",
            if dark { "default" } else { "prefer-dark" },
        ],
    )
    .map(|_| ())
    .ok_or_else(|| "The color scheme could not be changed".into())
}

fn desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
}

pub fn state(_excluded: &[String]) -> State {
    State {
        dark: dark_mode_with("gsettings", &desktop()),
        hidden_files_shown: None,
        desktop_icons_shown: None,
        ejectable: 0,
        recycle_items: trash_items(),
    }
}

pub fn run_action(action: Action, _excluded: &[String]) -> Result<(), String> {
    match action {
        Action::LockScreen => lock_screen(),
        Action::DisplayOff => display_off(),
        Action::ScreenSaver => screensaver(),
        Action::EmptyTrash => empty_trash_with("gio"),
        Action::DarkMode => toggle_dark_mode_with("gsettings", &desktop()),
        Action::HiddenFiles | Action::DesktopIcons | Action::EjectDisks => {
            Err("This is not offered on this desktop".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::command::fake_program;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("openbar-toggles-{name}-{}", std::process::id()))
    }

    #[test]
    fn the_first_command_that_works_is_the_one_used() {
        let dir = temp("first");
        let log = dir.join("calls");
        std::fs::create_dir_all(&dir).unwrap();
        let broken = fake_program(
            &dir,
            "broken",
            &format!("echo broken >> {}; exit 1", log.display()),
        );
        let good = fake_program(
            &dir,
            "good",
            &format!("echo \"good $*\" >> {}", log.display()),
        );
        let other = fake_program(&dir, "other", &format!("echo other >> {}", log.display()));
        let (broken, good, other) = (
            broken.to_str().unwrap(),
            good.to_str().unwrap(),
            other.to_str().unwrap(),
        );
        assert!(first_that_works(&[
            (broken, &["a"]),
            (good, &["lock"]),
            (other, &[])
        ]));
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "broken\ngood lock\n"
        );
        assert!(!first_that_works(&[(broken, &[]), ("/nonexistent/x", &[])]));
        assert!(!first_that_works(&[]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dark_mode_is_only_offered_where_the_desktop_keeps_it_in_the_settings() {
        for yes in [
            "GNOME",
            "ubuntu:GNOME",
            "X-Cinnamon",
            "Pantheon",
            "Budgie:GNOME",
        ] {
            assert!(keeps_color_scheme(yes), "{yes}");
        }
        for no in ["XFCE", "KDE", "", "LXQt", "MATE", "GNOME-Flashback"] {
            assert!(!keeps_color_scheme(no), "{no}");
        }
    }

    #[test]
    fn dark_mode_is_read_and_flipped_through_gsettings() {
        let dir = temp("dark");
        let state = dir.join("scheme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&state, "'default'\n").unwrap();
        let body = format!(
            "case \"$1\" in get) cat {s} ;; set) echo \"'$4'\" > {s} ;; esac",
            s = state.display()
        );
        let gsettings = fake_program(&dir, "gsettings", &body);
        let gsettings = gsettings.to_str().unwrap();
        assert_eq!(dark_mode_with(gsettings, "GNOME"), Some(false));
        toggle_dark_mode_with(gsettings, "GNOME").unwrap();
        assert_eq!(std::fs::read_to_string(&state).unwrap(), "'prefer-dark'\n");
        assert_eq!(dark_mode_with(gsettings, "GNOME"), Some(true));
        toggle_dark_mode_with(gsettings, "GNOME").unwrap();
        assert_eq!(dark_mode_with(gsettings, "GNOME"), Some(false));
        // On a desktop that does not keep it there it is not offered, nor flipped.
        assert_eq!(dark_mode_with(gsettings, "XFCE"), None);
        assert!(toggle_dark_mode_with(gsettings, "XFCE").is_err());
        assert_eq!(dark_mode_with("/nonexistent/gsettings", "GNOME"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_trash_is_emptied_by_gio_and_an_empty_one_is_left_alone() {
        let dir = temp("trash");
        let log = dir.join("calls");
        std::fs::create_dir_all(&dir).unwrap();
        let gio = fake_program(&dir, "gio", &format!("echo \"$*\" >> {}", log.display()));
        // Whatever the trash holds now, a working gio is asked, or the trash is empty already.
        let result = empty_trash_with(gio.to_str().unwrap());
        assert!(result.is_ok());
        let asked = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(asked.is_empty() || asked == "trash --empty\n", "{asked:?}");
        let failing = fake_program(&dir, "failing", "exit 1");
        match trash_items() {
            Some(0) | None => {}
            Some(_) => assert!(empty_trash_with(failing.to_str().unwrap()).is_err()),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn what_cannot_be_offered_is_refused_and_hidden() {
        let shown = state(&[]);
        assert_eq!(
            (
                shown.hidden_files_shown,
                shown.desktop_icons_shown,
                shown.ejectable
            ),
            (None, None, 0)
        );
        for action in [
            Action::HiddenFiles,
            Action::DesktopIcons,
            Action::EjectDisks,
        ] {
            assert!(run_action(action, &[]).is_err());
        }
    }

    #[test]
    fn without_an_x_server_the_display_stays_on() {
        // Only meaningful with no display: with one, this would turn it off.
        if std::env::var_os("DISPLAY").is_none() {
            assert!(display_off().is_err());
        }
    }

    #[test]
    #[ignore = "turns the display of DISPLAY off: only for a nested X server, never the desktop's own"]
    fn a_nested_x_server_is_asked_to_turn_its_display_off() {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(
            !display.is_empty() && !display.starts_with(":0"),
            "refusing to turn off {display:?}: use a nested server such as Xephyr on :97"
        );
        println!("display_off on {display}: {:?}", display_off());
    }
}
