// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The Claude desktop app in Sessions (#246): its Code tab's sessions told
/// apart by the registry's `entrypoint`, and Cowork's read from the files the
/// app keeps, `local_<id>.json` and `audit.jsonl`, never written.
enum ClaudeDesktopTests {
    static func run(_ suite: TestSuite) {
        suite.run("claude desktop: code tab") { codeTab(suite) }
        suite.run("claude desktop: cowork log") { log(suite) }
        suite.run("claude desktop: cowork files") { files(suite) }
        suite.run("claude desktop: strings") { strings(suite) }
    }

    static let now = Date(timeIntervalSince1970: 1_795_090_000)

    // MARK: Code tab

    static func codeTab(_ suite: TestSuite) {
        suite.expect(ClaudeDesktop.isCodeTab(entrypoint: "claude-desktop")
                     && ClaudeDesktop.isCodeTab(entrypoint: "claude-desktop-3p"),
                     "the Code tab's entrypoints, signed in and third-party, are the app's")
        suite.expect(!ClaudeDesktop.isCodeTab(entrypoint: "cli") && !ClaudeDesktop.isCodeTab(entrypoint: "local-agent")
                     && !ClaudeDesktop.isCodeTab(entrypoint: "sdk-ts") && !ClaudeDesktop.isCodeTab(entrypoint: nil)
                     && !ClaudeDesktop.isCodeTab(entrypoint: "claude-desktopx"),
                     "a terminal's, Cowork's, another SDK app's and none are not")
        // The record Claude Code 2.1.318 wrote for a session run the way the Code tab runs it (2026-11-19).
        let json = #"{"pid":87326,"sessionId":"af54c630-f416-4921-8550-0efb82f376da","cwd":"/private/tmp/ob246/sdk/proj","startedAt":1795088704749,"procStart":"Thu Nov 19 11:45:04 2026","version":"2.1.318","kind":"interactive","entrypoint":"claude-desktop-3p","pidDomain":"darwin","name":"proj-68","nameSource":"derived","status":"waiting","waitingFor":"permission prompt","updatedAt":1795088708901,"statusUpdatedAt":1795088708901}"#
        let record = ClaudeSessionRegistryReader.record(Data(json.utf8))
        suite.expect(record?.entrypoint == "claude-desktop-3p" && record?.status == .waiting,
                     "the registry record's entrypoint is read: \(String(describing: record))")
        let terminal = ClaudeSessionRegistryReader.record(Data(json.replacingOccurrences(of: "claude-desktop-3p", with: "cli").utf8))
        suite.expect(terminal?.entrypoint == "cli", "a terminal's too")

        let engine = ClaudeSessionEngine()
        var desktop = record!
        desktop.host = .app(ClaudeDesktop.bundle)
        var cli = terminal!
        cli = ClaudeSessionRecord(pid: 87327, sessionID: "b2", cwd: cli.cwd, name: cli.name, status: .idle, waitingFor: nil,
                                  statusUpdated: cli.statusUpdated, started: cli.started, version: cli.version)
        engine.ingest(records: [desktop, cli], now: now)
        let sessions = engine.snapshot(now: now).sessions
        let fromApp = sessions.first { $0.id == desktop.sessionID }
        suite.expect(fromApp?.origin == .desktop && fromApp?.provider == .claude && fromApp?.phase == .waitingPermission,
                     "a Code tab session is Claude's, from the app, waiting as its record says: \(String(describing: fromApp))")
        suite.expect(sessions.first { $0.id == "b2" }?.origin == nil, "a terminal's has no origin")

        var session = fromApp!
        suite.expect(ClaudeDesktop.link(for: session)?.absoluteString == "claude://resume?session=af54c630-f416-4921-8550-0efb82f376da",
                     "Open asks the app for the session by its id")
        session.origin = .cowork
        suite.expect(ClaudeDesktop.link(for: session) == nil, "Cowork has no link to one session")
        var odd = fromApp!
        odd = ClaudeSession(id: "x&y=1", pid: 1, cwd: "/", project: "p", name: "n", host: .unknown, phase: .idle,
                            phaseSince: now, timeline: [], subagents: [], tokens: AgentTokens(), cost: 0, hooked: true)
        odd.origin = .desktop
        suite.expect(ClaudeDesktop.link(for: odd) == nil, "an id that isn't one is never put in a link")
    }

    // MARK: Cowork log

    static func line(_ json: [String: Any], at seconds: TimeInterval) -> Data {
        var json = json
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        json["_audit_timestamp"] = formatter.string(from: now.addingTimeInterval(seconds))
        return try! JSONSerialization.data(withJSONObject: json)
    }

    static let prompt: [String: Any] = ["type": "user", "message": ["role": "user", "content": "tidy the downloads folder"],
                                        "parent_tool_use_id": NSNull(), "session_id": "c1"]
    static let toolCall: [String: Any] = ["type": "assistant", "parent_tool_use_id": NSNull(), "session_id": "c1",
                                          "message": ["role": "assistant", "model": "claude-opus-5-5", "content": [
                                              ["type": "text", "text": "Moving the files."],
                                              ["type": "tool_use", "id": "toolu_1", "name": "Bash",
                                               "input": ["command": "mv *.pdf Documents/", "description": "Move PDFs"]]]]]
    static let ask: [String: Any] = ["type": "system", "subtype": "permission_request", "uuid": "req-1", "session_id": "c1",
                                     "tool_name": "Bash", "tool_input": ["command": "mv *.pdf Documents/"]]
    static let granted: [String: Any] = ["type": "system", "subtype": "permission_response", "uuid": "req-1", "session_id": "c1",
                                         "tool_name": "Bash", "decision": "allow", "granted": true]
    static let toolResult: [String: Any] = ["type": "user", "parent_tool_use_id": NSNull(), "session_id": "c1",
                                            "message": ["role": "user", "content": [
                                                ["type": "tool_result", "tool_use_id": "toolu_1", "content": "", "is_error": false]]]]
    static let reply: [String: Any] = ["type": "assistant", "parent_tool_use_id": NSNull(), "session_id": "c1",
                                       "message": ["role": "assistant", "model": "claude-opus-5-5", "content": [
                                           ["type": "text", "text": "Done: 12 PDFs moved."]]]]
    static let result: [String: Any] = ["type": "result", "subtype": "success", "is_error": false, "session_id": "c1"]

    static func log(_ suite: TestSuite) {
        var state = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(["type": "system", "subtype": "init", "model": "claude-opus-5-5"], at: 0), to: &state, previews: true)
        ClaudeCoworkLog.apply(line(prompt, at: 1), to: &state, previews: true)
        suite.expect(state.turnOpen && state.model == "claude-opus-5-5" && state.chat.map(\.text) == ["tidy the downloads folder"],
                     "a prompt opens a turn; init names the model")
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: now.addingTimeInterval(2)) == .generating,
                     "the turn works")
        ClaudeCoworkLog.apply(line(toolCall, at: 2), to: &state, previews: true)
        suite.expect(state.openTool?.name == "Bash" && state.openTool?.summary == "mv *.pdf Documents/"
                     && ClaudeCoworkSessions.phase(state, acknowledged: nil, now: now.addingTimeInterval(3)) == .runningTool,
                     "a tool call runs: \(String(describing: state.openTool))")
        ClaudeCoworkLog.apply(line(ask, at: 3), to: &state, previews: true)
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: now.addingTimeInterval(4)) == .waitingPermission,
                     "a permission the app asks for waits on the person")
        ClaudeCoworkLog.apply(line(ask, at: 3), to: &state, previews: true)
        suite.expect(state.pending.count == 1, "the same request read twice waits once")
        ClaudeCoworkLog.apply(line(granted, at: 5), to: &state, previews: true)
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: now.addingTimeInterval(6)) == .runningTool,
                     "its answer lets the tool run")
        ClaudeCoworkLog.apply(line(toolResult, at: 7), to: &state, previews: true)
        ClaudeCoworkLog.apply(line(reply, at: 8), to: &state, previews: true)
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: now.addingTimeInterval(8)) == .generating
                     && state.tools.first?.finished != nil, "the tool's result ends it")
        ClaudeCoworkLog.apply(line(result, at: 9), to: &state, previews: true)
        let ended = now.addingTimeInterval(9)
        suite.expect(!state.turnOpen && state.completed && state.lastText == "Done: 12 PDFs moved."
                     && state.chat.last?.endsTurn == true,
                     "the result ends the turn on its last reply")
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: ended.addingTimeInterval(60)) == .finished,
                     "an ended turn is finished until looked at")
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: ended.addingTimeInterval(30), now: ended.addingTimeInterval(60)) == .idle,
                     "and idle once looked at")
        suite.expect(ClaudeCoworkSessions.phase(state, acknowledged: nil, now: ended.addingTimeInterval(ClaudeSessionEngine.finishedFor + 1)) == .idle,
                     "or once it's old")

        // A question asks through the same request.
        var question = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(prompt, at: 1), to: &question, previews: true)
        ClaudeCoworkLog.apply(line(["type": "system", "subtype": "permission_request", "uuid": "q", "tool_name": "AskUserQuestion"], at: 2),
                              to: &question, previews: true)
        suite.expect(ClaudeCoworkSessions.phase(question, acknowledged: nil, now: now.addingTimeInterval(3)) == .waitingInput,
                     "AskUserQuestion waits for an answer")

        // Previews off: nothing said is kept.
        var hidden = ClaudeCoworkLog.State()
        for (json, at) in [(prompt, 1.0), (toolCall, 2.0), (toolResult, 3.0), (reply, 4.0), (result, 5.0)] {
            ClaudeCoworkLog.apply(line(json, at: at), to: &hidden, previews: false)
        }
        suite.expect(hidden.chat.isEmpty && hidden.lastText == nil && hidden.tools.allSatisfy { $0.summary == nil }
                     && hidden.completed && hidden.tools.count == 1,
                     "with previews off the turn is followed but nothing said is kept")

        // A subagent's messages don't move the session's own turn.
        var sub = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(prompt, at: 1), to: &sub, previews: true)
        ClaudeCoworkLog.apply(line(result, at: 2), to: &sub, previews: true)
        var subPrompt = prompt
        subPrompt["parent_tool_use_id"] = "toolu_agent"
        var subResult = result
        subResult["parent_tool_use_id"] = "toolu_agent"
        ClaudeCoworkLog.apply(line(subPrompt, at: 3), to: &sub, previews: true)
        ClaudeCoworkLog.apply(line(subResult, at: 4), to: &sub, previews: true)
        suite.expect(!sub.turnOpen && sub.chat.count == 1, "a subagent's prompt opens no turn")

        // Tags the app adds are not prompts; a broken line is skipped.
        var tags = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(["type": "user", "message": ["role": "user", "content": "<system-reminder>x</system-reminder>"]], at: 1),
                              to: &tags, previews: true)
        ClaudeCoworkLog.apply(Data("{not json".utf8), to: &tags, previews: true)
        suite.expect(!tags.turnOpen && tags.chat.isEmpty, "a tagged reminder or a broken line opens nothing")

        // A turn the app stopped mid-way, writing nothing, is over once quiet.
        var stopped = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(prompt, at: 1), to: &stopped, previews: true)
        suite.expect(ClaudeCoworkSessions.phase(stopped, acknowledged: nil, now: now.addingTimeInterval(ClaudeCoworkSessions.quietFor + 2)) == .idle,
                     "a turn quiet for half an hour is not working")
        // An error ends the turn without finishing it.
        var failed = ClaudeCoworkLog.State()
        ClaudeCoworkLog.apply(line(prompt, at: 1), to: &failed, previews: true)
        ClaudeCoworkLog.apply(line(["type": "result", "subtype": "error_during_execution", "is_error": true], at: 2), to: &failed, previews: true)
        suite.expect(!failed.turnOpen && !failed.completed
                     && ClaudeCoworkSessions.phase(failed, acknowledged: nil, now: now.addingTimeInterval(3)) == .idle,
                     "an error ends the turn, unfinished")
    }

    // MARK: Cowork files

    static func files(_ suite: TestSuite) {
        let root = FileManager.default.temporaryDirectory.appending(path: "ob-cowork-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let org = root.appending(path: "local-agent-mode-sessions/acct-1/org-1")
        let sessionDir = org.appending(path: "local_aaa")
        try? FileManager.default.createDirectory(at: sessionDir.appending(path: ".claude"), withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: root.appending(path: "local-agent-mode-sessions/skills-plugin/x"),
                                                 withIntermediateDirectories: true)
        let live = Date()
        func writeMeta(_ id: String, title: String, archived: Bool = false, activity: Date = live) {
            let json: [String: Any] = ["sessionId": id, "cliSessionId": "cli-\(id)", "cwd": "/sessions/brave-owl",
                                       "userSelectedFolders": ["/Users/me/Downloads"], "createdAt": activity.timeIntervalSince1970 * 1000 - 60_000,
                                       "lastActivityAt": activity.timeIntervalSince1970 * 1000, "model": "claude-opus-5-5",
                                       "isArchived": archived, "title": title, "initialMessage": "secret prompt"]
            try? JSONSerialization.data(withJSONObject: json).write(to: org.appending(path: "\(id).json"))
        }
        writeMeta("local_aaa", title: "Tidy downloads")
        writeMeta("local_old", title: "Old one", activity: live.addingTimeInterval(-3 * 3600))
        writeMeta("local_arch", title: "Archived", archived: true)
        let audit = sessionDir.appending(path: "audit.jsonl")
        let body = [prompt, toolCall, ask].enumerated().map { index, json -> Data in
            var data = line(json, at: 0)
            // Stamped now, so the session counts as active.
            var object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            object["_audit_timestamp"] = formatter.string(from: live.addingTimeInterval(Double(index)))
            data = try! JSONSerialization.data(withJSONObject: object)
            return data
        }
        var text = Data()
        body.forEach { text.append($0); text.append(0x0A) }
        try? text.write(to: audit)

        let found = ClaudeCoworkSessions.sessionFiles(in: [root.appending(path: "local-agent-mode-sessions")]).map(\.lastPathComponent)
        suite.expect(Set(found) == ["local_aaa.json", "local_old.json", "local_arch.json"],
                     "every session file is found, never inside a session's folder: \(found)")

        let cowork = ClaudeCoworkSessions()
        let folders = [root.appending(path: "local-agent-mode-sessions")]
        var scan = cowork.scan(folders, now: live.addingTimeInterval(3), previews: false, running: true)
        suite.expect(scan.sessions.map(\.id) == ["cowork:local_aaa"],
                     "only the active, unarchived session is listed: \(scan.sessions.map(\.id))")
        let row = scan.sessions.first
        suite.expect(row?.origin == .cowork && row?.provider == .claude && row?.phase == .waitingPermission
                     && row?.project == "Downloads" && row?.name == "Downloads" && row?.title == nil
                     && row?.waitingFor == CodexSessionState.answerInTerminal && row?.permission == nil
                     && row?.host == .app(ClaudeDesktop.bundle) && row?.model == "claude-opus-5-5",
                     "previews off: Claude's row in its folder, waiting in the app, no title: \(String(describing: row))")
        scan = cowork.scan(folders, now: live.addingTimeInterval(3), previews: true, running: true)
        suite.expect(scan.sessions.first?.name == "Tidy downloads" && scan.sessions.first?.chat.isEmpty == false,
                     "previews on: its title and what was said")

        // More is written: the rest of the turn.
        let rest = [granted, toolResult, reply, result].enumerated().map { index, json -> Data in
            var object = json
            let formatter = ISO8601DateFormatter()
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            object["_audit_timestamp"] = formatter.string(from: live.addingTimeInterval(Double(4 + index)))
            return try! JSONSerialization.data(withJSONObject: object)
        }
        if let handle = try? FileHandle(forWritingTo: audit) {
            handle.seekToEndOfFile()
            rest.forEach { handle.write($0); handle.write(Data([0x0A])) }
            handle.write(Data(#"{"type":"assistant","partial"#.utf8))
            try? handle.close()
        }
        scan = cowork.scan(folders, now: live.addingTimeInterval(9), previews: true, running: true)
        suite.expect(scan.sessions.first?.phase == .finished && scan.sessions.first?.lastText == "Done: 12 PDFs moved.",
                     "what's appended is read, a line still being written is not: \(String(describing: scan.sessions.first?.phase))")
        suite.expect(scan.events.contains(.finished(id: "cowork:local_aaa", duration: 7)),
                     "its end is told, for the notice: \(scan.events)")
        cowork.acknowledge("cowork:local_aaa", now: live.addingTimeInterval(10))
        scan = cowork.scan(folders, now: live.addingTimeInterval(11), previews: true, running: true)
        suite.expect(scan.sessions.first?.phase == .idle, "looked at, it's idle")
        scan = cowork.scan(folders, now: live.addingTimeInterval(11), previews: true, running: false)
        suite.expect(scan.sessions.isEmpty, "with the app gone nothing is listed")
        scan = cowork.scan(folders, now: live.addingTimeInterval(ClaudeCoworkSessions.listedFor + 60), previews: true, running: true)
        suite.expect(scan.sessions.isEmpty, "quiet for a while, it leaves the list")
        suite.expect(cowork.scan([root.appending(path: "missing")], now: live, previews: true, running: true).sessions.isEmpty,
                     "no folder, no sessions and no error")
        let meta = ClaudeCoworkSessions.meta(Data("[]".utf8), id: "x")
        suite.expect(meta == nil, "a file that isn't a session is skipped")
    }

    // MARK: Strings

    static func strings(_ suite: TestSuite) {
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        var session = ClaudeSession(id: "s", pid: 1, cwd: "/w", project: "w", name: "w", host: .app(ClaudeDesktop.bundle),
                                    phase: .waitingPermission, phaseSince: now, timeline: [], subagents: [],
                                    tokens: AgentTokens(), cost: 0, hooked: false)
        session.origin = .cowork
        suite.expect(text.answerIn(session) == "Answer this one in Claude Desktop."
                     && text.replyUnavailable(for: session) == "Reply in Claude Desktop",
                     "a Cowork session is answered and replied to in the app")
        suite.expect(text.originBadge(.cowork) == "Cowork" && text.originBadge(.desktop) == "Desktop", "its label names the kind")
        session.origin = nil
        suite.expect(text.answerIn(session) == text.answerInTerminal, "a terminal's still says the terminal")
    }
}
