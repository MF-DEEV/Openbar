// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Every ask the island can answer, across sessions, in one list (#269):
/// "3 waiting", each ask on its own row, and Allow all for the ones that
/// look safe. Oldest first, so a new ask joins at the foot and the row being
/// read never moves.
struct NotchPermissionQueue: Equatable {
    struct Row: Equatable, Identifiable {
        let session: ClaudeSession
        let ask: ClaudePermission
        var id: String { session.id + "\u{1}" + ask.requestID }

        /// Answered from the row, without opening its page: not a question,
        /// a plan, or a command that has to be read whole first.
        var answersHere: Bool {
            !ask.asksQuestions && ask.tool != ClaudeToolSummary.planTool && ask.content?.requiresInspection != true
        }

        /// Always on the row: Codex's for the rest of its turn, any other
        /// agent's where it offers rules or a session-wide allow.
        var offersAlways: Bool { answersHere && (session.provider == .codex || ask.offersAlways) }

        /// Allow all takes it: answered here, and nothing about it looks risky.
        var joinsAllowAll: Bool { answersHere && ask.risk.isSafe }

        /// What the row shows of the ask on one line: the command or path.
        var line: String {
            let first = ask.detail.split(whereSeparator: \.isNewline).first.map(String.init)
            return [first, ask.summary].compactMap { $0?.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
        }
    }

    let rows: [Row]

    /// Shown once more than one ask waits; one ask keeps its own page.
    static let threshold = 2

    init(_ sessions: [ClaudeSession]) {
        let rows = sessions.flatMap { session in
            session.asks.map { Row(session: session, ask: $0) }
        }
        // Oldest first; asks of one moment keep the sessions' order.
        self.rows = rows.enumerated().sorted { ($0.element.ask.since, $0.offset) < ($1.element.ask.since, $1.offset) }
            .map(\.element)
    }

    var count: Int { rows.count }
    var shows: Bool { rows.count >= Self.threshold }
    var allowAll: [Row] { rows.filter(\.joinsAllowAll) }
    /// Asks Allow all leaves for the person, one by one.
    var heldBack: Int { rows.count - allowAll.count }

    /// The ask a key answers when no page is open: the oldest one a key may
    /// answer (a question is answered on its card).
    var keyTarget: Row? { rows.first { !$0.ask.asksQuestions } }
}

extension ClaudeSession {
    /// The session as its page shows it with this ask on the card: the
    /// chosen one first, the others behind it (#269). Unchanged when it no
    /// longer holds that ask.
    func showing(_ requestID: String?) -> ClaudeSession {
        guard let requestID, permission?.requestID != requestID, let chosen = ask(requestID) else { return self }
        var session = self
        session.permission = chosen
        session.laterAsks = asks.filter { $0.requestID != requestID }
        return session
    }
}
