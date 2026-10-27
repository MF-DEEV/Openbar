// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! "Skip text that looks sensitive" (`ClipboardHistorySensitiveText`,
//! CHSup:1073-1130): a named credential with a value, or one long unbroken
//! word mixing letters, digits and symbols. Links and GUIDs stay ordinary
//! text. Content an app marks as private is skipped whatever this says
//! (native.rs reads those marks).

use std::sync::OnceLock;

use regex::Regex;

/// A mention of "token" in a command or an error is not a credential: this
/// needs a named field with a nonempty value, including environment
/// variables, quoted JSON keys, Authorization headers and options such as
/// `--password VALUE`. Key names may go on past the credential word
/// (SECRET_KEY, aws_secret_access_key, password_confirmation, PASSWORD_2),
/// but only with these endings, so max_tokens or token_count stay ordinary.
/// The Mac's `(?<![\w-])` becomes a leading boundary group: Rust's regex has
/// no lookbehind, and either way the match has to start a word.
fn credential_assignment() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let name = r"(?:password|passwd|secret|token|api[_-]?key|private[_-]?key|authorization)";
        let ending = r"(?:[_-]?(?:access[_-]?)?key(?:[_-]?base)?|[_-]?confirmation|[_-]?\d{1,3})?";
        let pattern = format!(
            r#"(?i)(?:^|[^\w-])(?:[\w-]*{name}{ending}["']?[ \t]*[:=][ \t]*|--[\w-]*{name}{ending}[ \t]+)["']?[^\s"'`,;{{}}\[\]]"#
        );
        Regex::new(&pattern).expect("credential pattern")
    })
}

pub fn looks_sensitive(text: &str) -> bool {
    if credential_assignment().is_match(text) {
        return true;
    }
    if is_web_url(text) || is_identifier_code(text) {
        return false;
    }
    let count = text.chars().count();
    if !(20..=160).contains(&count) || text.chars().any(char::is_whitespace) {
        return false;
    }
    let letter = text.chars().any(char::is_alphabetic);
    let digit = text.chars().any(char::is_numeric);
    let symbol = text.chars().any(|c| !c.is_alphanumeric() && !c.is_whitespace());
    letter && digit && symbol
}

/// Thirty-two hex digits in five dashed groups, optionally in braces: the
/// one shape every system uses for a generated identifier (issue #423).
fn is_identifier_code(text: &str) -> bool {
    let value = text.trim();
    let value = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(value);
    let groups: Vec<&str> = value.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// An http or https link with a host, as `URL(string:)` reads one.
fn is_web_url(text: &str) -> bool {
    let value = text.trim();
    let lower = value.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    // A host comes first: "https://?x" or "https:///x" has none.
    rest.is_some_and(|r| r.chars().next().is_some_and(|c| !matches!(c, '/' | '?' | '#' | '@' | ':')))
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::looks_sensitive;

    #[test]
    fn named_credentials_with_values_are_sensitive() {
        for text in [
            "password=hunter2",
            "export API_KEY=abc123",
            r#"{"secret_key": "s3cr3t"}"#,
            "Authorization: Bearer abc.def",
            "aws_secret_access_key = AKIA",
            "mysql --password hunter2",
            "PASSWORD_2: x",
            "SECRET_KEY_BASE=1",
            "line one\ntoken: abc",
        ] {
            assert!(looks_sensitive(text), "{text}");
        }
    }

    #[test]
    fn mentions_and_lookalike_keys_are_ordinary_text() {
        for text in [
            "the token expired",
            "max_tokens: 4096",
            "token_count = 3",
            "password:",
            "password =  \nnext line",
            "my-password-reset is broken",
        ] {
            assert!(!looks_sensitive(text), "{text}");
        }
    }

    #[test]
    fn long_mixed_words_are_sensitive_but_links_and_guids_are_not() {
        assert!(looks_sensitive("ghp_A1b2C3d4E5f6G7h8I9j0"));
        assert!(!looks_sensitive("https://example.com/a1b2c3d4e5?x=1&y=2"));
        assert!(!looks_sensitive("HTTPS://example.com/a1b2c3d4e5f6?x=1"));
        assert!(looks_sensitive("https://?a1b2c3d4e5f6g7h8"));
        assert!(!looks_sensitive("{0F8FAD5B-D9CB-469F-A165-70867728950E}"));
        assert!(!looks_sensitive("0f8fad5b-d9cb-469f-a165-70867728950e"));
        // Too short, a space inside, or no digit.
        assert!(!looks_sensitive("a1-b2"));
        assert!(!looks_sensitive("abc123!@# abc123!@# abc"));
        assert!(!looks_sensitive("abcdefghij-klmnopqrstuv"));
    }
}
