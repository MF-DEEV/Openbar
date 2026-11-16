// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import Foundation

/// Reminders of a session nobody reacted to (#231).
enum NotchSessionReminderTests {
    static func run(_ suite: TestSuite) {
        suite.run("session reminders: when one is due") { due(suite) }
        suite.run("session reminders: waits answered or seen") { answered(suite) }
        suite.run("session reminders: finished turns") { finished(suite) }
        suite.run("session reminders: one for many, repeats, sound") { together(suite) }
        suite.run("session reminders: row mark and settings") { mark(suite) }
    }

    static let at = Date(timeIntervalSince1970: 100_000)

    static func session(_ id: String, _ phase: ClaudeSessionPhase, since: TimeInterval = 0,
                        change: (inout ClaudeSession) -> Void = { _ in }) -> ClaudeSession {
        var session = ClaudeSession(id: id, pid: 7, cwd: "/x/app", project: "app", name: id,
                                    host: .terminal(tty: "/dev/ttys001"), phase: phase,
                                    phaseSince: at.addingTimeInterval(since), turnStarted: nil,
                                    timeline: [], subagents: [], tokens: AgentTokens(), cost: 0, hooked: true)
        change(&session)
        return session
    }

    static func permission(_ id: String, since: TimeInterval = 0) -> ClaudePermission {
        ClaudePermission(requestID: id, tool: "Bash", summary: "ls", offersAlways: false, since: at.addingTimeInterval(since))
    }

    static let all = NotchSessionReminders.Settings(permission: true, question: true, finished: true, after: 300)

    static func due(_ suite: TestSuite) {
        var reminders = NotchSessionReminders()
        let waiting = [session("a", .waitingPermission) { $0.permission = permission("r1") }]
        suite.expect(reminders.due(waiting, settings: .init(), now: at.addingTimeInterval(3600)).isEmpty,
                     "everything is off by default")
        suite.expect(reminders.due(waiting, settings: all, now: at.addingTimeInterval(299)).isEmpty, "not before the time")
        let due = reminders.due(waiting, settings: all, now: at.addingTimeInterval(300))
        suite.expect(due.map(\.session) == ["a"] && due.first?.kind == .permission, "a permission at the time: \(due)")
        reminders.mark(due, at: at.addingTimeInterval(300))
        suite.expect(reminders.due(waiting, settings: all, now: at.addingTimeInterval(3000)).isEmpty,
                     "one reminder for each wait by default")

        var only = NotchSessionReminders()
        var questionsOnly = all
        questionsOnly.permission = false
        suite.expect(only.due(waiting, settings: questionsOnly, now: at.addingTimeInterval(600)).isEmpty,
                     "each kind has its own switch")
        let asking = [session("q", .waitingInput)]
        suite.expect(only.due(asking, settings: questionsOnly, now: at.addingTimeInterval(600)).first?.kind == .question,
                     "a question is told apart")
        let tool = [session("p", .waitingPermission) {
            $0.permission = permission("r2")
            $0.permission?.questions = [ClaudeQuestion(question: "Which?", header: "", options: [], multiSelect: false)]
        }]
        suite.expect(only.due(tool, settings: questionsOnly, now: at.addingTimeInterval(600)).first?.kind == .question,
                     "a question asked through a permission is a question")
    }

    static func answered(_ suite: TestSuite) {
        var reminders = NotchSessionReminders()
        let first = [session("a", .waitingPermission) { $0.permission = permission("r1") }]
        let due = reminders.due(first, settings: all, now: at.addingTimeInterval(300))
        reminders.mark(due, at: at.addingTimeInterval(300))
        // Answered in the terminal: the next request is a new wait.
        let next = [session("a", .waitingPermission, since: 400) { $0.permission = permission("r2", since: 400) }]
        suite.expect(reminders.due(next, settings: all, now: at.addingTimeInterval(600)).isEmpty,
                     "the next request starts its own time")
        suite.expect(reminders.due(next, settings: all, now: at.addingTimeInterval(700)).map(\.session) == ["a"],
                     "and has its own reminder")

        var named = NotchSessionReminders()
        let unnamed = [session("b", .waitingPermission)]
        let fired = named.due(unnamed, settings: all, now: at.addingTimeInterval(300))
        named.mark(fired, at: at.addingTimeInterval(300))
        let nowNamed = [session("b", .waitingPermission) { $0.permission = permission("r9") }]
        suite.expect(named.due(nowNamed, settings: all, now: at.addingTimeInterval(900)).isEmpty,
                     "a prompt the hook names later is the same wait")

        var gone = NotchSessionReminders()
        let working = [session("a", .generating, since: 200)]
        suite.expect(gone.due(working, settings: all, now: at.addingTimeInterval(900)).isEmpty, "nothing waits, nothing due")
    }

    static func finished(_ suite: TestSuite) {
        var reminders = NotchSessionReminders()
        let done = session("f", .finished, since: 0) {
            $0.turnStarted = at.addingTimeInterval(-100)
            $0.turnEndedAt = at
            $0.turnCompleted = true
        }
        suite.expect(reminders.due([done], settings: all, now: at.addingTimeInterval(300)).first?.kind == .finished,
                     "a finished turn nobody looked at")
        var idle = done
        idle.phase = .idle
        var later = NotchSessionReminders()
        suite.expect(later.due([idle], settings: all, now: at.addingTimeInterval(900)).first?.kind == .finished,
                     "still unseen once the phase went idle by itself")
        var looked = done
        looked.lookedAt = at.addingTimeInterval(10)
        suite.expect(NotchSessionReminders.wait(of: looked, now: at.addingTimeInterval(300)) == nil, "its page was opened")
        var failed = done
        failed.failedAt = at
        suite.expect(NotchSessionReminders.wait(of: failed, now: at.addingTimeInterval(300)) == nil, "an error is not a finish")
        var stopped = done
        stopped.turnCompleted = false
        suite.expect(NotchSessionReminders.wait(of: stopped, now: at.addingTimeInterval(300)) == nil, "an interrupted turn is not")
        suite.expect(NotchSessionReminders.wait(of: done, now: at.addingTimeInterval(3600)) == nil, "an hour-old turn is let go")
        var again = done
        again.phase = .generating
        again.turnStarted = at.addingTimeInterval(50)
        suite.expect(NotchSessionReminders.wait(of: again, now: at.addingTimeInterval(300)) == nil, "the next turn began")
    }

    static func together(_ suite: TestSuite) {
        var reminders = NotchSessionReminders()
        let three = [
            session("a", .waitingPermission) { $0.permission = permission("r1") },
            session("b", .waitingInput, since: 60),
            session("c", .waitingPermission, since: 120) { $0.permission = permission("r3", since: 120) },
            session("late", .waitingInput, since: 250),
        ]
        let due = reminders.due(three, settings: all, now: at.addingTimeInterval(300))
        suite.expect(due.map(\.session) == ["a", "b", "c"],
                     "sessions waiting together come in one reminder, a fresh one not: \(due.map(\.session))")
        reminders.mark(due, at: at.addingTimeInterval(300))
        suite.expect(reminders.due(three, settings: all, now: at.addingTimeInterval(549)).isEmpty, "no burst after it")
        suite.expect(reminders.due(three, settings: all, now: at.addingTimeInterval(550)).map(\.session) == ["late"],
                     "the fresh one gets its own in time")

        var repeating = NotchSessionReminders()
        var every = all
        every.every = 600
        let one = [session("a", .waitingPermission) { $0.permission = permission("r1") }]
        repeating.mark(repeating.due(one, settings: every, now: at.addingTimeInterval(300)), at: at.addingTimeInterval(300))
        suite.expect(repeating.due(one, settings: every, now: at.addingTimeInterval(899)).isEmpty, "not again too soon")
        suite.expect(repeating.due(one, settings: every, now: at.addingTimeInterval(900)).map(\.session) == ["a"],
                     "again every N minutes when asked to")

        let kinds = due.map(\.kind)
        suite.expect(NotchSessionReminders.sound(for: due) == .permission, "the most pressing wait's sound: \(kinds)")
        let questions = [NotchSessionReminders.Wait(session: "q", kind: .question,
                                                    request: .init(wait: "w", id: nil), since: at)]
        suite.expect(NotchSessionReminders.sound(for: questions) == .question, "a question's own sound")
        suite.expect(NotchSessionReminders.sound(for: []) == nil, "no sound for nothing")
    }

    static func mark(_ suite: TestSuite) {
        let waiting = session("a", .waitingPermission) { $0.permission = permission("r1", since: -30) }
        suite.expect(NotchSessionReminders.waitedMinutes(waiting, now: at.addingTimeInterval(20)) == nil,
                     "under a minute the seconds clock stays")
        suite.expect(NotchSessionReminders.waitedMinutes(waiting, now: at.addingTimeInterval(690)) == 12, "12 whole minutes")
        let strings = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(strings.waitingFor(minutes: 12) == "waiting 12 min", "the row reads \(strings.waitingFor(minutes: 12))")
        suite.expect(strings.waitingFor(minutes: 75) == "waiting 1h 15m", "and in hours")

        var focus = all
        suite.expect(NotchSessionReminders.heldByFocus(focus, focus: true), "Focus holds it by default")
        suite.expect(!NotchSessionReminders.heldByFocus(focus, focus: nil), "unknown Focus holds nothing")
        focus.duringFocus = true
        suite.expect(!NotchSessionReminders.heldByFocus(focus, focus: true), "unless allowed")

        let defaults = UserDefaults(suiteName: "com.openbar.tests.reminders")!
        defaults.removePersistentDomain(forName: "com.openbar.tests.reminders")
        let off = NotchSessionReminders.Settings.load(defaults)
        suite.expect(!off.anyOn && off.after == 300 && off.every == nil && !off.duringFocus, "off by default, 5 min, once")
        defaults.set(true, forKey: DefaultsKey.notchSessionsRemindQuestion)
        defaults.set(2, forKey: DefaultsKey.notchSessionsRemindAfter)
        defaults.set(10, forKey: DefaultsKey.notchSessionsRemindEvery)
        let on = NotchSessionReminders.Settings.load(defaults)
        suite.expect(on.question && !on.permission && on.after == 120 && on.every == 600, "read from Settings")
    }
}
