// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The UI language and the regional formats on Linux, where Windows asks its
//! regional settings: the locale variables say it, read as gettext and the C
//! library read them. `locale.rs` hands over to `settings`, and the calendar
//! takes its week from `week_rules`.

use std::ffi::{CStr, CString};

type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What `locale_settings` tells the page.
pub struct Settings {
    pub preferred: Vec<String>,
    pub region: String,
    pub hour_cycle: &'static str,
    /// 0 is Sunday, as the page counts.
    pub first_weekday: u8,
}

fn first_set(env: Env, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| env(name).filter(|value| !value.is_empty()))
}

/// A POSIX locale name as a language tag: `pl_PL.UTF-8@euro` is `pl-PL`. The C
/// locale names no language.
fn tag(name: &str) -> Option<String> {
    let base = name.split(['.', '@']).next()?;
    (!matches!(base, "" | "C" | "POSIX")).then(|| base.replace('_', "-"))
}

/// The languages asked for, first choice first. `LANGUAGE` leads, but gettext
/// ignores it while the messages locale is C, and so does this.
fn preferred(env: Env) -> Vec<String> {
    let Some(locale) = first_set(env, &["LC_ALL", "LC_MESSAGES", "LANG"])
        .as_deref()
        .and_then(tag)
    else {
        return Vec::new();
    };
    let mut list: Vec<String> = env("LANGUAGE")
        .iter()
        .flat_map(|value| value.split(':'))
        .filter_map(tag)
        .collect();
    if !list.contains(&locale) {
        list.push(locale);
    }
    list
}

/// The territory of a tag: `pl-PL` is `PL`.
fn territory(tag: &str) -> Option<String> {
    tag.split('-')
        .skip(1)
        .find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_uppercase)
}

/// The territory formats follow: the one the formatting locale names, else the
/// first language's, else the US.
fn region(env: Env, languages: &[String]) -> String {
    first_set(env, &["LC_ALL", "LC_TIME", "LANG"])
        .as_deref()
        .and_then(tag)
        .and_then(|tag| territory(&tag))
        .or_else(|| languages.first().and_then(|tag| territory(tag)))
        .unwrap_or_else(|| "US".into())
}

/// A 12-hour clock names the hour with `%I` or `%l`, or prints am or pm
/// (`%p`, `%P`, `%r`).
fn hour_cycle_of(time_format: &str) -> &'static str {
    let mut chars = time_format.chars();
    while let Some(c) = chars.next() {
        if c == '%' && matches!(chars.next(), Some('I' | 'l' | 'p' | 'P' | 'r')) {
            return "h12";
        }
    }
    "h23"
}

/// The clock format the C library has for a locale (`T_FMT`); None when that
/// locale is not installed.
fn time_format(locale: &str) -> Option<String> {
    let name = CString::new(locale).ok()?;
    // SAFETY: the locale object is made, read and freed here, with no base and
    // so without touching the process's own locale; the text is copied before
    // it is freed.
    unsafe {
        let object = libc::newlocale(libc::LC_TIME_MASK, name.as_ptr(), std::ptr::null_mut());
        if object.is_null() {
            return None;
        }
        let text = libc::nl_langinfo_l(libc::T_FMT, object);
        let format = (!text.is_null()).then(|| CStr::from_ptr(text).to_string_lossy().into_owned());
        libc::freelocale(object);
        format
    }
}

/// The first day of the week and the days the first week of the year needs,
/// by the region of the locale (Sunday first where the Americas, Japan and a few
/// others have it, else Monday and the ISO week).
pub fn week_rules_of(locale: &str) -> (u8, u8) {
    let region = locale
        .split(['.', '@'])
        .next()
        .and_then(|l| l.split_once('_'))
        .map_or("", |(_, region)| region);
    const SUNDAY_FIRST: [&str; 16] = [
        "US", "CA", "MX", "BR", "JP", "IL", "SA", "IN", "PH", "KR", "TW", "HK", "ZA", "AU", "CO",
        "VE",
    ];
    if SUNDAY_FIRST.contains(&region) {
        (0, 1)
    } else {
        (1, 4)
    }
}

fn formatting_locale(env: Env) -> String {
    first_set(env, &["LC_ALL", "LC_TIME", "LANG"]).unwrap_or_default()
}

pub fn week_rules() -> (u8, u8) {
    week_rules_of(&formatting_locale(&|name| std::env::var(name).ok()))
}

fn settings_from(env: Env, clock: impl Fn(&str) -> Option<String>) -> Settings {
    let mut preferred = preferred(env);
    let region = region(env, &preferred);
    if preferred.is_empty() {
        preferred.push("en-US".into());
    }
    let locale = formatting_locale(env);
    Settings {
        preferred,
        region,
        hour_cycle: clock(&locale)
            .filter(|_| !locale.is_empty())
            .map_or("h23", |format| hour_cycle_of(&format)),
        first_weekday: week_rules_of(&locale).0,
    }
}

pub fn settings() -> Settings {
    settings_from(&|name| std::env::var(name).ok(), time_format)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn a_locale_name_is_a_language_tag_and_the_c_locale_is_none() {
        assert_eq!(tag("pl_PL.UTF-8").as_deref(), Some("pl-PL"));
        assert_eq!(tag("de_DE@euro").as_deref(), Some("de-DE"));
        assert_eq!(tag("pt_BR").as_deref(), Some("pt-BR"));
        assert_eq!(tag("fr").as_deref(), Some("fr"));
        for none in ["", "C", "C.UTF-8", "POSIX"] {
            assert_eq!(tag(none), None, "{none}");
        }
    }

    #[test]
    fn languages_come_from_language_then_the_locale_and_never_from_the_c_locale() {
        let languages = |pairs: &[(&str, &str)]| preferred(&env(pairs));
        assert_eq!(languages(&[("LANG", "de_DE.UTF-8")]), ["de-DE"]);
        // LC_ALL beats LC_MESSAGES beats LANG.
        assert_eq!(
            languages(&[
                ("LANG", "de_DE.UTF-8"),
                ("LC_MESSAGES", "fr_FR.UTF-8"),
                ("LC_ALL", "es_ES.UTF-8")
            ]),
            ["es-ES"]
        );
        // LANGUAGE leads, and the locale follows unless it is already there.
        assert_eq!(
            languages(&[("LANG", "en_US.UTF-8"), ("LANGUAGE", "pl:de_DE:en_US")]),
            ["pl", "de-DE", "en-US"]
        );
        assert_eq!(
            languages(&[("LANG", "en_US.UTF-8"), ("LANGUAGE", "pl")]),
            ["pl", "en-US"]
        );
        // While the messages locale is C, LANGUAGE is ignored, as gettext does.
        assert!(languages(&[("LANG", "C.UTF-8"), ("LANGUAGE", "pl")]).is_empty());
        assert!(languages(&[]).is_empty());
        // An empty variable is not set.
        assert_eq!(
            languages(&[("LC_ALL", ""), ("LANG", "it_IT.UTF-8")]),
            ["it-IT"]
        );
    }

    #[test]
    fn the_region_is_the_formatting_locales_then_the_languages_then_the_us() {
        let of = |pairs: &[(&str, &str)]| {
            let list = preferred(&env(pairs));
            region(&env(pairs), &list)
        };
        assert_eq!(of(&[("LANG", "pl_PL.UTF-8")]), "PL");
        // Messages in one language, formats of another country.
        assert_eq!(
            of(&[("LANG", "en_US.UTF-8"), ("LC_TIME", "pl_PL.UTF-8")]),
            "PL"
        );
        assert_eq!(of(&[("LANG", "en_gb.UTF-8")]), "GB");
        // A locale with no territory falls back to the language's.
        assert_eq!(
            of(&[("LANG", "fr"), ("LANGUAGE", "fr_CA")]),
            "CA",
            "from LANGUAGE's first tag"
        );
        assert_eq!(of(&[("LANG", "fr")]), "US");
        assert_eq!(of(&[("LANG", "C.UTF-8")]), "US");
        assert_eq!(of(&[]), "US");
    }

    #[test]
    fn a_clock_is_12_hour_when_its_format_names_the_hour_that_way_or_the_am_pm() {
        for twelve in ["%r", "%I:%M:%S %p", "%l:%M %P", "%I.%M"] {
            assert_eq!(hour_cycle_of(twelve), "h12", "{twelve}");
        }
        for twenty_four in ["%H:%M:%S", "%T", "%k:%M", "%R", "%%I %H"] {
            assert_eq!(hour_cycle_of(twenty_four), "h23", "{twenty_four}");
        }
    }

    #[test]
    fn the_c_library_knows_the_clock_of_an_installed_locale_and_not_of_a_missing_one() {
        assert_eq!(time_format("C").as_deref(), Some("%H:%M:%S"));
        assert_eq!(time_format("no_SUCH.UTF-8"), None);
        assert_eq!(time_format("with\0nul"), None);
    }

    #[test]
    fn the_settings_are_put_together_from_the_variables_and_the_clock() {
        let clock = |name: &str| match name {
            "en_US.UTF-8" => Some("%r".to_string()),
            "pl_PL.UTF-8" => Some("%T".to_string()),
            _ => None,
        };
        let american = settings_from(&env(&[("LANG", "en_US.UTF-8")]), clock);
        assert_eq!(american.preferred, ["en-US"]);
        assert_eq!(american.region, "US");
        assert_eq!(american.hour_cycle, "h12");
        assert_eq!(american.first_weekday, 0);
        let polish = settings_from(&env(&[("LANG", "pl_PL.UTF-8")]), clock);
        assert_eq!(polish.preferred, ["pl-PL"]);
        assert_eq!(polish.region, "PL");
        assert_eq!(polish.hour_cycle, "h23");
        assert_eq!(polish.first_weekday, 1);
        // Nothing set, or a locale the system does not have: English, 24 hours, Monday.
        for pairs in [&[][..], &[("LANG", "xx_XX.UTF-8")][..]] {
            let none = settings_from(&env(pairs), clock);
            assert_eq!(none.hour_cycle, "h23");
            assert_eq!(none.first_weekday, 1);
        }
        let bare = settings_from(&env(&[]), clock);
        assert_eq!(
            (bare.preferred, bare.region),
            (vec!["en-US".to_string()], "US".to_string())
        );
    }

    #[test]
    fn the_settings_of_this_session_are_well_formed_whatever_its_locale() {
        let settings = settings();
        assert!(!settings.preferred.is_empty());
        assert!(
            settings.region.len() == 2 && settings.region.chars().all(|c| c.is_ascii_uppercase())
        );
        assert!(["h12", "h23"].contains(&settings.hour_cycle));
        assert!(settings.first_weekday <= 1);
    }

    #[test]
    fn the_week_starts_where_the_locale_starts_it() {
        assert_eq!(week_rules_of("en_US.UTF-8"), (0, 1));
        assert_eq!(week_rules_of("pt_BR"), (0, 1));
        assert_eq!(week_rules_of("pl_PL.UTF-8"), (1, 4));
        assert_eq!(week_rules_of("en_GB.UTF-8"), (1, 4));
        assert_eq!(week_rules_of("de_DE@euro"), (1, 4));
        assert_eq!(week_rules_of("C.UTF-8"), (1, 4));
        assert_eq!(week_rules_of(""), (1, 4));
    }
}
