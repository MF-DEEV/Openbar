// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Pi, Mistral Vibe and Devin CLI in the Sessions list (#220): what their
/// extension and hooks send, the sessions kept from it, the hooks put into
/// each and taken out again, and Devin running Claude Code's hooks.
enum MoreAgentHookTests {
    static func run(_ suite: TestSuite) {
        suite.run("more agents: pi") { pi(suite) }
        suite.run("more agents: vibe") { vibe(suite) }
        suite.run("more agents: devin") { devin(suite) }
        suite.run("more agents: devin runs claude's hooks") { devinClaude(suite) }
        suite.run("more agents: vibe's log") { vibeLog(suite) }
        suite.run("more agents: installer") { installer(suite) }
        suite.run("more agents: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    // MARK: Pi

    static func pi(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func send(_ event: [String: Any], at date: Date = now) { T.feed(tracker, T.envelope("pi", pid: 7001, event), at: date) }
        send(["kind": "created", "session": "p1", "directory": "/w/pi-app"])
        send(["kind": "model", "session": "p1", "directory": "/w/pi-app", "model": "claude-sonnet-4-5"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .pi && listed[0].id == "pi:p1" && listed[0].phase == .idle
                     && listed[0].model == "claude-sonnet-4-5" && listed[0].project == "pi-app",
                     "Pi's extension lists its session with its model: \(listed)")
        send(["kind": "prompt", "session": "p1", "text": "list the files"])
        send(["kind": "status", "session": "p1", "status": "busy"])
        send(["kind": "tool", "session": "p1", "call": "c1", "tool": "bash", "status": "running", "input": ["command": "ls -la"]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "ls -la" && listed[0].chat.map(\.text) == ["list the files"],
                     "a tool at work shows by Claude Code's name for it: \(listed[0].currentTool as Any)")
        send(["kind": "tool", "session": "p1", "call": "c1", "tool": "bash", "status": "completed"])
        send(["kind": "message", "session": "p1", "id": "m1", "model": "claude-sonnet-4-5", "cost": 0.0042,
              "tokens": ["input": 1200, "output": 40, "reasoning": 0, "cache": ["read": 0, "write": 0]]])
        send(["kind": "reply", "session": "p1", "text": "Here they are."])
        // A dialog of another extension's waits on the person, in Pi.
        send(["kind": "question", "session": "p1"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingInput && listed[0].waitingFor == CodexSessionState.answerInTerminal
                     && listed[0].tokens.input == 1200 && abs(listed[0].cost - 0.0042) < 0.000_01,
                     "an extension's dialog is a wait answered in Pi; tokens and cost add up: \(listed[0].phase)")
        send(["kind": "permissionReplied", "session": "p1"])
        send(["kind": "status", "session": "p1", "status": "idle"], at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Here they are." && listed[0].timeline.count == 1
                     && listed[0].timeline[0].finished != nil, "the run settling ends the turn: \(listed[0].phase)")
        send(["kind": "error", "session": "p1"])
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions[0].phase == .finished,
                     "an error after the turn ended changes nothing")
        // Pi asks no permission of its own; a permission envelope is not one of Pi's.
        send(["kind": "permission", "session": "p1", "id": "x", "permission": "bash"])
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions[0].permission == nil,
                     "only OpenCode's plugin asks the island to answer a permission")
        suite.expect(AgentHookMessage.waits(T.envelope("pi", ["kind": "listen"]))
                     && !AgentHookMessage.waits(T.envelope("pi", ["kind": "permission", "session": "p1", "id": "x"]))
                     && !AgentHookMessage.waits(T.envelope("pi", ["kind": "status", "session": "p1", "status": "busy"])),
                     "only Pi's listening extension waits on the island")
        suite.expect(AgentHookMessage.decode(T.envelope("pi", ["kind": "listen"]), now: now, policy: T.on)?.listens == true,
                     "the listening extension is told apart")
        suite.expect(ClaudeSessionProvider.pi.takesRepliesDirectly && ClaudeSessionProvider.pi.reportsThroughPlugin
                     && !ClaudeSessionProvider.pi.answersPermissions, "Pi takes replies through its extension")
        let prompt = AgentHookMessage.opencodePrompt(session: "p1", text: "and the hidden ones")
            .flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["prompt"] as? [String: String]
        suite.expect(prompt == ["session": "p1", "text": "and the hidden ones"], "a reply goes to the extension's session")
        send(["kind": "deleted", "session": "p1"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "the session's end takes it out")

        // Previews off: no prompt, reply or title is kept.
        let quiet = AgentHookSessions()
        for event in [["kind": "created", "session": "p2", "directory": "/w/a"], ["kind": "prompt", "session": "p2", "text": "secret"],
                      ["kind": "reply", "session": "p2", "text": "secret reply"], ["kind": "title", "session": "p2", "title": "Secret"]]
            as [[String: Any]] {
            T.feed(quiet, T.envelope("pi", event), policy: T.off)
        }
        let kept = T.snapshot(quiet, previews: false).sessions
        suite.expect(kept.count == 1 && kept[0].chat.isEmpty && kept[0].lastText == nil && kept[0].title == nil,
                     "with previews off nothing Pi was told is kept: \(kept)")

        // The extension the island installs is the Windows island's, word for word.
        suite.expect(AgentPiExtension.source.contains("pi.sendUserMessage") && AgentPiExtension.source.contains("socket.unref()")
                     && AgentPiExtension.source.contains("__OPENBAR_SOCKET__"),
                     "the extension sends replies, never keeps Pi from quitting, and has its socket written in")
        let file = String(decoding: AgentHookInstaller.piFile(socket: "/tmp/a \"b\".sock", bundle: "com.openbar.app.dev"), as: UTF8.self)
        suite.expect(file.contains(#"const SOCKET = "/tmp/a \"b\".sock";"#) && file.contains("# openbar-hook com.openbar.app.dev"),
                     "the socket is written in as a JS string, and the marker names the app")
    }

    // MARK: Vibe

    static func vibe(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        let base: [String: Any] = ["session_id": "v1", "cwd": "/w/vibe-app", "parent_session_id": NSNull(),
                                   "transcript_path": "/tmp/vibe/logs/session/unified/v1"]
        func send(_ hook: String, _ fields: [String: Any], at date: Date = now) {
            var event = base
            event["hook_event_name"] = hook
            for (key, value) in fields { event[key] = value }
            T.feed(tracker, T.envelope("vibe", hook: hook, pid: 7101, event), at: date)
        }
        send("session_start", ["source": "startup", "agent_name": "default", "model": "mistral-medium-3.5"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .vibe && listed[0].phase == .idle
                     && listed[0].model == "mistral-medium-3.5", "a new Vibe session is listed with its model: \(listed)")
        send("pre_agent", ["agent_name": "default", "turn_id": "turn-1", "prompt": "fix the tests"])
        send("pre_tool", ["tool_name": "file_system.bash", "tool_call_id": "call_1", "tool_input": ["command": "npm test"]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.id == "call_1" && listed[0].currentTool?.summary == "npm test",
                     "Vibe's qualified tool name reads as Claude Code's: \(listed[0].currentTool as Any)")
        send("notification", ["notification_type": "approval_requested", "message": "Vibe needs your approval to run file_system.bash",
                              "tool_name": "file_system.bash"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission == nil
                     && listed[0].waitingFor == CodexSessionState.answerInTerminal,
                     "an approval is answered in Vibe, not here")
        send("post_tool", ["tool_name": "file_system.bash", "tool_call_id": "call_1", "tool_input": ["command": "npm test"],
                           "tool_status": "failure"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.last?.failed == true,
                     "the tool that ran after the approval clears the wait, and its failure shows")
        send("notification", ["notification_type": "question_asked", "message": "Vibe has a question for you"])
        suite.expect(T.snapshot(tracker).sessions[0].phase == .waitingInput, "a question waits on the person")
        // A subagent's hooks name the session it works for; they are not a row of their own.
        T.feed(tracker, T.envelope("vibe", hook: "pre_tool", pid: 7101,
                                   ["hook_event_name": "pre_tool", "session_id": "sub", "parent_session_id": "v1",
                                    "tool_name": "file_system.read_file", "tool_input": [:] as [String: Any]]))
        suite.expect(T.snapshot(tracker).sessions.count == 1, "a subagent adds no row")
        send("notification", ["notification_type": "turn_completed"], at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].chat.map(\.text) == ["fix the tests"],
                     "a completed turn finishes the session: \(listed[0].phase)")
        let failed = AgentHookSessions()
        for (hook, fields) in [("pre_agent", ["prompt": "x"]), ("notification", ["notification_type": "turn_failed"])]
            as [(String, [String: Any])] {
            var event = base
            event["hook_event_name"] = hook
            for (key, value) in fields { event[key] = value }
            T.feed(failed, T.envelope("vibe", hook: hook, event))
        }
        suite.expect(T.snapshot(failed).sessions[0].phase == .idle, "a failed turn is not shown as done")
        send("session_end", ["reason": "exit"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "the session's end takes it out")
        let decoded = AgentHookMessage.decode(T.envelope("vibe", hook: "notification", base.merging(
            ["hook_event_name": "notification", "notification_type": "turn_completed"]) { $1 }), now: now, policy: T.on)
        suite.expect(decoded?.transcript == "/tmp/vibe/logs/session/unified/v1", "the session's log folder is passed on")
        suite.expect(!AgentHookMessage.waits(T.envelope("vibe", hook: "pre_tool", base)), "Vibe's hooks never wait on the island")
    }

    // MARK: Devin

    static func devin(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func send(_ hook: String, _ fields: [String: Any], at date: Date = now) {
            var event: [String: Any] = ["hook_event_name": hook, "session_id": "d1", "prompt_id": "t1"]
            for (key, value) in fields { event[key] = value }
            T.feed(tracker, T.envelope("devin", hook: hook, pid: 7201, event), at: date)
        }
        send("SessionStart", ["source": "startup"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .devin && listed[0].id == "devin:d1" && listed[0].phase == .idle,
                     "a Devin session is listed from its start: \(listed)")
        send("UserPromptSubmit", ["prompt": "add a test"])
        send("PreToolUse", ["tool_name": "exec", "tool_input": ["command": "cargo test", "shell_id": "main"]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "cargo test", "Devin's exec reads as Bash")
        send("PermissionRequest", ["tool_name": "exec", "tool_input": ["command": "rm -rf target"]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission == nil
                     && listed[0].waitingFor == CodexSessionState.answerInTerminal,
                     "Devin's permission prompt is answered in Devin")
        send("PostToolUse", ["tool_name": "exec", "tool_input": ["command": "cargo test", "shell_id": "main"],
                             "tool_response": ["success": false, "output": "", "error": "1 failed"]])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.first?.failed == true,
                     "a tool that ran clears the wait; its failure shows: \(listed[0].timeline)")
        send("PreToolUse", ["tool_name": "ask_user_question", "tool_input": ["question": "Which one?"]])
        suite.expect(T.snapshot(tracker).sessions[0].phase == .waitingInput, "Devin's question waits on the person")
        send("PostToolUse", ["tool_name": "ask_user_question", "tool_input": [:] as [String: Any],
                             "tool_response": ["success": true, "output": "the first"]])
        suite.expect(T.snapshot(tracker).sessions[0].phase == .generating, "an answered question lets it work on")
        send("PreToolUse", ["tool_name": "mcp__github__create_issue", "tool_input": ["title": "x"]])
        suite.expect(T.snapshot(tracker).sessions[0].currentTool?.name == "github__create_issue", "an MCP tool loses its prefix")
        send("Stop", ["stop_hook_active": false], at: now.addingTimeInterval(1))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions[0].phase == .finished, "Stop ends the turn")
        send("SessionEnd", ["reason": "exit"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "the session's end takes it out")
        suite.expect(!AgentHookMessage.waits(T.envelope("devin", hook: "PermissionRequest", ["session_id": "d1"])),
                     "Devin's permission hook never waits on the island")
        // The hook prints nothing, so Devin asks in its own terminal.
        let command = AgentHookInstaller.command(.devin, event: "PermissionRequest", socket: "/tmp/s.sock", bundle: "b")
        suite.expect(command.hasSuffix("; exit 0 # openbar-hook b") && !command.contains("decision"),
                     "Devin's permission hook answers nothing")
    }

    static func devinClaude(_ suite: TestSuite) {
        func json(_ object: [String: Any]) -> Data { try! JSONSerialization.data(withJSONObject: object) }
        suite.expect(AgentDevinClaudeHooks.isDevinPayload(json(["hook_event_name": "PreToolUse", "session_id": "d1",
                                                                "prompt_id": "t1", "tool_name": "exec"])),
                     "a hook with Devin's turn id and no transcript is Devin's")
        suite.expect(!AgentDevinClaudeHooks.isDevinPayload(json(["hook_event_name": "PreToolUse", "session_id": "c1",
                                                                 "transcript_path": "/x.jsonl", "cwd": "/w",
                                                                 "prompt_id": "t1", "tool_name": "Bash"]))
                     && !AgentDevinClaudeHooks.isDevinPayload(json(["hook_event_name": "Stop", "session_id": "c1",
                                                                  "transcript_path": "/x.jsonl"])),
                     "Claude Code's own hooks always name their transcript")
        suite.expect(AgentDevinClaudeHooks.isDevinProcess("devin") && AgentDevinClaudeHooks.isDevinProcess("Devin")
                     && !AgentDevinClaudeHooks.isDevinProcess("claude") && !AgentDevinClaudeHooks.isDevinProcess("node")
                     && !AgentDevinClaudeHooks.isDevinProcess(nil) && !AgentDevinClaudeHooks.isDevinProcess("devinci"),
                     "Devin's process is told by its name")
        suite.expect(!AgentDevinClaudeHooks.isDevin(json(["hook_event_name": "Stop", "session_id": "c1",
                                                          "transcript_path": "/x.jsonl"]), peer: getpid()),
                     "a hook from a process that isn't Devin's is Claude Code's")
    }

    // MARK: Vibe's log

    static func vibeLog(_ suite: TestSuite) {
        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-vibe-log-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let chunks = folder.appending(path: "chunks")
        try? FileManager.default.createDirectory(at: chunks, withIntermediateDirectories: true)
        // Written by Vibe 3.1.2 against a stand-in model (/tmp/ob220): the shape of a history chunk.
        let history = #"[{"message":{"content":[{"text":"list the files","type":"text"}],"role":"user"},"source":"history","turn_index":0},{"message":{"content":[{"arguments":{"raw":"{}","type":"json","value":{"command":"ls"}},"id":"call_1","name":"bash","type":"tool_call"}],"role":"assistant"},"source":"history","turn_index":0},{"message":{"content":[{"text":"{}","type":"text"}],"name":"bash","outcome":"success","role":"tool","tool_call_id":"call_1"},"source":"history","turn_index":0},{"message":{"content":[{"text":"Done: the folder holds the files. ","type":"text"}],"role":"assistant"},"source":"history","turn_index":0}]"#
        let older = #"[{"message":{"content":[{"text":"An older reply.","type":"text"}],"role":"assistant"},"source":"history","turn_index":0}]"#
        let other = #"[{"createdAt":1,"type":"notice","message":"Running hooks"}]"#
        try? Data(older.utf8).write(to: chunks.appending(path: "a.json"))
        try? FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-60)],
                                               ofItemAtPath: chunks.appending(path: "a.json").path)
        try? Data(history.utf8).write(to: chunks.appending(path: "b.json"))
        try? FileManager.default.setAttributes([.modificationDate: now], ofItemAtPath: chunks.appending(path: "b.json").path)
        try? Data(other.utf8).write(to: chunks.appending(path: "c.json"))
        try? FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(10)],
                                               ofItemAtPath: chunks.appending(path: "c.json").path)
        suite.expect(AgentVibeLog.lastReply(in: folder.path) == "Done: the folder holds the files.",
                     "the newest history ends on the last reply, past a tool call with no text and a chunk of notices")
        suite.expect(AgentVibeLog.lastReply(in: folder.appending(path: "missing").path) == nil
                     && AgentVibeLog.lastReply(chunk: Data("not json".utf8)) == nil,
                     "a log that isn't there, or isn't JSON, gives no reply")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-more-agents-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let none: [String: String] = [:]
        let manager = FileManager.default

        // Vibe: the island's tables after the person's own, between its markers.
        let vibe = AgentHookInstaller.vibeHooks(home: home, environment: none)
        try? manager.createDirectory(at: vibe.deletingLastPathComponent(), withIntermediateDirectories: true)
        let own = "# my hooks\n[[hooks]]\nname = \"lint\"\ntype = \"post_agent\"\ncommand = \"eslint .\"\n"
        try? Data(own.utf8).write(to: vibe)
        suite.expect(AgentHookInstaller.state(.vibe, home: home, socket: socket, bundle: bundle) == .missing,
                     "nothing is in before Install")
        try? AgentHookInstaller.install(.vibe, home: home, socket: socket, bundle: bundle)
        let text = (try? String(contentsOf: vibe, encoding: .utf8)) ?? ""
        suite.expect(text.hasPrefix(own) && text.contains("name = \"openbar-com.openbar.app.dev-pre_tool\"")
                     && text.components(separatedBy: "[[hooks]]").count - 1 == 1 + AgentHookInstaller.vibeEvents.count,
                     "the island's hooks go after the person's, which stay as they were:\n\(text)")
        suite.expect(AgentHookInstaller.state(.vibe, home: home, socket: socket, bundle: bundle) == .installed
                     && AgentHookInstaller.state(.vibe, home: home, socket: "/elsewhere.sock", bundle: bundle) == .outdated,
                     "the hooks are known as the island's, and another socket's need an update")
        let backups = ((try? manager.contentsOfDirectory(atPath: vibe.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasPrefix("hooks.toml.openbar-") && $0.hasSuffix(".bak") }
        suite.expect(backups.count == 1, "hooks.toml was copied aside first: \(backups)")
        try? AgentHookInstaller.install(.vibe, home: home, socket: "/elsewhere.sock", bundle: bundle)
        let updated = (try? String(contentsOf: vibe, encoding: .utf8)) ?? ""
        suite.expect(updated.components(separatedBy: AgentHookInstaller.vibeStart(bundle: bundle)).count == 2
                     && updated.contains("/elsewhere.sock") && !updated.contains("island'\\\\''s"),
                     "an update replaces the island's hooks, never adds them twice")
        try? AgentHookInstaller.install(.vibe, home: home, socket: socket, bundle: "com.openbar.app")
        try? AgentHookInstaller.remove(.vibe, home: home, bundle: bundle)
        let afterRemove = (try? String(contentsOf: vibe, encoding: .utf8)) ?? ""
        suite.expect(afterRemove.hasPrefix(own) && afterRemove.contains("com.openbar.app-pre_tool")
                     && !afterRemove.contains("com.openbar.app.dev"),
                     "Remove takes out only this app's hooks; the release's and the person's stay")
        try? AgentHookInstaller.remove(.vibe, home: home, bundle: "com.openbar.app")
        suite.expect((try? String(contentsOf: vibe, encoding: .utf8)) == own, "with both gone the file is as it was")
        // Each command, read back as TOML would, is the hook's exact shell line.
        let block = AgentHookInstaller.vibeBlock(socket: socket, bundle: bundle)
        let commandLine = block.components(separatedBy: "\n").first { $0.hasPrefix("command = ") } ?? ""
        let decoded = (try? JSONSerialization.jsonObject(with: Data(String(commandLine.dropFirst("command = ".count)).utf8),
                                                         options: [.fragmentsAllowed])) as? String
        suite.expect(decoded == AgentHookInstaller.command(.vibe, event: "session_start", socket: socket, bundle: bundle),
                     "the TOML string holds the command exactly: \(commandLine)")
        suite.expect(AgentHookInstaller.tomlString("a\"b\\c\nd\u{1}") == #""a\"b\\c\nd\u0001""#, "TOML escapes")
        // Hooks kept as an inline array can't have tables beside them: the file is left alone.
        let inline = "hooks = [{ name = \"x\", type = \"post_agent\", command = \"true\" }]\n"
        try? Data(inline.utf8).write(to: vibe)
        var refused = false
        do { try AgentHookInstaller.install(.vibe, home: home, socket: socket, bundle: bundle) } catch { refused = true }
        suite.expect(refused && (try? String(contentsOf: vibe, encoding: .utf8)) == inline
                     && AgentHookInstaller.state(.vibe, home: home, socket: socket, bundle: bundle) == .unreadable,
                     "an inline hooks array is never touched")
        try? manager.removeItem(at: vibe)
        try? AgentHookInstaller.install(.vibe, home: home, socket: socket, bundle: bundle)
        suite.expect(AgentHookInstaller.state(.vibe, home: home, socket: socket, bundle: bundle) == .installed,
                     "a missing hooks.toml is made")

        // Devin: Claude Code's groups in its config, beside the person's own.
        let devin = AgentHookInstaller.devinConfig(home: home, environment: none)
        try? manager.createDirectory(at: devin.deletingLastPathComponent(), withIntermediateDirectories: true)
        let devinOwn: [String: Any] = ["version": 1, "permission_mode": "auto",
                                       "hooks": ["PreToolUse": [["matcher": "exec", "hooks": [["type": "command", "command": "./guard.sh"]]]]]]
        try? JSONSerialization.data(withJSONObject: devinOwn).write(to: devin)
        try? AgentHookInstaller.install(.devin, home: home, socket: socket, bundle: bundle)
        let config = (try? ClaudeHookInstaller.read(devin)) ?? [:]
        let pre = (config["hooks"] as? [String: Any])?["PreToolUse"] as? [[String: Any]] ?? []
        suite.expect(config["permission_mode"] as? String == "auto" && pre.count == 2 && pre[0]["matcher"] as? String == "exec"
                     && Set(((config["hooks"] as? [String: Any]) ?? [:]).keys) == Set(AgentHookInstaller.devinEvents),
                     "Devin's hooks go beside the person's own: \(pre)")
        suite.expect(AgentHookInstaller.state(.devin, home: home, socket: socket, bundle: bundle) == .installed,
                     "every event holds the island's hook")
        try? AgentHookInstaller.remove(.devin, home: home, bundle: bundle)
        suite.expect(NSDictionary(dictionary: (try? ClaudeHookInstaller.read(devin)) ?? [:]).isEqual(to: devinOwn),
                     "Remove leaves Devin's config as it was")
        try? manager.removeItem(at: devin)
        try? AgentHookInstaller.install(.devin, home: home, socket: socket, bundle: bundle)
        suite.expect(((try? ClaudeHookInstaller.read(devin)) ?? [:])["version"] as? Int == 1, "a new config gets Devin's version")

        // Pi: an extension file of the island's own.
        let pi = AgentHookInstaller.piExtension(home: home, bundle: bundle, environment: none)
        try? AgentHookInstaller.install(.pi, home: home, socket: socket, bundle: bundle)
        suite.expect(pi.path.hasSuffix(".pi/agent/extensions/openbar-com.openbar.app.dev.js")
                     && AgentHookInstaller.state(.pi, home: home, socket: socket, bundle: bundle) == .installed,
                     "Pi's extension goes into its extensions folder")
        AgentHookInstaller.removeAll(home: home, bundle: bundle)
        suite.expect(!manager.fileExists(atPath: pi.path)
                     && AgentHookInstaller.state(.devin, home: home, socket: socket, bundle: bundle) == .missing
                     && AgentHookInstaller.state(.vibe, home: home, socket: socket, bundle: bundle) == .missing,
                     "removing everything takes the three out")
        // Their folders move with their own variables.
        let moved = ["PI_CODING_AGENT_DIR": "/x/pi", "VIBE_HOME": "/x/vibe", "XDG_CONFIG_HOME": "/x/config"]
        suite.expect(AgentHookInstaller.piExtension(home: home, bundle: "b", environment: moved).path == "/x/pi/extensions/openbar-b.js"
                     && AgentHookInstaller.vibeHooks(home: home, environment: moved).path == "/x/vibe/hooks.toml"
                     && AgentHookInstaller.devinConfig(home: home, environment: moved).path == "/x/config/devin/config.json",
                     "PI_CODING_AGENT_DIR, VIBE_HOME and XDG_CONFIG_HOME are followed")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(!AgentHookInstaller.isPresent(.pi, home: home, path: "/opt/bin", isExecutable: { $0 == "/opt/bin/pi" },
                                                   exists: { _ in false })
                     && AgentHookInstaller.isPresent(.pi, home: home, path: "/opt/bin", isExecutable: { $0 == "/opt/bin/pi" },
                                                     exists: { $0 == "/Users/someone/.pi/agent" }),
                     "another tool's `pi` is not Pi; with Pi's folder it is")
        suite.expect(AgentHookInstaller.isPresent(.vibe, home: home, path: nil, isExecutable: { $0 == "/Users/someone/.local/bin/vibe" },
                                                  exists: { _ in false })
                     && AgentHookInstaller.isPresent(.devin, home: home, path: nil,
                                                     isExecutable: { $0 == "/Users/someone/.local/bin/devin" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.devin, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { _ in true }),
                     "Vibe and Devin are found where their installers put them, never by a folder")
        suite.expect(AgentCLI.startArguments(.devin, prompt: "go") == ["--", "go"] && AgentCLI.startArguments(.pi, prompt: "go") == ["go"]
                     && AgentCLI.startArguments(.vibe, prompt: "go") == ["go"] && AgentCLI.startArguments(.vibe, prompt: nil).isEmpty,
                     "New session hands each its first prompt")
        suite.expect(ClaudeSessionProvider.hookAgents.suffix(3) == [.pi, .vibe, .devin]
                     && [ClaudeSessionProvider.pi, .vibe, .devin].map(\.displayName) == ["Pi", "Mistral Vibe", "Devin CLI"]
                     && ![ClaudeSessionProvider.vibe, .devin].contains { $0.takesRepliesDirectly || $0.answersPermissions },
                     "the three are listed after the others; Vibe's and Devin's replies are typed into their terminal")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect([ClaudeSessionProvider.pi, .vibe, .devin].allSatisfy { !text.agentHint($0).isEmpty }
                     && text.answerIn(.vibe) == "Answer this one in Mistral Vibe." && text.answerIn(.devin) == "Answer this one in Devin CLI.",
                     "each has a hint in Settings, and a request says where to answer it")
        suite.expect(NotchClaudeSessionsSupport.agentKey(.pi) == DefaultsKey.notchSessionsAgentPi
                     && NotchClaudeSessionsSupport.agentKey(.vibe) == DefaultsKey.notchSessionsAgentVibe
                     && NotchClaudeSessionsSupport.agentKey(.devin) == DefaultsKey.notchSessionsAgentDevin,
                     "each has its own switch")
    }
}
