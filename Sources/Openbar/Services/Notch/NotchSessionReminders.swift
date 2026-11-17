// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import Foundation

/// What a reminder can be about (#231): a permission or a question nobody
/// answered, or a finished turn nobody looked at.
enum SessionReminderKind: String, CaseIterable, Hashable {
    case permission, question, finished

    /// The sound the event itself plays (#262) is the reminder's sound too.
    var sound: SessionSoundKind {
        switch self {
        case .permission: .permission
        case .question: .question
        case .finished: .finished
        }
    }
}

/// When a session has waited a while and nobody reacted, the island
/// reminds the person once more (#231). The sessions on hand are the
/// authority: a wait answered in the island or in the terminal is gone from
/// them, so its reminder goes with it. The same as Windows'
/// `sessions/reminders.ts`.
struct NotchSessionReminders {
    /// How often the waits are read.
    static let checkEvery: TimeInterval = 15
    /// How long after an unlock or a wake a held reminder comes, so the
    /// island is back and the away card (#215) has opened first.
    static let afterUnlock: TimeInterval = 2
    /// The longest the terminals and Control Center are asked.
    static let checkLimit: TimeInterval = 3

    struct Settings: Equatable {
        var permission = false
        var question = false
        var finished = false
        /// How long a wait goes before the first reminder.
        var after: TimeInterval = 5 * 60
        /// How often it comes back while nobody reacts; nil for once.
        var every: TimeInterval?
        /// Whether to remind while Focus or Do Not Disturb is on.
        var duringFocus = false

        var anyOn: Bool { permission || question || finished }

        func allows(_ kind: SessionReminderKind) -> Bool {
            switch kind {
            case .permission: permission
            case .question: question
            case .finished: finished
            }
        }

        static let afterChoices = [2, 5, 10, 15]
        /// Minutes between repeats; 0 is once.
        static let everyChoices = [0, 5, 10, 15, 30]

        static func load(_ defaults: UserDefaults = .standard) -> Settings {
            let after = defaults.object(forKey: DefaultsKey.notchSessionsRemindAfter) as? Int ?? 5
            let every = defaults.object(forKey: DefaultsKey.notchSessionsRemindEvery) as? Int ?? 0
            return Settings(permission: defaults.bool(forKey: DefaultsKey.notchSessionsRemindPermission),
                            question: defaults.bool(forKey: DefaultsKey.notchSessionsRemindQuestion),
                            finished: defaults.bool(forKey: DefaultsKey.notchSessionsRemindFinished),
                            after: TimeInterval(max(1, after) * 60),
                            every: every > 0 ? TimeInterval(every * 60) : nil,
                            duringFocus: defaults.bool(forKey: DefaultsKey.notchSessionsRemindDuringFocus))
        }
    }

    /// One thing a session waits on the person for, and since when.
    struct Wait: Equatable {
        let session: String
        let kind: SessionReminderKind
        /// The prompt, as NotchSessionAttention names it, or the turn's end.
        let request: NotchSessionAttention.Request
        let since: Date

        func isSame(as other: Wait) -> Bool {
            session == other.session && kind == other.kind && request.isSame(as: other.request)
        }
    }

    /// A finished turn nobody looked at counts this long. Read in the
    /// terminal, a turn never shows as looked at, so an old one is let go
    /// rather than brought up again and again.
    static let finishedFor: TimeInterval = 60 * 60

    /// What the session waits on the person for now, if anything.
    static func wait(of session: ClaudeSession, now: Date = Date()) -> Wait? {
        if let request = NotchSessionAttention.request(of: session) {
            let kind: SessionReminderKind = session.phase == .waitingInput || session.permission?.asksQuestions == true
                ? .question : .permission
            return Wait(session: session.id, kind: kind, request: request,
                        since: session.permission?.since ?? session.phaseSince)
        }
        guard let ended = unseenTurnEnd(session), now.timeIntervalSince(ended) < finishedFor else { return nil }
        return Wait(session: session.id, kind: .finished,
                    request: .init(wait: "finished:\(ended.timeIntervalSinceReferenceDate)", id: nil), since: ended)
    }

    /// When the turn the person has not looked at yet ended. Not the
    /// `.finished` phase, which turns idle after a while by itself: a turn
    /// stays unseen until its page is opened or the next one begins.
    static func unseenTurnEnd(_ session: ClaudeSession) -> Date? {
        guard session.phase == .finished || session.phase == .idle, session.background == nil,
              session.turnCompleted, let ended = session.turnEndedAt,
              ended >= (session.turnStarted ?? .distantPast) else { return nil }
        if let looked = session.lookedAt, looked >= ended { return nil }
        // A turn that stopped on an error is told by its own sound, not this.
        if let failed = session.failedAt, failed >= (session.turnStarted ?? .distantPast) { return nil }
        return ended
    }

    /// Each session's wait and when it was last reminded of (or seen in its
    /// own terminal), so the same wait is not brought up again too soon.
    private var reminded: [String: (wait: Wait, at: Date)] = [:]

    /// The waits that call for a reminder now, oldest first. A wait already
    /// due brings along every other one that has waited at least half the
    /// time, so sessions that wait together give one reminder, not a burst.
    mutating func due(_ sessions: [ClaudeSession], settings: Settings, now: Date) -> [Wait] {
        let waits = sessions.compactMap { Self.wait(of: $0, now: now) }
        // A wait that ended, or turned into another, forgets its reminder.
        reminded = reminded.filter { id, entry in waits.contains { $0.session == id && $0.isSame(as: entry.wait) } }
        guard settings.anyOn else { return [] }
        let wanted = waits.filter { settings.allows($0.kind) && $0.since <= now }
        // `slack` lets a wait come along a little early with one that is due.
        func ready(_ wait: Wait, slack: TimeInterval) -> Bool {
            if let last = reminded[wait.session]?.at {
                guard let every = settings.every else { return false }
                return now.timeIntervalSince(last) >= every - slack
            }
            return now.timeIntervalSince(wait.since) >= settings.after - slack
        }
        guard wanted.contains(where: { ready($0, slack: 0) }) else { return [] }
        return wanted.filter { ready($0, slack: settings.after / 2) }.sorted { $0.since < $1.since }
    }

    /// Notes the waits as reminded of, or as seen in their own terminal.
    mutating func mark(_ waits: [Wait], at now: Date) {
        for wait in waits { reminded[wait.session] = (wait, now) }
    }

    /// The sound of the most pressing wait, as each event plays it (#262).
    static func sound(for waits: [Wait]) -> SessionSoundKind? {
        waits.map(\.kind.sound).max { $0.priority < $1.priority }
    }

    /// Whole minutes a session has waited on a permission or a question,
    /// from one minute on; under that the row keeps its seconds clock.
    static func waitedMinutes(_ session: ClaudeSession, now: Date) -> Int? {
        guard NotchSessionAttention.request(of: session) != nil else { return nil }
        let minutes = Int(now.timeIntervalSince(session.permission?.since ?? session.phaseSince) / 60)
        return minutes >= 1 ? minutes : nil
    }

    /// The reminder may wait for Focus or Do Not Disturb to end.
    static func heldByFocus(_ settings: Settings, focus: Bool?) -> Bool {
        !settings.duringFocus && focus == true
    }
}
