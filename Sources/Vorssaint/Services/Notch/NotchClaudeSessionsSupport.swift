// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Decisions for the island's Sessions section, kept out of its views so they
/// can be tested.
enum NotchClaudeSessionsSupport {
    /// The section rides on the AI feature, which installs everything about
    /// coding agents, and has its own switch; it can also be hidden.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        NotchSupport.isEnabled(in: defaults) && NotchSupport.modules(in: defaults).contains(.sessions)
    }

    static func showsActivity(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsLiveActivity) as? Bool ?? true
    }

    static func autoExpands(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsAutoExpand) as? Bool ?? true
    }

    static func policy(in defaults: UserDefaults = .standard) -> ClaudeContentPolicy {
        ClaudeContentPolicy(previews: defaults.bool(forKey: DefaultsKey.notchSessionsContentPreview))
    }

    // MARK: Order

    /// Sessions that wait on the person come first, then the ones at work,
    /// then work in the background and unread results; quiet sessions follow
    /// from the one that worked last, then by folder.
    static func sorted(_ sessions: [ClaudeSession]) -> [ClaudeSession] {
        sessions.sorted { lhs, rhs in
            let left = rank(lhs.phase), right = rank(rhs.phase)
            if left != right { return left < right }
            switch lhs.phase {
            case .waitingPermission, .waitingInput:
                if lhs.phaseSince != rhs.phaseSince { return lhs.phaseSince < rhs.phaseSince }
            case .generating, .runningTool:
                let a = lhs.turnStarted ?? lhs.phaseSince, b = rhs.turnStarted ?? rhs.phaseSince
                if a != b { return a < b }
            case .background, .finished:
                if lhs.phaseSince != rhs.phaseSince { return lhs.phaseSince > rhs.phaseSince }
            case .idle:
                // The one that last worked comes first, so a session just
                // looked at does not sink among the long quiet ones.
                let a = lhs.lastActive ?? .distantPast, b = rhs.lastActive ?? .distantPast
                if a != b { return a > b }
            }
            let byName = lhs.project.localizedStandardCompare(rhs.project)
            if byName != .orderedSame { return byName == .orderedAscending }
            return lhs.pid < rhs.pid
        }
    }

    static func rank(_ phase: ClaudeSessionPhase) -> Int {
        switch phase {
        case .waitingPermission: return 0
        case .waitingInput: return 1
        case .runningTool, .generating: return 2
        case .background: return 3
        case .finished: return 4
        case .idle: return 5
        }
    }

    // MARK: Island

    /// Whether a session's change should open the island by itself: only a
    /// closed one, so a page or list the person has open never changes
    /// under the pointer, and never while the island is held back, as during
    /// a recording or in full screen.
    static func shouldAutoExpand(_ event: ClaudeSessionEvent, enabled: Bool, expanded: Bool, suspended: Bool) -> Bool {
        guard enabled, !suspended, !expanded else { return false }
        switch event {
        case .needsPermission, .needsInput: return true
        case .finished: return false
        }
    }

    /// How long the buttons wait after the request on screen changes.
    static let settleDelay: TimeInterval = 0.8

    /// The counts beside the camera, leaving out the empty ones.
    static func stripReading(_ counts: ClaudeSessionCounts) -> [(phase: ClaudeSessionPhase, count: Int)] {
        [(ClaudeSessionPhase.waitingPermission, counts.waiting), (.generating, counts.working), (.finished, counts.finished)]
            .filter { $0.count > 0 }
    }

    /// The counts as text, for measuring the strip.
    static func stripText(_ counts: ClaudeSessionCounts) -> String {
        stripReading(counts).map { "• \($0.count)" }.joined(separator: "  ")
    }

    /// How wide the strip draws the counts: a mark, its number, and room
    /// between them.
    static func stripReadingWidth(_ counts: ClaudeSessionCounts, textSize: CGFloat) -> CGFloat {
        let items = stripReading(counts)
        guard !items.isEmpty else { return 0 }
        let digits = items.map { String($0.count).count }.reduce(0, +)
        return CGFloat(items.count) * (textSize * 0.9 + 2) + CGFloat(digits) * textSize * 0.62
            + CGFloat(items.count - 1) * 6
    }

    static func symbol(_ phase: ClaudeSessionPhase) -> String {
        switch phase {
        case .waitingPermission: return "hand.raised.fill"
        case .waitingInput: return "questionmark.bubble.fill"
        case .runningTool: return "hammer.fill"
        case .generating: return "sparkle"
        case .background: return "hourglass"
        case .finished: return "checkmark.circle.fill"
        case .idle: return "moon.zzz.fill"
        }
    }

    // MARK: Replies

    /// A reply can be typed only where the session listens for one: never
    /// while a dialog waits, where the keys would answer it instead.
    static func canReply(_ session: ClaudeSession) -> Bool {
        // Keys typed while any dialog is open would go to the dialog.
        session.host.acceptsReplies && !session.phase.isWaiting && session.waitingFor == nil
    }

    static let replyLimit = 4000

    /// How many colors projects are told apart by.
    static let projectColors = 8

    /// A project's color, the same each time for the same folder name so a
    /// project is known by it at a glance.
    static func projectColor(_ project: String) -> Int {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in project.lowercased().utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return Int(hash % UInt64(projectColors))
    }

    /// What a session is called: the person's own name for it, then one
    /// given with /rename, the title of its conversation, then Claude Code's
    /// own name, which follows the folder.
    static func name(_ session: ClaudeSession, custom: String?) -> String {
        if let custom = custom?.trimmingCharacters(in: .whitespacesAndNewlines), !custom.isEmpty { return custom }
        if session.nameSource == "user", !session.name.isEmpty { return session.name }
        if let title = session.title, !title.isEmpty { return title }
        return session.name.isEmpty ? session.project : session.name
    }

    /// The longest name the island keeps, and how many it keeps at most.
    static let nameLimit = 60
    static let namesKept = 100

    /// The names with one changed. Names of sessions no longer running go
    /// first once there are too many.
    static func renamed(_ names: [String: String], id: String, to name: String, live: Set<String>) -> [String: String] {
        var result = names
        let trimmed = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(nameLimit))
        result[id] = trimmed.isEmpty ? nil : trimmed
        if result.count > namesKept {
            for key in result.keys.sorted() where !live.contains(key) && key != id && result.count > namesKept {
                result[key] = nil
            }
        }
        return result
    }

    /// What the conversation says Claude is doing while a turn runs, picked
    /// once per turn as Claude Code's own spinner does.
    static let thinkingWords = [
        "Thinking", "Pondering", "Brewing", "Musing", "Noodling", "Percolating", "Simmering", "Tinkering",
        "Mulling", "Crafting", "Conjuring", "Pontificating", "Ruminating", "Churning", "Whirring", "Cogitating",
    ]
    static func thinkingWord(turn: Date?) -> String {
        let seed = Int((turn?.timeIntervalSinceReferenceDate ?? 0).rounded(.down))
        return thinkingWords[((seed % thinkingWords.count) + thinkingWords.count) % thinkingWords.count]
    }

    /// The spinner's glyphs, played forward and back.
    static let thinkingGlyphs = ["·", "✢", "✳", "✶", "✻", "✽"]
    static func thinkingGlyph(step: Int) -> String {
        let cycle = thinkingGlyphs.count * 2 - 2
        let place = ((step % cycle) + cycle) % cycle
        return thinkingGlyphs[place < thinkingGlyphs.count ? place : cycle - place]
    }

    /// What the conversation sets apart: each reply a turn ended on, which
    /// sums the turn up, and the replies written on the way to one. A turn
    /// needs no prompt of its own, as one a slash command, a finished task or
    /// a timer began; replies of a turn still running are neither.
    static func answers(in chat: [ClaudeChatMessage]) -> (answers: Set<Int>, onTheWay: Set<Int>) {
        var answers = Set<Int>(), onTheWay = Set<Int>()
        var answered = false
        for message in chat.reversed() {
            if message.role == .user {
                answered = false
            } else if message.endsTurn {
                answers.insert(message.id)
                answered = true
            } else if answered {
                onTheWay.insert(message.id)
            }
        }
        return (answers, onTheWay)
    }

    /// Text sizes a session's enlarged page offers.
    static let largeTextSizes: ClosedRange<Double> = 12...24
    static func largeTextSize(_ size: Double) -> Double {
        guard size.isFinite else { return largeTextSizes.lowerBound + 3 }
        return min(max(size.rounded(), largeTextSizes.lowerBound), largeTextSizes.upperBound)
    }

    /// Claude writes Markdown; bold, italics, code and links read as such,
    /// and line breaks stay. A fenced block keeps its lines as written, in
    /// code type; anything else that does not parse shows as written.
    static func markdown(_ text: String) -> AttributedString {
        var result = AttributedString()
        for (index, part) in text.components(separatedBy: "```").enumerated() {
            guard !index.isMultiple(of: 2) else {
                result += inlineMarkdown(part)
                continue
            }
            var code = Substring(part)
            // The fence's own line names the language, which is not shown.
            if let line = code.firstIndex(of: "\n"), !code[..<line].contains(" ") { code = code[code.index(after: line)...] }
            if code.hasSuffix("\n") { code = code.dropLast() }
            var block = AttributedString(code)
            block.inlinePresentationIntent = .code
            result += block
        }
        return linked(result)
    }

    /// Addresses written out, which Markdown leaves as text, become links;
    /// code keeps them as written.
    static func linked(_ text: AttributedString) -> AttributedString {
        var result = text
        let string = String(result.characters)
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return result }
        for match in detector.matches(in: string, range: NSRange(string.startIndex..., in: string)) {
            guard let url = match.url, ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
                  let range = Range(match.range, in: string) else { continue }
            let start = result.index(result.startIndex, offsetByCharacters: string.distance(from: string.startIndex, to: range.lowerBound))
            let end = result.index(start, offsetByCharacters: string.distance(from: range.lowerBound, to: range.upperBound))
            let piece = result[start..<end]
            let taken = piece.runs.contains { $0.link != nil || $0.inlinePresentationIntent?.contains(.code) == true }
            if !taken { result[start..<end].link = url }
        }
        return result
    }

    private static func inlineMarkdown(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }

    /// ⌘Y allows and ⌘N denies, as in the dialogs that ask the same.
    static func decision(key: String?, commandOnly: Bool) -> ClaudeHookMessage.Decision? {
        guard commandOnly else { return nil }
        switch key?.lowercased() {
        case "y": return .allow
        case "n": return .deny
        default: return nil
        }
    }

    /// The reply as it goes to the terminal, or nil when nothing is left.
    static func reply(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(replyLimit))
    }
}
