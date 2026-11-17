// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The same cases as windows/src-tauri/src/discord_reply.rs, so the three
/// platforms agree on what a reply may be typed into.
enum DiscordReplyTests {
    static func run(_ suite: TestSuite) {
        typealias S = DiscordReplySupport
        suite.expect(S.target("Alicja (#general, TEST)") == .channel(name: "general", server: "test")
                     && S.target("Alicja") == .directMessage("alicja")
                     && S.target("Discord") == nil && S.target("  ") == nil,
                     "a Discord title names a server channel or a direct message")

        let channel = S.target("Alicja (#general, TEST)")!
        suite.expect(["#general | TEST", "(3) #general | TEST", "\u{2022} #general | TEST", "Discord | #general | TEST"]
                        .allSatisfy { S.matches(channel, composer: "Message #general", window: $0) },
                     "the desktop and browser window titles of a channel match, unread counts included")
        suite.expect(S.matches(channel, composer: "Napisz na #general", window: "#general | TEST")
                     && !S.matches(channel, composer: "Napisz na #general", window: "#general | Friends")
                     && !S.matches(channel, composer: "Napisz na #general-chat", window: "#general | TEST"),
                     "a reply never goes to another channel or server, in any client language")

        let dm = S.target("Alicja")!
        suite.expect(S.matches(dm, composer: "Message @Alicja", window: "@Alicja")
                     && S.matches(dm, composer: "Napisz do @Alicja", window: "(1) @Alicja")
                     && !S.matches(dm, composer: "Message @Alicja2", window: "@Alicja2")
                     && !S.matches(dm, composer: "Message #general", window: "#general | TEST"),
                     "a direct message matches only its own conversation")

        suite.expect(S.isBlank("", label: "Message #general") && S.isBlank("\u{FEFF}", label: "Message #general")
                     && S.isBlank(" \n\u{200B}", label: "Message #general")
                     && S.isBlank("Message #general", label: "Message #general")
                     && !S.isBlank("half written", label: "Message #general")
                     && !S.isBlank("Message", label: "Message #general")
                     && !S.isBlank("x", label: "") && S.isBlank("", label: ""),
                     "only an empty message box counts as empty, so a draft there is never sent along")

        let image = "https://cdn.discordapp.com/attachments/123/456/photo.png?ex=abc"
        suite.expect(S.imageURL(in: "Photo: \(image)")?.absoluteString == image
                     && S.imageURL(in: "https://evil.example/attachments/123/photo.png") == nil
                     && S.imageURL(in: "https://cdn.discordapp.com.evil.example/attachments/1/p.png") == nil
                     && S.imageURL(in: "https://cdn.discordapp.com/attachments/1/2/secret.txt") == nil
                     && S.imageURL(in: "sent an image") == nil,
                     "only Discord's own attachment images are shown")

        suite.expect(AppLanguage.allCases.allSatisfy { language in
            let text = FeatureStrings.discordReply(language)
            return S.Failure.allCases.allSatisfy { !(text.failures[$0] ?? "").isEmpty }
                && text.replyToFormat.contains("%@") && !text.reply.isEmpty && !text.send.isEmpty
        }, "every language says every reply failure")

        let content = NotchNotificationContent(app: "Discord", title: "Alicja", subtitle: "", body: "Hi \(image)")
        let plain = NotchNotificationContent(app: "Slack", title: "Alicja", subtitle: "", body: "Hi \(image)")
        let width: CGFloat = 300
        let base = NotchNotificationPreviewLayout.contentHeight(for: plain, width: width)
        suite.expect(NotchNotificationPreviewLayout.contentHeight(for: content, width: width)
                        == base + NotchNotificationPreviewLayout.spacing + NotchNotificationPreviewLayout.imageHeight
                     && NotchNotificationPreviewLayout.contentHeight(for: plain, width: width, replying: true)
                        == base + NotchNotificationPreviewLayout.spacing + NotchNotificationPreviewLayout.replyHeight,
                     "the held preview grows by the picture and the reply field it draws")
    }
}
