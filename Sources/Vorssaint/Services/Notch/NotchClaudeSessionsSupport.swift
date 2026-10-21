// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Decisions for the island's Sessions section, kept out of its views so they
/// can be tested.
enum NotchClaudeSessionsSupport {
    /// The section rides on the AI feature, which installs everything about
    /// coding agents, and has its own switch.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        NotchSupport.isEnabled(in: defaults) && AppFeature.notchAgents.isAvailable(in: defaults)
            && defaults.bool(forKey: DefaultsKey.notchSessionsEnabled)
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

    /// Whether a session's change should open the island by itself. It never
    /// takes the island from a page the person has open, and stays shut while
    /// the island is held back, as during a recording or in full screen.
    static func shouldAutoExpand(_ event: ClaudeSessionEvent, enabled: Bool, expanded: Bool,
                                 showingSessions: Bool, suspended: Bool) -> Bool {
        guard enabled, !suspended else { return false }
        switch event {
        case .needsPermission, .needsInput: return !expanded || showingSessions
        case .finished: return false
        }
    }

    /// The counts beside the camera, leaving out the empty ones.
    static func stripReading(_ counts: ClaudeSessionCounts) -> [(phase: ClaudeSessionPhase, count: Int)] {
        [(ClaudeSessionPhase.waitingPermission, counts.waiting), (.generating, counts.working), (.finished, counts.finished)]
            .filter { $0.count > 0 }
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

    /// The reply as it goes to the terminal, or nil when nothing is left.
    static func reply(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(replyLimit))
    }
}
