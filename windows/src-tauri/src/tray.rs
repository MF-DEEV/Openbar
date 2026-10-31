// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The tray icon by the clock: settings and quit. A low battery marks it with
//! a dot in the warning's colour, where the Mac tints its menu bar reading.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::settings;

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Vorssaint", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])?;

    let mut tray = TrayIconBuilder::with_id("vorssaint")
        .tooltip("Vorssaint")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => settings::open(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                settings::open(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Puts a dot in the icon's lower right corner, ringed so it reads on any taskbar.
fn with_dot(rgba: &[u8], width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let size = width.min(height) as f32;
    let radius = size * 0.3;
    let ring = radius + size * 0.05;
    let (cx, cy) = (width as f32 - ring, height as f32 - ring);
    for y in 0..height {
        for x in 0..width {
            let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
            if distance > ring {
                continue;
            }
            let i = ((y * width + x) * 4) as usize;
            let Some(pixel) = out.get_mut(i..i + 4) else {
                continue;
            };
            let [r, g, b] = if distance <= radius { color } else { [0, 0, 0] };
            pixel.copy_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

fn warning_color(level: &str) -> Option<[u8; 3]> {
    match level {
        "early" => Some([0xff, 0x9f, 0x0a]),
        "low" => Some([0xff, 0x45, 0x3a]),
        _ => None,
    }
}

fn tooltip(percent: Option<i64>, warned: bool) -> String {
    match percent {
        Some(percent) if warned => format!("Vorssaint: battery {percent}%"),
        _ => "Vorssaint".to_owned(),
    }
}

#[tauri::command]
pub fn tray_battery_warning<R: Runtime>(
    app: AppHandle<R>,
    level: String,
    percent: Option<i64>,
) -> Result<(), String> {
    let (Some(tray), Some(icon)) = (app.tray_by_id("vorssaint"), app.default_window_icon()) else {
        return Ok(());
    };
    let color = warning_color(&level);
    let image = match color {
        Some(color) => Image::new_owned(
            with_dot(icon.rgba(), icon.width(), icon.height(), color),
            icon.width(),
            icon.height(),
        ),
        None => icon.clone().to_owned(),
    };
    tray.set_icon(Some(image)).map_err(|e| e.to_string())?;
    tray.set_tooltip(Some(tooltip(percent, color.is_some())))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_sits_in_the_corner_and_leaves_the_rest() {
        let (w, h) = (20u32, 20u32);
        let icon = vec![9u8; (w * h * 4) as usize];
        let out = with_dot(&icon, w, h, [1, 2, 3]);
        let at = |x: u32, y: u32| &out[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0), [9, 9, 9, 9], "the far corner stays the icon");
        assert_eq!(at(13, 13), [1, 2, 3, 255], "the dot is drawn opaque");
        assert_eq!(out.len(), icon.len());
    }

    #[test]
    fn a_short_buffer_does_not_panic() {
        assert_eq!(with_dot(&[0; 8], 20, 20, [1, 2, 3]).len(), 8);
    }

    #[test]
    fn levels_and_tooltips() {
        assert_eq!(warning_color("low"), Some([0xff, 0x45, 0x3a]));
        assert_eq!(warning_color("early"), Some([0xff, 0x9f, 0x0a]));
        assert_eq!(warning_color("none"), None);
        assert_eq!(warning_color("anything"), None);
        assert_eq!(tooltip(Some(7), true), "Vorssaint: battery 7%");
        assert_eq!(tooltip(Some(7), false), "Vorssaint");
        assert_eq!(tooltip(None, true), "Vorssaint");
    }
}
