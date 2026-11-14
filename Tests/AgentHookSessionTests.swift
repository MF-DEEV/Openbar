// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Gemini CLI, Copilot CLI, Cursor and OpenCode in the Sessions list: what
/// their hooks and plugin send, the sessions the island keeps from it, and
/// the hooks it puts into each agent and takes out again.
enum AgentHookSessionTests {
    static func run(_ suite: TestSuite) {
        suite.run("hook agents: envelopes") { envelopes(suite) }
        suite.run("hook agents: gemini") { gemini(suite) }
        suite.run("hook agents: copilot") { copilot(suite) }
        suite.run("hook agents: cursor") { cursor(suite) }
        suite.run("hook agents: opencode") { opencode(suite) }
        suite.run("hook agents: lifetime") { lifetime(suite) }
        suite.run("hook agents: previews") { previews(suite) }
        suite.run("hook agents: installer") { installer(suite) }
        suite.run("hook agents: strip") { strip(suite) }
        suite.run("hook agents: branch") { branch(suite) }
        suite.run("hook agents: new session") { newSession(suite) }
        suite.run("hook agents: marks") { marks(suite) }
        suite.run("hook agents: automatic hooks") { automatic(suite) }
    }

    static let now = Date(timeIntervalSince1970: 1_800_000_000)
    static let on = ClaudeContentPolicy(previews: true)
    static let off = ClaudeContentPolicy(previews: false)

    static func envelope(_ provider: String, hook: String = "", pid: Int = 4242, _ event: [String: Any]) -> Data {
        var json: [String: Any] = ["openbar": provider, "pid": pid, "event": event]
        if !hook.isEmpty { json["hook"] = hook }
        return try! JSONSerialization.data(withJSONObject: json)
    }

    static func decode(_ data: Data, policy: ClaudeContentPolicy = ClaudeContentPolicy(previews: true),
                       at date: Date = now) -> [AgentHookEvent] {
        AgentHookMessage.decode(data, now: date, policy: policy)?.events ?? []
    }

    static func feed(_ tracker: AgentHookSessions, _ data: Data, policy: ClaudeContentPolicy = on, at date: Date = now) {
        for event in decode(data, policy: policy, at: date) { tracker.ingest(event, now: date) }
    }

    static func snapshot(_ tracker: AgentHookSessions, at date: Date = now, previews: Bool = true)
        -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        tracker.snapshot(now: date, previews: previews, host: { _ in .terminal(tty: "/dev/ttys009") }, branch: { _ in "main" })
    }

    // MARK: Envelopes

    static func envelopes(_ suite: TestSuite) {
        let gemini = envelope("gemini", ["hook_event_name": "SessionStart", "session_id": "g1", "cwd": "/w/app"])
        suite.expect(AgentHookMessage.isEnvelope(gemini), "the island's envelope is told apart cheaply")
        let claude = try! JSONSerialization.data(withJSONObject: ["hook_event_name": "SessionStart", "session_id": "c1"])
        suite.expect(!AgentHookMessage.isEnvelope(claude) && AgentHookMessage.decode(claude, now: now, policy: on) == nil,
                     "Claude Code's own hooks are not an envelope")
        suite.expect(AgentHookMessage.decode(envelope("claude", ["session_id": "x"]), now: now, policy: on) == nil
                     && AgentHookMessage.decode(envelope("aider", ["session_id": "x"]), now: now, policy: on) == nil,
                     "only the agents that report through hooks are taken from an envelope")
        suite.expect(!AgentHookMessage.waits(gemini), "a Gemini hook never waits on the island")
        suite.expect(AgentHookMessage.waits(envelope("opencode", ["kind": "permission", "session": "o", "id": "p"]))
                     && AgentHookMessage.waits(envelope("opencode", ["kind": "listen"]))
                     && !AgentHookMessage.waits(envelope("opencode", ["kind": "status", "session": "o", "status": "busy"])),
                     "only OpenCode's permissions and its listening plugin wait")
        suite.expect(decode(Data("{\"openbar\":\"gemini\",\"pid\":1,\"event\":".utf8)).isEmpty,
                     "a cut-off envelope, as from a hook with no input, is dropped")
        let ids = decode(gemini).map(\.id)
        suite.expect(ids == ["gemini:g1"], "a session's id names its agent, so no two agents' ids meet: \(ids)")
    }

    // MARK: Gemini

    static func gemini(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        let base: [String: Any] = ["session_id": "g1", "cwd": "/w/app", "transcript_path": "/t.jsonl",
                                   "timestamp": "2027-01-15T08:00:00.000Z"]
        func event(_ name: String, _ extra: [String: Any] = [:]) -> Data {
            envelope("gemini", base.merging(["hook_event_name": name]) { $1 }.merging(extra) { $1 })
        }
        feed(tracker, event("SessionStart", ["source": "startup"]))
        var listed = snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].phase == .idle && listed[0].provider == .gemini
                     && listed[0].project == "app" && listed[0].cwd == "/w/app" && listed[0].branch == "main",
                     "a started session is listed at rest, in its folder: \(listed)")
        feed(tracker, event("BeforeAgent", ["prompt": "fix the tests"]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["fix the tests"],
                     "a prompt begins a turn")
        feed(tracker, event("BeforeTool", ["tool_name": "run_shell_command", "tool_input": ["command": "npm test"]]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "npm test",
                     "Gemini's shell tool runs as Bash, summed up by its command: \(String(describing: listed[0].currentTool))")
        feed(tracker, event("Notification", ["notification_type": "ToolPermission", "message": "Allow npm test?",
                                             "details": ["type": "exec", "command": "npm test", "rootCommand": "npm"]]))
        let asked = snapshot(tracker)
        listed = asked.sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission == nil
                     && listed[0].waitingFor == CodexSessionState.answerInTerminal,
                     "a permission Gemini asks for waits in its terminal, where it is answered")
        suite.expect(asked.events == [.needsPermission(id: "gemini:g1", tool: "Bash")] || asked.events.contains {
            if case .needsPermission(let id, _) = $0 { return id == "gemini:g1" }
            return false
        }, "the wait is news: \(asked.events)")
        suite.expect(listed[0].waitingDiffs.isEmpty, "a command has no diff to show")
        // An edit Gemini asks about shows what it changes, though it is answered there.
        feed(tracker, event("Notification", ["notification_type": "ToolPermission", "message": "Apply this change?",
                                             "details": ["type": "edit", "filePath": "/w/app/a.ts", "fileName": "a.ts",
                                                         "originalContent": "a\nb\n", "newContent": "a\nc\n"]]))
        let edit = snapshot(tracker).sessions[0]
        suite.expect(edit.permission == nil && edit.waitingDiffs.first?.path == "/w/app/a.ts"
                     && edit.waitingDiffs.first?.added == 1 && edit.waitingDiffs.first?.removed == 1,
                     "a Gemini edit shows its diff above the terminal's answer: \(edit.waitingDiffs)")
        feed(tracker, event("AfterTool", ["tool_name": "run_shell_command", "tool_input": ["command": "npm test"],
                                          "tool_response": ["llmContent": "ok"]]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed,
                     "the tool ran, so it was allowed, and the turn goes on: \(listed[0].phase)")
        feed(tracker, event("AfterAgent", ["prompt": "fix the tests", "prompt_response": "All green."]))
        let done = snapshot(tracker)
        suite.expect(done.sessions[0].phase == .finished && done.sessions[0].lastText == "All green."
                     && done.events.contains { if case .finished(let id, _) = $0 { return id == "gemini:g1" }; return false },
                     "the turn's end is news, with the reply it ended on: \(done.events)")
        tracker.acknowledge("gemini:g1", now: now.addingTimeInterval(1))
        suite.expect(snapshot(tracker, at: now.addingTimeInterval(2)).sessions[0].phase == .idle, "a read result rests")
        feed(tracker, event("BeforeTool", ["tool_name": "read_file", "tool_input": ["file_path": "/w/app/Sources/a.swift"]]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.summary == "a.swift",
                     "a tool seen without its prompt opens a turn, and a path reads as its file's name")
        feed(tracker, event("AfterTool", ["tool_name": "read_file", "tool_input": ["file_path": "/w/app/Sources/a.swift"],
                                          "tool_response": ["error": ["message": "no such file"]]]))
        suite.expect(snapshot(tracker).sessions[0].timeline.last?.failed == true, "a tool's error marks it failed")
        feed(tracker, event("BeforeAgent", ["prompt": "again"]))
        feed(tracker, event("AfterAgent", ["prompt": "again", "prompt_response": "[no response text]"]))
        suite.expect(snapshot(tracker).sessions[0].lastText == "All green.",
                     "Gemini's stand-in for a turn with no text is no reply")
        suite.expect(ClaudeTyping.steps("hi", enterAfter: ClaudeTyping.enterDelay(.gemini) ?? 0) == [.paste("hi"), .wait(0.5), .enter]
                     && ClaudeTyping.enterDelay(.claude) == nil
                     && ClaudeTerminalScripts.type("hi", tty: "/dev/ttys1", enterAfter: 0.5).contains("do script \"\" in theTab"),
                     "a reply to Gemini is sent by a return of its own, after the paste settles")
        feed(tracker, event("SessionEnd", ["reason": "exit"]))
        suite.expect(snapshot(tracker).sessions.isEmpty, "a session that ended leaves the list")
    }

    // MARK: Copilot

    static func copilot(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func event(_ hook: String, _ extra: [String: Any] = [:]) -> Data {
            envelope("copilot", hook: hook, ["sessionId": "c1", "timestamp": 1_800_000_000_000.0, "cwd": "/w/api"]
                .merging(extra) { $1 })
        }
        feed(tracker, event("sessionStart", ["source": "new"]))
        feed(tracker, event("userPromptSubmitted", ["prompt": "add a route"]))
        feed(tracker, event("preToolUse", ["toolName": "bash", "toolArgs": "{\"command\":\"ls -la\",\"description\":\"list\"}"]))
        var listed = snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .copilot && listed[0].phase == .runningTool
                     && listed[0].currentTool?.name == "Bash" && listed[0].currentTool?.summary == "ls -la",
                     "Copilot's tool arguments, given as JSON text, are read like an object: \(listed)")
        feed(tracker, event("notification", ["notification_type": "permission_prompt", "message": "Allow ls -la?",
                                             "hook_event_name": "Notification"]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission == nil,
                     "a permission prompt Copilot shows is answered in its terminal")
        feed(tracker, event("postToolUseFailure", ["toolName": "bash", "toolArgs": "{\"command\":\"ls -la\",\"description\":\"list\"}",
                                                   "error": "denied"]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.last?.failed == true,
                     "a denied tool ends failed and lets the turn go on")
        feed(tracker, event("notification", ["notification_type": "elicitation_dialog"]))
        suite.expect(snapshot(tracker).sessions[0].phase == .waitingInput, "a dialog of Copilot's waits on the person")
        feed(tracker, event("agentStop", ["stopReason": "end_turn"]))
        suite.expect(snapshot(tracker).sessions[0].phase == .finished, "the agent's stop ends the turn")
        feed(tracker, event("errorOccurred", ["recoverable": true, "error": ["message": "x"]]))
        suite.expect(snapshot(tracker).sessions[0].phase == .finished, "an error it recovers from changes nothing")
        feed(tracker, event("sessionEnd", ["reason": "user_exit"]))
        suite.expect(snapshot(tracker).sessions.isEmpty, "the session's end takes it out")
    }

    // MARK: Cursor

    static func cursor(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func event(_ name: String, _ extra: [String: Any] = [:]) -> Data {
            envelope("cursor", hook: name, ["conversation_id": "k1", "generation_id": "gen", "hook_event_name": name,
                                            "workspace_roots": ["/w/site"], "model": "composer-2"].merging(extra) { $1 })
        }
        feed(tracker, event("sessionStart", ["session_id": "k1"]))
        feed(tracker, event("beforeSubmitPrompt", ["prompt": "make it pink"]))
        feed(tracker, event("afterAgentThought", ["text": "thinking"]))
        var listed = snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .cursor && listed[0].phase == .generating
                     && listed[0].project == "site" && listed[0].model == "composer-2",
                     "a Cursor conversation lists by its workspace, with its model: \(listed)")
        feed(tracker, event("postToolUse", ["tool_name": "Shell", "tool_input": ["command": "npm run build"], "duration": 1200,
                                            "tool_use_id": "t1"]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 1 && listed[0].timeline[0].name == "Bash" && listed[0].timeline[0].finished != nil
                     && listed[0].phase == .generating,
                     "a tool Cursor tells of once it ran joins the timeline, done")
        feed(tracker, event("afterAgentResponse", ["text": "Done, it is pink."]))
        feed(tracker, event("stop", ["status": "completed", "loop_count": 0]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Done, it is pink.",
                     "the reply before the stop is the one the turn ended on")
        let ago = now.addingTimeInterval(AgentHookSessions.quietFor + 60)
        tracker.prune(now: ago) { _ in true }
        suite.expect(snapshot(tracker, at: ago).sessions.isEmpty,
                     "a Cursor conversation long quiet leaves, since the app outlives its conversations")
        let command = AgentHookInstaller.command(.cursor, event: "beforeSubmitPrompt", socket: "/s", bundle: "b")
        suite.expect(command.contains("printf '{\"continue\":true}'"),
                     "Cursor's prompt hook answers that the prompt goes on, island or not")
        suite.expect(!AgentHookInstaller.cursorEvents.contains("beforeShellExecution")
                     && !AgentHookInstaller.cursorEvents.contains("preToolUse")
                     && !AgentHookInstaller.cursorEvents.contains("beforeMCPExecution"),
                     "no hook that may allow or deny a tool is installed into Cursor")
    }

    // MARK: OpenCode

    static func opencode(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        func event(_ fields: [String: Any]) -> Data {
            envelope("opencode", ["directory": "/w/cli", "at": 1_800_000_000_000.0].merging(fields) { $1 })
        }
        feed(tracker, event(["kind": "created", "session": "ses_1"]))
        feed(tracker, event(["kind": "prompt", "session": "ses_1", "text": "Run ls"]))
        feed(tracker, event(["kind": "status", "session": "ses_1", "status": "busy"]))
        feed(tracker, event(["kind": "message", "session": "ses_1", "id": "msg_1", "model": "big-pickle", "cost": 0.012,
                             "tokens": ["input": 100, "output": 20, "reasoning": 5, "cache": ["read": 50, "write": 10]]]))
        feed(tracker, event(["kind": "tool", "session": "ses_1", "call": "call_1", "tool": "bash", "status": "running",
                             "input": ["command": "ls", "description": "List"]]))
        feed(tracker, event(["kind": "tool", "session": "ses_1", "call": "call_1", "tool": "bash", "status": "running",
                             "input": ["command": "ls", "description": "List"]]))
        var listed = snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].provider == .opencode && listed[0].phase == .runningTool
                     && listed[0].timeline.count == 1 && listed[0].model == "big-pickle"
                     && listed[0].tokens.output == 25 && listed[0].tokens.cacheRead == 50 && abs(listed[0].cost - 0.012) < 1e-9,
                     "OpenCode's tools, model, tokens and cost come through, a repeated part once: \(listed)")
        let ask = event(["kind": "permission", "session": "ses_1", "id": "per_1", "permission": "bash",
                         "patterns": ["rm -rf build"], "metadata": ["command": "rm -rf build"], "always": ["rm *"]])
        let decoded = AgentHookMessage.decode(ask, now: now, policy: off)
        suite.expect(decoded?.permission?.request == "per_1" && decoded?.permission?.session == "ses_1",
                     "a permission names the request the answer goes to")
        feed(tracker, ask, policy: off)
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.requestID == "per_1"
                     && listed[0].permission?.detail == "rm -rf build" && listed[0].permission?.offersAlways == true
                     && listed[0].waitingFor == nil,
                     "OpenCode's request is answered in the island, read whole even with previews off")
        suite.expect(AgentHookMessage.opencodeReply(.allow) == Data(#"{"reply":"once"}"#.utf8)
                     && AgentHookMessage.opencodeReply(.allowAlways) == Data(#"{"reply":"always"}"#.utf8)
                     && AgentHookMessage.opencodeReply(.deny) == Data(#"{"reply":"reject"}"#.utf8)
                     && AgentHookMessage.opencodeReply(.answer([:])) == nil,
                     "allow, always and deny are OpenCode's once, always and reject")
        tracker.resolve("opencode:ses_1", request: "per_other")
        suite.expect(snapshot(tracker).sessions[0].permission != nil, "an answer to another request leaves this one")
        feed(tracker, event(["kind": "permissionReplied", "session": "ses_1", "id": "per_1"]))
        suite.expect(snapshot(tracker).sessions[0].phase == .runningTool, "answered in OpenCode, the wait ends")
        feed(tracker, event(["kind": "tool", "session": "ses_1", "call": "call_1", "tool": "bash", "status": "completed"]))
        feed(tracker, event(["kind": "reply", "session": "ses_1", "text": "Here is the list."]))
        feed(tracker, event(["kind": "status", "session": "ses_1", "status": "idle"]))
        listed = snapshot(tracker).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Here is the list."
                     && listed[0].chat.last?.endsTurn == true,
                     "idle ends the turn on its last reply")
        suite.expect(AgentHookMessage.decode(event(["kind": "listen"]), now: now, policy: on)?.listens == true,
                     "the plugin's listening connection is known as such")
        let prompt = AgentHookMessage.opencodePrompt(session: "ses_1", text: "and now ls -a")
        let parsed = prompt.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        suite.expect((parsed?["prompt"] as? [String: String]) == ["session": "ses_1", "text": "and now ls -a"],
                     "a reply goes to the plugin by session")
        feed(tracker, event(["kind": "deleted", "session": "ses_1"]))
        suite.expect(snapshot(tracker).sessions.isEmpty, "a deleted session leaves")
        let source = AgentHookInstaller.opencodeSource(AgentOpenCodePlugin.source, socket: "/Users/a b/it's.sock", bundle: "x.y")
        suite.expect(source.contains(#"const SOCKET = "/Users/a b/it's.sock""#) && source.contains("# openbar-hook x.y")
                     && !source.contains("__OPENBAR"),
                     "the plugin has the socket and the marker written in")
    }

    // MARK: Lifetime

    static func lifetime(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        feed(tracker, envelope("gemini", pid: 11, ["hook_event_name": "SessionStart", "session_id": "a", "cwd": "/w/a"]))
        feed(tracker, envelope("copilot", hook: "sessionStart", pid: 12, ["sessionId": "b", "cwd": "/w/b"]))
        suite.expect(snapshot(tracker).sessions.count == 2, "both sessions are listed")
        tracker.prune(now: now) { $0 == 12 }
        let left = snapshot(tracker).sessions.map(\.id)
        suite.expect(left == ["copilot:b"], "a session whose agent quit leaves without a ghost row: \(left)")
        tracker.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor * 3)) { _ in true }
        suite.expect(snapshot(tracker).sessions.count == 1, "a CLI session at rest stays while its agent runs")
        suite.expect(AgentHookSessions.isAlive(getpid()) && !AgentHookSessions.isAlive(Int32.max - 1),
                     "a process is alive while it runs")
        let pid = AgentHookProcess.agent(fromHook: getpid(), hint: 77)
        suite.expect(pid == getpid(), "a process that is no shell is the agent itself")
        suite.expect(AgentHookProcess.agent(fromHook: 0, hint: 77) == 77, "without a peer the hook's hint stands")
    }

    // MARK: Previews

    static func previews(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        feed(tracker, envelope("gemini", ["hook_event_name": "BeforeAgent", "session_id": "p", "cwd": "/w/p", "prompt": "secret"]),
             policy: off)
        feed(tracker, envelope("gemini", ["hook_event_name": "BeforeTool", "session_id": "p", "tool_name": "run_shell_command",
                                          "tool_input": ["command": "cat .env"]]), policy: off)
        feed(tracker, envelope("gemini", ["hook_event_name": "AfterAgent", "session_id": "p", "prompt_response": "the key is"]),
             policy: off)
        let listed = snapshot(tracker, previews: false).sessions
        suite.expect(listed[0].chat.isEmpty && listed[0].lastText == nil && listed[0].timeline.allSatisfy { $0.summary == nil },
                     "with previews off no prompt, reply or tool argument is kept")
        let shown = AgentHookSessions()
        feed(shown, envelope("gemini", ["hook_event_name": "BeforeAgent", "session_id": "p", "prompt": "hello"]))
        suite.expect(snapshot(shown, previews: false).sessions[0].chat.isEmpty, "turning previews off hides what was read")
        shown.purgeContent()
        suite.expect(snapshot(shown, previews: true).sessions[0].chat.isEmpty, "and forgets it")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-hook-agents-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let none: [String: String] = [:]
        // Gemini: beside the person's own hooks, with a backup.
        let gemini = AgentHookInstaller.geminiSettings(home: home, environment: none)
        try? FileManager.default.createDirectory(at: gemini.deletingLastPathComponent(), withIntermediateDirectories: true)
        let own: [String: Any] = ["theme": "dark", "hooks": ["BeforeTool": [["matcher": "write_file",
                                                                            "hooks": [["type": "command", "command": "mine.sh"]]]]]]
        try? JSONSerialization.data(withJSONObject: own).write(to: gemini)
        suite.expect(AgentHookInstaller.state(.gemini, home: home, socket: socket, bundle: bundle) == .missing,
                     "nothing is in before Install")
        try? AgentHookInstaller.install(.gemini, home: home, socket: socket, bundle: bundle)
        let installed = (try? ClaudeHookInstaller.read(gemini)) ?? [:]
        let tools = (installed["hooks"] as? [String: Any])?["BeforeTool"] as? [[String: Any]] ?? []
        suite.expect(installed["theme"] as? String == "dark" && tools.count == 2
                     && (tools[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String == "mine.sh",
                     "the island's hook goes beside the person's own, which stays first: \(tools)")
        suite.expect(AgentHookInstaller.state(.gemini, home: home, socket: socket, bundle: bundle) == .installed,
                     "every event holds the island's hook")
        suite.expect(AgentHookInstaller.state(.gemini, home: home, socket: "/elsewhere.sock", bundle: bundle) == .outdated,
                     "hooks for another socket need an update")
        let backups = ((try? FileManager.default.contentsOfDirectory(atPath: gemini.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasSuffix(".bak") }
        suite.expect(backups.count == 1, "the settings were copied aside first: \(backups)")
        try? AgentHookInstaller.remove(.gemini, home: home, bundle: bundle)
        let removed = (try? ClaudeHookInstaller.read(gemini)) ?? [:]
        suite.expect(NSDictionary(dictionary: removed).isEqual(to: own), "Remove leaves the settings as they were: \(removed)")

        // Cursor: one list per event.
        let cursor = AgentHookInstaller.cursorHooks(home: home)
        try? FileManager.default.createDirectory(at: cursor.deletingLastPathComponent(), withIntermediateDirectories: true)
        let cursorOwn: [String: Any] = ["version": 1, "hooks": ["stop": [["command": "./audit.sh"]]]]
        try? JSONSerialization.data(withJSONObject: cursorOwn).write(to: cursor)
        try? AgentHookInstaller.install(.cursor, home: home, socket: socket, bundle: bundle)
        let cursorIn = (try? ClaudeHookInstaller.read(cursor)) ?? [:]
        let stops = (cursorIn["hooks"] as? [String: Any])?["stop"] as? [[String: Any]] ?? []
        suite.expect(stops.count == 2 && stops[0]["command"] as? String == "./audit.sh"
                     && AgentHookInstaller.state(.cursor, home: home, socket: socket, bundle: bundle) == .installed,
                     "Cursor's hooks go after the person's own: \(stops)")
        try? AgentHookInstaller.install(.cursor, home: home, socket: socket, bundle: bundle)
        let again = ((try? ClaudeHookInstaller.read(cursor))?["hooks"] as? [String: Any])?["stop"] as? [[String: Any]] ?? []
        suite.expect(again.count == 2, "installing twice adds nothing twice")
        try? AgentHookInstaller.remove(.cursor, home: home, bundle: bundle)
        suite.expect(NSDictionary(dictionary: (try? ClaudeHookInstaller.read(cursor)) ?? [:]).isEqual(to: cursorOwn),
                     "Remove takes out only the island's")

        // Copilot and OpenCode: a file of the island's own.
        let copilot = AgentHookInstaller.copilotHooks(home: home, bundle: bundle, environment: none)
        try? AgentHookInstaller.install(.copilot, home: home, socket: socket, bundle: bundle)
        let file = (try? Data(contentsOf: copilot)).flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        let events = Set(((file?["hooks"] as? [String: Any]) ?? [:]).keys)
        suite.expect(file?["version"] as? Int == 1 && events == Set(AgentHookInstaller.copilotEvents)
                     && !events.contains("permissionRequest"),
                     "Copilot gets a hooks file of its own, with no permission hook: \(events)")
        suite.expect(AgentHookInstaller.state(.copilot, home: home, socket: socket, bundle: bundle) == .installed,
                     "the file is known as the island's")
        let plugin = AgentHookInstaller.opencodePlugin(home: home, bundle: bundle, environment: none)
        try? FileManager.default.createDirectory(at: plugin.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data("export const Mine = async () => ({})\n".utf8).write(to: plugin)
        var refused = false
        do { try AgentHookInstaller.install(.opencode, home: home, socket: socket, bundle: bundle) } catch { refused = true }
        suite.expect(refused, "a file that is not the island's is never written over")
        try? FileManager.default.removeItem(at: plugin)
        try? AgentHookInstaller.install(.opencode, home: home, socket: socket, bundle: bundle)
        suite.expect(AgentHookInstaller.state(.opencode, home: home, socket: socket, bundle: bundle) == .installed,
                     "OpenCode's plugin goes in")
        AgentHookInstaller.removeAll(home: home, bundle: bundle)
        suite.expect(!FileManager.default.fileExists(atPath: copilot.path) && !FileManager.default.fileExists(atPath: plugin.path),
                     "uninstalling takes the island's own files away")
        suite.expect(AgentHookInstaller.copilotHooks(home: home, bundle: "com.openbar.app", environment: none)
                     != AgentHookInstaller.copilotHooks(home: home, bundle: bundle, environment: none),
                     "the release and the Developer build never share a file")
        suite.expect(AgentHookInstaller.geminiSettings(home: home, environment: ["GEMINI_CLI_HOME": "/g"]).path
                     == "/g/.gemini/settings.json"
                     && AgentHookInstaller.copilotHooks(home: home, bundle: bundle, environment: ["COPILOT_HOME": "/c"]).path
                     == "/c/hooks/openbar-\(bundle).json",
                     "the agents' own home settings are followed")

        // The hook itself.
        let command = AgentHookInstaller.command(.gemini, event: "BeforeTool", socket: socket, bundle: bundle)
        suite.expect(command.contains("/usr/bin/nc -U '/tmp/island'\\''s.sock'") && command.hasSuffix("# openbar-hook \(bundle)")
                     && command.contains("exit 0"),
                     "the hook quotes the socket, never fails the agent, and carries the marker: \(command)")
    }

    // MARK: Strip

    static func strip(_ suite: TestSuite) {
        let counts = ClaudeSessionCounts(working: 4, workingOthers: [.gemini: 1, .opencode: 2], waiting: 1)
        let reading = NotchClaudeSessionsSupport.stripReading(counts)
        suite.expect(reading == [.init(phase: .waitingPermission, count: 1), .init(phase: .generating, count: 1, provider: .claude),
                                 .init(phase: .generating, count: 1, provider: .gemini),
                                 .init(phase: .generating, count: 2, provider: .opencode)],
                     "each agent at work gets its own count, in the agents' order: \(reading)")
        suite.expect(NotchClaudeSessionsSupport.stripAgents(counts) == [.claude, .gemini]
                     && NotchClaudeSessionsSupport.stripAgents(ClaudeSessionCounts(working: 1, workingOthers: [.cursor: 1]))
                     == [.cursor],
                     "two marks stack at most, the agents' order deciding")
        var sessions = [NotchSessionAgentTests.session("a", .generating, provider: .copilot),
                        NotchSessionAgentTests.session("b", .runningTool, provider: .copilot),
                        NotchSessionAgentTests.session("c", .generating, provider: .claude)]
        sessions.append(NotchSessionAgentTests.session("d", .finished, provider: .cursor))
        let derived = ClaudeSessionCounts(sessions)
        suite.expect(derived.working == 3 && derived.working(.copilot) == 2 && derived.working(.claude) == 1
                     && derived.finished == 1,
                     "counts follow each session's agent")
        var cursor = NotchSessionAgentTests.session("k", .idle, provider: .cursor)
        cursor.host = .ide(bundle: "com.todesktop.230313mzl4w4u92", port: nil)
        suite.expect(!NotchClaudeSessionsSupport.canReply(cursor), "Cursor's app keeps its chat; no reply is typed into it")
        cursor.host = .terminal(tty: "/dev/ttys004")
        suite.expect(NotchClaudeSessionsSupport.canReply(cursor), "cursor-agent in a terminal takes replies there")
        var open = NotchSessionAgentTests.session("o", .idle, provider: .opencode)
        open.host = .unknown
        suite.expect(NotchClaudeSessionsSupport.canReply(open), "OpenCode takes replies through its plugin, wherever it runs")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.answerIn(.gemini) == "Answer this one in Gemini CLI." && text.answerIn(.codex) == text.answerInTerminal
                     && text.replyPlaceholder(for: .copilot) == "Reply to Copilot CLI…",
                     "the island names the agent where it sends the person")
        suite.expect(ClaudeSessionProvider.allCases.map(\.character).count == Set(ClaudeSessionProvider.allCases.map(\.character)).count,
                     "every agent has a figure of its own")
    }

    // MARK: Branch

    static func branch(_ suite: TestSuite) {
        let root = FileManager.default.temporaryDirectory.appending(path: "vs-branch-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let nested = root.appending(path: "repo/Sources/App")
        try? FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: root.appending(path: "repo/.git"), withIntermediateDirectories: true)
        try? Data("ref: refs/heads/feat/agents\n".utf8).write(to: root.appending(path: "repo/.git/HEAD"))
        suite.expect(AgentGitBranch.read(nested.path) == "feat/agents", "a folder deep in a repository reads its branch")
        let worktree = root.appending(path: "wt")
        try? FileManager.default.createDirectory(at: worktree, withIntermediateDirectories: true)
        let gitdir = root.appending(path: "repo/.git/worktrees/wt")
        try? FileManager.default.createDirectory(at: gitdir, withIntermediateDirectories: true)
        try? Data("ref: refs/heads/fix/x\n".utf8).write(to: gitdir.appending(path: "HEAD"))
        try? Data("gitdir: \(gitdir.path)\n".utf8).write(to: worktree.appending(path: ".git"))
        suite.expect(AgentGitBranch.read(worktree.path) == "fix/x", "a worktree reads its own branch")
        try? Data("0123456789abcdef\n".utf8).write(to: root.appending(path: "repo/.git/HEAD"))
        suite.expect(AgentGitBranch.read(nested.path) == nil, "a detached head has no branch")
    }

    // MARK: New session

    static func newSession(_ suite: TestSuite) {
        suite.expect(AgentCLI.startArguments(.gemini, prompt: "fix it") == ["-i", "fix it"]
                     && AgentCLI.startArguments(.copilot, prompt: "fix it") == ["-i", "fix it"]
                     && AgentCLI.startArguments(.opencode, prompt: "fix it") == ["--prompt", "fix it"]
                     && AgentCLI.startArguments(.cursor, prompt: "fix it") == ["fix it"]
                     && AgentCLI.startArguments(.gemini, prompt: "") == [] && AgentCLI.startArguments(.claude, prompt: "x") == [],
                     "each agent opens with the first prompt already sent, by its own flag")
        let home = URL(fileURLWithPath: "/Users/someone")
        let found = AgentCLI.locate(.opencode, path: "/usr/bin:/opt/tools", home: home) {
            $0 == "/Users/someone/.opencode/bin/opencode" || $0 == "/opt/tools/opencode"
        }
        suite.expect(found == "/opt/tools/opencode", "the PATH comes before the installers' folders")
        suite.expect(AgentCLI.locate(.cursor, path: nil, home: home) { $0 == "/Users/someone/.local/bin/cursor-agent" }
                     == "/Users/someone/.local/bin/cursor-agent"
                     && AgentCLI.locate(.gemini, path: nil, home: home) { _ in false } == nil
                     && AgentCLI.locate(.codex, path: "/bin", home: home) { _ in true } == nil,
                     "cursor-agent is found where its installer puts it, and an agent not installed is skipped")
        let line = CodexCLI.line(executable: "/opt/tools/gemini", arguments: AgentCLI.startArguments(.gemini, prompt: "it's done"),
                                 folder: "/w/my app")
        suite.expect(line == "cd '/w/my app' && '/opt/tools/gemini' '-i' 'it'\\''s done'", "the terminal line quotes everything: \(line)")
    }

    // MARK: Marks

    static func marks(_ suite: TestSuite) {
        for provider in ClaudeSessionProvider.hookAgents {
            let layers = NotchHookAgentMarkPaths.mark(provider)?.layers ?? []
            let boxes = layers.map { $0.path.boundingBoxOfPath }
            // A mark of several colors (Pi's i, Mistral's rows) has narrow parts; together they fill the square.
            let whole = boxes.reduce(CGRect.null) { $0.union($1) }
            suite.expect(!layers.isEmpty && boxes.allSatisfy { !$0.isEmpty && $0.minX >= -0.01 && $0.minY >= -0.01
                                                                && $0.maxX <= 24.01 && $0.maxY <= 24.01 && $0.width > 4 }
                         && whole.width > 12 && whole.height > 12,
                         "\(provider)'s own mark parses and fills its 24-point square: \(boxes)")
        }
    }

    // MARK: Automatic

    static func automatic(_ suite: TestSuite) {
        let all = Set(ClaudeSessionProvider.hookAgents)
        var states: [ClaudeSessionProvider: AgentHookInstaller.State] =
            [.gemini: .missing, .copilot: .outdated, .opencode: .installed, .cursor: .unreadable]
        var plan = AgentHookInstaller.plan(found: [.gemini, .copilot, .opencode, .cursor], shown: all, states: states)
        suite.expect(plan.install == [.gemini, .copilot] && plan.remove.isEmpty,
                     "found agents get their hooks or an update, an unreadable file is left alone: \(plan)")
        plan = AgentHookInstaller.plan(found: [], shown: all, states: [.gemini: .missing])
        suite.expect(plan.install.isEmpty && plan.remove.isEmpty, "an agent not found gets nothing")
        states = [.gemini: .installed, .copilot: .outdated, .opencode: .missing, .cursor: .unreadable]
        plan = AgentHookInstaller.plan(found: all, shown: [], states: states)
        suite.expect(plan.remove == [.gemini, .copilot] && plan.install.isEmpty,
                     "an agent switched off has the island's hooks taken out, found or not: \(plan)")
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(!AgentHookInstaller.isPresent(.gemini, home: home, path: "/usr/bin", isExecutable: { _ in false },
                                                   exists: { $0.hasSuffix(".gemini") || $0.hasSuffix(".copilot") })
                     && !AgentHookInstaller.isPresent(.copilot, home: home, path: nil, isExecutable: { _ in false }, exists: { _ in true }),
                     "a folder another app made is not the agent")
        suite.expect(AgentHookInstaller.isPresent(.gemini, home: home, path: "/opt/bin", isExecutable: { $0 == "/opt/bin/gemini" },
                                                  exists: { _ in false })
                     && AgentHookInstaller.isPresent(.cursor, home: home, path: nil, isExecutable: { _ in false },
                                                     exists: { $0 == "/Applications/Cursor.app" })
                     && !AgentHookInstaller.isPresent(.copilot, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { _ in false }),
                     "an agent is found by its command, or Cursor by its app")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentAutoState(.installed, present: true, shown: true) == "Connected"
                     && text.agentAutoState(.missing, present: false, shown: true) == text.codexNotFound
                     && text.agentAutoState(.missing, present: true, shown: false) == "Off",
                     "the row says what happened, with no button to press")
    }
}
