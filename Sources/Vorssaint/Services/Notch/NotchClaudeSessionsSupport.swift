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
    /// then unread results; quiet sessions keep a steady order by folder.
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
            case .finished:
                if lhs.phaseSince != rhs.phaseSince { return lhs.phaseSince > rhs.phaseSince }
            case .idle: break
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
        case .finished: return 3
        case .idle: return 4
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
        case .finished: return "checkmark.circle.fill"
        case .idle: return "moon.zzz.fill"
        }
    }

    // MARK: Replies

    /// A reply can be typed only where the session listens for one: never
    /// while a dialog waits, where the keys would answer it instead.
    static func canReply(_ session: ClaudeSession) -> Bool {
        session.host.acceptsReplies && !session.phase.isWaiting
    }

    static let replyLimit = 4000

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
