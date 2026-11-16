// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Masks what a diagnostics file must not carry: the home folder (shown as
//! `~`), email addresses, and keys and tokens. Prompts, replies, commands and
//! file contents are never collected in the first place; this pass catches
//! what an error message or a path might still hold. The Mac app masks the
//! same way (`DiagnosticsRedactor.swift`); both are checked against
//! `Tests/Fixtures/diagnostics-redaction.json`.

use regex::{Captures, Regex};
use std::sync::OnceLock;

pub const SECRET: &str = "<secret>";
pub const EMAIL: &str = "<email>";

/// Folders under Users that belong to nobody in particular.
const KEPT_FOLDERS: [&str; 5] = ["shared", "public", "default", "all users", "default user"];

struct Patterns {
    homes: Regex,
    prefixed: Regex,
    bearer: Regex,
    key_value: Regex,
    long_run: Regex,
    email: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        homes: Regex::new(
            r#"(?i)(?:/Users/|/home/|\b[a-z]:(?:\\\\|\\|/)users(?:\\\\|\\|/))([^\\/\s"'<>:]+)"#,
        )
        .unwrap(),
        prefixed: Regex::new(
            r"sk-ant-[A-Za-z0-9_-]{8,}|sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{16,}|github_pat_[A-Za-z0-9_]{16,}|xox[abposr]-[A-Za-z0-9-]{8,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
        )
        .unwrap(),
        bearer: Regex::new(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{8,}").unwrap(),
        key_value: Regex::new(
            r#"(?i)([A-Za-z0-9_-]*(?:api[_-]?key|access[_-]?key|secret|token|password|passwd|credential)s?)(["']?\s*[:=]\s*["']?)[^\s"',;&<]+"#,
        )
        .unwrap(),
        long_run: Regex::new(r"[A-Za-z0-9_+]{40,}").unwrap(),
        email: Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap(),
    })
}

pub fn redact(text: &str, home: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let p = patterns();
    let text = mask_home(text, home);
    let text = p.homes.replace_all(&text, |c: &Captures| {
        let name = c.get(1).map_or("", |m| m.as_str()).to_lowercase();
        if KEPT_FOLDERS.contains(&name.as_str()) {
            c[0].to_string()
        } else {
            "~".to_string()
        }
    });
    let text = p.prefixed.replace_all(&text, SECRET);
    let text = p
        .bearer
        .replace_all(&text, |c: &Captures| format!("{} {SECRET}", &c[1]));
    let text = p
        .key_value
        .replace_all(&text, |c: &Captures| format!("{}{}{SECRET}", &c[1], &c[2]));
    let text = p.long_run.replace_all(&text, |c: &Captures| {
        let run = &c[0];
        let mixed = run.chars().any(|ch| ch.is_ascii_uppercase())
            && run.chars().any(|ch| ch.is_ascii_lowercase())
            && run.chars().any(|ch| ch.is_ascii_digit());
        if mixed {
            SECRET.to_string()
        } else {
            run.to_string()
        }
    });
    p.email.replace_all(&text, EMAIL).into_owned()
}

/// The exact home folder, for one outside the usual places (a test's, a
/// network home, a Windows profile on another drive): as written,
/// JSON-escaped with its backslashes doubled, and with forward slashes.
fn mask_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['/', '\\']);
    if home.chars().count() <= 1 {
        return text.to_string();
    }
    let mut forms: Vec<String> = Vec::new();
    for form in [
        home.replace('\\', "\\\\"),
        home.to_string(),
        home.replace('\\', "/"),
    ] {
        if !forms.contains(&form) {
            forms.push(form);
        }
    }
    let mut text = text.to_string();
    for form in &forms {
        text = mask_exact(&text, form);
    }
    text
}

/// Each `home` in `text` becomes `~`, unless the next character carries the name on.
fn mask_exact(text: &str, home: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(home) {
        let after = &rest[index + home.len()..];
        let continues = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-');
        out.push_str(&rest[..index]);
        out.push_str(if continues { home } else { "~" });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        #[serde(rename = "in")]
        input: String,
        out: String,
        home: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        home: String,
        cases: Vec<Case>,
    }

    /// The Mac app's tests read the same file: one masking for every system.
    #[test]
    fn masks_like_the_shared_fixture() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../Tests/Fixtures/diagnostics-redaction.json"
        ))
        .unwrap();
        assert!(fixture.cases.len() >= 25);
        for case in &fixture.cases {
            let home = case.home.as_deref().unwrap_or(&fixture.home);
            assert_eq!(
                redact(&case.input, home),
                case.out,
                "masking {:?}",
                case.input
            );
            assert_eq!(
                redact(&case.out, home),
                case.out,
                "masking twice {:?}",
                case.out
            );
        }
    }

    #[test]
    fn windows_homes_are_masked_exactly() {
        assert_eq!(
            redact(r"log C:\Users\jane\AppData\Local", r"C:\Users\jane"),
            r"log ~\AppData\Local"
        );
        assert_eq!(redact("root /", "/"), "root /");
        assert_eq!(redact("", "/Users/jane"), "");
    }
}
