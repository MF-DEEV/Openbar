// SPDX-License-Identifier: GPL-3.0-or-later
//! UI language plus the person's regional formatting choices. The existing
//! calendar broadcast window notifies us when Windows sends "intl".
use serde::Serialize;
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionalSettings {
    preferred: Vec<String>,
    region: String,
    hour_cycle: String,
    first_weekday: u8,
}
static APP: OnceLock<AppHandle<Wry>> = OnceLock::new();
pub fn start(app: AppHandle<Wry>) {
    let _ = APP.set(app);
}
pub fn changed() {
    if let Some(app) = APP.get() {
        let _ = crate::tray::refresh(app);
        refresh_titles(app);
        let _ = app.emit("locale://changed", locale_settings());
    }
}

pub fn refresh_titles<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(crate::settings::LABEL) {
        let _ = window.set_title(&format!(
            "Vorssaint — {}",
            text(app, "settings").trim_end_matches('…')
        ));
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("screenshot-preview-") {
            let _ = window.set_title(&format!("{} — Vorssaint", text(app, "screenshot")));
        }
    }
}

// This tiny generated subset is derived from the exact same JSON as React.
// Native menus/dialogs need it before a webview exists; it is not another catalog.
pub fn text<R: Runtime>(app: &AppHandle<R>, key: &str) -> String {
    static TABLES: OnceLock<serde_json::Value> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/i18n/native.json"))
            .expect("generated native strings")
    });
    let saved = app.state::<crate::prefs::Prefs>().get();
    let preference = saved
        .pointer("/island/language")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let settings = locale_settings();
    let language = if tables.get(preference).is_some() {
        preference.to_owned()
    } else {
        detect(settings.preferred.first().map_or("en", String::as_str)).to_owned()
    };
    tables[&language][key]
        .as_str()
        .or_else(|| tables["en-US"][key].as_str())
        .unwrap_or(key)
        .to_owned()
}

fn detect(preferred: &str) -> &'static str {
    let p = preferred.to_lowercase().replace('_', "-");
    if p.starts_with("zh-hk") || p.starts_with("zh-hant-hk") {
        return "zh-HK";
    }
    if p.starts_with("zh-tw") || p.starts_with("zh-hant") {
        return "zh-TW";
    }
    for (prefix, language) in [
        ("pt", "pt-BR"),
        ("tr", "tr"),
        ("ru", "ru"),
        ("es", "es"),
        ("sk", "sk"),
        ("de", "de"),
        ("fr", "fr"),
        ("it", "it"),
        ("ja", "ja"),
        ("ko", "ko"),
        ("uk", "uk"),
        ("zh", "zh-Hans"),
    ] {
        if p.starts_with(prefix) {
            return language;
        }
    }
    "en-US"
}

#[tauri::command]
pub fn locale_settings() -> RegionalSettings {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{
            GetLocaleInfoEx, GetUserPreferredUILanguages, LOCALE_IFIRSTDAYOFWEEK, LOCALE_ITIME,
            LOCALE_SISO3166CTRYNAME, MUI_LANGUAGE_NAME,
        };
        use windows::core::{PCWSTR, PWSTR};
        fn field(kind: u32) -> Option<String> {
            let mut value = [0u16; 128];
            let length = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut value)) };
            (length > 1).then(|| String::from_utf16_lossy(&value[..length as usize - 1]))
        }
        let (mut count, mut length) = (0, 0);
        let mut preferred = Vec::new();
        unsafe {
            let _ = GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut length);
            if length > 0 && length <= 4096 {
                let mut value = vec![0u16; length as usize];
                if GetUserPreferredUILanguages(
                    MUI_LANGUAGE_NAME,
                    &mut count,
                    Some(PWSTR(value.as_mut_ptr())),
                    &mut length,
                )
                .is_ok()
                {
                    preferred = value
                        .split(|c| *c == 0)
                        .filter(|s| !s.is_empty())
                        .map(String::from_utf16_lossy)
                        .collect();
                }
            }
        }
        if preferred.is_empty() {
            preferred.push("en-US".into());
        }
        return RegionalSettings {
            preferred,
            region: field(LOCALE_SISO3166CTRYNAME).unwrap_or_else(|| "US".into()),
            hour_cycle: if field(LOCALE_ITIME).as_deref() == Some("0") {
                "h12"
            } else {
                "h23"
            }
            .into(),
            first_weekday: field(LOCALE_IFIRSTDAYOFWEEK)
                .and_then(|n| n.parse::<u8>().ok())
                .map_or(1, |n| (n + 1) % 7),
        };
    }
    #[cfg(not(windows))]
    RegionalSettings {
        preferred: vec!["en-US".into()],
        region: "US".into(),
        hour_cycle: "h23".into(),
        first_weekday: 1,
    }
}
