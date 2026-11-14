// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// "While you were away" (#215): how the sessions stood when the person was
/// last at the Mac, and what changed by the time they came back. Built from
/// what the island already knows; no AI service is asked anything.
enum NotchAway {
    /// One session as it stood.
    struct Before: Equatable {
        var phase: ClaudeSessionPhase
        var done: Int
        var added: Int
        var removed: Int
    }

    /// The sessions when the person was last seen.
    struct Mark: Equatable {
        var at: Date
        var sessions: [String: Before]
    }

    enum Outcome: Int, Comparable {
        case waiting, error, limited, finished, working
        static func < (a: Outcome, b: Outcome) -> Bool { a.rawValue < b.rawValue }
    }

    /// One tile of the card: a session that changed, by id.
    struct Line: Equatable, Identifiable {
        var id: String
        var outcome: Outcome
        var files: Int
        var added: Int
        var removed: Int
        /// Tasks ticked off while away, and where the list stands.
        var tasks: Tasks?

        struct Tasks: Equatable {
            var from: Int
            var to: Int
            var total: Int
        }
    }

    static func mark(_ sessions: [ClaudeSession], at date: Date) -> Mark {
        Mark(at: date, sessions: Dictionary(sessions.map {
            ($0.id, Before(phase: $0.phase, done: $0.tasks?.done ?? 0, added: $0.edits?.added ?? 0, removed: $0.edits?.removed ?? 0))
        }, uniquingKeysWith: { first, _ in first }))
    }

    /// What each session did since the mark; a session that did nothing has no line.
    static func lines(since mark: Mark, sessions: [ClaudeSession]) -> [Line] {
        var lines: [Line] = []
        for session in sessions {
            let before = mark.sessions[session.id]
            guard let outcome = outcome(session, before: before, since: mark.at) else { continue }
            let files = (session.edits?.touched ?? []).filter { $0 > mark.at }.count
            var line = Line(id: session.id, outcome: outcome, files: files,
                            added: files > 0 ? max(0, (session.edits?.added ?? 0) - (before?.added ?? 0)) : 0,
                            removed: files > 0 ? max(0, (session.edits?.removed ?? 0) - (before?.removed ?? 0)) : 0)
            if let tasks = session.tasks, tasks.total > 0 {
                let from = min(before?.done ?? 0, tasks.done)
                if tasks.done > from { line.tasks = Line.Tasks(from: from, to: tasks.done, total: tasks.total) }
            }
            // A session still at work shows only when it moved on.
            if outcome == .working, line.files == 0, line.tasks == nil { continue }
            lines.append(line)
        }
        return lines.enumerated().sorted { ($0.element.outcome, $0.offset) < ($1.element.outcome, $1.offset) }.map(\.element)
    }

    private static func outcome(_ session: ClaudeSession, before: Before?, since: Date) -> Outcome? {
        let changed = session.phaseSince > since || before == nil || before?.phase != session.phase
        if session.phase.isWaiting { return changed ? .waiting : nil }
        if session.phase == .limited { return changed ? .limited : nil }
        if let failed = session.failedAt, failed > since, !session.phase.isWorking { return .error }
        if session.phase == .finished || session.phase == .idle {
            let wasWorking = before.map { $0.phase.isWorking || $0.phase.isWaiting } ?? ((session.turnStarted ?? .distantPast) > since)
            return wasWorking && (session.lastActive ?? session.phaseSince) > since ? .finished : nil
        }
        return session.phase.isWorking ? .working : nil
    }
}
