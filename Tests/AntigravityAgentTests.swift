// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Google Antigravity in the Sessions list (#247): what its lifecycle hooks
/// send, the sessions kept from it, the answer its hook must always print,
/// the named hook put into its hooks.json and taken out again, and the
/// transcript read for prompts and replies.
enum AntigravityAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("antigravity: hooks") { hooks(suite) }
        suite.run("antigravity: lifetime") { lifetime(suite) }
        suite.run("antigravity: hook answers") { answers(suite) }
        suite.run("antigravity: installer") { installer(suite) }
        suite.run("antigravity: transcript") { transcript(suite) }
        suite.run("antigravity: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    static func common(_ extra: [String: Any] = [:]) -> [String: Any] {
        var json: [String: Any] = ["conversationId": "a1", "workspacePaths": ["/w/rocket"],
                                   "transcriptPath": "/h/.gemini/antigravity-ide/brain/a1/.system_generated/logs/transcript.jsonl",
                                   "artifactDirectoryPath": "/h/.gemini/antigravity-ide/brain/a1", "modelName": "Gemini 3.8 Flash"]
        for (key, value) in extra { json[key] = value }
        return json
    }

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func send(_ hook: String, _ extra: [String: Any] = [:], policy: ClaudeContentPolicy = T.on, at date: Date = now) {
            T.feed(tracker, T.envelope("antigravity", hook: hook, pid: 9001, common(extra)), policy: policy, at: date)
        }
        let decoded = AgentHookMessage.decode(T.envelope("antigravity", hook: "PreInvocation", common(["invocationNum": 1])),
                                              now: now, policy: T.on)
        suite.expect(decoded?.provider == .antigravity && decoded?.transcript?.hasSuffix("logs/transcript.jsonl") == true,
                     "the envelope is Antigravity's and names its transcript")
        send("PreInvocation", ["invocationNum": 1])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "antigravity:a1" && listed[0].provider == .antigravity
                     && listed[0].project == "rocket" && listed[0].phase == .generating && listed[0].model == "Gemini 3.8 Flash",
                     "an invocation lists the conversation in its workspace, at work, with its model: \(listed)")
        send("PreToolUse", ["stepIdx": 4, "toolCall": ["name": "run_command",
                                                      "args": ["CommandLine": "npm test", "Cwd": "/w/rocket", "toolSummary": "Run tests"]]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "npm test",
                     "a command runs as Bash, summed up by its command line: \(listed[0].currentTool as Any)")
        send("PostToolUse", ["stepIdx": 4])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && listed[0].timeline[0].failed == false,
                     "the tool's step ties its end to its start: \(listed[0].timeline)")
        send("PreToolUse", ["stepIdx": 5, "toolCall": ["name": "write_to_file", "args": ["TargetFile": "/w/rocket/src/a.ts"]]])
        send("PostToolUse", ["stepIdx": 5, "error": "permission denied"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 2 && listed[0].timeline[1].name == "Write" && listed[0].timeline[1].summary == "a.ts"
                     && listed[0].timeline[1].failed, "a failed write shows by its file: \(listed[0].timeline)")
        send("PreToolUse", ["stepIdx": 6, "toolCall": ["name": "browser_click", "args": [:] as [String: Any]]])
        suite.expect(T.snapshot(tracker).sessions[0].currentTool?.name == "Browser_click", "a tool of its own keeps its name")
        send("PostInvocation", ["invocationNum": 1])
        send("Stop", ["executionNum": 1, "terminationReason": "model_stop", "fullyIdle": true], at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].timeline.allSatisfy { $0.finished != nil },
                     "Stop ends the turn and every tool still open: \(listed[0].phase)")
        send("PreInvocation", ["invocationNum": 2], at: now.addingTimeInterval(2))
        send("Stop", ["terminationReason": "error", "error": "quota"], at: now.addingTimeInterval(3))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(3)).sessions[0].phase == .idle,
                     "a turn that ended on an error is not a finished one")
        // There is no hook for a prompt or a permission: nothing waits on the island.
        suite.expect(T.snapshot(tracker).sessions[0].permission == nil
                     && !AgentHookMessage.waits(T.envelope("antigravity", hook: "PreToolUse", common())),
                     "Antigravity's hooks never wait on the island")
        suite.expect(T.decode(T.envelope("antigravity", hook: "PreInvocation", ["workspacePaths": ["/w"]])).isEmpty,
                     "a payload with no conversation is dropped")
        let auto = T.decode(T.envelope("antigravity", hook: "PreInvocation", common(["modelName": "auto"])))
        suite.expect(!auto.contains { if case .model = $0.kind { return true } else { return false } },
                     "the model picker's \"auto\" names no model")
        let quiet = T.decode(T.envelope("antigravity", hook: "PreToolUse",
                                        common(["stepIdx": 1, "toolCall": ["name": "run_command", "args": ["CommandLine": "rm -rf x"]]])),
                             policy: T.off)
        suite.expect(quiet.contains { if case .toolStarted(let use) = $0.kind { return use.summary == nil } else { return false } },
                     "without previews a command line is not kept")
    }

    // MARK: Lifetime

    static func lifetime(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        T.feed(tracker, T.envelope("antigravity", hook: "PreInvocation", pid: 31, common()))
        T.feed(tracker, T.envelope("antigravity", hook: "Stop", pid: 31, common(["terminationReason": "model_stop"])))
        tracker.prune(now: now.addingTimeInterval(60)) { _ in true }
        suite.expect(T.snapshot(tracker).sessions.count == 1, "a conversation at rest stays a while")
        tracker.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor + 1)) { _ in true }
        suite.expect(T.snapshot(tracker).sessions.isEmpty,
                     "a conversation long quiet leaves while its language server runs on")
        T.feed(tracker, T.envelope("antigravity", hook: "PreInvocation", pid: 32, common(["conversationId": "a2"])))
        tracker.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor * 2)) { _ in true }
        suite.expect(T.snapshot(tracker).sessions.count == 1, "a turn still open is never quiet")
        tracker.prune(now: now) { $0 != 32 }
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "its window or agy closing takes the row out")
    }

    // MARK: Hook answers

    /// Runs one installed hook as Antigravity does (`sh -c`), with `input` on stdin.
    static func runHook(_ command: String, input: String) -> (output: String, status: Int32, seconds: TimeInterval) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", command]
        let stdin = Pipe(), stdout = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice
        let start = Date()
        guard (try? process.run()) != nil else { return ("", -1, 0) }
        stdin.fileHandleForWriting.write(Data(input.utf8))
        try? stdin.fileHandleForWriting.close()
        let data = stdout.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (String(decoding: data, as: UTF8.self), process.terminationStatus, Date().timeIntervalSince(start))
    }

    static func answers(_ suite: TestSuite) {
        let folder = URL(fileURLWithPath: "/tmp/vs-ag-\(UUID().uuidString.prefix(8))")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let socket = folder.appending(path: "island.sock").path
        let pre = AgentHookInstaller.command(.antigravity, event: "PreToolUse", socket: socket, bundle: "b")
        let stop = AgentHookInstaller.command(.antigravity, event: "Stop", socket: socket, bundle: "b")
        suite.expect(pre.hasPrefix(#"printf '{"decision":"ask"}';"#) && pre.hasSuffix("; exit 0 # openbar-hook b")
                     && pre.contains("nc -w 2 -U") && stop.hasPrefix("printf '{}';"),
                     "the answer is printed before anything else, and the island gets two seconds at most")
        // With the island not running, the tool call is still handed to Antigravity's own check.
        var ran = runHook(pre, input: #"{"conversationId":"a1","toolCall":{"name":"run_command"}}"#)
        suite.expect(ran.output == #"{"decision":"ask"}"# && ran.status == 0,
                     "with no island the PreToolUse hook still answers ask and exits 0: \(ran)")
        ran = runHook(stop, input: #"{"conversationId":"a1"}"#)
        suite.expect(ran.output == "{}" && ran.status == 0, "every other hook answers an empty object: \(ran)")
        // With the island there, it gets the event in its envelope and the answer stays the same.
        let received = folder.appending(path: "received")
        let island = Process()
        island.executableURL = URL(fileURLWithPath: "/usr/bin/nc")
        island.arguments = ["-lU", socket]
        FileManager.default.createFile(atPath: received.path, contents: nil)
        island.standardOutput = try? FileHandle(forWritingTo: received)
        try? island.run()
        for _ in 0..<50 where !FileManager.default.fileExists(atPath: socket) { usleep(20_000) }
        ran = runHook(pre, input: #"{"conversationId":"a1","stepIdx":3}"#)
        island.waitUntilExit()
        let envelope = (try? Data(contentsOf: received)) ?? Data()
        let json = (try? JSONSerialization.jsonObject(with: envelope)) as? [String: Any]
        suite.expect(ran.output == #"{"decision":"ask"}"# && json?["openbar"] as? String == "antigravity"
                     && json?["hook"] as? String == "PreToolUse" && (json?["event"] as? [String: Any])?["stepIdx"] as? Int == 3
                     && (json?["pid"] as? Int ?? 0) > 0,
                     "the island hears the event in its envelope: \(String(decoding: envelope, as: UTF8.self))")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-antigravity-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentHookInstaller.antigravityHooks(home: home)
        suite.expect(url.path.hasSuffix("/.gemini/config/hooks.json"), "the global hooks.json")
        suite.expect(AgentHookInstaller.state(.antigravity, home: home, socket: socket, bundle: bundle) == .missing,
                     "no file, no hooks")
        try? AgentHookInstaller.install(.antigravity, home: home, socket: socket, bundle: bundle)
        var file = (try? ClaudeHookInstaller.read(url)) ?? [:]
        let own = file["openbar-com.openbar.app.dev"] as? [String: Any] ?? [:]
        let tool = (own["PreToolUse"] as? [[String: Any]])?.first
        let flat = (own["Stop"] as? [[String: Any]])?.first
        suite.expect(Set(own.keys) == Set(AgentHookInstaller.antigravityEvents) && tool?["matcher"] as? String == "*"
                     && ((tool?["hooks"] as? [[String: Any]])?.first?["command"] as? String)?.contains("decision") == true
                     && flat?["type"] as? String == "command" && flat?["timeout"] as? Int == AgentHookInstaller.eventTimeout
                     && (flat?["command"] as? String)?.hasSuffix("# openbar-hook com.openbar.app.dev") == true,
                     "a missing hooks.json gets one named hook of the island's, tool events grouped: \(own)")
        suite.expect(AgentHookInstaller.state(.antigravity, home: home, socket: socket, bundle: bundle) == .installed,
                     "it reads as installed")
        suite.expect(AgentHookInstaller.state(.antigravity, home: home, socket: "/tmp/other.sock", bundle: bundle) == .outdated,
                     "another socket is an older install")

        // Beside the person's own named hooks, after a copy of the file.
        try? manager.removeItem(at: url)
        let person: [String: Any] = [
            "lint-checker": ["PostToolUse": [["matcher": "run_command", "hooks": [["type": "command", "command": "./lint.sh"]]]]],
            "safety-gate": ["enabled": false, "PreToolUse": [["matcher": "run_command", "hooks": [["command": "./safety.sh"]]]]],
        ]
        try? JSONSerialization.data(withJSONObject: person).write(to: url)
        try? AgentHookInstaller.install(.antigravity, home: home, socket: socket, bundle: bundle)
        file = (try? ClaudeHookInstaller.read(url)) ?? [:]
        let backups = ((try? manager.contentsOfDirectory(atPath: url.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasPrefix("hooks.json.openbar-") && $0.hasSuffix(".bak") }
        suite.expect(Set(file.keys) == ["lint-checker", "safety-gate", "openbar-com.openbar.app.dev"]
                     && NSDictionary(dictionary: file["safety-gate"] as? [String: Any] ?? [:])
                        .isEqual(to: person["safety-gate"] as! [String: Any]) && backups.count == 1,
                     "the island's hook goes in beside the person's, after a backup: \(file.keys), \(backups)")
        // The Developer build's hook sits beside the release's, each its own.
        try? AgentHookInstaller.install(.antigravity, home: home, socket: socket, bundle: "com.openbar.app")
        try? AgentHookInstaller.remove(.antigravity, home: home, bundle: bundle)
        file = (try? ClaudeHookInstaller.read(url)) ?? [:]
        suite.expect(Set(file.keys) == ["lint-checker", "safety-gate", "openbar-com.openbar.app"],
                     "Remove takes out only this app's hook: \(file.keys)")
        try? AgentHookInstaller.remove(.antigravity, home: home, bundle: "com.openbar.app")
        suite.expect(NSDictionary(dictionary: (try? ClaudeHookInstaller.read(url)) ?? [:]).isEqual(to: person),
                     "and the file is as the person left it")
        // A hook of the island's under another name still goes out by its marker.
        var renamed = person
        renamed["mine"] = AgentHookInstaller.antigravityEntry(socket: socket, bundle: bundle)
        suite.expect(AgentHookInstaller.antigravityRemoving(from: renamed, bundle: bundle).count == 2,
                     "the marker, not the name, says what is the island's")
        // A file that isn't JSON is never written.
        try? Data("{ not json".utf8).write(to: url)
        var refused = false
        do { try AgentHookInstaller.install(.antigravity, home: home, socket: socket, bundle: bundle) } catch { refused = true }
        suite.expect(refused && (try? String(contentsOf: url, encoding: .utf8)) == "{ not json"
                     && AgentHookInstaller.state(.antigravity, home: home, socket: socket, bundle: bundle) == .unreadable,
                     "an unreadable hooks.json is left alone")
        try? manager.removeItem(at: url)
        try? AgentHookInstaller.remove(.antigravity, home: home, bundle: bundle)
        suite.expect(!manager.fileExists(atPath: url.path), "Remove with no file makes none")
    }

    // MARK: Transcript

    static func transcript(_ suite: TestSuite) {
        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-ag-log-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let log = folder.appending(path: "transcript.jsonl")
        let lines = [
            #"{"type":"USER_INPUT","status":"DONE","source":"USER_EXPLICIT","step_index":0,"content":"<USER_REQUEST>\nlist the files\n</USER_REQUEST>"}"#,
            #"{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":1,"tool_calls":[{"name":"run_command","args":{"CommandLine":"ls"}}]}"#,
            #"{"type":"RUN_COMMAND","status":"DONE","source":"MODEL","step_index":2,"content":"a.txt","exit_code":0}"#,
            #"{"type":"PLANNER_RESPONSE","status":"DONE","source":"MODEL","step_index":3,"content":"The folder holds a.txt."}"#,
            "not json",
        ]
        try? Data((lines.joined(separator: "\n") + "\n").utf8).write(to: log)
        suite.expect(AgentAntigravityLog.lastPrompt(in: log.path) == "list the files"
                     && AgentAntigravityLog.lastReply(in: log.path) == "The folder holds a.txt.",
                     "the prompt comes out of its tags, the reply is the last text, past a tool call with none")
        let other = folder.appending(path: "notes.jsonl")
        try? Data((lines.joined(separator: "\n")).utf8).write(to: other)
        suite.expect(AgentAntigravityLog.lastReply(in: other.path) == nil
                     && AgentAntigravityLog.lastReply(in: folder.appending(path: "missing/transcript.jsonl").path) == nil,
                     "only a transcript that is there is read")
        suite.expect(AgentAntigravityLog.unwrapped("plain") == "plain", "an unwrapped prompt stays as it is")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.antigravity, home: home, path: nil, isExecutable: { _ in false },
                                                  exists: { $0 == "/Applications/Antigravity IDE.app" })
                     && AgentHookInstaller.isPresent(.antigravity, home: home, path: nil, isExecutable: { _ in false },
                                                     exists: { $0 == "/Users/someone/Applications/Antigravity.app" })
                     && AgentHookInstaller.isPresent(.antigravity, home: home, path: nil,
                                                     isExecutable: { $0 == "/Users/someone/.local/bin/agy" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.antigravity, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".gemini") }),
                     "found by its app, its IDE or agy, never by ~/.gemini")
        suite.expect(ClaudeSessionProvider.antigravity.displayName == "Google Antigravity"
                     && ClaudeSessionProvider.antigravity.shortName == "Antigravity"
                     && ClaudeSessionProvider.hookAgents.last == .antigravity
                     && !ClaudeSessionProvider.antigravity.answersPermissions
                     && !ClaudeSessionProvider.antigravity.takesRepliesDirectly
                     && AgentCLI.startArguments(.antigravity, prompt: "go") == ["-i", "go"],
                     "named, listed last, and its approvals stay in Antigravity")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.antigravity).contains("~/.gemini/config/hooks.json")
                     && text.answerIn(.antigravity) == "Answer this one in Google Antigravity."
                     && NotchClaudeSessionsSupport.agentKey(.antigravity) == DefaultsKey.notchSessionsAgentAntigravity,
                     "its hint, where it is answered, and its own switch")
        var session = NotchSessionAgentTests.session("g", .idle, provider: .antigravity)
        session.host = .ide(bundle: ClaudeSessionHosts.antigravity, port: nil)
        suite.expect(!NotchClaudeSessionsSupport.repliesReach(session) && text.replyUnavailable(for: session) == "Reply in Google Antigravity",
                     "the IDE's agent panel takes no typed reply; the field says where to reply")
        session.host = .app("com.google.antigravity")
        suite.expect(!NotchClaudeSessionsSupport.repliesReach(session), "nor does the Antigravity app")
        session.host = .terminal(tty: "/dev/ttys004")
        suite.expect(NotchClaudeSessionsSupport.repliesReach(session), "agy in a terminal is typed into")
        suite.expect(ClaudeSessionProvider.antigravity.character == .antigravity
                     && NotchHookAgentMarkPaths.mark(.antigravity)?.layers.count == 1,
                     "its own figure and mark")
    }
}
