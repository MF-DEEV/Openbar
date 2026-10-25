// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Codex's app-server threads as island sessions, the WebSocket framing and
/// the client against a fake daemon on a temporary socket. Nothing here
/// connects to the person's own `~/.codex`.
enum CodexSessionTests {
    static func run(_ suite: TestSuite) {
        suite.run("codex sessions state") { state(suite) }
        suite.run("codex conversation parity") { CodexConversationTests.run(suite) }
        suite.run("codex conversation review") { CodexConversationReviewTests.run(suite) }
        suite.run("codex sessions requests") { requests(suite) }
        suite.run("codex sessions commands") { commands(suite) }
        suite.run("codex sessions websocket") { websocket(suite) }
        suite.run("codex sessions home") { home(suite) }
        suite.run("codex sessions client") { client(suite) }
        suite.run("codex sessions review findings") { findings(suite) }
    }

    // MARK: Review findings

    /// Behaviors the review found wrong; each check states what should hold.
    static func findings(_ suite: TestSuite) {
        // An approval that arrives while an MCP elicitation waits must be the
        // card shown: the elicitation cannot be answered from the island, and
        // decide() only answers the request on screen.
        let mixed = stateWithThread()
        _ = mixed.request(.number(12), method: "mcpServer/elicitation/request",
                          params: ["threadId": "t1", "serverName": "figma", "mode": ["message": "Sign in?"]], now: at(1))
        _ = mixed.request(.number(13), method: "item/commandExecution/requestApproval",
                          params: ["threadId": "t1", "command": "ls"], now: at(2))
        let mixedSession = mixed.snapshot(now: at(2), previews: false).sessions.first
        suite.expect(mixedSession?.phase == .waitingPermission, "the session waits on the approval")
        suite.expect(mixedSession?.permission?.requestID == "codex:13",
                     "the answerable approval is the card, not the elicitation before it: got \(mixedSession?.permission?.requestID ?? "nil")")

        // A subagent's approval waits on its parent's row and is answered there.
        let family = stateWithThread()
        family.upsertThread(["id": "c1", "cwd": "/Users/me/code/app", "parentThreadId": "t1",
                             "status": ["type": "active", "activeFlags": []]])
        family.upsertThread(["id": "g1", "cwd": "/Users/me/code/app", "parentThreadId": "c1",
                             "status": ["type": "active", "activeFlags": []]])
        _ = family.request(.number(21), method: "item/fileChange/requestApproval", params: ["threadId": "g1"], now: at(1))
        let parentRow = family.snapshot(now: at(1), previews: false).sessions
        suite.expect(parentRow.map(\.id) == ["t1"] && parentRow.first?.phase == .waitingPermission
                         && parentRow.first?.permission?.requestID == "codex:21",
                     "a sub-subagent's approval is the parent's card: \(parentRow.map(\.phase))")
        suite.expect(family.takePending(thread: "t1", requestID: "codex:21") != nil,
                     "and is taken from the parent's row")
        suite.expect(family.threads["g1"]?.pending.isEmpty == true
                         && family.snapshot(now: at(2), previews: false).sessions.first?.permission == nil,
                     "and goes once answered")
        let other = stateWithThread()
        other.upsertThread(["id": "x1", "cwd": "/a", "status": ["type": "idle"]])
        _ = other.request(.number(22), method: "item/fileChange/requestApproval", params: ["threadId": "x1"], now: at(1))
        suite.expect(other.takePending(thread: "t1", requestID: "codex:22") == nil, "another row's request is not taken")
        let loop = CodexSessionState()
        loop.upsertThread(["id": "a", "cwd": "/a", "parentThreadId": "b"])
        loop.upsertThread(["id": "b", "cwd": "/a", "parentThreadId": "a"])
        suite.expect(loop.snapshot(now: at(1), previews: false).sessions.isEmpty && !loop.belongs("a", to: "c"),
                     "threads that name each other as parents end the walk")

        // A thread that a read reports as not loaded (it unloaded while the
        // read was in flight) is gone, as thread/status/changed notLoaded is;
        // no later broadcast would ever remove it.
        let ghost = CodexSessionState()
        ghost.notify("thread/status/changed", params: ["threadId": "t9", "status": ["type": "active", "activeFlags": []]],
                     now: at(1))
        ghost.notify("thread/status/changed", params: ["threadId": "t9", "status": ["type": "notLoaded"]], now: at(2))
        ghost.upsertThread(["id": "t9", "cwd": "/Users/me/code/app", "status": ["type": "notLoaded"]])
        suite.expect(ghost.snapshot(now: at(3), previews: false).sessions.isEmpty,
                     "a late read of an unloaded thread leaves no idle ghost: \(ghost.snapshot(now: at(3), previews: false).sessions.map(\.id))")

        // Streaming deltas change nothing the island shows, so they must not
        // wake the service to republish every session.
        guard let daemon = FakeDaemon() else { return suite.expect(false, "temporary home made") }
        defer { daemon.finish() }
        suite.expect(daemon.listen(), "the fake daemon listens")
        let changes = Counter()
        let client = CodexSessionClient(home: daemon.home, clientVersion: "test", retry: 0.1)
        client.start { changes.add() }
        defer { client.stop() }
        guard daemon.accept() else { return suite.expect(false, "the island connects") }
        let initialize = daemon.message()
        daemon.send(["id": initialize?["id"] ?? 0, "result": [:]])
        _ = daemon.message() // initialized
        let list = daemon.message()
        daemon.send(["id": list?["id"] ?? 0, "result": ["data": ["t1"]]])
        let read = daemon.message()
        daemon.send(["id": read?["id"] ?? 0, "result": ["thread": ["id": "t1", "cwd": "/Users/me/code/app",
                                                                    "status": ["type": "idle"]]]])
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.count == 1 }, "the thread shows")
        // A ping answered means everything sent before it was handled.
        daemon.raw(CodexWebSocket.encode(opcode: .ping, payload: Data("a".utf8), mask: nil))
        _ = daemon.frame()
        let before = changes.count
        for index in 0..<20 {
            daemon.send(["method": "item/agentMessage/delta",
                         "params": ["threadId": "t1", "turnId": "u", "itemId": "m", "delta": "word\(index) "]])
        }
        daemon.raw(CodexWebSocket.encode(opcode: .ping, payload: Data("b".utf8), mask: nil))
        _ = daemon.frame()
        suite.expect(changes.count == before,
                     "20 text deltas raise no change: they raised \(changes.count - before)")
    }

    static let t0 = Date(timeIntervalSince1970: 1_792_600_000)

    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    static func stateWithThread() -> CodexSessionState {
        let state = CodexSessionState()
        state.upsertThread([
            "id": "t1", "cwd": "/Users/vors/Projekty/checkout-api", "model": "gpt-5.5-codex",
            "gitInfo": ["branch": "feat/x"], "createdAt": 100, "updatedAt": 200, "status": ["type": "idle"],
        ])
        return state
    }

    static func json(_ value: Any?) -> String {
        guard let value,
              let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .fragmentsAllowed])
        else { return "nil" }
        return String(decoding: data, as: UTF8.self)
    }

    static func phase(_ state: CodexSessionState, _ seconds: TimeInterval, id: String = "t1") -> ClaudeSessionPhase? {
        state.threads[id].map { CodexSessionState.phase($0, now: at(seconds)) }
    }

    // MARK: State

    static func state(_ suite: TestSuite) {
        // a_thread_reads_as_a_codex_session
        let state = stateWithThread()
        let sessions = state.snapshot(now: Date(timeIntervalSince1970: 300), previews: false).sessions
        suite.expect(sessions.count == 1, "the thread is one session")
        let session = sessions.first
        suite.expect(session?.provider == .codex, "a thread reads as a Codex session")
        suite.expect(session?.project == "checkout-api", "the project is the folder")
        suite.expect(session?.name == "checkout-api" && session?.nameSource == nil, "an unnamed thread takes its folder's name")
        suite.expect(session?.branch == "feat/x", "the branch comes from gitInfo")
        suite.expect(session?.model == "gpt-5.5-codex", "the model is kept")
        suite.expect(session?.phase == .idle, "an idle thread is idle")
        suite.expect(session?.pid == 0 && session?.hooked == true, "no pid ties it to a terminal, and it can be answered")
        suite.expect(session?.lastActive == Date(timeIntervalSince1970: 200), "updatedAt orders quiet sessions")

        // each_turn_seen_only_by_its_status_starts_its_own_clock
        let clocks = stateWithThread()
        let active: [String: Any] = ["threadId": "t1", "status": ["type": "active", "activeFlags": []]]
        let idle: [String: Any] = ["threadId": "t1", "status": ["type": "idle"]]
        clocks.notify("thread/status/changed", params: active, now: at(300))
        clocks.notify("thread/status/changed", params: idle, now: at(310))
        clocks.notify("thread/status/changed", params: active, now: at(400))
        suite.expect(clocks.threads["t1"]?.turnStarted == at(400), "a second turn seen only by its status starts its own clock")

        // status_and_items_drive_the_phase
        let driven = stateWithThread()
        driven.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "active", "activeFlags": []]],
                      now: at(300))
        suite.expect(phase(driven, 300) == .generating, "an active thread generates")
        suite.expect(driven.threads["t1"]?.turnStarted == at(300), "going active starts the turn clock")
        driven.notify("item/started", params: ["threadId": "t1", "item": ["type": "commandExecution", "id": "i1",
                                                                           "command": "npm test"]], now: at(301))
        suite.expect(phase(driven, 301) == .runningTool, "a started command runs a tool")
        driven.notify("item/completed", params: ["threadId": "t1", "item": ["type": "commandExecution", "id": "i1",
                                                                             "status": "completed"]], now: at(302))
        suite.expect(phase(driven, 302) == .generating, "after the command it generates again")
        suite.expect(driven.threads["t1"]?.tools.first?.finished == at(302), "the tool ends when it completes")
        driven.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "active",
                                                                                      "activeFlags": ["waitingOnApproval"]]],
                      now: at(303))
        suite.expect(phase(driven, 303) == .waitingPermission, "waitingOnApproval waits on permission")
        driven.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "active",
                                                                                      "activeFlags": ["waitingOnUserInput"]]],
                      now: at(304))
        suite.expect(phase(driven, 304) == .waitingInput, "waitingOnUserInput waits on input")
        driven.notify("turn/completed", params: ["threadId": "t1", "turn": ["status": "completed", "completedAt": at(310).timeIntervalSince1970]],
                      now: at(310))
        driven.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "idle"]], now: at(310))
        suite.expect(phase(driven, 311) == .finished, "a completed turn finishes")
        suite.expect(phase(driven, 310 + CodexSessionState.finishedFor + 1) == .idle, "an unread result fades to idle")
        driven.acknowledge("t1", now: at(312))
        suite.expect(phase(driven, 313) == .idle, "looking at it makes it idle")

        // An interrupted turn is not news.
        let interrupted = stateWithThread()
        interrupted.notify("turn/completed", params: ["threadId": "t1", "turn": ["status": "interrupted"]], now: at(5))
        suite.expect(phase(interrupted, 6) == .idle, "an interrupted turn does not read as finished")
        suite.expect(interrupted.threads["t1"]?.turnEnded?.at == at(5), "without completedAt the turn ends now")

        // Milliseconds or seconds, whichever Codex sends.
        let millis = CodexSessionState()
        millis.upsertThread(["id": "m", "cwd": "/a", "createdAt": 1_792_600_000_123.0, "updatedAt": 1_792_600_001])
        suite.expectClose(millis.threads["m"]?.created?.timeIntervalSince1970 ?? 0, 1_792_600_000.123, "milliseconds read as seconds")
        suite.expect(millis.threads["m"]?.updated == Date(timeIntervalSince1970: 1_792_600_001), "seconds stay seconds")

        // Upserts keep what a later read leaves out.
        let kept = stateWithThread()
        kept.upsertThread(["id": "t1", "name": "Fix checkout", "preview": "fix the checkout bug", "status": ["type": "active"]])
        suite.expect(kept.threads["t1"]?.model == "gpt-5.5-codex" && kept.threads["t1"]?.branch == "feat/x"
                         && kept.threads["t1"]?.cwd == "/Users/vors/Projekty/checkout-api",
                     "a read without a field keeps the one known")
        kept.upsertThread(["id": "t1", "name": "", "model": ""])
        suite.expect(kept.threads["t1"]?.name == "Fix checkout" && kept.threads["t1"]?.model == "gpt-5.5-codex",
                     "empty strings count as absent")
        kept.upsertThread(["cwd": "/nowhere"])
        suite.expect(kept.threads.count == 1, "a thread without an id is ignored")
        let named = kept.snapshot(now: at(0), previews: true).sessions.first
        suite.expect(named?.name == "Fix checkout" && named?.nameSource == "user", "with previews the thread's own name wins")
        let unnamed = kept.snapshot(now: at(0), previews: false).sessions.first
        suite.expect(unnamed?.name == "checkout-api" && unnamed?.nameSource == nil,
                     "without previews the folder names it, as Codex may have made the name from the prompt")
        suite.expect(kept.notify("thread/name/updated", params: ["threadId": "t1", "threadName": "Ship it"], now: at(0)).changed
                         && kept.threads["t1"]?.name == "Ship it", "a rename is kept")
        suite.expect(!kept.notify("thread/name/updated", params: ["threadId": "t1", "threadName": "Ship it"], now: at(0)).changed,
                     "the same name again changes nothing")
        suite.expect(kept.notify("thread/name/updated", params: ["threadId": "t1", "threadName": NSNull()], now: at(0)).changed
                         && kept.threads["t1"]?.name == nil, "a cleared name goes")
        kept.upsertThread(["id": "t1", "name": "Fix checkout"])
        suite.expect(named?.phase == .generating, "status from the read drives the phase")

        // Previews gate what the session works on.
        let gated = stateWithThread()
        gated.upsertThread(["id": "t1", "preview": "secret prompt", "status": ["type": "active"]])
        gated.notify("item/started", params: ["threadId": "t1", "item": ["type": "commandExecution", "id": "i1",
                                                                          "command": "/bin/zsh -lc 'cat secrets.txt'"]],
                     now: at(1))
        let hidden = gated.snapshot(now: at(2), previews: false).sessions.first
        suite.expect(hidden?.title == nil, "without previews the title is withheld")
        suite.expect(hidden?.timeline.first?.summary == nil && hidden?.currentTool?.summary == nil,
                     "without previews tool summaries are withheld")
        suite.expect(hidden?.currentTool?.name == "Command", "the running tool is still named")
        let shown = gated.snapshot(now: at(3), previews: true).sessions.first
        suite.expect(shown?.title == "secret prompt", "with previews the title shows")
        suite.expect(shown?.timeline.first?.summary == "cat secrets.txt", "with previews the command shows, unwrapped")
        suite.expect(gated.threads["t1"]?.tools.first?.summary != nil, "the summary is kept for when previews turn on")

        // Hidden threads.
        let hiding = stateWithThread()
        hiding.upsertThread(["id": "e1", "cwd": "/a", "ephemeral": true, "status": ["type": "active"]])
        hiding.upsertThread(["id": "c1", "cwd": "/a", "parentThreadId": "t1"])
        hiding.notify("thread/status/changed", params: ["threadId": "n1", "status": ["type": "idle"]], now: at(0))
        let visible = hiding.snapshot(now: at(1), previews: true).sessions.map(\.id)
        suite.expect(visible == ["t1"], "ephemeral helpers, subagents and threads not read yet are hidden: \(visible)")
        hiding.upsertThread(["id": "e1", "ephemeral": false])
        suite.expect(hiding.snapshot(now: at(2), previews: true).sessions.map(\.id) == ["e1", "t1"],
                     "sessions come in a steady order")

        // Status notifications.
        let statuses = stateWithThread()
        suite.expect(statuses.notify("thread/status/changed", params: ["threadId": "new", "status": ["type": "idle"]],
                                     now: at(0)).read == "new", "a status for an unseen thread asks for a read")
        suite.expect(statuses.notify("thread/status/changed", params: ["threadId": "new", "status": ["type": "active"]],
                                     now: at(1)) == .init(changed: true), "a known thread is not read again")
        suite.expect(statuses.notify("thread/status/changed", params: ["threadId": "new", "status": ["type": "active"]],
                                     now: at(1)) == .init(), "the same status again changes nothing")
        suite.expect(statuses.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "notLoaded"]],
                                     now: at(2)) == .init(changed: true) && statuses.threads["t1"] == nil,
                     "notLoaded forgets the thread")
        suite.expect(statuses.notify("thread/status/changed", params: ["threadId": "gone", "status": ["type": "notLoaded"]],
                                     now: at(2)) == .init() && statuses.threads["gone"] == nil, "notLoaded never adds a thread")
        suite.expect(statuses.notify("item/agentMessage/delta", params: ["threadId": "new", "delta": "hi"], now: at(2)) == .init()
                         && statuses.notify("item/reasoning/textDelta", params: ["threadId": "new", "delta": "hm"], now: at(2)) == .init()
                         && statuses.notify("what/ever", params: ["threadId": "new"], now: at(2)) == .init()
                         && statuses.notify("item/started", params: ["threadId": "new", "item": ["type": "agentMessage", "id": "a"]],
                                            now: at(2)) == .init()
                         && statuses.notify("turn/started", params: ["threadId": "nobody"], now: at(2)) == .init(),
                     "deltas, messages, unknown methods and unknown threads change nothing")
        statuses.notify("thread/closed", params: ["threadId": "new"], now: at(3))
        suite.expect(statuses.threads.isEmpty, "a closed thread is forgotten")
        statuses.notify("turn/started", params: ["threadId": "nobody"], now: at(4))
        statuses.notify("item/started", params: ["threadId": "nobody", "item": ["type": "commandExecution"]], now: at(4))
        suite.expect(statuses.threads.isEmpty, "notifications for unknown threads add nothing")
        suite.expect(CodexSessionState.Status(json: ["type": "systemError"]) == .systemError
                         && CodexSessionState.Status(json: nil) == .notLoaded
                         && CodexSessionState.Status(json: ["type": "what"]) == .notLoaded,
                     "statuses decode leniently")

        // Turns and tools.
        let turns = stateWithThread()
        turns.notify("turn/started", params: ["threadId": "t1", "turn": ["startedAt": at(10).timeIntervalSince1970]], now: at(11))
        suite.expect(turns.threads["t1"]?.turnStarted == at(10), "turn/started gives the turn's start")
        turns.notify("item/started", params: ["threadId": "t1", "item": ["type": "agentMessage", "id": "a"]], now: at(12))
        suite.expect(turns.threads["t1"]?.tools.isEmpty == true, "messages are not tools")
        turns.notify("item/started", params: ["threadId": "t1", "startedAtMs": at(13).timeIntervalSince1970 * 1000,
                                              "item": ["type": "fileChange", "id": "f",
                                                       "changes": [["path": "/a/b/main.swift"], ["path": "C:\\x\\y.rs"]]]],
                     now: at(14))
        turns.notify("item/started", params: ["threadId": "t1", "item": ["type": "mcpToolCall", "id": "m",
                                                                          "server": "github", "tool": "search"]], now: at(15))
        turns.notify("item/started", params: ["threadId": "t1", "item": ["type": "webSearch", "id": "w",
                                                                          "query": String(repeating: "q", count: 100)]],
                     now: at(16))
        let tools = turns.threads["t1"]?.tools ?? []
        suite.expect(tools.map(\.name) == ["Edit", "MCP", "Search"], "tools are named by kind: \(tools.map(\.name))")
        suite.expect(tools.first?.started == at(13), "startedAtMs gives the tool's start")
        suite.expect(tools.first?.summary == "main.swift, y.rs", "file changes list their names")
        suite.expect(tools.dropFirst().first?.summary == "github.search", "MCP calls name server and tool")
        suite.expect(tools.last?.summary == String(repeating: "q", count: 79) + "…", "long summaries are clipped")
        turns.notify("item/completed", params: ["threadId": "t1", "item": ["type": "mcpToolCall", "id": "m",
                                                                            "status": "failed"]], now: at(17))
        suite.expect(turns.threads["t1"]?.tools[1].failed == true, "a failed tool is marked")
        turns.notify("item/completed", params: ["threadId": "t1", "item": ["type": "webSearch", "id": "w",
                                                                            "status": "declined"]], now: at(1))
        suite.expect(turns.threads["t1"]?.tools[2].finished == at(16) && turns.threads["t1"]?.tools[2].failed == true,
                     "a tool never ends before it started, and a declined one failed")
        turns.notify("item/started", params: ["threadId": "t1", "item": ["type": "commandExecution", "id": "c"]], now: at(18))
        _ = turns.request(.number(1), method: "item/fileChange/requestApproval", params: ["threadId": "t1"], now: at(18))
        turns.notify("turn/completed", params: ["threadId": "t1", "turn": ["status": "failed"]], now: at(20))
        suite.expect(turns.threads["t1"]?.pending.isEmpty == true, "a turn that ends takes its requests")
        suite.expect(turns.threads["t1"]?.tools.allSatisfy { $0.finished != nil } == true, "and closes its tools")
        suite.expect(turns.threads["t1"]?.tools.last?.finished == at(20), "at the turn's end")
        suite.expect(turns.threads["t1"]?.turnEnded?.completed == true, "a failed turn counts as done")

        let many = stateWithThread()
        for index in 0...CodexSessionState.toolsKept {
            many.notify("item/started", params: ["threadId": "t1", "item": ["type": "webSearch", "id": "w\(index)"]],
                        now: at(Double(index)))
        }
        suite.expect(many.threads["t1"]?.tools.count == CodexSessionState.timelineLength
                         && many.threads["t1"]?.tools.last?.id == "w\(CodexSessionState.toolsKept)",
                     "past the limit only the latest tools are kept")
        suite.expect(many.snapshot(now: at(100), previews: false).sessions.first?.timeline.count
                         == CodexSessionState.timelineLength, "the timeline shows the latest")

        // token_usage_takes_cache_out_of_input
        let tokens = stateWithThread()
        tokens.notify("thread/tokenUsage/updated", params: ["threadId": "t1", "tokenUsage": ["total": [
            "inputTokens": 1000, "cachedInputTokens": 600, "cacheWriteInputTokens": 100, "outputTokens": 50,
            "reasoningOutputTokens": 20,
        ]]], now: at(1))
        let t = tokens.threads["t1"]?.tokens
        suite.expect(t?.input == 300 && t?.cacheRead == 600 && t?.cacheWrite == 100 && t?.output == 50 && t?.reasoning == 20,
                     "token usage takes the cache out of the input")
        tokens.notify("thread/tokenUsage/updated", params: ["threadId": "t1", "tokenUsage": ["total": [
            "inputTokens": 10, "cachedInputTokens": 600,
        ]]], now: at(2))
        suite.expect(tokens.threads["t1"]?.tokens.input == 0, "input never goes below zero")

        // Events.
        let events = stateWithThread()
        suite.expect(events.snapshot(now: at(0), previews: false).events.isEmpty, "the first snapshot raises nothing")
        events.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "active"]], now: at(1))
        let working = events.snapshot(now: at(1), previews: false)
        suite.expect(working.events.isEmpty, "starting work is not news")
        suite.expect(working.sessions.first?.phaseSince == at(1), "the phase clock starts at the change")
        suite.expect(events.snapshot(now: at(2), previews: false).sessions.first?.phaseSince == at(1),
                     "the phase clock holds while the phase does")
        _ = events.request(.number(3), method: "item/commandExecution/requestApproval",
                           params: ["threadId": "t1", "command": "ls"], now: at(3))
        suite.expect(events.snapshot(now: at(3), previews: false).events == [.needsPermission(id: "t1", tool: "Codex")],
                     "an approval asks for permission")
        events.notify("serverRequest/resolved", params: ["threadId": "t1", "requestId": 3], now: at(4))
        _ = events.snapshot(now: at(4), previews: false)
        _ = events.request(.string("q"), method: "item/tool/requestUserInput", params: ["threadId": "t1"], now: at(5))
        suite.expect(events.snapshot(now: at(5), previews: false).events == [.needsInput(id: "t1")], "a question asks for input")
        events.notify("turn/completed", params: ["threadId": "t1", "turn": ["status": "completed"]], now: at(9))
        events.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "idle"]], now: at(9))
        suite.expect(events.snapshot(now: at(9), previews: false).events == [.finished(id: "t1", duration: 8)],
                     "a finished turn tells how long it ran")
        suite.expect(events.snapshot(now: at(10), previews: false).events.isEmpty, "and only once")
    }

    // MARK: Requests

    static func requests(_ suite: TestSuite) {
        // an_approval_becomes_a_card_and_is_answered_without_stopping_the_turn
        let state = stateWithThread()
        let params: [String: Any] = ["threadId": "t1", "turnId": "u", "itemId": "i", "command": "rm -rf build",
                                     "cwd": "/x", "reason": "clean"]
        suite.expect(state.request(.number(7), method: "item/commandExecution/requestApproval", params: params, now: at(400)),
                     "an approval is kept")
        let snapshot = state.snapshot(now: at(400), previews: false)
        let card = snapshot.sessions.first?.permission
        suite.expect(card?.requestID == "codex:7", "the card is known by the JSON-RPC id")
        suite.expect(card?.detail == "rm -rf build\n\nin /x\n\nclean", "the card reads command, folder and reason: \(card?.detail ?? "")")
        suite.expect(card?.summary == "rm -rf build" && card?.tool == "Command", "the card names the command")
        suite.expect(card?.offersAlways == true, "commands offer allowing for the session")
        suite.expect(card?.since == at(400), "the card tells when it was asked")
        suite.expect(snapshot.events.isEmpty, "the first snapshot raises nothing")
        suite.expect(snapshot.sessions.first?.phase == .waitingPermission, "the session waits on permission")
        guard let pending = state.threads["t1"]?.pending.first else { return suite.expect(false, "pending kept") }
        suite.expect(pending.answerable, "a command approval can be answered")
        suite.expect(json(CodexSessionState.result(pending, .deny)) == #"{"decision":"decline"}"#, "deny declines")
        suite.expect(json(CodexSessionState.result(pending, .allowForSession)) == #"{"decision":"acceptForSession"}"#,
                     "always accepts for the session")
        suite.expect(json(CodexSessionState.result(pending, .allow)) == #"{"decision":"accept"}"#, "allow accepts")
        suite.expect(CodexSessionState.result(pending, .questions(["a": ["b"]])) == nil, "an approval takes no answers")
        state.notify("serverRequest/resolved", params: ["threadId": "t1", "requestId": 7], now: at(401))
        suite.expect(state.threads["t1"]?.pending.isEmpty == true, "an answer elsewhere drops the card")

        // The same id again replaces the request.
        _ = state.request(.number(8), method: "item/fileChange/requestApproval", params: ["threadId": "t1"], now: at(402))
        _ = state.request(.number(8), method: "item/fileChange/requestApproval",
                          params: ["threadId": "t1", "reason": "again"], now: at(403))
        suite.expect(state.threads["t1"]?.pending.count == 1, "a repeated request is kept once")
        let edit = state.snapshot(now: at(403), previews: false).sessions.first?.permission
        suite.expect(edit?.tool == "Edit" && edit?.detail == "again" && edit?.summary == nil, "a file change reads its reason")
        state.notify("serverRequest/resolved", params: ["threadId": "t1", "requestId": "8"], now: at(404))
        suite.expect(state.threads["t1"]?.pending.count == 1, "a string id never matches a number id")
        suite.expect(state.takePending(thread: "t1", requestID: "codex:8") != nil
                         && state.threads["t1"]?.pending.isEmpty == true, "the island takes the request it answers")
        _ = state.request(.number(9), method: "item/fileChange/requestApproval", params: ["threadId": "t1"], now: at(405))
        suite.expect(state.snapshot(now: at(405), previews: false).sessions.first?.permission?.detail
                         == "Codex wants to change files.", "a file change without a reason says what it wants")

        // questions_are_answered_by_their_codex_ids
        let asking = stateWithThread()
        let questionParams: [String: Any] = ["threadId": "t1", "questions": [
            ["id": "q1", "header": "DB", "question": "Which database?",
             "options": [["label": "Postgres", "description": "server"], ["label": "SQLite"]]],
            ["id": "q2", "question": "Name?", "options": ["plain"]],
            ["header": "no id", "question": "Skipped?"],
        ]]
        suite.expect(asking.request(.string("r2"), method: "item/tool/requestUserInput", params: questionParams, now: at(500)),
                     "a question is kept")
        suite.expect(phase(asking, 500) == .waitingInput, "a question waits on input")
        guard let question = asking.threads["t1"]?.pending.first else { return suite.expect(false, "question kept") }
        suite.expect(question.requestID == #"codex:"r2""#, "a string id is written as JSON writes it")
        let askedCard = asking.snapshot(now: at(500), previews: false).sessions.first?.permission
        suite.expect(askedCard?.questions.map(\.question) == ["Which database?", "Name?"], "questions without an id are left out")
        suite.expect(askedCard?.questions.first?.header == "DB"
                         && askedCard?.questions.first?.options.map(\.label) == ["Postgres", "SQLite"]
                         && askedCard?.questions.first?.options.first?.description == "server",
                     "options keep labels and descriptions")
        suite.expect(askedCard?.questions.last?.options.map(\.label) == ["plain"], "plain string options are labels")
        suite.expect(askedCard?.asksQuestions == true && askedCard?.tool == "Codex", "the card asks rather than allows")
        suite.expect(json(CodexSessionState.result(question, .questions(["Which database?": ["SQLite"]])))
                         == #"{"answers":{"q1":{"answers":["SQLite"]}}}"#,
                     "answers go back by Codex's question ids")
        suite.expect(json(CodexSessionState.result(question, .deny)) == #"{"answers":{}}"#, "skipping answers nothing")
        suite.expect(CodexSessionState.result(question, .allow) == nil, "a question is never allowed")

        // Permissions.
        let access = stateWithThread()
        let asked: [String: Any] = ["network": ["enabled": true], "fileSystem": ["write": ["/tmp/a"]]]
        _ = access.request(.number(11), method: "item/permissions/requestApproval",
                           params: ["threadId": "t1", "reason": "needs the network", "permissions": asked], now: at(1))
        guard let grant = access.threads["t1"]?.pending.first else { return suite.expect(false, "permissions kept") }
        suite.expect(json(CodexSessionState.result(grant, .allow)) == json(["permissions": asked, "scope": "turn"]),
                     "allow grants what was asked for the turn")
        suite.expect(json(CodexSessionState.result(grant, .allowForSession)) == json(["permissions": asked, "scope": "session"]),
                     "always grants it for the session")
        suite.expect(json(CodexSessionState.result(grant, .deny)) == #"{"permissions":{}}"#, "deny grants nothing")
        let accessCard = access.snapshot(now: at(1), previews: false).sessions.first?.permission
        suite.expect(accessCard?.tool == "Permissions" && accessCard?.detail.hasPrefix("needs the network\n\n{") == true
                         && accessCard?.detail.contains("/tmp/a") == true, "the card shows what is asked")

        // Elicitations show and wait.
        let elicit = stateWithThread()
        _ = elicit.request(.number(12), method: "mcpServer/elicitation/request",
                           params: ["threadId": "t1", "serverName": "figma", "mode": ["message": "Sign in?"]], now: at(1))
        guard let elicitation = elicit.threads["t1"]?.pending.first else { return suite.expect(false, "elicitation kept") }
        suite.expect(!elicitation.answerable, "an elicitation is not answered from the island")
        suite.expect(CodexSessionState.result(elicitation, .allow) == nil, "and has no answer")
        suite.expect(phase(elicit, 1) == .waitingInput, "it waits on input")
        let elicitSession = elicit.snapshot(now: at(1), previews: false).sessions.first
        suite.expect(elicitSession?.permission == nil && elicitSession?.waitingFor == CodexSessionState.answerInTerminal
                         && elicitSession?.phase == .waitingInput,
                     "with no card it can answer, the session waits on the terminal")
        let elicitCard = CodexSessionState.permission(elicitation)
        suite.expect(elicitCard.tool == "figma" && elicitCard.detail == "Sign in?" && !elicitCard.offersAlways,
                     "its card would name the server and its message")
        suite.expect(elicit.takePending(thread: "t1", requestID: "codex:12") == nil, "the island cannot take it")
        // An approval outranks a question.
        _ = elicit.request(.number(13), method: "item/fileChange/requestApproval", params: ["threadId": "t1"], now: at(2))
        suite.expect(phase(elicit, 2) == .waitingPermission, "an approval waits on permission before a question")

        // unknown_requests_are_left_to_the_terminal
        let unknown = stateWithThread()
        suite.expect(!unknown.request(.number(9), method: "item/tool/call", params: ["threadId": "t1"], now: at(1)),
                     "an unknown request is not kept")
        suite.expect(!unknown.request(.number(10), method: "item/fileChange/requestApproval", params: ["threadId": "x"], now: at(1)),
                     "a request for an unknown thread is not kept")
        suite.expect(!unknown.request(.number(10), method: "item/fileChange/requestApproval", params: [:], now: at(1)),
                     "a request without a thread is not kept")
        suite.expect(unknown.threads["t1"]?.pending.isEmpty == true, "nothing is pending")

        // Long details are clipped and say so.
        let long = stateWithThread()
        _ = long.request(.number(1), method: "item/commandExecution/requestApproval",
                         params: ["threadId": "t1", "command": String(repeating: "x", count: 8010)], now: at(1))
        let clipped = long.snapshot(now: at(1), previews: false).sessions.first?.permission?.detail ?? ""
        suite.expect(clipped.hasSuffix("\n… 10 more characters") && clipped.hasPrefix(String(repeating: "x", count: 8000)),
                     "a long command is clipped and says so")

        // JSON-RPC ids.
        suite.expect(CodexRPCID(json: 7) == .number(7) && CodexRPCID(json: "7") == .string("7"), "ids keep their kind")
        suite.expect(CodexRPCID(json: true) == nil && CodexRPCID(json: NSNull()) == nil && CodexRPCID(json: 1.5) == nil,
                     "only numbers and strings are ids")
        suite.expect(CodexRPCID.string(#"a"b"#).description == #""a\"b""#, "string ids are JSON-escaped")

        // The island's decisions.
        suite.expect(CodexAnswer(.allow) == .allow && CodexAnswer(.allowAlways) == .allowForSession
                         && CodexAnswer(.deny) == .deny && CodexAnswer(.answer(["q": ["a"]])) == .questions(["q": ["a"]]),
                     "the island's decisions map to Codex's answers")
    }

    // MARK: Commands

    static func commands(_ suite: TestSuite) {
        // shell_wrappers_are_peeled_off_commands
        let cases: [(String, String)] = [
            (#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -Command 'curl.exe -sI https://example.com'"#,
             "curl.exe -sI https://example.com"),
            ("pwsh.exe -NoProfile -Command 'echo ''hi'''", "echo 'hi'"),
            (#"powershell.exe -Command "dir""#, "dir"),
            ("POWERSHELL.EXE -Command Get-Date", "Get-Date"),
            ("cmd.exe /c dir", "dir"),
            ("git status", "git status"),
            ("  git status  ", "git status"),
            // macOS and Linux: `<shell> -lc <script>`, shell-quoted.
            ("/bin/zsh -lc 'npm test'", "npm test"),
            ("/bin/bash -lc 'git diff --stat'", "git diff --stat"),
            ("bash -c 'ls -la'", "ls -la"),
            ("/bin/sh -l -c 'make'", "make"),
            ("zsh --login -c 'whoami'", "whoami"),
            ("/bin/zsh -lc ls", "ls"),
            (#"/bin/zsh -lc 'echo '"'"'hi'"'"''"#, "echo 'hi'"),
            (#"/bin/zsh -lc "echo \"hi\" \$HOME""#, #"echo "hi" $HOME"#),
            (#"/bin/zsh -lc 'echo cmd /c foo'"#, "echo cmd /c foo"),
            ("/bin/zsh -lc 'line one\nline two'", "line one\nline two"),
            (#"/bin/zsh -lc 'printf "%s" a\ b'"#, #"printf "%s" a\ b"#),
            // Not a wrapper, or not a whole one: shown as it came.
            ("zsh script.sh", "zsh script.sh"),
            ("/bin/zsh -lc", "/bin/zsh -lc"),
            ("/bin/zsh -lc 'unclosed", "/bin/zsh -lc 'unclosed"),
            ("/usr/bin/python3 -c 'print(1)'", "/usr/bin/python3 -c 'print(1)'"),
            ("bash -e script.sh", "bash -e script.sh"),
            ("", ""),
        ]
        for (command, expected) in cases {
            let readable = CodexSessionState.readableCommand(command)
            suite.expect(readable == expected, "\(command) reads as \(expected), got \(readable)")
        }
        suite.expect(CodexSessionState.shellWords(#"a 'b c' "d\"e" f\ g ''"#) == ["a", "b c", "d\"e", "f g", ""],
                     "words split as a shell splits them")
        suite.expect(CodexSessionState.shellWords(#"a "b"#) == nil && CodexSessionState.shellWords("a \\") == nil,
                     "an open quote or a trailing backslash is not a command line")
        suite.expect(CodexSessionState.shellWords(#""a\nb""#) == [#"a\nb"#], "inside double quotes only some backslashes escape")

        let state = stateWithThread()
        _ = state.request(.number(1), method: "item/commandExecution/requestApproval",
                          params: ["threadId": "t1", "command": "/bin/zsh -lc 'npm run build\nnpm test'"], now: at(1))
        let card = state.snapshot(now: at(1), previews: false).sessions.first?.permission
        suite.expect(card?.summary == "npm run build" && card?.detail == "npm run build\nnpm test",
                     "a wrapped command is shown as its script, the first line for the list")
    }

    // MARK: WebSocket

    static func decode(_ data: Data) throws -> [CodexWebSocket.Frame] {
        var decoder = CodexWebSocket.Decoder()
        decoder.append(data)
        var frames: [CodexWebSocket.Frame] = []
        while let frame = try decoder.next() { frames.append(frame) }
        return frames
    }

    static func websocket(_ suite: TestSuite) {
        let mask: [UInt8] = [0x37, 0xfa, 0x21, 0x3d]
        // RFC 6455 §5.7: a masked "Hello".
        let hello = CodexWebSocket.encode(opcode: .text, payload: Data("Hello".utf8), mask: mask)
        suite.expect([UInt8](hello) == [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58],
                     "a masked frame matches RFC 6455's example")
        let unmasked = CodexWebSocket.encode(opcode: .text, payload: Data("Hello".utf8), mask: nil)
        suite.expect([UInt8](unmasked) == [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f], "an unmasked frame too")
        let decodedHello = try? decode(hello)
        suite.expect(decodedHello == [CodexWebSocket.Frame(fin: true, opcode: .text, payload: Data("Hello".utf8), masked: true)],
                     "a masked frame decodes unmasked")
        suite.expect(CodexWebSocket.newMask().count == 4, "a mask is four bytes")

        for length in [0, 1, 125, 126, 127, 65_535, 65_536, 200_000] {
            let payload = Data((0..<length).map { UInt8(truncatingIfNeeded: $0 &* 31) })
            let encoded = CodexWebSocket.encode(opcode: .binary, payload: payload, mask: CodexWebSocket.newMask())
            let header = length < 126 ? 2 : length <= 65_535 ? 4 : 10
            suite.expect(encoded.count == header + 4 + length, "a \(length)-byte frame has a \(header)-byte header")
            let marker = encoded[1] & 0x7f
            suite.expect(marker == (length < 126 ? UInt8(length) : length <= 65_535 ? 126 : 127),
                         "a \(length)-byte frame uses the right length form")
            suite.expect(encoded[1] & 0x80 != 0, "client frames are masked")
            let frames = try? decode(encoded)
            suite.expect(frames?.count == 1 && frames?.first?.payload == payload, "a \(length)-byte frame round-trips")
        }
        let big = CodexWebSocket.encode(opcode: .text, payload: Data(count: 65_536), mask: nil)
        suite.expect([UInt8](big.prefix(10)) == [0x81, 127, 0, 0, 0, 0, 0, 1, 0, 0], "a 64-bit length is big-endian")
        let medium = CodexWebSocket.encode(opcode: .text, payload: Data(count: 300), mask: nil)
        suite.expect([UInt8](medium.prefix(4)) == [0x81, 126, 0x01, 0x2c], "a 16-bit length is big-endian")

        // Bytes a few at a time.
        let two = CodexWebSocket.encode(opcode: .text, payload: Data(repeating: 65, count: 300), mask: nil)
            + CodexWebSocket.encode(opcode: .ping, payload: Data("p".utf8), mask: nil)
        var decoder = CodexWebSocket.Decoder()
        var frames: [CodexWebSocket.Frame] = []
        for byte in two {
            decoder.append(Data([byte]))
            while let frame = try? decoder.next() { frames.append(frame) }
        }
        suite.expect(frames.map(\.opcode) == [.text, .ping] && frames.first?.payload.count == 300,
                     "frames split across reads come out whole")

        // Fragments, with a ping between them.
        var assembler = CodexWebSocket.Assembler()
        let parts: [CodexWebSocket.Frame] = [
            .init(fin: false, opcode: .text, payload: Data("Hel".utf8)),
            .init(fin: true, opcode: .ping, payload: Data("x".utf8)),
            .init(fin: false, opcode: .continuation, payload: Data("l".utf8)),
            .init(fin: true, opcode: .continuation, payload: Data("o ✓".utf8)),
        ]
        let messages = parts.compactMap { try? assembler.push($0) }
        suite.expect(messages == [.ping(Data("x".utf8)), .text("Hello ✓")], "fragments join around a control frame: \(messages)")
        // A character split between fragments.
        var split = CodexWebSocket.Assembler()
        let check = Array("✓".utf8)
        _ = try? split.push(.init(fin: false, opcode: .text, payload: Data(check.prefix(1))))
        let joined = try? split.push(.init(fin: true, opcode: .continuation, payload: Data(check.dropFirst())))
        suite.expect(joined == .text("✓"), "a character split between fragments joins")

        func fails(_ frames: [CodexWebSocket.Frame], _ label: String) {
            var assembler = CodexWebSocket.Assembler()
            var threw = false
            for frame in frames {
                do { _ = try assembler.push(frame) } catch { threw = true }
            }
            suite.expect(threw, label)
        }
        fails([.init(fin: true, opcode: .continuation, payload: Data())], "a continuation without a message is refused")
        fails([.init(fin: false, opcode: .text, payload: Data()), .init(fin: true, opcode: .text, payload: Data())],
              "a new message inside a fragmented one is refused")
        fails([.init(fin: true, opcode: .text, payload: Data([0xff, 0xfe]))], "text that is not UTF-8 is refused")
        suite.expect((try? decode(Data([0x89, 126, 0, 126] + [UInt8](repeating: 0, count: 126)))) == nil,
                     "a control frame over 125 bytes is refused")
        suite.expect((try? decode(Data([0x09, 0x00]))) == nil, "a fragmented control frame is refused")
        suite.expect((try? decode(Data([0xC1, 0x00]))) == nil, "reserved bits are refused")
        suite.expect((try? decode(Data([0x83, 0x00]))) == nil, "unknown opcodes are refused")
        suite.expect((try? decode(Data([0x82, 127, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]))) == nil,
                     "an impossible length is refused")
        suite.expect((try? decode(Data([0x81, 0x05, 0x48]))) == [], "a frame still arriving waits")
        let pong = try? decode(CodexWebSocket.encode(opcode: .pong, payload: Data("x".utf8), mask: mask))
        suite.expect(pong?.first?.opcode == .pong && pong?.first?.payload == Data("x".utf8), "a pong carries the ping's data")

        // The upgrade.
        suite.expect(CodexWebSocket.acceptKey(for: "dGhlIHNhbXBsZSBub25jZQ==") == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=",
                     "the accept key matches RFC 6455's example")
        let request = String(decoding: CodexWebSocket.request(key: "abc"), as: UTF8.self)
        suite.expect(request.hasPrefix("GET /rpc HTTP/1.1\r\nHost: localhost\r\n") && request.hasSuffix("\r\n\r\n")
                         && request.contains("Sec-WebSocket-Key: abc\r\n") && request.contains("Sec-WebSocket-Version: 13\r\n"),
                     "the upgrade asks for ws://localhost/rpc")
        let key = "dGhlIHNhbXBsZSBub25jZQ=="
        let good = "HTTP/1.1 101 Switching Protocols\r\nupgrade: WebSocket\r\nConnection: upgrade\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        suite.expect((try? CodexWebSocket.validate(response: good, key: key)) != nil, "a proper answer is accepted")
        suite.expect((try? CodexWebSocket.validate(response: good.replacingOccurrences(of: "101", with: "400"), key: key)) == nil,
                     "a refused upgrade fails")
        suite.expect((try? CodexWebSocket.validate(response: good.replacingOccurrences(of: "xOo=", with: "xOx="), key: key)) == nil,
                     "a wrong accept key fails")
    }

    // MARK: Home

    static func home(_ suite: TestSuite) {
        let previous = ProcessInfo.processInfo.environment["CODEX_HOME"]
        defer {
            if let previous { setenv("CODEX_HOME", previous, 1) } else { unsetenv("CODEX_HOME") }
        }
        setenv("CODEX_HOME", "/tmp/somewhere-codex", 1)
        suite.expect(CodexSessionClient.home()?.path == "/tmp/somewhere-codex", "CODEX_HOME is respected")
        setenv("CODEX_HOME", "", 1)
        suite.expect(CodexSessionClient.home()?.path == FileManager.default.homeDirectoryForCurrentUser
                         .appendingPathComponent(".codex").path, "an empty CODEX_HOME means ~/.codex")
        unsetenv("CODEX_HOME")
        suite.expect(CodexSessionClient.home()?.lastPathComponent == ".codex", "without CODEX_HOME it is ~/.codex")
        suite.expect(CodexSessionClient.socketPath(home: URL(fileURLWithPath: "/h")).path
                         == "/h/app-server-control/app-server-control.sock", "the control socket lives under the home")
        let uuid = CodexSessionClient.uuidV7(now: Date(timeIntervalSince1970: 1_700_000_000.123))
        let parts = uuid.split(separator: "-").map(String.init)
        suite.expect(parts.map(\.count) == [8, 4, 4, 4, 12], "a UUID has five groups: \(uuid)")
        suite.expect(parts.first.map { $0 + parts[1] } == String(format: "%012llx", UInt64(1_700_000_000_123)),
                     "a UUIDv7 starts with its milliseconds")
        suite.expect(parts[2].hasPrefix("7") && "89ab".contains(parts[3].first ?? "x"), "and carries version 7 and the variant")
        suite.expect(CodexSessionClient.uuidV7() != CodexSessionClient.uuidV7(), "the rest is random")
    }

    // MARK: Client

    /// A stand-in for Codex's daemon on a temporary socket.
    final class FakeDaemon {
        let home: URL
        let path: String
        private var listener: Int32 = -1
        private(set) var connection: CodexSocket?
        private var decoder = CodexWebSocket.Decoder()
        private var assembler = CodexWebSocket.Assembler()
        private(set) var allMasked = true

        init?() {
            var template = Array("/tmp/vcx.XXXXXX".utf8CString)
            guard let made = mkdtemp(&template) else { return nil }
            home = URL(fileURLWithPath: String(cString: made), isDirectory: true)
            let control = home.appendingPathComponent("app-server-control")
            try? FileManager.default.createDirectory(at: control, withIntermediateDirectories: true)
            path = CodexSessionClient.socketPath(home: home).path
        }

        func listen() -> Bool {
            listener = socket(AF_UNIX, SOCK_STREAM, 0)
            var address = CodexSocket.address(path)
            let bound = withUnsafePointer(to: &address) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    bind(listener, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
                }
            }
            return bound == 0 && Darwin.listen(listener, 4) == 0
        }

        /// Accepts the island and answers its upgrade.
        func accept(timeout: TimeInterval = 5) -> Bool {
            var fds = [pollfd(fd: listener, events: Int16(POLLIN), revents: 0)]
            guard poll(&fds, 1, Int32(timeout * 1000)) == 1 else { return false }
            let fd = Darwin.accept(listener, nil, nil)
            guard fd >= 0 else { return false }
            let socket = CodexSocket(fd: fd)
            connection = socket
            decoder = CodexWebSocket.Decoder()
            assembler = CodexWebSocket.Assembler()
            var head = Data()
            let end = Data("\r\n\r\n".utf8)
            let deadline = Date().addingTimeInterval(timeout)
            while head.range(of: end) == nil, Date() < deadline {
                guard (try? socket.wait(timeout: 0.2)) == .readable, let data = try? socket.read() else { continue }
                head.append(data)
            }
            let text = String(decoding: head, as: UTF8.self)
            guard text.hasPrefix("GET /rpc HTTP/1.1\r\n"), text.contains("Host: localhost\r\n"),
                  let line = text.components(separatedBy: "\r\n").first(where: { $0.hasPrefix("Sec-WebSocket-Key: ") })
            else { return false }
            let key = String(line.dropFirst("Sec-WebSocket-Key: ".count))
            let answer = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                + "Sec-WebSocket-Accept: \(CodexWebSocket.acceptKey(for: key))\r\n\r\n"
            return (try? socket.write(Data(answer.utf8))) != nil
        }

        /// The island's next frame.
        func frame(timeout: TimeInterval = 3) -> CodexWebSocket.Frame? {
            guard let socket = connection else { return nil }
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline {
                if let frame = try? decoder.next() {
                    if !frame.masked { allMasked = false }
                    return frame
                }
                guard (try? socket.wait(timeout: 0.1)) == .readable else { continue }
                guard let data = try? socket.read() else { return nil }
                decoder.append(data)
            }
            return nil
        }

        /// The island's next JSON-RPC message.
        func message(timeout: TimeInterval = 3) -> [String: Any]? {
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline, let frame = frame(timeout: deadline.timeIntervalSinceNow) {
                do {
                    guard let message = try assembler.push(frame) else { continue }
                    if case .text(let text) = message {
                        return (try? JSONSerialization.jsonObject(with: Data(text.utf8))) as? [String: Any]
                    }
                } catch {
                    return nil
                }
            }
            return nil
        }

        func send(_ message: [String: Any]) {
            guard let data = try? JSONSerialization.data(withJSONObject: message) else { return }
            raw(CodexWebSocket.encode(opcode: .text, payload: data, mask: nil))
        }

        func raw(_ data: Data) {
            try? connection?.write(data)
        }

        func hangUp() {
            connection?.close()
            connection = nil
        }

        func finish() {
            hangUp()
            if listener >= 0 { close(listener) }
            try? FileManager.default.removeItem(at: home)
        }
    }

    static func eventually(_ timeout: TimeInterval = 3, _ condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if condition() { return true }
            usleep(10_000)
        }
        return condition()
    }

    final class Counter {
        private let lock = NSLock()
        private var value = 0
        func add() { lock.withLock { value += 1 } }
        var count: Int { lock.withLock { value } }
    }

    static func client(_ suite: TestSuite) {
        // No daemon: nothing to show, nothing to send.
        let idle = CodexSessionClient(home: nil, clientVersion: "test", retry: 0.05)
        idle.start {}
        usleep(100_000)
        suite.expect(!idle.connected && idle.snapshot(now: Date(), previews: false).sessions.isEmpty,
                     "without a daemon there are no sessions")
        suite.expect(!idle.reply("t1", text: "hi") && !idle.answer("t1", requestID: "codex:1", answer: .allow),
                     "without a daemon nothing is sent")
        idle.stop()

        guard let daemon = FakeDaemon() else { return suite.expect(false, "temporary home made") }
        defer { daemon.finish() }
        let missing = CodexSessionClient(home: daemon.home, clientVersion: "test", retry: 0.05)
        missing.start {}
        usleep(150_000)
        suite.expect(!missing.connected, "a home without the socket is not joined")
        missing.stop()

        suite.expect(daemon.listen(), "the fake daemon listens at \(daemon.path)")
        let changes = Counter()
        let client = CodexSessionClient(home: daemon.home, clientVersion: "9.9", retry: 0.1)
        client.start { changes.add() }
        defer { client.stop() }
        guard daemon.accept() else { return suite.expect(false, "the island connects and upgrades") }

        func expectCall(_ method: String, _ label: String) -> [String: Any]? {
            let message = daemon.message()
            suite.expect(message?["method"] as? String == method, "\(label): expected \(method), got \(json(message))")
            return message
        }
        func params(_ message: [String: Any]?) -> [String: Any] { message?["params"] as? [String: Any] ?? [:] }

        // Handshake.
        let initialize = expectCall("initialize", "the island introduces itself")
        let info = params(initialize)["clientInfo"] as? [String: Any]
        suite.expect(info?["name"] as? String == "vorssaint" && info?["title"] as? String == "Vorssaint"
                         && info?["version"] as? String == "9.9", "with its name and version")
        suite.expect((params(initialize)["capabilities"] as? [String: Any])?["experimentalApi"] as? Bool == true,
                     "and asks for the experimental API")
        let optedOut = (params(initialize)["capabilities"] as? [String: Any])?["optOutNotificationMethods"] as? [String]
        suite.expect(optedOut == CodexSessionClient.quietNotifications && optedOut?.contains("item/agentMessage/delta") == true,
                     "and opts out of streamed deltas: \(optedOut ?? [])")
        suite.expect(initialize?["jsonrpc"] == nil, "Codex's dialect has no jsonrpc field")
        suite.expect(eventually { client.connected }, "the island reads as connected")
        daemon.send(["id": initialize?["id"] ?? 0, "result": ["userAgent": "codex_cli_rs/0.164.0"]])
        let initialized = daemon.message()
        suite.expect(initialized?["method"] as? String == "initialized" && initialized?["id"] == nil,
                     "the island confirms with a notification")
        let list = expectCall("thread/loaded/list", "the island lists loaded threads")
        suite.expect(eventually { client.version == "codex_cli_rs/0.164.0" }, "the daemon's version is kept")
        daemon.send(["id": list?["id"] ?? 0, "result": ["data": ["t1", "t2"], "nextCursor": "c1"]])
        let read1 = expectCall("thread/read", "each loaded thread is read")
        let read2 = expectCall("thread/read", "each loaded thread is read")
        let next = expectCall("thread/loaded/list", "the list's next page is asked for")
        suite.expect(params(read1)["threadId"] as? String == "t1" && params(read1)["includeTurns"] as? Bool == false
                         && params(read2)["threadId"] as? String == "t2", "reads name their threads, without turns")
        suite.expect(params(next)["cursor"] as? String == "c1", "the next page carries the cursor")
        daemon.send(["id": next?["id"] ?? 0, "result": ["data": ["t3"]]])
        let read3 = expectCall("thread/read", "the next page's thread is read")
        let t1: [String: Any] = ["id": "t1", "cwd": "/Users/me/code/app", "name": "Fix login", "preview": "fix it",
                                 "status": ["type": "active", "activeFlags": []]]
        daemon.send(["id": read1?["id"] ?? 0, "result": ["thread": t1]])
        let resume = expectCall("thread/resume", "an active thread is joined")
        suite.expect(json(params(resume)) == #"{"excludeTurns":true,"threadId":"t1"}"#,
                     "joining sends no overrides: \(json(params(resume)))")
        daemon.send(["id": read2?["id"] ?? 0, "result": ["thread": ["id": "t2", "cwd": "/Users/me/code/api",
                                                                     "status": ["type": "idle"]]]])
        daemon.send(["id": read3?["id"] ?? 0, "result": ["thread": ["id": "t3", "cwd": "/Users/me/code/api", "ephemeral": true,
                                                                     "status": ["type": "idle"]]]])
        daemon.send(["id": resume?["id"] ?? 0, "result": ["thread": t1]])
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.map(\.id) == ["t1", "t2"] },
                     "the island shows the person's threads, not Codex's helpers")
        suite.expect(client.owns("t3") && client.owns("t1") && !client.owns("nope"), "the client owns its threads")
        let shown = client.snapshot(now: Date(), previews: true).sessions
        suite.expect(shown.first?.phase == .generating && shown.first?.name == "Fix login" && shown.first?.title == "fix it"
                         && shown.first?.provider == .codex, "the joined thread works")
        suite.expect(shown.last?.phase == .idle && shown.last?.project == "api", "the idle one rests")
        suite.expect(changes.count > 0, "the island hears that sessions changed")

        // Preview changes travel over the real socket and populate the same
        // snapshot the island consumes, without reconnecting the session.
        let historyReads = (0..<3).compactMap { _ in expectCall("thread/read", "enabling previews reloads history") }
        suite.expect(
            Set(historyReads.compactMap { params($0)["threadId"] as? String }) == ["t1", "t2", "t3"]
                && historyReads.allSatisfy { params($0)["includeTurns"] as? Bool == true },
            "history is requested for every known thread only after opt-in")
        let history: [[String: Any]] = [
            [
                "startedAt": 100,
                "items": [
                    ["type": "userMessage", "id": "u1", "content": [["type": "text", "text": "Hello"]]],
                    ["type": "agentMessage", "id": "a1", "text": "Answer", "phase": "final_answer"],
                ],
            ]
        ]
        for read in historyReads {
            let id = params(read)["threadId"] as? String ?? ""
            if id == "t1" {
                var thread = t1
                thread["turns"] = history
                daemon.send(["id": read["id"] ?? 0, "result": ["thread": thread]])
            }
        }
        suite.expect(
            eventually {
                client.snapshot(now: Date(), previews: true).sessions.first?.chat.map(\.text) == ["Hello", "Answer"]
            },
            "daemon history reaches the island's conversation")
        daemon.send([
            "method": "item/completed",
            "params": [
                "threadId": "t1",
                "item": [
                    "type": "agentMessage", "id": "a2", "text": "New answer", "phase": "final_answer",
                ],
            ],
        ])
        suite.expect(
            eventually { client.snapshot(now: Date(), previews: true).sessions.first?.lastText == "New answer" },
            "a live Codex reply updates the compact preview")
        let hidden = client.snapshot(now: Date(), previews: false).sessions.first
        suite.expect(
            hidden?.chat.isEmpty == true && hidden?.lastText == nil, "disabling previews clears the conversation")
        // A response already on its way must not restore content after opt-out.
        if let read = historyReads.first(where: { params($0)["threadId"] as? String == "t2" }) {
            daemon.send([
                "id": read["id"] ?? 0,
                "result": [
                    "thread": [
                        "id": "t2", "turns": history,
                    ]
                ],
            ])
        }

        // An approval, answered from the island.
        daemon.send(["id": 7, "method": "item/commandExecution/requestApproval",
                     "params": ["threadId": "t1", "command": "/bin/zsh -lc 'rm -rf build'", "cwd": "/Users/me/code/app"]])
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.first?.permission?.requestID == "codex:7" },
                     "the approval reaches the island")
        let card = client.snapshot(now: Date(), previews: false).sessions.first
        suite.expect(card?.phase == .waitingPermission && card?.permission?.detail.hasPrefix("rm -rf build") == true,
                     "and reads as its command")
        suite.expect(!client.reply("t1", text: "hi"), "no reply while a request waits")
        suite.expect(!client.answer("t1", requestID: "codex:8", answer: .allow), "an unknown request is not answered")
        suite.expect(!client.answer("t2", requestID: "codex:7", answer: .allow), "nor one of another session")
        suite.expect(!client.answer("t1", requestID: "codex:7", answer: .questions(["q": ["a"]])),
                     "nor with an answer of the wrong kind")
        suite.expect(client.answer("t1", requestID: "codex:7", answer: .allow), "the approval is answered")
        let answer = daemon.message()
        suite.expect(json(answer) == #"{"id":7,"result":{"decision":"accept"}}"#, "the answer accepts by the same id: \(json(answer))")
        suite.expect(!client.answer("t1", requestID: "codex:7", answer: .deny), "the first answer wins")
        suite.expect(client.snapshot(now: Date(), previews: false).sessions.first?.permission == nil, "the card goes")

        // A request with a string id, answered back with it.
        daemon.send(["id": "r9", "method": "item/fileChange/requestApproval", "params": ["threadId": "t1"]])
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.first?.permission != nil },
                     "a string-id request arrives")
        suite.expect(client.answer("t1", requestID: #"codex:"r9""#, answer: .deny), "and is answered")
        suite.expect(json(daemon.message()) == #"{"id":"r9","result":{"decision":"decline"}}"#, "with its own id")

        // Unknown requests are never answered.
        daemon.send(["id": 30, "method": "item/tool/call", "params": ["threadId": "t1"]])

        // Ping, and a notification in fragments around another ping.
        daemon.raw(CodexWebSocket.encode(opcode: .ping, payload: Data("hb".utf8), mask: nil))
        let pong = daemon.frame()
        suite.expect(pong?.opcode == .pong && pong?.payload == Data("hb".utf8), "a ping is answered with its data")
        let status = Data(#"{"method":"thread/status/changed","params":{"threadId":"t2","status":{"type":"active"}}}"#.utf8)
        daemon.raw(CodexWebSocket.encode(opcode: .text, payload: status.prefix(20), fin: false, mask: nil))
        daemon.raw(CodexWebSocket.encode(opcode: .ping, payload: Data("mid".utf8), mask: nil))
        daemon.raw(CodexWebSocket.encode(opcode: .continuation, payload: status.dropFirst(20).prefix(30), fin: false, mask: nil))
        daemon.raw(CodexWebSocket.encode(opcode: .continuation, payload: status.dropFirst(50), mask: nil))
        let midPong = daemon.frame()
        suite.expect(midPong?.opcode == .pong && midPong?.payload == Data("mid".utf8), "a ping between fragments is answered")
        let resume2 = expectCall("thread/resume", "a thread that goes to work is joined")
        suite.expect(params(resume2)["threadId"] as? String == "t2", "the fragmented status was read whole")
        daemon.send(["id": resume2?["id"] ?? 0, "result": ["thread": ["id": "t2", "status": ["type": "active"]]]])
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.last?.phase == .generating },
                     "the second thread works")

        // Replies: queued while a turn runs, a new turn when idle.
        suite.expect(client.reply("t1", text: "and the tests"), "a working thread takes a reply")
        let queued = expectCall("thread/queue/add", "a reply to a working thread is queued")
        let queuedParams = params(queued)
        suite.expect(queuedParams["threadId"] as? String == "t1"
                         && json(queuedParams["input"]) == #"[{"text":"and the tests","text_elements":[],"type":"text"}]"#,
                     "the reply is the text: \(json(queuedParams["input"]))")
        suite.expect((queuedParams["clientUserMessageId"] as? String)?.count == 36, "a queued reply carries a UUID")
        daemon.send(["method": "thread/status/changed", "params": ["threadId": "t2", "status": ["type": "idle"]]])
        let leave = expectCall("thread/unsubscribe", "a thread at rest nobody looks at is left")
        suite.expect(json(params(leave)) == #"{"threadId":"t2"}"#, "the right thread is left")
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.last?.phase == .idle },
                     "the second thread rests")
        suite.expect(client.reply("t2", text: "go on"), "an idle thread takes a reply")
        let turn = expectCall("turn/start", "a reply to an idle thread starts a turn")
        suite.expect(params(turn)["threadId"] as? String == "t2" && params(turn)["clientUserMessageId"] == nil,
                     "the new turn is for that thread")
        suite.expect(!client.reply("nope", text: "x"), "an unknown thread takes no reply")

        // A page open keeps its thread joined.
        client.focus(["t2"])
        let focused = expectCall("thread/resume", "an open page joins its thread")
        suite.expect(params(focused)["threadId"] as? String == "t2", "the page's thread")
        client.focus(["t2"])
        client.focus([])
        let unfocused = expectCall("thread/unsubscribe", "closing the page leaves it")
        suite.expect(params(unfocused)["threadId"] as? String == "t2", "the page's thread is left")

        // The island acknowledges a finished turn.
        daemon.send(["method": "turn/completed", "params": ["threadId": "t1", "turn": ["status": "completed"]]])
        daemon.send(["method": "thread/status/changed", "params": ["threadId": "t1", "status": ["type": "idle"]]])
        _ = expectCall("thread/unsubscribe", "a finished thread is left")
        suite.expect(eventually { client.snapshot(now: Date(), previews: false).sessions.first?.phase == .finished },
                     "the finished turn shows")
        client.acknowledge("t1", now: Date().addingTimeInterval(1))
        suite.expect(client.snapshot(now: Date().addingTimeInterval(2), previews: false).sessions.first?.phase == .idle,
                     "looked at, it rests")

        // Garbage is ignored, not fatal.
        daemon.send(["no": "method"])
        daemon.raw(CodexWebSocket.encode(opcode: .text, payload: Data("not json".utf8), mask: nil))
        daemon.send(["id": 999, "result": [:]])
        usleep(100_000)
        suite.expect(client.connected, "messages it cannot read leave the connection up")
        suite.expect(daemon.allMasked, "every frame the island sent was masked")

        // The daemon goes away, and comes back.
        let before = changes.count
        daemon.hangUp()
        suite.expect(eventually { !client.connected }, "a closed socket disconnects the island")
        suite.expect(client.snapshot(now: Date(), previews: false).sessions.isEmpty && !client.owns("t1"),
                     "and forgets what the daemon said")
        suite.expect(changes.count > before, "the island hears it")
        suite.expect(!client.reply("t1", text: "x"), "no reply without the daemon")
        guard daemon.accept() else { return suite.expect(false, "the island reconnects") }
        _ = expectCall("initialize", "the island introduces itself again")
        suite.expect(eventually { client.connected }, "and is connected again")

        // A close frame from the daemon is echoed, then the island reconnects.
        daemon.raw(CodexWebSocket.encode(opcode: .close, payload: Data([0x03, 0xE8]), mask: nil))
        let close = daemon.frame()
        suite.expect(close?.opcode == .close && close?.payload == Data([0x03, 0xE8]), "a close is echoed with its code")
        suite.expect(eventually { !client.connected }, "and the island lets go")
        daemon.hangUp()
        guard daemon.accept() else { return suite.expect(false, "the island reconnects after a close") }
        _ = expectCall("initialize", "after a close it introduces itself again")

        // Stopping ends it for good.
        client.stop()
        suite.expect(!client.connected || eventually { !client.connected }, "a stopped client disconnects")
        daemon.hangUp()
        suite.expect(!daemon.accept(timeout: 0.5), "a stopped client never reconnects")
    }
}
