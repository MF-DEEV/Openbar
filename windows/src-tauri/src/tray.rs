// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! The tray icon by the clock: settings and quit. A low battery marks it with
//! a dot in the warning's colour, where the Mac tints its menu bar reading.
//!
//! The sessions have an icon of their own beside it when they show in the
//! tray (#331): the agent's face, a dot and the count while sessions wait,
//! the panel on a click (Windows) and the waiting asks in its menu, where
//! Linux, which has no click, opens the panel too.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Runtime};

use crate::settings;

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = localized_menu(app)?;

    let mut tray = TrayIconBuilder::with_id("openbar")
        .tooltip("Openbar")
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

fn localized_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(
        app,
        "settings",
        crate::locale::text(app, "settings"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        crate::locale::text(app, "quit"),
        true,
        None::<&str>,
    )?;
    Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &quit])
}
pub fn refresh<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("openbar") {
        tray.set_menu(Some(localized_menu(app)?))?;
    }
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
        Some(percent) if warned => format!("Openbar: battery {percent}%"),
        _ => "Openbar".to_owned(),
    }
}

#[tauri::command]
pub fn tray_battery_warning<R: Runtime>(
    app: AppHandle<R>,
    level: String,
    percent: Option<i64>,
) -> Result<(), String> {
    let (Some(tray), Some(icon)) = (app.tray_by_id("openbar"), app.default_window_icon()) else {
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

// MARK: Sessions (#331)

pub const SESSIONS_ID: &str = "openbar-sessions";
const OPEN_ID: &str = "sessions-open";
const SETTINGS_ID: &str = "sessions-settings";
/// Parts an ask's menu id: `ask<US>allow<US>session<US>request`.
const SEPARATOR: char = '\u{1f}';
/// The dot breathes this many times for a new ask, this long each way.
const PULSES: u32 = 3;
const PULSE_STEP: Duration = Duration::from_millis(300);

/// One waiting ask as the menu lists it (src/sessions/trayState.ts).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrayAsk {
    pub session: String,
    pub request: String,
    pub title: String,
    /// Allow and Deny in the menu, where the island's queue answers in place.
    pub decides: bool,
}

/// What the island tells the sessions' icon to show, in the person's language.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TraySessions {
    pub visible: bool,
    pub waiting: u32,
    /// The icon's name, the tooltip it is added with.
    #[serde(default)]
    pub name: String,
    pub tooltip: String,
    pub label: String,
    pub header: String,
    pub asks: Vec<TrayAsk>,
    pub open_sessions: String,
    pub settings: String,
    pub allow: String,
    pub deny: String,
    pub open_ask: String,
}

/// A choice in the menu, handed to the island, which answers asks the way
/// its own buttons do (`tray://sessions-action`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionsAction {
    Open,
    Allow { session: String, request: String },
    Deny { session: String, request: String },
    OpenAsk { session: String, request: String },
}

#[derive(Default)]
struct SessionsTray {
    model: Option<TraySessions>,
    icon: Option<Image<'static>>,
    swollen: Option<Image<'static>>,
    /// Counts pulses, so a newer one or a new picture ends the one running.
    pulse: u64,
}

fn sessions_tray() -> &'static Mutex<SessionsTray> {
    static STATE: OnceLock<Mutex<SessionsTray>> = OnceLock::new();
    STATE.get_or_init(Mutex::default)
}

fn ask_id(kind: &str, ask: &TrayAsk) -> String {
    format!(
        "ask{SEPARATOR}{kind}{SEPARATOR}{}{SEPARATOR}{}",
        ask.session, ask.request
    )
}

/// The choice a menu id names, if it is one of the sessions' menu.
pub fn sessions_action(id: &str) -> Option<SessionsAction> {
    if id == OPEN_ID {
        return Some(SessionsAction::Open);
    }
    let mut parts = id.split(SEPARATOR);
    if parts.next()? != "ask" {
        return None;
    }
    let (kind, session, request) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || session.is_empty() || request.is_empty() {
        return None;
    }
    let (session, request) = (session.to_owned(), request.to_owned());
    match kind {
        "allow" => Some(SessionsAction::Allow { session, request }),
        "deny" => Some(SessionsAction::Deny { session, request }),
        "open" => Some(SessionsAction::OpenAsk { session, request }),
        _ => None,
    }
}

fn sessions_menu<R: Runtime>(app: &AppHandle<R>, model: &TraySessions) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    // The state, first and disabled: what a screen reader starts the menu with.
    menu.append(&MenuItem::with_id(
        app,
        "sessions-header",
        &model.header,
        false,
        None::<&str>,
    )?)?;
    if !model.asks.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    for ask in &model.asks {
        let open = MenuItem::with_id(
            app,
            ask_id("open", ask),
            &model.open_ask,
            true,
            None::<&str>,
        )?;
        let submenu = if ask.decides {
            let allow =
                MenuItem::with_id(app, ask_id("allow", ask), &model.allow, true, None::<&str>)?;
            let deny =
                MenuItem::with_id(app, ask_id("deny", ask), &model.deny, true, None::<&str>)?;
            Submenu::with_items(
                app,
                &ask.title,
                true,
                &[&allow, &deny, &PredefinedMenuItem::separator(app)?, &open],
            )?
        } else {
            Submenu::with_items(app, &ask.title, true, &[&open])?
        };
        menu.append(&submenu)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        OPEN_ID,
        &model.open_sessions,
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        SETTINGS_ID,
        &model.settings,
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn sessions_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if id == SETTINGS_ID {
        settings::open_section(app, "content", "sessions");
        return;
    }
    match sessions_action(id) {
        Some(SessionsAction::Open) => crate::sessions_panel::show(app, None, None, true),
        Some(SessionsAction::OpenAsk { session, request }) => crate::sessions_panel::show(
            app,
            None,
            Some(crate::sessions_panel::Focus {
                session,
                request: Some(request),
            }),
            true,
        ),
        Some(action) => {
            let _ = app.emit_to(crate::island::LABEL, "tray://sessions-action", action);
        }
        None => {}
    }
}

/// Makes the icon the first time it is to show. Made after the Openbar icon,
/// so Windows gives it the same place each start and keeps it where the
/// person put it; hidden rather than removed when it is not wanted.
fn sessions_icon<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
) -> tauri::Result<tauri::tray::TrayIcon<R>> {
    if let Some(tray) = app.tray_by_id(SESSIONS_ID) {
        return Ok(tray);
    }
    let icon = sessions_tray()
        .lock()
        .ok()
        .and_then(|state| state.icon.clone())
        .or_else(|| {
            app.default_window_icon()
                .map(|icon| icon.clone().to_owned())
        });
    let mut tray = TrayIconBuilder::with_id(SESSIONS_ID)
        // Windows 11 keeps this first tooltip as the start of the name
        // Narrator reads, followed by the current one.
        .tooltip(if name.is_empty() {
            "Openbar Sessions"
        } else {
            name
        })
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| sessions_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                crate::sessions_panel::toggle_from_tray(tray.app_handle(), rect);
            }
        });
    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)
}

#[tauri::command]
pub fn tray_sessions_update<R: Runtime>(
    app: AppHandle<R>,
    model: TraySessions,
) -> Result<(), String> {
    let (changed, existed) = {
        let mut state = sessions_tray().lock().map_err(|e| e.to_string())?;
        let changed = state.model.as_ref() != Some(&model);
        state.model = Some(model.clone());
        (changed, app.tray_by_id(SESSIONS_ID).is_some())
    };
    if !changed {
        return Ok(());
    }
    if !model.visible && !existed {
        return Ok(());
    }
    let tray = sessions_icon(&app, &model.name).map_err(|e| e.to_string())?;
    if model.visible {
        tray.set_menu(Some(
            sessions_menu(&app, &model).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
        tray.set_tooltip(Some(&model.tooltip))
            .map_err(|e| e.to_string())?;
        // Beside the icon where the desktop shows a label: the count from two.
        tray.set_title(Some(&model.label))
            .map_err(|e| e.to_string())?;
    } else {
        crate::sessions_panel::hide(&app);
    }
    tray.set_visible(model.visible).map_err(|e| e.to_string())
}

/// The face, and the face with its dot swollen for the pulse, as the island
/// drew them (src/sessions/trayIcon.ts).
#[tauri::command]
pub fn tray_sessions_icon<R: Runtime>(
    app: AppHandle<R>,
    icon: Vec<u8>,
    pulse: Option<Vec<u8>>,
) -> Result<(), String> {
    let image = Image::from_bytes(&icon).map_err(|e| e.to_string())?;
    let swollen = match pulse {
        Some(bytes) => Some(Image::from_bytes(&bytes).map_err(|e| e.to_string())?),
        None => None,
    };
    {
        let mut state = sessions_tray().lock().map_err(|e| e.to_string())?;
        state.icon = Some(image.clone());
        state.swollen = swollen;
        // A new picture ends a pulse drawn with the old one.
        state.pulse += 1;
    }
    if let Some(tray) = app.tray_by_id(SESSIONS_ID) {
        tray.set_icon(Some(image)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The dot breathes a few times for a new ask. The island asks only when
/// motion is not reduced.
#[tauri::command]
pub fn tray_sessions_pulse<R: Runtime>(app: AppHandle<R>) {
    let (generation, icon, swollen) = {
        let Ok(mut state) = sessions_tray().lock() else {
            return;
        };
        state.pulse += 1;
        (state.pulse, state.icon.clone(), state.swollen.clone())
    };
    let (Some(icon), Some(swollen)) = (icon, swollen) else {
        return;
    };
    std::thread::spawn(move || {
        for step in 0..PULSES * 2 {
            let current = sessions_tray().lock().map(|state| state.pulse).unwrap_or(0);
            if current != generation {
                return;
            }
            let Some(tray) = app.tray_by_id(SESSIONS_ID) else {
                return;
            };
            let frame = if step % 2 == 0 { &swollen } else { &icon };
            let _ = tray.set_icon(Some(frame.clone()));
            std::thread::sleep(PULSE_STEP);
        }
    });
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
    fn a_sessions_menu_id_names_its_ask_and_nothing_else() {
        let ask = TrayAsk {
            session: "opencode:s1".into(),
            request: "per_1".into(),
            title: "api — rm".into(),
            decides: true,
        };
        assert_eq!(
            sessions_action(&ask_id("allow", &ask)),
            Some(SessionsAction::Allow {
                session: "opencode:s1".into(),
                request: "per_1".into()
            })
        );
        assert_eq!(
            sessions_action(&ask_id("deny", &ask)),
            Some(SessionsAction::Deny {
                session: "opencode:s1".into(),
                request: "per_1".into()
            })
        );
        assert_eq!(
            sessions_action(&ask_id("open", &ask)),
            Some(SessionsAction::OpenAsk {
                session: "opencode:s1".into(),
                request: "per_1".into()
            })
        );
        assert_eq!(sessions_action(OPEN_ID), Some(SessionsAction::Open));
        for other in [
            "settings",
            "quit",
            SETTINGS_ID,
            "sessions-header",
            "ask",
            "ask\u{1f}allow\u{1f}s",
        ] {
            assert_eq!(sessions_action(other), None, "{other:?}");
        }
        let empty = TrayAsk::default();
        assert_eq!(sessions_action(&ask_id("allow", &empty)), None);
        assert_eq!(
            sessions_action(&format!("{}\u{1f}x", ask_id("allow", &ask))),
            None
        );
    }

    #[test]
    fn a_menu_choice_reaches_the_island_as_its_kind() {
        let allow = SessionsAction::Allow {
            session: "s".into(),
            request: "r".into(),
        };
        assert_eq!(
            serde_json::to_value(&allow).unwrap(),
            serde_json::json!({"kind": "allow", "session": "s", "request": "r"})
        );
        let open = SessionsAction::OpenAsk {
            session: "s".into(),
            request: "r".into(),
        };
        assert_eq!(serde_json::to_value(&open).unwrap()["kind"], "openAsk");
        assert_eq!(
            serde_json::to_value(&SessionsAction::Open).unwrap(),
            serde_json::json!({"kind": "open"})
        );
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
        assert_eq!(tooltip(Some(7), true), "Openbar: battery 7%");
        assert_eq!(tooltip(Some(7), false), "Openbar");
        assert_eq!(tooltip(None, true), "Openbar");
    }
}
