// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClaudeSessionEngineTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions engine phases") { phases(suite) }
        suite.run("sessions engine events") { events(suite) }
        suite.run("sessions engine permissions") { permissions(suite) }
        suite.run("sessions engine content") { content(suite) }
        suite.run("sessions engine bookkeeping") { bookkeeping(suite) }
        suite.run("sessions support") { support(suite) }
    }

    static let t0 = Date(timeIntervalSince1970: 1_792_600_000)

    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    static func record(_ id: String = "s1", pid: Int32 = 100, status: ClaudeRegistryStatus? = .idle,
                       waitingFor: String? = nil, changed: TimeInterval = 0, cwd: String = "/Users/me/code/app",
                       host: ClaudeSessionHost = .terminal(tty: "/dev/ttys001")) -> ClaudeSessionRecord {
        var record = ClaudeSessionRecord(pid: pid, sessionID: id, cwd: cwd, name: "app-1", status: status,
                                         waitingFor: waitingFor, statusUpdated: at(changed), started: t0, version: "2.1.306")
        record.host = host
        return record
    }

    static func hook(_ event: ClaudeHookFact.Event, _ seconds: TimeInterval, id: String = "s1",
                     agent: String? = nil) -> ClaudeHookFact {
        ClaudeHookFact(sessionID: id, date: at(seconds), event: event, agentID: agent)
    }

    static func tool(_ id: String, _ seconds: TimeInterval, _ name: String = "Bash",
                     summary: String? = "npm test") -> ClaudeToolUse {
        ClaudeToolUse(id: id, name: name, summary: summary, started: at(seconds))
    }

    static func only(_ engine: ClaudeSessionEngine, _ seconds: TimeInterval) -> ClaudeSession? {
        engine.snapshot(now: at(seconds)).sessions.first
    }

    // MARK: Phases

    static func phases(_ suite: TestSuite) {
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [record(status: .idle)], now: at(0))
        suite.expect(only(engine, 1)?.phase == .idle, "a listed session with nothing else known is idle")

        engine.ingest(records: [record(status: .busy, changed: 5)], now: at(5))
        suite.expect(only(engine, 6)?.phase == .generating, "a busy registry reads as working")
        suite.expect(only(engine, 6)?.turnStarted == at(5), "the busy change starts the turn clock")

        engine.ingest(hook: hook(.toolStarted(tool("t1", 7)), 7), now: at(7))
        suite.expect(only(engine, 8)?.phase == .runningTool, "a tool reported after the registry change runs")
        suite.expect(only(engine, 8)?.currentTool?.name == "Bash", "the running tool is named")

        engine.ingest(hook: hook(.toolFinished(id: "t1", failed: false), 9), now: at(9))
        suite.expect(only(engine, 10)?.phase == .generating, "after the tool the model writes again")

        engine.ingest(hook: hook(.stopped(lastText: nil), 12), now: at(12))
        suite.expect(only(engine, 13)?.phase == .finished, "a stop newer than the registry finishes the turn")

        engine.ingest(records: [record(status: .idle, changed: 12.2)], now: at(13))
        suite.expect(only(engine, 14)?.phase == .finished, "the registry agreeing keeps it finished")
        suite.expect(only(engine, 12 + ClaudeSessionEngine.finishedFor + 1)?.phase == .idle,
                     "an unread result fades to idle after a while")

        engine.acknowledge("s1", now: at(20))
        suite.expect(only(engine, 21)?.phase == .idle, "looking at a finished session makes it idle")

        let waiting = ClaudeSessionEngine()
        waiting.ingest(records: [record(status: .waiting, waitingFor: "permission prompt", changed: 3)], now: at(3))
        suite.expect(only(waiting, 4)?.phase == .waitingPermission, "the registry's permission prompt is a permission wait")
        waiting.ingest(records: [record(status: .waiting, waitingFor: "input needed", changed: 5)], now: at(5))
        suite.expect(only(waiting, 6)?.phase == .waitingInput, "other prompts wait for input")
        suite.expect(only(waiting, 6)?.waitingFor == "input needed", "the reason is kept while it waits")

        // The registry going idle after the last line read ends the turn.
        let missed = ClaudeSessionEngine()
        missed.ingest(records: [record(status: .busy, changed: 1)], now: at(1))
        missed.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .toolStarted(tool("t1", 2))], now: at(2))
        missed.ingest(records: [record(status: .idle, changed: 30)], now: at(30))
        let ended = only(missed, 31)
        suite.expect(ended?.phase == .finished && ended?.currentTool == nil,
                     "a newer idle registry finishes a turn whose end was not read")
        suite.expect(ended?.timeline.first?.isRunning == false, "and closes the tools it left open")

        // A busy registry that is older than the last line defers to it.
        let stale = ClaudeSessionEngine()
        stale.ingest(records: [record(status: .busy, changed: 1)], now: at(1))
        stale.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .turnEnded(at(4), completed: true)], now: at(4))
        suite.expect(only(stale, 5)?.phase == .finished, "a later end_turn wins over an older busy registry")

        let interrupted = ClaudeSessionEngine()
        interrupted.ingest(records: [record(status: .idle, changed: 0)], now: at(0))
        interrupted.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .turnEnded(at(2), completed: false)], now: at(2))
        suite.expect(only(interrupted, 3)?.phase == .idle, "an interrupted turn is not a result to read")

        let unknown = ClaudeSessionEngine()
        unknown.ingest(records: [record(status: nil)], now: at(0))
        unknown.ingest(sessionID: "s1", facts: [.turnBegan(at(1))], now: at(1))
        suite.expect(only(unknown, 2)?.phase == .generating, "without a registry status the transcript decides")

        let late = ClaudeSessionEngine()
        late.ingest(records: [record(status: .idle, changed: 0)], now: at(0))
        late.ingest(sessionID: "s1", facts: [.turnBegan(at(10)), .turnEnded(at(20), completed: true)], now: at(20))
        late.ingest(sessionID: "s1", facts: [.turnBegan(at(15))], now: at(21))
        suite.expect(only(late, 22)?.phase == .finished, "a prompt read after its turn ended does not reopen it")

        let queued = ClaudeSessionEngine()
        queued.ingest(records: [record(status: .idle, changed: 0)], now: at(0))
        queued.ingest(hook: hook(.promptSubmitted, 1), now: at(1))
        queued.ingest(hook: hook(.toolStarted(tool("t1", 2)), 2), now: at(2))
        queued.ingest(hook: hook(.promptSubmitted, 3), now: at(3))
        let busy = only(queued, 4)
        suite.expect(busy?.turnStarted == at(1) && busy?.phase == .runningTool,
                     "a prompt queued while working joins the turn and leaves its tool running")
    }

    // MARK: Events

    static func events(_ suite: TestSuite) {
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [record(status: .waiting, waitingFor: "permission prompt", changed: 0)], now: at(0))
        suite.expect(engine.snapshot(now: at(1)).events.isEmpty, "a session seen for the first time raises nothing")

        engine.ingest(records: [record(status: .busy, changed: 2)], now: at(2))
        suite.expect(engine.snapshot(now: at(3)).events.isEmpty, "going back to work raises nothing")

        engine.ingest(hook: hook(.toolStarted(tool("t1", 4)), 4), now: at(4))
        engine.ingest(records: [record(status: .waiting, waitingFor: "permission prompt", changed: 5)], now: at(5))
        suite.expect(engine.snapshot(now: at(6)).events == [.needsPermission(id: "s1", tool: "Bash")],
                     "a permission wait names the tool")
        suite.expect(engine.snapshot(now: at(7)).events.isEmpty, "and is raised once")

        engine.ingest(records: [record(status: .waiting, waitingFor: "input needed", changed: 8)], now: at(8))
        suite.expect(engine.snapshot(now: at(9)).events == [.needsInput(id: "s1")], "a question raises a wait for input")

        engine.ingest(records: [record(status: .busy, changed: 10)], now: at(10))
        _ = engine.snapshot(now: at(11))
        engine.ingest(hook: hook(.stopped(lastText: nil), 20), now: at(20))
        let finished = engine.snapshot(now: at(21)).events
        suite.expect(finished == [.finished(id: "s1", duration: 18)], "a finished turn reports how long it took")

        engine.ingest(records: [], now: at(30))
        suite.expect(engine.snapshot(now: at(31)).sessions.isEmpty, "a session gone from the registry leaves")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        let ask = ClaudePermission(requestID: "r1", tool: "Bash", summary: "rm -rf build", offersAlways: true, since: at(2))
        engine.ingest(hook: hook(.permissionRequested(ask), 2), now: at(2))
        suite.expect(only(engine, 3)?.phase == .waitingPermission && only(engine, 3)?.permission == ask,
                     "a hook's permission request waits on the person before the registry says so")
        suite.expect(only(engine, 3)?.hooked == true, "a session the hooks report can be answered here")

        engine.ingest(records: [record(status: .busy, changed: 1)], now: at(3))
        suite.expect(engine.permission(of: "s1") == ask, "an older registry change does not drop the request")

        engine.ingest(records: [record(status: .busy, changed: 4)], now: at(4))
        suite.expect(engine.permission(of: "s1") == nil, "the registry leaving the prompt means it was answered")

        engine.ingest(hook: hook(.permissionRequested(ask), 5), now: at(5))
        engine.resolvePermission(requestID: "other")
        suite.expect(engine.permission(of: "s1") != nil, "resolving another request leaves this one")
        engine.resolvePermission(requestID: "r1")
        suite.expect(engine.permission(of: "s1") == nil, "resolving the request clears it")

        engine.ingest(hook: hook(.permissionRequested(ask), 6), now: at(6))
        engine.ingest(hook: hook(.toolFinished(id: "t9", failed: false), 7), now: at(7))
        suite.expect(engine.permission(of: "s1") == nil, "a tool finishing after the request means it was answered")

        engine.ingest(hook: hook(.permissionRequested(ask), 8), now: at(8))
        engine.ingest(hook: hook(.stopped(lastText: nil), 9), now: at(9))
        suite.expect(engine.permission(of: "s1") == nil, "a turn ending drops its request")

        let sub = ClaudeSessionEngine()
        sub.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        sub.ingest(hook: hook(.subagentStarted(id: "a1", type: "Explore"), 1), now: at(1))
        sub.ingest(hook: hook(.toolStarted(tool("t2", 2, "Grep")), 2, agent: "a1"), now: at(2))
        sub.ingest(hook: hook(.permissionRequested(ask), 3, agent: "a1"), now: at(3))
        let state = only(sub, 4)
        suite.expect(state?.timeline.isEmpty == true, "a subagent's tools stay out of the session's timeline")
        suite.expect(state?.phase == .waitingPermission, "a subagent's permission still waits on the person")
        suite.expect(state?.subagents.map(\.type) == ["Explore"] && state?.subagents.first?.isRunning == true,
                     "the subagent is listed as running")
        sub.ingest(hook: hook(.subagentStopped(id: "a1"), 5, agent: "a1"), now: at(5))
        suite.expect(only(sub, 6)?.subagents.first?.isRunning == false, "and as done once it stops")
    }

    // MARK: Content

    static func content(_ suite: TestSuite) {
        let secret = "SENTINEL-7f3a"
        let closed = ClaudeSessionEngine()
        closed.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        closed.ingest(sessionID: "s1", facts: [
            .turnBegan(at(1)), .title(secret), .text(secret, at: at(2)),
            .toolStarted(tool("t1", 3, summary: secret)),
            .subagentInfo(id: "a1", type: "Plan", description: secret),
            .tasks(ClaudeTaskProgress(done: 1, total: 3, current: secret)),
        ], now: at(3))
        let quiet = ClaudeSessionEngine()
        quiet.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        quiet.ingest(hook: hook(.toolStarted(tool("t1", 1, summary: secret)), 1), now: at(1))
        quiet.ingest(hook: hook(.stopped(lastText: secret), 2), now: at(2))
        for (engine, label) in [(closed, "transcript"), (quiet, "hooks")] {
            let state = only(engine, 4)
            suite.expect(state != nil && !String(describing: state!).contains(secret),
                         "with previews off nothing a session wrote reaches the island (\(label))")
        }
        suite.expect(only(closed, 4)?.tasks?.done == 1 && only(closed, 4)?.tasks?.total == 3,
                     "task progress counts are kept without their names")
        suite.expect(only(closed, 4)?.subagents.first?.type == "Plan", "a subagent's type is kept without its task")

        let ask = ClaudePermission(requestID: "r1", tool: "Bash", summary: secret, offersAlways: false, since: at(5))
        quiet.ingest(hook: hook(.permissionRequested(ask), 5), now: at(5))
        suite.expect(only(quiet, 6)?.permission?.summary == secret,
                     "a pending permission always shows what it would run")

        let open = ClaudeSessionEngine(policy: ClaudeContentPolicy(previews: true))
        open.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        open.ingest(sessionID: "s1", facts: [
            .turnBegan(at(1)), .title("Fix the build"), .text("Done, tests pass.", at: at(2)),
            .toolStarted(tool("t1", 3, summary: "npm test")),
        ], now: at(3))
        let shown = only(open, 4)
        suite.expect(shown?.title == "Fix the build" && shown?.lastText == "Done, tests pass."
                     && shown?.timeline.first?.summary == "npm test", "with previews on titles, replies and commands show")
        open.policy = ClaudeContentPolicy(previews: false)
        let purged = only(open, 5)
        suite.expect(purged?.title == nil && purged?.lastText == nil && purged?.timeline.first?.summary == nil,
                     "turning previews off clears what was read")
        open.policy = ClaudeContentPolicy(previews: true)
        suite.expect(only(open, 6)?.title == nil, "and turning them back on does not bring it back by itself")
    }

    // MARK: Bookkeeping

    static func bookkeeping(_ suite: TestSuite) {
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        let first = AgentTokens(input: 10, cacheWrite: 0, cacheRead: 100, output: 5)
        let streamed = AgentTokens(input: 10, cacheWrite: 0, cacheRead: 100, output: 40)
        engine.ingest(sessionID: "s1", facts: [
            .usage(key: "m1", tokens: first, cost: 0.01), .usage(key: "m1", tokens: streamed, cost: 0.02),
            .usage(key: "m2", tokens: first, cost: 0.01), .branch("feat/x"), .model("claude-opus-5-5"),
        ], now: at(1))
        let state = only(engine, 2)
        suite.expect(state?.tokens.output == 45, "streamed blocks of one reply count once at their largest")
        suite.expectClose(state?.cost ?? 0, 0.03, "cost adds up across replies")
        suite.expect(state?.branch == "feat/x" && state?.model == "claude-opus-5-5", "branch and model are kept")
        suite.expect(state?.project == "app", "the project is the folder's name")

        var tools: [ClaudeTranscriptFact] = [.turnBegan(at(1))]
        for index in 0..<40 {
            tools.append(.toolStarted(tool("t\(index)", Double(index) + 2)))
            tools.append(.toolFinished(id: "t\(index)", at: at(Double(index) + 2.5), failed: index == 39))
        }
        engine.ingest(sessionID: "s1", facts: tools, now: at(50))
        let timeline = only(engine, 51)?.timeline ?? []
        suite.expect(timeline.count == ClaudeSessionEngine.timelineLength && timeline.last?.id == "t39"
                     && timeline.last?.failed == true, "the timeline keeps the latest tools in order")

        let early = ClaudeSessionEngine()
        early.ingest(hook: hook(.promptSubmitted, 0, id: "s2"), now: at(0))
        early.ingest(records: [], now: at(10))
        early.ingest(records: [record("s2", status: .busy, changed: 0)], now: at(11))
        suite.expect(only(early, 12)?.turnStarted == at(0), "hook events before the registry lists a session are kept")
        let gone = ClaudeSessionEngine()
        gone.ingest(hook: hook(.promptSubmitted, 0, id: "s3"), now: at(0))
        gone.ingest(records: [], now: at(ClaudeSessionEngine.unlistedGrace + 1))
        gone.ingest(records: [record("s3", status: .idle, changed: 0)], now: at(ClaudeSessionEngine.unlistedGrace + 2))
        suite.expect(only(gone, ClaudeSessionEngine.unlistedGrace + 3)?.turnStarted == nil,
                     "but not for long when it never appears")

        suite.expect(ClaudeToolSummary.summary(name: "Bash", input: ["command": "npm test\necho done"]) == "npm test",
                     "a command reads as its first line")
        suite.expect(ClaudeToolSummary.summary(name: "Edit", input: ["file_path": "/a/b/NotchView.swift"]) == "NotchView.swift",
                     "a file reads as its name")
        suite.expect(ClaudeToolSummary.summary(name: "WebFetch", input: ["url": "https://docs.example.com/x"]) == "docs.example.com",
                     "a fetch reads as its host")
        suite.expect(ClaudeToolSummary.summary(name: "Bash", input: ["command": "   "]) == nil, "blank input reads as nothing")
        let long = ClaudeToolSummary.summary(name: "Bash", input: ["command": String(repeating: "x", count: 200)]) ?? ""
        suite.expect(long.count == ClaudeToolSummary.limit && long.hasSuffix("…"), "long input is clipped")
    }

    // MARK: Support

    static func support(_ suite: TestSuite) {
        func session(_ id: String, _ phase: ClaudeSessionPhase, since: TimeInterval, project: String,
                     host: ClaudeSessionHost = .terminal(tty: "/dev/ttys001")) -> ClaudeSession {
            ClaudeSession(id: id, pid: Int32(id.count), cwd: "/x/" + project, project: project, name: project,
                          host: host, phase: phase, phaseSince: at(since), turnStarted: at(since), timeline: [],
                          subagents: [], tokens: AgentTokens(), cost: 0, hooked: false)
        }
        let order = NotchClaudeSessionsSupport.sorted([
            session("a", .idle, since: 0, project: "zeta"), session("b", .finished, since: 5, project: "b"),
            session("c", .generating, since: 9, project: "c"), session("d", .waitingPermission, since: 7, project: "d"),
            session("e", .finished, since: 8, project: "e"), session("f", .runningTool, since: 3, project: "f"),
            session("g", .idle, since: 0, project: "alpha"), session("h", .waitingInput, since: 1, project: "h"),
        ]).map(\.id)
        suite.expect(order == ["d", "h", "f", "c", "e", "b", "g", "a"],
                     "waiting sessions lead, then work by start, unread results newest first, idle by name: \(order)")

        let expand = NotchClaudeSessionsSupport.shouldAutoExpand
        let ask = ClaudeSessionEvent.needsPermission(id: "a", tool: "Bash")
        suite.expect(expand(ask, true, false, false, false), "a permission opens a closed island")
        suite.expect(expand(.needsInput(id: "a"), true, true, true, false), "and keeps the Sessions page open")
        suite.expect(!expand(ask, true, true, false, false), "but never takes over another open page")
        suite.expect(!expand(ask, false, false, false, false), "nor when the setting is off")
        suite.expect(!expand(ask, true, false, false, true), "nor while the island is held back")
        suite.expect(!expand(.finished(id: "a", duration: 3), true, false, false, false), "a result only shows a notice")

        suite.expect(NotchClaudeSessionsSupport.canReply(session("a", .finished, since: 0, project: "a")),
                     "a reply goes to a session in Terminal")
        suite.expect(!NotchClaudeSessionsSupport.canReply(session("a", .waitingPermission, since: 0, project: "a")),
                     "never while it waits on a dialog")
        suite.expect(!NotchClaudeSessionsSupport.canReply(session("a", .idle, since: 0, project: "a", host: .app("com.x"))),
                     "nor to a terminal the island cannot type into")
        suite.expect(NotchClaudeSessionsSupport.reply("  hi\n") == "hi" && NotchClaudeSessionsSupport.reply(" \n ") == nil,
                     "replies are trimmed and blank ones are not sent")
        suite.expect(NotchClaudeSessionsSupport.reply(String(repeating: "a", count: 9000))?.count
                     == NotchClaudeSessionsSupport.replyLimit, "long replies are capped")

        let reading = NotchClaudeSessionsSupport.stripReading(ClaudeSessionCounts(working: 2, waiting: 0, finished: 1))
        suite.expect(reading.map(\.count) == [2, 1], "the strip leaves out empty counts")
        let counts = ClaudeSessionCounts([session("a", .waitingInput, since: 0, project: "a"),
                                          session("b", .runningTool, since: 0, project: "b"),
                                          session("c", .finished, since: 0, project: "c")])
        suite.expect(counts == ClaudeSessionCounts(working: 1, waiting: 1, finished: 1), "sessions are counted by phase")

        let activities = NotchSupport.compactActivities(timer: false, watch: true, sessions: true, downloads: true, agents: true,
                                                        calendar: false, music: true)
        suite.expect(activities == [.watch, .sessions, .downloads, .agents, .music],
                     "sessions come before downloads and the AI reading beside the camera: \(activities)")
        suite.expect(NotchSupport.compactActivities(timer: false, downloads: false, agents: true, calendar: false, music: false)
                     == [.agents], "and are left out unless they show")
        suite.expect(NotchCompactActivity.sessions.module == .sessions && NotchModule.sessions.shortcutKey == "l",
                     "the strip opens the Sessions page, which has its own key")
        suite.expect(NotchEvent.sessions.preferenceKey == DefaultsKey.notchSessionsEnabled
                     && NotchEvent.sessions.priority >= NotchEvent.agents.priority,
                     "a session waiting on the person outranks the AI section's notices")
        let narrow = NotchClaudeSessionsSupport.stripReadingWidth(ClaudeSessionCounts(working: 1), textSize: 13)
        let wide = NotchClaudeSessionsSupport.stripReadingWidth(ClaudeSessionCounts(working: 12, waiting: 1, finished: 3), textSize: 13)
        suite.expect(narrow > 0 && wide > narrow && NotchClaudeSessionsSupport.stripReadingWidth(ClaudeSessionCounts(), textSize: 13) == 0,
                     "the strip grows with what it counts")

        let strings = FeatureStrings.notchClaudeSessions(.de)
        suite.expect(strings.title == NotchClaudeSessionStrings.enUS.title, "every language reads the English text for now")
        suite.expect(strings.tasks(done: 2, total: 5) == "2 of 5 tasks", "task progress formats both numbers")
    }
}
