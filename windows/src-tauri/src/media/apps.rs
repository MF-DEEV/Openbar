// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint
use super::Source;

/// Saved choices and the last music app win only while still available.
pub fn preferred(choice: &str, last: Option<&str>, players: &[Source]) -> Option<Source> {
    players
        .iter()
        .find(|s| s.id == choice)
        .or_else(|| {
            players
                .iter()
                .find(|s| Some(s.id.as_str()) == last && s.is_music_app)
        })
        .or_else(|| players.iter().find(|s| s.is_music_app))
        .cloned()
}

/// One on-demand Shell scan, never an additional polling loop. Windows has no
/// music category; reuse the existing music-app classifier and include every
/// dynamically discovered SMTC source separately, including unknown players.
#[cfg(windows)]
pub fn installed(preferred: &str) -> Vec<Source> {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName,
        SIGDN_NORMALDISPLAY,
    };
    use windows::core::{Interface, w};
    let Ok(_apartment) = crate::com::Apartment::sta() else {
        return vec![];
    };
    let mut players = Vec::new();
    unsafe {
        let Ok(folder): Result<IShellItem, _> =
            SHCreateItemFromParsingName(w!("shell:AppsFolder"), None)
        else {
            return players;
        };
        let Ok(items): Result<IEnumShellItems, _> = folder.BindToHandler(None, &BHID_EnumItems)
        else {
            return players;
        };
        // System.AppUserModel.ID (documented Shell property).
        let key = PROPERTYKEY {
            fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
            pid: 5,
        };
        loop {
            let mut next = [None];
            let mut fetched = 0;
            if items.Next(&mut next, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = next[0].take() else {
                continue;
            };
            let Ok(properties) = item.cast::<IShellItem2>() else {
                continue;
            };
            let Ok(value) = properties.GetString(&key) else {
                continue;
            };
            let mut id = value.to_string().unwrap_or_default();
            CoTaskMemFree(Some(value.0.cast()));
            let name = item
                .GetDisplayName(SIGDN_NORMALDISPLAY)
                .map(|value| {
                    let name = value.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(value.0.cast()));
                    name
                })
                .unwrap_or_else(|_| super::source_name(&id));
            let mut launch_path = None;
            if std::path::Path::new(&id).is_absolute() {
                let path = std::path::Path::new(&id);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let image = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let chosen = !preferred.is_empty()
                    && (image.eq_ignore_ascii_case(preferred)
                        || stem.eq_ignore_ascii_case(preferred));
                if !path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("exe"))
                    || (!chosen
                        && !super::MUSIC_APPS
                            .iter()
                            .any(|known| stem.eq_ignore_ascii_case(known)))
                {
                    continue;
                }
                launch_path = Some(id);
                id = if chosen { preferred.to_string() } else { image };
            }
            let music = super::is_music_app(&id) || super::is_music_app(&name);
            if !id.is_empty() && (id == preferred || music) {
                players.push(Source {
                    id,
                    name,
                    is_playing: false,
                    is_music_app: music,
                    has_track: false,
                    launch_path,
                });
            }
        }
    }
    players.sort_by(|a, b| a.name.cmp(&b.name));
    players.dedup_by(|a, b| a.id == b.id);
    players
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    #[ignore = "reads the music applications installed on this Windows host"]
    fn installed_catalog_contains_named_players() {
        let players = installed("");
        assert!(
            !players.is_empty(),
            "Start-menu music applications were not found"
        );
        assert!(
            players
                .iter()
                .all(|s| !s.id.is_empty() && !s.name.is_empty())
        );
        println!(
            "Music applications: {:?}",
            players.iter().map(|s| (&s.name, &s.id)).collect::<Vec<_>>()
        );
    }
    #[test]
    fn preference_is_for_idle_opening_not_live_selection() {
        let players = [Source {
            id: "installed".into(),
            name: "Installed".into(),
            is_playing: false,
            is_music_app: true,
            has_track: false,
            launch_path: None,
        }];
        assert_eq!(
            preferred("removed", None, &players).unwrap().id,
            "installed"
        );
        assert!(preferred("removed", None, &[]).is_none());
    }
}
