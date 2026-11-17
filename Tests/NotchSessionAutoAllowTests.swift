// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Always on a Codex card allows every request of the turn under way, shows
/// those waits as work, and ends with the turn.
enum NotchSessionAutoAllowTests {
    static func run(_ suite: TestSuite) {
        suite.run("session always hides the waits it answers") { hides(suite) }
        suite.run("session always presents them as work") { presents(suite) }
        suite.run("session always turns on only at work") { turnsOn(suite) }
        suite.run("session always waits and answers once") { answers(suite) }
        suite.run("session always ends with the turn") { ends(suite) }
        suite.run("session always lets go of answers no longer due") { cancels(suite) }
        suite.run("session always edges") { edges(suite) }
        suite.run("session always key") { key(suite) }
    }

    static let turn = Date(timeIntervalSinceReferenceDate: 100)

    static func codex(_ phase: ClaudeSessionPhase = .waitingPermission, id: String = "c", request: String? = "r1",
                      questions: Bool = false, turn: Date? = NotchSessionAutoAllowTests.turn) -> ClaudeSession {
        var session = NotchSessionAgentTests.session(id, phase, provider: .codex)
        session.phaseSince = at(0)
        session.turnStarted = turn
        session.permission = request.map {
            var permission = ClaudePermission(requestID: $0, tool: "Command", summary: nil, offersAlways: true,
                                              since: session.phaseSince)
            if questions { permission.questions = [ClaudeQuestion(question: "Which?")] }
            return permission
        }
        return session
    }

    static func at(_ seconds: TimeInterval) -> Date { Date(timeIntervalSinceReferenceDate: 1000 + seconds) }

    static func hides(_ suite: TestSuite) {
        let on: Set<String> = ["c"]
        suite.expect(NotchClaudeSessionsSupport.hidesAutoApproval(codex(), autoAllowed: on, now: at(1)),
                     "a Codex request it covers")
        suite.expect(NotchClaudeSessionsSupport.hidesAutoApproval(codex(request: nil), autoAllowed: on, now: at(1)),
                     "and the moments Codex says it waits with no request listed")
        suite.expect(!NotchClaudeSessionsSupport.hidesAutoApproval(codex(), autoAllowed: [], now: at(1)),
                     "not without Always")
        suite.expect(!NotchClaudeSessionsSupport.hidesAutoApproval(codex(questions: true), autoAllowed: on, now: at(1)),
                     "a question still waits for an answer")
        var terminal = codex(request: nil)
        terminal.waitingFor = CodexSessionState.answerInTerminal
        suite.expect(!NotchClaudeSessionsSupport.hidesAutoApproval(terminal, autoAllowed: on, now: at(1)),
                     "a request only the terminal answers still shows")
        var claude = codex()
        claude.provider = .claude
        suite.expect(!NotchClaudeSessionsSupport.hidesAutoApproval(claude, autoAllowed: on, now: at(1)),
                     "Claude Code's never")
        suite.expect(NotchClaudeSessionsSupport.autoAllowRequest(codex(), autoAllowed: on, now: at(1)) == "r1"
                     && NotchClaudeSessionsSupport.autoAllowRequest(codex(request: nil), autoAllowed: on, now: at(1)) == nil,
                     "only a listed request is answered")
        suite.expect(!NotchClaudeSessionsSupport.shows(.needsPermission(id: "c", tool: "Codex"), hidden: ["c"])
                     && NotchClaudeSessionsSupport.shows(.needsPermission(id: "d", tool: "Codex"), hidden: ["c"])
                     && NotchClaudeSessionsSupport.shows(.finished(id: "c", duration: 3), hidden: ["c"]),
                     "no notice of a hidden wait; its end still tells")
    }

    static func presents(_ suite: TestSuite) {
        var other = NotchSessionAgentTests.session("a", .waitingPermission, provider: .claude)
        other.permission = ClaudePermission(requestID: "x", tool: "Bash", summary: nil, offersAlways: false,
                                            since: other.phaseSince)
        let raw = [codex(), other]
        let shown = NotchClaudeSessionsSupport.presented(raw, autoAllowed: ["c"], now: at(1))
        let hidden = shown.first { $0.id == "c" }
        suite.expect(hidden?.phase == .generating && hidden?.permission == nil && hidden?.waitingFor == nil,
                     "the session is shown at work, with no card: \(String(describing: hidden?.phase))")
        suite.expect(hidden?.phaseSince == turn, "its clock counts from the turn's start")
        suite.expect(shown.first?.id == "a", "a session that still waits comes first")
        let counts = ClaudeSessionCounts(shown)
        suite.expect(counts.waiting == 1 && counts.working == 1 && counts.workingCodex == 1,
                     "the counts show the wait as Codex at work: \(counts)")
        suite.expect(raw[0].permission?.requestID == "r1", "the request itself stays as it came")
        var running = codex()
        running.currentTool = ClaudeToolUse(id: "t", name: "Command", started: turn)
        suite.expect(NotchClaudeSessionsSupport.presented([running], autoAllowed: ["c"], now: at(1)).first?.phase
                     == .runningTool,
                     "with its command under way it runs a tool")
        suite.expect(NotchClaudeSessionsSupport.presented(raw, autoAllowed: [], now: at(1)) == raw,
                     "without Always nothing changes")
    }

    static func turnsOn(_ suite: TestSuite) {
        var always = NotchSessionAutoAllow()
        suite.expect(!always.set("c", on: nil, session: nil, now: at(0)), "a session the island does not list")
        suite.expect(!always.set("c", on: nil, session: codex(.idle, request: nil), now: at(0)), "an idle one")
        suite.expect(!always.set("c", on: nil, session: codex(.finished, request: nil), now: at(0)), "a finished one")
        var claude = codex()
        claude.provider = .claude
        suite.expect(!always.set("c", on: nil, session: claude, now: at(0)), "or Claude Code's")
        suite.expect(always.sessions.isEmpty, "none of them gets Always")
        suite.expect(always.set("c", on: nil, session: codex(.generating, request: nil), now: at(0)),
                     "a Codex session at work does")
        suite.expect(always.set("c", on: nil, session: codex(), now: at(0)) && always.sessions.isEmpty, "pressed again, it stops")
        suite.expect(!always.set("c", on: false, session: codex(), now: at(0)), "turning off what is off changes nothing")
        suite.expect(always.set("c", on: true, session: codex(), now: at(0))
                     && !always.set("c", on: true, session: codex(), now: at(0)),
                     "nor does turning on what is on")
    }

    static func answers(_ suite: TestSuite) {
        var always = NotchSessionAutoAllow()
        // The request came at 0 and is still settling when Always comes on.
        _ = always.update([codex()], now: at(0))
        _ = always.set("c", on: true, session: codex(), now: at(0))
        let plan = always.update([codex()], now: at(0.3))
        suite.expect(plan.schedule.count == 1 && plan.schedule[0].request == "r1"
                     && abs(plan.schedule[0].after - (NotchClaudeSessionsSupport.settleDelay - 0.3)) < 0.001,
                     "the request waits out the rest of the settle delay: \(plan)")
        suite.expect(always.update([codex()], now: at(0.5)) == .init(), "and is not planned twice")
        suite.expect(always.fire("c", request: "r1", in: [codex()], now: at(1)), "when its time comes it is answered")
        suite.expect(!always.fire("c", request: "r1", in: [codex()], now: at(1)), "once")
        suite.expect(always.update([codex()], now: at(1.2)) == .init(),
                     "a request still listed after its answer is not answered again")
        let next = always.update([codex(request: "r2")], now: at(1.5))
        suite.expect(next.schedule == [.init(id: "c", request: "r2", after: NotchClaudeSessionsSupport.settleDelay)],
                     "the next request settles like any other: \(next)")
        suite.expect(always.update([codex(questions: true)], now: at(2)).schedule.isEmpty,
                     "a question is left to the person")

        var late = NotchSessionAutoAllow()
        _ = late.update([codex()], now: at(0))
        _ = late.set("c", on: true, session: codex(), now: at(0))
        suite.expect(late.update([codex()], now: at(5)).schedule.first?.after == 0,
                     "Always given on the card answers the request on it at once")
        late.failed("c")
        suite.expect(late.sessions.isEmpty, "an answer Codex did not take ends Always")
        suite.expect(!late.fire("c", request: "r1", in: [codex()], now: at(1)), "and nothing more is sent")
    }

    static func ends(_ suite: TestSuite) {
        func given(_ session: ClaudeSession = codex()) -> NotchSessionAutoAllow {
            var always = NotchSessionAutoAllow()
            _ = always.set("c", on: true, session: session, now: at(0))
            _ = always.update([session], now: at(0))
            return always
        }
        var finished = given()
        _ = finished.update([codex(.finished, request: nil)], now: at(1))
        suite.expect(finished.sessions.isEmpty, "the turn's end takes it")
        var idle = given()
        _ = idle.update([codex(.idle, request: nil)], now: at(1))
        suite.expect(idle.sessions.isEmpty, "so does going quiet")
        var gone = given()
        _ = gone.update([], now: at(1))
        suite.expect(gone.sessions.isEmpty, "and the session leaving")
        var missed = given()
        _ = missed.update([codex(turn: Date(timeIntervalSinceReferenceDate: 500))], now: at(1))
        suite.expect(missed.sessions.isEmpty, "a new turn whose start alone was seen ends it too")
        var unknown = given(codex(turn: nil))
        _ = unknown.update([codex()], now: at(1))
        _ = unknown.update([codex()], now: at(2))
        suite.expect(unknown.sessions == ["c"], "a turn whose start comes later keeps it")
        _ = unknown.update([codex(turn: Date(timeIntervalSinceReferenceDate: 500))], now: at(3))
        suite.expect(unknown.sessions.isEmpty, "until another turn starts")
        // The next prompt asks again.
        var fresh = given()
        _ = fresh.update([codex(.finished, request: nil)], now: at(1))
        let next = fresh.update([codex(request: "r2", turn: Date(timeIntervalSinceReferenceDate: 900))], now: at(2))
        suite.expect(next.schedule.isEmpty, "the next turn's requests wait for the person")
    }

    static func cancels(_ suite: TestSuite) {
        var always = NotchSessionAutoAllow()
        _ = always.set("c", on: true, session: codex(), now: at(0))
        _ = always.update([codex()], now: at(0))
        suite.expect(always.update([codex(.runningTool, request: nil)], now: at(0.2)).cancel == ["c"],
                     "a request answered in the terminal lets its answer go")
        suite.expect(!always.fire("c", request: "r1", in: [codex(.runningTool, request: nil)], now: at(1)),
                     "and a late one sends nothing")
        _ = always.update([codex(request: "r2")], now: at(0.4))
        _ = always.set("c", on: false, session: codex(request: "r2"), now: at(0))
        suite.expect(always.update([codex(request: "r2")], now: at(0.5)).cancel == ["c"], "so does turning Always off")
        suite.expect(!always.fire("c", request: "r2", in: [codex(request: "r2")], now: at(1)), "after which nothing is sent")
    }

    static func edges(_ suite: TestSuite) {
        // Codex read the thread mid-turn: the turn's start was never seen.
        var unseen = NotchSessionAutoAllow()
        _ = unseen.set("c", on: true, session: codex(turn: nil), now: at(0))
        _ = unseen.update([codex(turn: nil)], now: at(1))
        _ = unseen.update([codex(.generating, request: nil, turn: at(5))], now: at(6))
        suite.expect(unseen.sessions.isEmpty, "the next turn, though the end of this one went unseen, takes it")

        let hiddenNow = { (now: Date) in
            NotchClaudeSessionsSupport.hidesAutoApproval(codex(request: nil), autoAllowed: ["c"], now: now)
        }
        suite.expect(hiddenNow(at(1)) && !hiddenNow(at(NotchClaudeSessionsSupport.statusOnlyWait + 0.1)),
                     "a wait with no request the island holds shows again after a moment, to be answered in the terminal")
        suite.expect(NotchClaudeSessionsSupport.hidesAutoApproval(codex(), autoAllowed: ["c"], now: at(60)),
                     "a request it holds stays hidden until answered")
        suite.expect(!NotchClaudeSessionsSupport.hidesAutoApproval(codex(.waitingInput), autoAllowed: ["c"], now: at(1)),
                     "Codex waiting for input is never hidden")

        // A refused answer ends Always; given again, the request is tried again.
        var again = NotchSessionAutoAllow()
        _ = again.set("c", on: true, session: codex(), now: at(0))
        _ = again.update([codex()], now: at(1))
        _ = again.fire("c", request: "r1", in: [codex()], now: at(1))
        again.failed("c")
        _ = again.update([codex()], now: at(2))
        _ = again.set("c", on: true, session: codex(), now: at(3))
        suite.expect(again.update([codex()], now: at(3)).schedule.map(\.request) == ["r1"],
                     "Always given again answers the request it was refused for")

        var two = NotchSessionAutoAllow()
        let d = codex(id: "d", request: "r9")
        _ = two.set("c", on: true, session: codex(), now: at(0))
        _ = two.set("d", on: true, session: d, now: at(0))
        suite.expect(Set(two.update([codex(), d], now: at(0)).schedule.map(\.id)) == ["c", "d"],
                     "each session's request is planned at once")
        _ = two.set("c", on: false, session: codex(), now: at(0.1))
        let plan = two.update([codex(), d], now: at(0.2))
        suite.expect(plan.cancel == ["c"] && two.sessions == ["d"] && two.scheduled == ["d": "r9"],
                     "turning one off leaves the other's: \(plan)")

        var next = NotchSessionAutoAllow()
        _ = next.set("c", on: true, session: codex(), now: at(0))
        _ = next.update([codex()], now: at(0))
        let swap = next.update([codex(request: "r2")], now: at(0.4))
        suite.expect(swap.cancel == ["c"] && swap.schedule == [.init(id: "c", request: "r2",
                                                                   after: NotchClaudeSessionsSupport.settleDelay)],
                     "a request replaced before its answer lets it go for the new one: \(swap)")
        var question = codex(request: "r2")
        question.permission?.questions = [ClaudeQuestion(question: "Which?")]
        suite.expect(next.scheduled == ["c": "r2"], "the new request waits for its answer")
        suite.expect(!next.fire("c", request: "r2", in: [question], now: at(1.3)),
                     "a request that turned into a question is not allowed")
    }

    static func key(_ suite: TestSuite) {
        suite.expect(NotchClaudeSessionsSupport.decision(key: "t", commandOnly: true) == .allowAlways
                     && NotchClaudeSessionsSupport.decision(key: "T", commandOnly: true) == .allowAlways,
                     "⌘T picks Always")
        suite.expect(NotchClaudeSessionsSupport.decision(key: "t", commandOnly: false) == nil, "with ⌘ alone")
    }
}
