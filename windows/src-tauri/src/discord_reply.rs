// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

//! Replies to a Discord notification through the person's running Discord
//! client. Never uses an account token. A reply is typed only when the visible
//! message box belongs to the conversation the notification names and holds no
//! unsent text, and Enter is pressed only while Discord still has the keyboard.
//! Errors are codes the island translates (notifications/reply.ts).
//!
//! What Discord shows, from its client code (checked 2026-11-08):
//! - the notification title is `Sender (#channel, Server)` for a server channel
//!   and `Sender` for a direct message;
//! - the window title is `[(n) ]#channel | Server` in the desktop app
//!   (`Discord | #channel | Server` in a browser), `@Name` for a DM;
//! - the message box is labelled with its placeholder, `Message #channel` or
//!   `Message @Name` in the client's language, and an empty one holds U+FEFF.
//!
//! The platform part finds the window and the box, reads it, and types:
//! UI Automation on Windows, X11 and AT-SPI on Linux.

#[cfg(windows)]
#[path = "discord_reply_windows.rs"]
mod imp;
#[cfg(target_os = "linux")]
#[path = "linux/discord_reply.rs"]
mod imp;
#[cfg(any(windows, target_os = "linux"))]
pub use imp::send;

/// Longest reply Discord takes without Nitro.
pub(crate) const LIMIT: usize = 2000;

/// The stable client, the only one the platform parts look for: its Windows
/// AUMID, and its desktop entry on Linux (package, Flatpak). Canary, PTB and
/// Development name themselves differently and get no Reply
/// (notifications/presentation.ts keeps the same list).
pub(crate) fn is_stable_client(app_id: &str) -> bool {
    [
        "com.squirrel.discord.discord",
        "discord",
        "com.discordapp.discord",
    ]
    .contains(&app_id.to_ascii_lowercase().as_str())
}

#[derive(Debug, PartialEq)]
pub(crate) enum Target {
    Channel { name: String, server: String },
    DirectMessage(String),
}

pub(crate) fn target(title: &str) -> Option<Target> {
    let title = title.trim();
    if title.is_empty() || title.eq_ignore_ascii_case("discord") {
        return None;
    }
    if let Some((_, context)) = title.rsplit_once(" (#") {
        let (channel, server) = context.strip_suffix(')')?.split_once(',')?;
        let name = channel.trim();
        let server = server.trim();
        if !name.is_empty() && !server.is_empty() {
            return Some(Target::Channel {
                name: name.to_lowercase(),
                server: server.to_lowercase(),
            });
        }
        return None;
    }
    let name = title.split(" (").next()?.trim().trim_start_matches('@');
    (!name.is_empty()).then(|| Target::DirectMessage(name.to_lowercase()))
}

pub(crate) fn matches_target(target: &Target, composer: &str, window: &str) -> bool {
    let composer = composer.to_lowercase();
    let window = window.to_lowercase();
    match target {
        Target::Channel { name, server } => {
            contains_name(&composer, &format!("#{name}")) && contains_name(&window, server)
        }
        Target::DirectMessage(name) => {
            contains_name(&composer, &format!("@{name}"))
                && contains_name(&window, &format!("@{name}"))
        }
    }
}

fn contains_name(haystack: &str, name: &str) -> bool {
    haystack.match_indices(name).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + name.len()..].chars().next();
        let boundary =
            |ch: Option<char>| ch.is_none_or(|ch| !ch.is_alphanumeric() && ch != '-' && ch != '_');
        boundary(before) && boundary(after)
    })
}

/// Chromium's empty editor can report a zero-width placeholder character, and some
/// builds report the placeholder ("Message #general", the editor's name) as text.
pub(crate) fn is_blank(value: &str, editor_name: &str) -> bool {
    let normalize = |text: &str| {
        text.chars()
            .filter(|ch| {
                !ch.is_whitespace()
                    && !matches!(
                        ch,
                        '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
                    )
            })
            .collect::<String>()
            .to_lowercase()
    };
    let value = normalize(value);
    value.is_empty() || (!editor_name.trim().is_empty() && value == normalize(editor_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_channel_and_direct_message() {
        assert_eq!(
            target("Alicja (#general, TEST)"),
            Some(Target::Channel {
                name: "general".into(),
                server: "test".into()
            })
        );
        assert_eq!(
            target("Alicja"),
            Some(Target::DirectMessage("alicja".into()))
        );
        assert_eq!(target("Discord"), None);
    }

    #[test]
    fn treats_only_an_empty_editor_as_blank() {
        assert!(is_blank("", "Message #general"));
        assert!(is_blank("\u{feff}", "Message #general"));
        assert!(is_blank(" \n\u{200b}", "Message #general"));
        assert!(is_blank("Message #general", "Message #general"));
        assert!(!is_blank("half written", "Message #general"));
        assert!(!is_blank("Message", "Message #general"));
        assert!(!is_blank("x", ""));
        assert!(is_blank("", ""));
    }

    #[test]
    fn only_the_stable_client_gets_replies() {
        for id in [
            "com.squirrel.Discord.Discord",
            "discord",
            "com.discordapp.Discord",
        ] {
            assert!(is_stable_client(id), "{id}");
        }
        for id in [
            "com.squirrel.DiscordCanary.DiscordCanary",
            "discord-canary",
            "discord-ptb",
            "com.discordapp.DiscordCanary",
            "slack",
        ] {
            assert!(!is_stable_client(id), "{id}");
        }
    }

    #[test]
    fn matches_the_titles_discord_uses() {
        let channel = target("Alicja (#general, TEST)").unwrap();
        for window in [
            "#general | TEST",
            "(3) #general | TEST",
            "\u{2022} #general | TEST",
            "Discord | #general | TEST",
        ] {
            assert!(
                matches_target(&channel, "Message #general", window),
                "{window}"
            );
        }
        let dm = target("Alicja").unwrap();
        assert!(matches_target(&dm, "Message @Alicja", "@Alicja"));
        assert!(matches_target(&dm, "Napisz do @Alicja", "(1) @Alicja"));
        assert!(!matches_target(&dm, "Message @Alicja2", "@Alicja2"));
        assert!(!matches_target(&dm, "Message #general", "#general | TEST"));
    }

    #[test]
    fn never_sends_to_another_channel_or_server() {
        let expected = target("Alicja (#general, TEST)").unwrap();
        assert!(matches_target(
            &expected,
            "Napisz na #general",
            "#general - TEST - Discord"
        ));
        assert!(!matches_target(
            &expected,
            "Napisz na #general",
            "#general - Friends - Discord"
        ));
        assert!(!matches_target(
            &expected,
            "Napisz na #general-chat",
            "#general - TEST - Discord"
        ));
    }
}
