// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The "while you were away" card's lines (#215).
enum NotchAwayTests {
    static func run(_ suite: TestSuite) {
        suite.run("away card lines") { lines(suite) }
    }

    static let at = Date(timeIntervalSince1970: 1000)

    static func session(_ id: String, _ phase: ClaudeSessionPhase, since: TimeInterval = -100,
                        change: (inout ClaudeSession) -> Void = { _ in }) -> ClaudeSession {
        var session = ClaudeSession(id: id, pid: 7, cwd: "/x/app", project: "app", name: "app",
                                    host: .terminal(tty: "/dev/ttys001"), phase: phase,
                                    phaseSince: at.addingTimeInterval(since), turnStarted: nil,
                                    timeline: [], subagents: [], tokens: AgentTokens(), cost: 0, hooked: false)
        change(&session)
        return session
    }

    static func lines(_ suite: TestSuite) {
        let quiet = [session("a", .idle), session("b", .waitingInput)]
        suite.expect(NotchAway.lines(since: NotchAway.mark(quiet, at: at), sessions: quiet).isEmpty,
                     "nothing changed shows no card")

        let before = [session("done", .generating), session("wait", .runningTool),
                      session("lim", .generating), session("err", .generating)]
        let after = [
            session("done", .finished, since: 50) { $0.lastActive = at.addingTimeInterval(50) },
            session("wait", .waitingPermission, since: 60),
            session("lim", .limited, since: 70),
            session("err", .idle, since: 80) { $0.failedAt = at.addingTimeInterval(80) },
        ]
        let outcomes = NotchAway.lines(since: NotchAway.mark(before, at: at), sessions: after).map { "\($0.id):\($0.outcome)" }
        suite.expect(outcomes == ["wait:waiting", "err:error", "lim:limited", "done:finished"],
                     "what waits comes first, then errors, limits and results: \(outcomes)")

        let working = [session("w", .generating) {
            $0.edits = ClaudeEditStats(added: 10, removed: 2, touched: [at.addingTimeInterval(-50)])
            $0.tasks = ClaudeTaskProgress(done: 1, total: 5)
        }]
        let moved = [session("w", .runningTool) {
            $0.edits = ClaudeEditStats(added: 40, removed: 5, touched: [at.addingTimeInterval(-50), at.addingTimeInterval(10), at.addingTimeInterval(20)])
            $0.tasks = ClaudeTaskProgress(done: 4, total: 5)
        }]
        let line = NotchAway.lines(since: NotchAway.mark(working, at: at), sessions: moved).first
        suite.expect(line?.outcome == .working && line?.files == 2 && line?.added == 30 && line?.removed == 3,
                     "only files and lines changed while away count")
        suite.expect(line?.tasks == NotchAway.Line.Tasks(from: 1, to: 4, total: 5), "and the tasks ticked off then")

        let still = [session("w", .generating) { $0.tasks = ClaudeTaskProgress(done: 2, total: 5) }]
        suite.expect(NotchAway.lines(since: NotchAway.mark(still, at: at), sessions: still).isEmpty,
                     "a session at work that moved nothing has no tile")

        let fresh = [session("n", .finished, since: 90) {
            $0.turnStarted = at.addingTimeInterval(5)
            $0.lastActive = at.addingTimeInterval(90)
        }]
        suite.expect(NotchAway.lines(since: NotchAway.mark([], at: at), sessions: fresh).map { $0.outcome } == [NotchAway.Outcome.finished],
                     "a session begun and finished while away counts")
    }
}
