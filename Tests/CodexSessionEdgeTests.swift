// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Edge cases found in review. Each check states the behavior the island
/// should have; a failing check is the proof of an issue. Nothing here
/// connects to the person's own `~/.codex`.
enum CodexSessionEdgeTests {
    static func run(_ suite: TestSuite) {
        suite.run("codex edge state") { state(suite) }
        suite.run("codex edge client") { client(suite) }
        suite.run("codex edge terminals") { terminals(suite) }
    }

    static let t0 = Date(timeIntervalSince1970: 1_792_600_000)
    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    static func state(_ suite: TestSuite) {
        // E1: a thread read back as notLoaded (it unloaded between the list
        // and the read, or right after a notLoaded notification) is shown
        // as a session forever; notify() forgets such threads.
        let ghost = CodexSessionState()
        ghost.upsertThread(["id": "g1", "cwd": "/Users/me/code/app", "status": ["type": "notLoaded"]])
        let ghostIDs = ghost.snapshot(now: at(0), previews: false).sessions.map(\.id)
        suite.expect(!ghostIDs.contains("g1"), "E1 a thread read as notLoaded is not shown: \(ghostIDs)")

        // E2: an approval asked by a subagent's thread is kept on that
        // thread, which the snapshot hides, so no card shows anywhere.
        let parent = CodexSessionState()
        parent.upsertThread(["id": "p1", "cwd": "/Users/me/code/app", "status": ["type": "active", "activeFlags": []]])
        parent.upsertThread(["id": "c1", "cwd": "/Users/me/code/app", "parentThreadId": "p1",
                             "status": ["type": "active", "activeFlags": []]])
        let kept = parent.request(.number(4), method: "item/commandExecution/requestApproval",
                                  params: ["threadId": "c1", "command": "rm -rf build"], now: at(1))
        let shown = parent.snapshot(now: at(1), previews: false).sessions
        suite.expect(!kept || shown.contains { $0.permission != nil || $0.phase.isWaiting },
                     "E2 a subagent's approval shows on some row: kept=\(kept) phases=\(shown.map(\.phase))")

        // E3: an elicitation becomes a card with Allow/Deny buttons, but the
        // island cannot answer it, so the buttons do nothing.
        let elicit = CodexSessionState()
        elicit.upsertThread(["id": "t1", "cwd": "/Users/me/code/app", "status": ["type": "active"]])
        _ = elicit.request(.number(12), method: "mcpServer/elicitation/request",
                           params: ["threadId": "t1", "serverName": "figma", "message": "Sign in?"], now: at(1))
        let card = elicit.snapshot(now: at(1), previews: false).sessions.first?.permission
        let pending = elicit.threads["t1"]?.pending.first
        let answerable = pending.map { CodexSessionState.result($0, .allow) != nil || CodexSessionState.result($0, .deny) != nil }
        suite.expect(card == nil || answerable == true,
                     "E3 a card the island shows buttons for can be answered: card=\(card != nil) answerable=\(String(describing: answerable))")

        // E4: thread/name/updated is ignored, so a rename or a generated
        // title after the first read never reaches the island.
        let renamed = CodexSessionState()
        renamed.upsertThread(["id": "t1", "cwd": "/Users/me/code/app", "status": ["type": "idle"]])
        renamed.notify("thread/name/updated", params: ["threadId": "t1", "threadName": "Fix login"], now: at(1))
        let name = renamed.snapshot(now: at(1), previews: true).sessions.first?.name
        suite.expect(name == "Fix login", "E4 a renamed thread shows its new name: \(name ?? "nil")")

        // E5: with previews off, the thread's name is shown as the person's
        // own, but Codex generates thread titles from the conversation.
        let titled = CodexSessionState()
        titled.upsertThread(["id": "t1", "cwd": "/Users/me/code/app", "name": "Rotate the leaked AWS key",
                             "status": ["type": "idle"]])
        let hidden = titled.snapshot(now: at(1), previews: false).sessions.first
        suite.expect(hidden.map { NotchClaudeSessionsSupport.name($0, custom: nil) } != "Rotate the leaked AWS key",
                     "E5 with previews off a thread's (possibly generated) title is withheld: \(hidden?.name ?? "nil")")
    }

    static func client(_ suite: TestSuite) {
        typealias Daemon = CodexSessionTests.FakeDaemon
        guard let daemon = Daemon() else { return suite.expect(false, "temporary home made") }
        defer { daemon.finish() }
        guard daemon.listen() else { return suite.expect(false, "fake daemon listens") }
        let changes = CodexSessionTests.Counter()
        let client = CodexSessionClient(home: daemon.home, clientVersion: "t", retry: 0.1)
        client.start { changes.add() }
        defer { client.stop() }
        guard daemon.accept() else { return suite.expect(false, "the island connects") }

        func method(_ message: [String: Any]?) -> String? { message?["method"] as? String }
        func params(_ message: [String: Any]?) -> [String: Any] { message?["params"] as? [String: Any] ?? [:] }
        /// The next message with `name`, skipping others, within the timeout.
        func next(_ name: String, timeout: TimeInterval = 1) -> [String: Any]? {
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline {
                guard let message = daemon.message(timeout: deadline.timeIntervalSinceNow) else { return nil }
                if method(message) == name { return message }
            }
            return nil
        }

        guard let initialize = next("initialize", timeout: 3) else { return suite.expect(false, "initialize") }
        daemon.send(["id": initialize["id"] ?? 0, "result": ["userAgent": "codex/test"]])
        guard let list = next("thread/loaded/list", timeout: 3) else { return suite.expect(false, "loaded list") }
        daemon.send(["id": list["id"] ?? 0, "result": ["data": ["t1", "e1"]]])
        guard let read1 = next("thread/read"), let read2 = next("thread/read") else { return suite.expect(false, "reads") }
        daemon.send(["id": read1["id"] ?? 0, "result": ["thread": ["id": "t1", "cwd": "/Users/me/code/app",
                                                                   "status": ["type": "idle"]]]])

        // E6: an ephemeral helper (title generation) that works is joined
        // like a person's thread, though it is never shown.
        daemon.send(["id": read2["id"] ?? 0, "result": ["thread": ["id": "e1", "cwd": "/Users/me/code/app",
                                                                   "ephemeral": true,
                                                                   "status": ["type": "active", "activeFlags": []]]]])
        let joinedHelper = next("thread/resume", timeout: 0.6)
        suite.expect(joinedHelper == nil, "E6 an ephemeral helper thread is not joined: \(params(joinedHelper))")
        guard CodexSessionTests.eventually(3, { client.owns("t1") }) else { return suite.expect(false, "t1 known") }

        // E7: every notification, even a token delta for a thread nobody
        // shows, wakes the service to publish.
        usleep(200_000)
        let before = changes.count
        for index in 0..<50 {
            daemon.send(["method": "item/agentMessage/delta",
                         "params": ["threadId": "t1", "turnId": "u", "itemId": "m", "delta": "tok\(index)"]])
        }
        usleep(400_000)
        let woken = changes.count - before
        suite.expect(woken < 5, "E7 token deltas that change nothing do not wake the service: \(woken) wakes for 50 deltas")

        // E8: a page's focus outlives its thread: once the thread closes and
        // comes back, the island joins it with no page open, and keeps it
        // joined (so it can no longer unload).
        client.focus(["t1"])
        guard let focused = next("thread/resume") else { return suite.expect(false, "focus joins t1") }
        daemon.send(["id": focused["id"] ?? 0, "result": ["thread": ["id": "t1", "cwd": "/Users/me/code/app",
                                                                     "status": ["type": "idle"]]]])
        daemon.send(["method": "thread/closed", "params": ["threadId": "t1"]])
        guard CodexSessionTests.eventually(3, { !client.owns("t1") }) else { return suite.expect(false, "t1 closed") }
        // The service clears focusedID when the session goes, but never
        // tells the client (ClaudeSessionService.publish).
        daemon.send(["method": "thread/status/changed", "params": ["threadId": "t1", "status": ["type": "idle"]]])
        var seen: [String] = []
        let deadline = Date().addingTimeInterval(1)
        while Date() < deadline, let message = daemon.message(timeout: deadline.timeIntervalSinceNow) {
            seen.append(method(message) ?? "?")
            if method(message) == "thread/read" {
                daemon.send(["id": message["id"] ?? 0, "result": ["thread": ["id": "t1", "cwd": "/Users/me/code/app",
                                                                             "status": ["type": "idle"]]]])
            }
        }
        suite.expect(!seen.contains("thread/resume"),
                     "E8 an idle thread back after closing is not joined for a page long gone: \(seen)")
    }

    static func terminals(_ suite: TestSuite) {
        let a: [(pid: Int32, cwd: String)] = [(pid: 10, cwd: "/tmp/vs-a")]
        suite.expect(ClaudeSessionHosts.codexTerminal(cwd: "/tmp/vs-a", among: a) == 10, "the terminal in the thread's folder is its own")
        suite.expect(ClaudeSessionHosts.codexTerminal(cwd: "/tmp/vs-b", among: a) == 10,
                     "the only terminal stands in when none runs in the folder")
        suite.expect(ClaudeSessionHosts.codexTerminal(cwd: "/tmp/vs-b", among: a, others: ["/tmp/vs-a"]) == nil,
                     "but never when another listed session runs in its folder")
        let two: [(pid: Int32, cwd: String)] = [(pid: 10, cwd: "/tmp/vs-a"), (pid: 11, cwd: "/tmp/vs-a")]
        suite.expect(ClaudeSessionHosts.codexTerminal(cwd: "/tmp/vs-a", among: two) == nil, "two in one folder can't be told apart")
    }
}
