// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The checks a reply to a Discord notification passes before anything is
/// typed, the same on every platform (windows/src-tauri/src/discord_reply.rs).
///
/// What Discord shows, from its client code (checked 2026-11-08):
/// - the notification title is `Sender (#channel, Server)` for a server
///   channel and `Sender` for a direct message;
/// - the window title is `[(n) ]#channel | Server` in the desktop app,
///   `@Name` for a DM;
/// - the message box is labelled with its placeholder, `Message #channel` or
///   `Message @Name` in the client's language, and an empty one holds U+FEFF.
enum DiscordReplySupport {
    /// Longest reply Discord takes without Nitro.
    static let limit = 2_000
    /// The stable client, the only one replies go to; Canary and PTB name
    /// themselves differently and get no Reply.
    static let stableBundleIdentifier = "com.hnc.Discord"

    enum Target: Equatable {
        case channel(name: String, server: String)
        case directMessage(String)
    }

    /// Why a reply was not sent. Nothing is typed in any of these cases
    /// except `typingInterrupted`, and Enter is never pressed.
    enum Failure: String, Error, CaseIterable {
        case noChat, tooLong, openFailed, inspectFailed, wrongChat, draftInDiscord
        case focusLost, typingInterrupted, inputRefused, gone, unavailable
    }

    static func target(_ title: String) -> Target? {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, title.caseInsensitiveCompare("discord") != .orderedSame else { return nil }
        if let open = title.range(of: " (#", options: .backwards) {
            let context = title[open.upperBound...]
            guard context.hasSuffix(")"), let comma = context.firstIndex(of: ",") else { return nil }
            let name = context[..<comma].trimmingCharacters(in: .whitespaces)
            let server = context[context.index(after: comma)..<context.index(before: context.endIndex)]
                .trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty, !server.isEmpty else { return nil }
            return .channel(name: name.lowercased(), server: server.lowercased())
        }
        var name = Substring(title)
        if let open = name.range(of: " (") { name = name[..<open.lowerBound] }
        let trimmed = name.trimmingCharacters(in: .whitespaces).drop { $0 == "@" }
        return trimmed.isEmpty ? nil : .directMessage(String(trimmed).lowercased())
    }

    static func matches(_ target: Target, composer: String, window: String) -> Bool {
        let composer = composer.lowercased(), window = window.lowercased()
        switch target {
        case let .channel(name, server):
            return containsName(composer, "#" + name) && containsName(window, server)
        case let .directMessage(name):
            return containsName(composer, "@" + name) && containsName(window, "@" + name)
        }
    }

    /// `name` appears in `haystack` as a whole word: not inside a longer
    /// channel or user name such as `#general-chat`.
    static func containsName(_ haystack: String, _ name: String) -> Bool {
        guard !name.isEmpty else { return false }
        func boundary(_ ch: Character?) -> Bool {
            guard let ch else { return true }
            return !(ch.isLetter || ch.isNumber) && ch != "-" && ch != "_"
        }
        var searchStart = haystack.startIndex
        while let found = haystack.range(of: name, range: searchStart..<haystack.endIndex) {
            let before = found.lowerBound == haystack.startIndex ? nil : haystack[haystack.index(before: found.lowerBound)]
            let after = found.upperBound == haystack.endIndex ? nil : haystack[found.upperBound]
            if boundary(before), boundary(after) { return true }
            searchStart = haystack.index(after: found.lowerBound)
        }
        return false
    }

    /// Chromium's empty editor can report a zero-width placeholder character,
    /// and some builds report the placeholder (the editor's label) as text.
    static func isBlank(_ value: String, label: String) -> Bool {
        let invisible: Set<Unicode.Scalar> = ["\u{200B}", "\u{200C}", "\u{200D}", "\u{2060}", "\u{FEFF}"]
        func normalize(_ text: String) -> String {
            String(String.UnicodeScalarView(text.unicodeScalars.filter {
                !$0.properties.isWhitespace && !invisible.contains($0)
            })).lowercased()
        }
        let value = normalize(value)
        return value.isEmpty || (!label.trimmingCharacters(in: .whitespaces).isEmpty && value == normalize(label))
    }

    /// Only an image link the Discord notification text itself carries, on
    /// Discord's own attachment hosts.
    static func imageURL(in body: String) -> URL? {
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return nil }
        let range = NSRange(body.startIndex..., in: body)
        for match in detector.matches(in: body, range: range) {
            guard let url = match.url, url.scheme == "https",
                  let host = url.host?.lowercased(), ["cdn.discordapp.com", "media.discordapp.net"].contains(host),
                  url.path.hasPrefix("/attachments/"),
                  ["png", "jpg", "jpeg", "gif", "webp", "avif"].contains(url.pathExtension.lowercased()) else { continue }
            return url
        }
        return nil
    }
}
