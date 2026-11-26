// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Kiro CLI in the Sessions list (#238): what its hooks send (payloads
/// copied from Kiro CLI 3.6.2 runs), the sessions kept from them, the tool
/// request its hook holds for the island, the keys its own dialog takes,
/// what its session log tells, the island's hooks file, and how it is found
/// and named.
enum KiroAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("kiro: hooks") { hooks(suite) }
        suite.run("kiro: permissions") { permissions(suite) }
        suite.run("kiro: keys") { keys(suite) }
        suite.run("kiro: log") { log(suite) }
        suite.run("kiro: installer") { installer(suite) }
        suite.run("kiro: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    static let session = "sess_d01ac6eb-aece-4bcf-84cb-51b9c20d3dfe"

    static func envelope(_ event: String, _ extra: [String: Any] = [:]) -> Data {
        var json: [String: Any] = ["session_id": session, "hook_event_name": event, "cwd": "/private/tmp/ob238/rocket"]
        for (key, value) in extra { json[key] = value }
        return T.envelope("kiro", hook: event, pid: 35932, json)
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, _ extra: [String: Any] = [:],
                     policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, envelope(event, extra), policy: policy, at: date)
    }

    static let shell: [String: Any] = ["tool_name": "execute_bash", "tool_input": [
        "command": "touch hello.txt", "description": "Create hello.txt", "cwd": "/private/tmp/ob238/rocket",
        "run_in_background": false, "timeout": NSNull(),
    ]]

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "SessionStart")
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "kiro:\(session)" && listed[0].provider == .kiro
                     && listed[0].project == "rocket" && listed[0].phase == .idle,
                     "a session start lists it idle in its folder: \(listed)")
        send(tracker, "UserPromptSubmit", ["prompt": "Run the shell command `touch hello.txt` and then say done."])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["Run the shell command `touch hello.txt` and then say done."],
                     "a prompt opens a turn, its text with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", ["prompt": "secret plan"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first?.chat.isEmpty == true, "without previews the prompt isn't kept")
        send(tracker, "PreToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "touch hello.txt",
                     "its shell runs as Bash, summed up by the command: \(listed[0].currentTool as Any)")
        send(tracker, "PostToolUse", shell.merging(["tool_response": "Output:\n\n\nExit Code: 0"]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed, "the tool ends with no call id, by its kind: \(listed[0].timeline)")
        let write: [String: Any] = ["tool_name": "fs_write", "tool_input": ["path": "/private/tmp/ob238/rocket/notes.txt",
                                                                             "text": "alpha\nbeta\n"]]
        send(tracker, "PreToolUse", write)
        send(tracker, "PostToolUse", write.merging(["tool_response": "ok"]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 2 && listed[0].timeline[1].name == "Write"
                     && listed[0].timeline[1].summary == "notes.txt",
                     "its write is a Write of the file: \(listed[0].timeline)")
        send(tracker, "Stop", at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished, "Stop ends the turn: \(listed[0].phase)")
        send(tracker, "SessionEnd", ["reason": "shutdown"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "/quit takes the row away")
        let gone = AgentHookSessions()
        send(gone, "SessionStart")
        gone.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(gone).sessions.isEmpty, "no ghost row once Kiro's process is gone")
        // Escape at Kiro's dialog cancels the turn: no Stop, only the tool's result says so.
        let cancelled = AgentHookSessions()
        send(cancelled, "UserPromptSubmit", ["prompt": "go"])
        send(cancelled, "PreToolUse", shell)
        send(cancelled, "PostToolUse", shell.merging([
            "tool_response": "Output:\nThe tool call was cancelled before it was approved.\n\nExit Code: -1",
        ]) { $1 })
        listed = T.snapshot(cancelled).sessions
        suite.expect(listed[0].phase == .finished && listed[0].timeline.first?.failed == true,
                     "a dialog cancelled with Escape ends the turn: \(listed[0].phase)")
        let input = AgentKiro.claudeInput("str_replace", ["path": "/a/b.txt", "oldStr": "beta", "newStr": "gamma", "replace_all": false])
        suite.expect(input["file_path"] as? String == "/a/b.txt" && input["old_string"] as? String == "beta"
                     && input["new_string"] as? String == "gamma"
                     && AgentHookTools.name(.kiro, "str_replace") == "Edit" && AgentHookTools.name(.kiro, "execute_pwsh") == "Bash"
                     && AgentHookTools.name(.kiro, "read_file") == "Read" && AgentHookTools.name(.kiro, "fs_write") == "Write",
                     "Kiro's tools and arguments read as Claude Code's")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let request = envelope("PermissionRequest", shell.merging(["permission": [
            "capability": "shell", "resource": "touch hello.txt", "command": "touch hello.txt",
        ]]) { $1 })
        suite.expect(AgentHookMessage.waits(request), "its permission hook waits for the island's answer")
        suite.expect(!AgentHookMessage.waits(envelope("PreToolUse", shell)) && !AgentHookMessage.waits(envelope("Stop")),
                     "every other hook is let go at once")
        let decoded = AgentHookMessage.decode(request, now: now, policy: T.off)
        guard let decoded, case .heldPermission(let asked)? = decoded.events.first?.kind else {
            return suite.expect(false, "a tool's request is held: \(decoded?.events as Any)")
        }
        suite.expect(decoded.permission?.session == session && decoded.permission?.request == asked.requestID
                     && asked.tool == "Bash" && asked.detail == "touch hello.txt" && asked.content?.complete == true
                     && !asked.offersAlways && !asked.inAgentDialog,
                     "the request shows its command whatever the previews: \(asked)")
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "go"])
        send(tracker, "PreToolUse", shell)
        T.feed(tracker, request)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.tool == "Bash" && listed[0].waitingFor == nil,
                     "the session waits on the island's card: \(listed[0].phase)")
        // Let go for Kiro's own dialog: answered by keys where the terminal takes them.
        if let id = listed.first?.id, let held = listed.first?.permission?.requestID { tracker.moveToDialog(id, request: held) }
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission?.inAgentDialog == true, "once let go the card stays for Kiro's dialog")
        send(tracker, "PostToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "a tool that ran was answered")
        // A write's and an edit's requests carry their diff, as Claude Code's do.
        let edit = AgentHookMessage.decode(envelope("PermissionRequest", [
            "tool_name": "str_replace",
            "tool_input": ["path": "/private/tmp/ob238/rocket/notes.txt", "oldStr": "beta", "newStr": "gamma", "replace_all": false],
            "permission": ["capability": "fs_write", "resource": "notes.txt"],
        ]), now: now, policy: T.on)
        if case .heldPermission(let change)? = edit?.events.first?.kind {
            suite.expect(change.tool == "Edit" && !change.diffs.isEmpty, "an edit shows what it changes: \(change.diffs)")
        } else {
            suite.expect(false, "an edit's request is held")
        }
        func decision(_ data: Data?) -> [String: Any] {
            let json = data.flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
            return (json["hookSpecificOutput"] as? [String: Any])?["decision"] as? [String: Any] ?? [:]
        }
        suite.expect(decision(AgentKiro.reply(.allow))["behavior"] as? String == "allow"
                     && decision(AgentKiro.reply(.allowAlways))["behavior"] as? String == "allow"
                     && decision(AgentKiro.reply(.deny))["behavior"] as? String == "deny"
                     && decision(AgentKiro.reply(.deny))["message"] as? String == ClaudeHookMessage.denyMessage
                     && AgentKiro.reply(.answer(["q": ["a"]])) == nil && AgentKiro.reply(.approvePlan(.manual)) == nil,
                     "Allow and Deny print the decision Kiro takes")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        // node (no terminal) under bun (the terminal UI, on ttys199) under kiro-cli.
        let ttys: [Int32: String] = [48958: "/dev/ttys199", 48917: "/dev/ttys199"]
        let parents: [Int32: Int32] = [48969: 48958, 48958: 48917, 48917: 48915]
        suite.expect(AgentKiro.terminalProcess(of: 48969, tty: { ttys[$0] }, parent: { parents[$0] }) == 48958
                     && AgentKiro.terminalProcess(of: 48958, tty: { ttys[$0] }, parent: { parents[$0] }) == 48958
                     && AgentKiro.terminalProcess(of: 7, tty: { _ in nil }, parent: { _ in nil }) == 7,
                     "the session is the terminal UI above Kiro's agent server, or the server when none has a terminal")
        suite.expect(AgentKiro.keys(.allow) == [.enter] && AgentKiro.keys(.deny) == [.keys([.down]), .enter]
                     && AgentKiro.keys(.allowAlways) == nil && AgentKiro.keys(.answer([:])) == nil,
                     "its dialog takes Return for Allow once and Down, Return for Deny once")
    }

    // MARK: Log

    static func log(_ suite: TestSuite) {
        let lines = [
            #"{"id":"d97b","timestamp":"2026-11-26T08:22:48.828Z","payload":{"type":"user","content":"Run it"}}"#,
            #"{"id":"988f-turn-start","timestamp":"2026-11-26T08:22:48.849Z","payload":{"type":"turn_start","turnId":"988f"}}"#,
            #"{"id":"31fa-say","timestamp":"2026-11-26T08:22:51.334Z","payload":{"type":"assistant","content":"...","operationType":"Reasoning"}}"#,
            #"{"id":"3185-say","timestamp":"2026-11-26T08:23:24.750Z","payload":{"type":"assistant","content":"Done.","operationType":"Say"}}"#,
            #"{"id":"36fe-say","timestamp":"2026-11-26T08:23:24.750Z","payload":{"type":"assistant","content":"...","operationType":"Reasoning"}}"#,
            #"{"id":"988f-usage","timestamp":"2026-11-26T08:23:24.812Z","payload":{"type":"usage_summary","status":"success"}}"#,
            #"{"id":"988f-turn-end","timestamp":"2026-11-26T08:23:24.810Z","payload":{"type":"turn_end","stopReason":"end_turn"}}"#,
            ##"{"id":"session_start","timestamp":"2026-11-26T08:23:24.820Z","payload":{"type":"session_start","content":"# Kiro"}}"##,
        ]
        var turn = AgentKiro.lastTurn(lines.joined(separator: "\n"), text: true)
        suite.expect(turn.ended && !turn.cancelled && turn.reply == "Done." && turn.at != nil,
                     "the last turn's end and what it said: \(turn)")
        turn = AgentKiro.lastTurn(lines.joined(separator: "\n"), text: false)
        suite.expect(turn.ended && turn.reply == nil, "without previews no text is kept")
        let running = AgentKiro.lastTurn(lines.prefix(4).joined(separator: "\n"), text: true)
        suite.expect(!running.ended, "a turn still going hasn't ended")
        let cancelled = AgentKiro.lastTurn((lines.prefix(4) + [
            #"{"id":"t","timestamp":"2026-11-26T08:29:48.016Z","payload":{"type":"turn_end","stopReason":"cancelled"}}"#,
        ]).joined(separator: "\n"), text: true)
        suite.expect(cancelled.ended && cancelled.cancelled, "a turn cancelled with Escape ends in the log")

        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-kiro-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let sessionFolder = folder.appending(path: "sessions/1881f463eb2dbfa5/\(session)")
        try? FileManager.default.createDirectory(at: sessionFolder, withIntermediateDirectories: true)
        try? Data(lines.joined(separator: "\n").utf8).write(to: sessionFolder.appending(path: "messages.jsonl"))
        try? Data(#"{"id":"\#(session)","title":"Create File With Touch Command","modelId":"auto","status":"idle"}"#.utf8)
            .write(to: sessionFolder.appending(path: "session.json"))
        let found = AgentKiro.sessionFolder(session, folder: folder)
        let details = found.map { AgentKiro.details(sessionFolder: $0) }
        suite.expect(found?.lastPathComponent == session && details?.title == "Create File With Touch Command"
                     && details?.model == "auto" && AgentKiro.sessionFolder("../x", folder: folder) == nil
                     && AgentKiro.sessionFolder("sess_missing", folder: folder) == nil,
                     "its folder found by id, its title and model read: \(details as Any)")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-kiro-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentKiro.hooks(home: home, bundle: bundle, environment: [:])
        suite.expect(url.path.hasSuffix("/.kiro/hooks/openbar-com.openbar.app.dev.json")
                     && AgentKiro.hooks(home: home, bundle: bundle, environment: ["KIRO_HOME": "/k"]).path
                        == "/k/hooks/openbar-com.openbar.app.dev.json"
                     && AgentKiro.folder(home: home, environment: ["KIRO_HOME": ""]) == home.appending(path: ".kiro")
                     && AgentHookInstaller.file(.kiro, folder: URL(fileURLWithPath: "/k"), bundle: bundle)?.path
                        == "/k/hooks/openbar-com.openbar.app.dev.json",
                     "a file of its own in Kiro's hooks folder, where KIRO_HOME moves it")
        let folder = home.appending(path: ".kiro")
        let file = AgentHookInstaller.file(.kiro, folder: folder, bundle: bundle)!
        suite.expect(AgentHookInstaller.state(.kiro, home: home, socket: socket, bundle: bundle, folder: folder) == .missing,
                     "no file, no hooks")
        try? AgentHookInstaller.install(.kiro, home: home, socket: socket, bundle: bundle, folder: folder)
        let json = (try? Data(contentsOf: file)).flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
        let hooks = json["hooks"] as? [[String: Any]] ?? []
        let pre = hooks.first { $0["trigger"] as? String == "PreToolUse" }
        let ask = hooks.first { $0["trigger"] as? String == "PermissionRequest" }
        let stop = hooks.first { $0["trigger"] as? String == "Stop" }
        let command = (pre?["action"] as? [String: Any])?["command"] as? String ?? ""
        let asking = (ask?["action"] as? [String: Any])?["command"] as? String ?? ""
        suite.expect(json["version"] as? String == "v1" && hooks.map { $0["trigger"] as? String ?? "" } == AgentKiro.events
                     && pre?["matcher"] as? String == "*" && stop?["matcher"] == nil && pre?["enabled"] as? Bool == true
                     && pre?["name"] as? String == "openbar-com.openbar.app.dev-PreToolUse"
                     && pre?["timeout"] as? Int == AgentHookInstaller.eventTimeout
                     && ask?["timeout"] as? Int == ClaudeHookInstaller.permissionTimeout
                     && command.contains(#"{"openbar":"kiro","hook":"PreToolUse""#) && command.contains(" >/dev/null 2>")
                     && !asking.contains(" >/dev/null 2>") && command.hasSuffix("# openbar-hook com.openbar.app.dev"),
                     "every event, tools matched, only the permission prints, and waits: \(hooks)")
        suite.expect(AgentHookInstaller.state(.kiro, home: home, socket: socket, bundle: bundle, folder: folder) == .installed,
                     "it reads as installed")
        suite.expect(AgentHookInstaller.state(.kiro, home: home, socket: "/tmp/other.sock", bundle: bundle, folder: folder) == .outdated,
                     "another socket is older")
        // The person's own hooks file beside it stays.
        let mine = folder.appending(path: "hooks/lint.json")
        try? Data(#"{"version":"v1","hooks":[]}"#.utf8).write(to: mine)
        try? AgentHookInstaller.remove(.kiro, home: home, bundle: bundle, folder: folder)
        let left = (try? FileManager.default.contentsOfDirectory(atPath: folder.appending(path: "hooks").path)) ?? []
        suite.expect(!FileManager.default.fileExists(atPath: file.path) && left.contains("lint.json")
                     && !left.contains { $0.hasSuffix(".json") && $0.hasPrefix("openbar-") },
                     "Remove deletes only the island's file, keeping a copy Kiro doesn't load: \(left)")
        // A file of the same name that isn't the island's is never written over.
        try? Data(#"{"version":"v1","hooks":[]}"#.utf8).write(to: file)
        var refused = false
        do { try AgentHookInstaller.install(.kiro, home: home, socket: socket, bundle: bundle, folder: folder) } catch { refused = true }
        suite.expect(refused, "someone else's file of that name stays")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.kiro, home: home, path: "/opt/kiro/bin",
                                                  isExecutable: { $0 == "/opt/kiro/bin/kiro-cli" }, exists: { _ in false })
                     && AgentHookInstaller.isPresent(.kiro, home: home, path: nil,
                                                     isExecutable: { $0 == "/Users/someone/.local/bin/kiro-cli" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.kiro, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".kiro") }),
                     "found by its command, never by ~/.kiro alone, which the Kiro IDE makes too")
        suite.expect(ClaudeSessionProvider.kiro.displayName == "Kiro CLI" && ClaudeSessionProvider.kiro.shortName == "Kiro"
                     && ClaudeSessionProvider.hookAgents.last == .kiro && ClaudeSessionProvider.kiro.answersPermissions
                     && ClaudeSessionProvider.kiro.installsHooks && !ClaudeSessionProvider.kiro.takesRepliesDirectly
                     && AgentCLI.command(.kiro) == "kiro-cli" && AgentCLI.startArguments(.kiro, prompt: "go") == ["chat", "go"]
                     && AgentCLI.startsWithPrompt(.kiro) && AgentHookInstaller.foldersByHand.contains(.kiro),
                     "named, listed last, answered here, started with its first prompt")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.kiro).contains("~/.kiro/hooks")
                     && text.answerIn(.kiro) == "Answer this one in Kiro CLI."
                     && NotchClaudeSessionsSupport.agentKey(.kiro) == DefaultsKey.notchSessionsAgentKiro,
                     "its hint, where it is answered, and its own switch")
        suite.expect(ClaudeSessionProvider.kiro.character == .kiro && ClawdCharacter.kiro.favouredShell == .orchid
                     && NotchHookAgentMarkPaths.mark(.kiro) != nil
                     && AgentHookInstaller.hookFile(.kiro, home: home, bundle: "b")?.wholeFile == true
                     && AgentHookInstaller.agentFolder(.kiro, home: home, environment: [:])?.path == "/Users/someone/.kiro",
                     "its own figure, shell, mark and file of its own")
    }
}
