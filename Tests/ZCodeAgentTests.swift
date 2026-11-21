// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// ZCode in the Sessions list (#245): what its hooks send, the sessions kept
/// from them, the permission and question its hook holds for the island's
/// answer, the island's entries in its CLI config and `hooks.enabled`, and
/// how it is found and named.
enum ZCodeAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("zcode: hooks") { hooks(suite) }
        suite.run("zcode: permissions") { permissions(suite) }
        suite.run("zcode: questions") { questions(suite) }
        suite.run("zcode: answers") { answers(suite) }
        suite.run("zcode: installer") { installer(suite) }
        suite.run("zcode: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    /// The fields ZCode's hook input carries for every event: its own
    /// camelCase ones and Claude Code's snake_case copies
    /// (core/src/hooks/configured-runner-input.ts).
    static func common(_ event: String, _ extra: [String: Any] = [:]) -> [String: Any] {
        var json: [String: Any] = [
            "hookEventName": event, "hook_event_name": event, "sessionId": "ses_7f3a", "session_id": "ses_7f3a",
            "agentName": "build", "agent_type": "build", "mode": "build", "permission_mode": "build",
            "cwd": "/w/rocket", "timestamp": 1_795_197_421_000, "traceId": "tr_1",
            "transcriptPath": "/tmp/zcode-hook-a1/transcript.jsonl", "transcript_path": "/tmp/zcode-hook-a1/transcript.jsonl",
        ]
        for (key, value) in extra { json[key] = value }
        return json
    }

    static func tool(_ name: String, _ call: String, _ input: [String: Any]) -> [String: Any] {
        ["toolName": name, "toolInput": input, "toolCallId": call, "tool_name": name, "tool_input": input, "tool_use_id": call]
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, _ extra: [String: Any] = [:],
                     policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, T.envelope("zcode", hook: event, pid: 9200, common(event, extra)), policy: policy, at: date)
    }

    static let shell = tool("Bash", "call_1", ["command": "rm -rf build", "description": "Clean the build"])

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "SessionStart", ["source": "startup", "model": "mock/glm-5.3"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "zcode:ses_7f3a" && listed[0].provider == .zcode
                     && listed[0].project == "rocket" && listed[0].phase == .idle && listed[0].model == "glm-5.3",
                     "a session start lists it idle in its folder, the model without its provider: \(listed)")
        send(tracker, "UserPromptSubmit", ["prompt": "clean up"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["clean up"],
                     "a prompt opens a turn, its text with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", ["prompt": "secret plan"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first?.chat.isEmpty == true,
                     "without previews the prompt isn't kept")
        send(tracker, "PreToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "rm -rf build",
                     "its Bash runs, summed up by the command: \(listed[0].currentTool as Any)")
        send(tracker, "PostToolUse", shell.merging(["tool_response": ["stdout": "ok"]]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed, "the tool ends by its call id")
        let patch = tool("ApplyPatch", "call_2", ["patch": "*** Begin Patch"])
        send(tracker, "PreToolUse", patch)
        send(tracker, "PostToolUseFailure", patch.merging(["error": "no match", "is_interrupt": false]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 2 && listed[0].timeline[1].name == "Edit" && listed[0].timeline[1].failed,
                     "a failed patch shows as a failed edit: \(listed[0].timeline)")
        send(tracker, "Stop", ["responsePreview": "Clean", "responseText": "Cleaned.", "last_assistant_message": "Cleaned.",
                               "stopHookActive": false, "stop_hook_active": false, "toolCallCount": 2],
             at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Cleaned.",
                     "Stop ends the turn on its last message: \(listed[0].phase)")
        // ZCode has no hook at its end: the row goes with its process.
        tracker.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "no ghost row once ZCode's process is gone")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let envelope = T.envelope("zcode", hook: "PermissionRequest", pid: 9200,
                                  common("PermissionRequest", shell.merging(["requestId": "perm_1", "reason": "shell"]) { $1 }))
        suite.expect(AgentHookMessage.waits(envelope), "its permission hook waits for the island's answer")
        suite.expect(!AgentHookMessage.waits(T.envelope("zcode", hook: "PreToolUse", common("PreToolUse", shell))),
                     "every other hook is let go at once")
        let decoded = AgentHookMessage.decode(envelope, now: now, policy: T.off)
        guard let decoded, case .heldPermission(let asked)? = decoded.events.first?.kind else {
            return suite.expect(false, "a permission request is held: \(decoded?.events as Any)")
        }
        suite.expect(decoded.permission?.session == "ses_7f3a" && decoded.permission?.request == "call_1"
                     && asked.requestID == "call_1" && asked.tool == "Bash" && asked.detail == "rm -rf build"
                     && asked.content?.complete == true && !asked.offersAlways && decoded.input == nil,
                     "the request is the tool call's, its command shown whatever the previews: \(asked)")
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "clean"])
        send(tracker, "PreToolUse", shell)
        T.feed(tracker, envelope)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.requestID == "call_1",
                     "the session waits on the island's card: \(listed[0].phase)")
        // Answered in ZCode first: the tool runs, and its PostToolUse settles the card.
        let settled = AgentHookMessage.decode(T.envelope("zcode", hook: "PostToolUse", common("PostToolUse", shell)),
                                              now: now, policy: T.on)
        suite.expect(settled?.events.first?.kind == .permissionSettled("call_1"), "a tool that ran settles its request")
        send(tracker, "PostToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "a tool that ran was let through")
        // An edit's request carries its diff, as Claude Code's does.
        let edit = AgentHookMessage.decode(T.envelope("zcode", hook: "PermissionRequest", common("PermissionRequest",
            tool("Edit", "call_3", ["file_path": "/w/rocket/a.ts", "old_string": "let a = 1", "new_string": "let a = 2"]))),
                                           now: now, policy: T.on)
        if case .heldPermission(let change)? = edit?.events.first?.kind {
            suite.expect(change.tool == "Edit" && !change.diffs.isEmpty, "an edit shows what it changes: \(change.diffs)")
        } else {
            suite.expect(false, "an edit's request is held")
        }
    }

    // MARK: Questions

    static let asking: [String: Any] = tool("AskUserQuestion", "call_q", ["questions": [[
        "question": "Which database?", "header": "DB", "multiSelect": false,
        "options": [["label": "Postgres", "description": "Relational"], ["label": "SQLite", "description": "One file"]],
    ]]])

    static func questions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "pick"])
        // A question's PreToolUse isn't a tool run: its request tells it.
        send(tracker, "PreToolUse", asking)
        suite.expect(T.snapshot(tracker).sessions[0].currentTool == nil, "a question doesn't run as a tool")
        let envelope = T.envelope("zcode", hook: "PermissionRequest", common("PermissionRequest", asking))
        let decoded = AgentHookMessage.decode(envelope, now: now, policy: T.on)
        guard let decoded, case .heldPermission(let asked)? = decoded.events.first?.kind else {
            return suite.expect(false, "a question is held")
        }
        suite.expect(asked.asksQuestions && asked.questions.first?.question == "Which database?"
                     && asked.questions.first?.options.map(\.label) == ["Postgres", "SQLite"]
                     && (decoded.input?["questions"] as? [Any])?.count == 1,
                     "the question shows with its options, its input kept for the answers: \(asked)")
        T.feed(tracker, envelope)
        suite.expect(T.snapshot(tracker).sessions[0].phase == .waitingPermission, "the session waits on the question")
        send(tracker, "PostToolUse", asking)
        let listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].timeline.isEmpty, "answered, it settles without a tool row")
        // A question the island can't show is answered in ZCode.
        let bare = AgentHookMessage.decode(T.envelope("zcode", hook: "PermissionRequest", common("PermissionRequest",
            tool("AskUserQuestion", "call_r", ["questions": "?"]))), now: now, policy: T.on)
        suite.expect(bare?.events.first?.kind == .question && bare?.permission == nil, "an unreadable question waits in ZCode")
    }

    // MARK: Answers

    static func json(_ data: Data?) -> [String: Any] {
        data.flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
    }

    static func answers(_ suite: TestSuite) {
        func decision(_ data: Data?) -> [String: Any] {
            ((json(data)["hookSpecificOutput"] as? [String: Any])?["decision"] as? [String: Any]) ?? [:]
        }
        let allow = json(AgentHookMessage.zcodeReply(.allow))["hookSpecificOutput"] as? [String: Any]
        suite.expect(allow?["hookEventName"] as? String == "PermissionRequest"
                     && (allow?["decision"] as? [String: Any])?["behavior"] as? String == "allow",
                     "Allow prints a PermissionRequest decision ZCode takes: \(allow as Any)")
        suite.expect(decision(AgentHookMessage.zcodeReply(.allowAlways))["behavior"] as? String == "allow"
                     && decision(AgentHookMessage.zcodeReply(.allowAlways))["updatedPermissions"] == nil,
                     "Always allows this one, with no rules ZCode would read otherwise")
        let deny = decision(AgentHookMessage.zcodeReply(.deny))
        suite.expect(deny["behavior"] as? String == "deny" && deny["message"] as? String == ClaudeHookMessage.denyMessage,
                     "Deny tells the model it was denied here")
        let input = asking["tool_input"] as? [String: Any]
        let answered = decision(AgentHookMessage.zcodeReply(.answer(["Which database?": ["Postgres", "SQLite"]]), input: input))
        let updated = answered["updatedInput"] as? [String: Any]
        suite.expect(answered["behavior"] as? String == "allow" && (updated?["questions"] as? [Any])?.count == 1
                     && (updated?["answers"] as? [String: String]) == ["Which database?": "Postgres, SQLite"],
                     "answers go back in the question's input, each as one text: \(answered)")
        suite.expect(AgentHookMessage.zcodeReply(.answer(["Which database?": ["SQLite"]])) == nil,
                     "no answers without the question's input")
        suite.expect(decision(AgentHookMessage.zcodeReply(.deny, input: input))["message"] as? String == ClaudeHookMessage.skipMessage,
                     "skipping a question tells it was skipped")
        suite.expect(AgentHookMessage.zcodeModel("zai/glm-5.3") == "glm-5.3" && AgentHookMessage.zcodeModel("glm-5") == "glm-5"
                     && AgentHookMessage.zcodeModel("openrouter/anthropic/claude") == "anthropic/claude"
                     && AgentHookMessage.zcodeModel("x/") == nil && AgentHookMessage.zcodeModel("") == nil,
                     "the model loses only its provider")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-zcode-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentHookInstaller.zcodeConfig(home: home)
        suite.expect(url.path.hasSuffix("/.zcode/cli/config.json")
                     && AgentHookInstaller.file(.zcode, home: home, bundle: bundle) == url, "its CLI config")
        suite.expect(AgentHookInstaller.state(.zcode, home: home, socket: socket, bundle: bundle) == .missing, "no file, no hooks")

        // No file before: Remove leaves none of the island's keys behind.
        try? AgentHookInstaller.install(.zcode, home: home, socket: socket, bundle: bundle)
        var settings = (try? ClaudeHookInstaller.read(url)) ?? [:]
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        let events = hooks["events"] as? [String: Any] ?? [:]
        let pre = (events["PreToolUse"] as? [[String: Any]])?.first
        let start = (events["SessionStart"] as? [[String: Any]])?.first
        let permission = ((events["PermissionRequest"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
        let handler = (pre?["hooks"] as? [[String: Any]])?.first
        suite.expect(hooks["enabled"] as? Bool == true && Set(events.keys) == Set(AgentHookInstaller.zcodeEvents)
                     && pre?["matcher"] as? String == "*" && start?["matcher"] == nil
                     && handler?["type"] as? String == "command" && handler?["timeoutMs"] as? Int == AgentHookInstaller.eventTimeout * 1000
                     && handler?["timeout"] == nil
                     && permission?["timeoutMs"] as? Int == ClaudeHookInstaller.permissionTimeout * 1000
                     && (handler?["command"] as? String)?.contains(#"{"openbar":"zcode","hook":"PreToolUse""#) == true
                     && (handler?["command"] as? String)?.contains(" >/dev/null") == false
                     && (handler?["command"] as? String)?.hasSuffix("# openbar-hook com.openbar.app.dev") == true,
                     "every event under hooks.events, tools matched, hooks on, the permission's wait long: \(hooks)")
        suite.expect(AgentHookInstaller.state(.zcode, home: home, socket: socket, bundle: bundle) == .installed, "it reads as installed")
        suite.expect(AgentHookInstaller.state(.zcode, home: home, socket: "/tmp/other.sock", bundle: bundle) == .outdated,
                     "another socket is older")
        try? AgentHookInstaller.remove(.zcode, home: home, bundle: bundle)
        settings = (try? ClaudeHookInstaller.read(url)) ?? [:]
        suite.expect(settings["hooks"] == nil, "Remove takes the island's entries and the switch it turned on: \(settings)")

        // The person's own config: their keys and hooks stay, hooks stay on for them.
        let other = home.appending(path: "other")
        let mine = AgentHookInstaller.zcodeConfig(home: other)
        try? manager.createDirectory(at: mine.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(#"{"mcp":{"servers":{}},"hooks":{"timeoutMs":30000,"events":{"Stop":[{"hooks":[{"type":"process","command":"say","args":["done"]}]}]}}}"#.utf8)
            .write(to: mine)
        try? AgentHookInstaller.install(.zcode, home: other, socket: socket, bundle: bundle)
        settings = (try? ClaudeHookInstaller.read(mine)) ?? [:]
        hooks = settings["hooks"] as? [String: Any] ?? [:]
        let stops = (hooks["events"] as? [String: Any])?["Stop"] as? [[String: Any]] ?? []
        let backups = ((try? manager.contentsOfDirectory(atPath: mine.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasPrefix("config.json.openbar-") }
        suite.expect(settings["mcp"] != nil && hooks["timeoutMs"] as? Int == 30000 && hooks["enabled"] as? Bool == true
                     && stops.count == 2 && ((stops[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String) == "say"
                     && backups.contains("config.json.openbar-original.bak"),
                     "the island's entries come after the person's, a copy kept first: \(hooks) \(backups)")
        try? AgentHookInstaller.remove(.zcode, home: other, bundle: bundle)
        settings = (try? ClaudeHookInstaller.read(mine)) ?? [:]
        hooks = settings["hooks"] as? [String: Any] ?? [:]
        suite.expect(settings["mcp"] != nil && hooks["timeoutMs"] as? Int == 30000 && hooks["enabled"] as? Bool == true
                     && ((hooks["events"] as? [String: Any])?["Stop"] as? [[String: Any]])?.count == 1
                     && !ClaudeHookInstaller.hasAny(in: AgentHookInstaller.zcodeGroups(settings), bundle: bundle),
                     "Remove leaves the person's hook, and hooks on for it: \(hooks)")

        // Hooks that were off stay off once the island's are gone.
        let before: [String: Any] = ["hooks": ["enabled": false, "events": [:] as [String: Any]]]
        let installed = AgentHookInstaller.zcodeInstalling(into: before, socket: socket, bundle: bundle)
        let removed = AgentHookInstaller.zcodeRemoving(from: installed, bundle: bundle, enabledBefore: .some(false))
        suite.expect((removed["hooks"] as? [String: Any])?["enabled"] as? Bool == false, "off before, off again: \(removed)")
        let unknown = AgentHookInstaller.zcodeRemoving(from: installed, bundle: bundle, enabledBefore: nil)
        suite.expect((unknown["hooks"] as? [String: Any])?["enabled"] as? Bool == true,
                     "with nothing to tell what it was, the switch is left")
        // Hooks turned off by hand after Install read as outdated, so they come back on.
        var off = installed
        var offHooks = off["hooks"] as? [String: Any] ?? [:]
        offHooks["enabled"] = false
        off["hooks"] = offHooks
        suite.expect(AgentHookInstaller.zcodeState(off, socket: socket, bundle: bundle) == .outdated,
                     "the island's hooks with hooks off aren't installed")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.zcode, home: home, path: nil,
                                                  isExecutable: { $0 == "/Users/someone/.local/bin/zcode" }, exists: { _ in false })
                     && AgentHookInstaller.isPresent(.zcode, home: home, path: nil, isExecutable: { _ in false },
                                                     exists: { $0 == "/Applications/ZCode.app" })
                     && !AgentHookInstaller.isPresent(.zcode, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".zcode") }),
                     "found by its command or its app, never by ~/.zcode alone")
        suite.expect(ClaudeSessionProvider.zcode.displayName == "ZCode" && ClaudeSessionProvider.zcode.shortName == "ZCode"
                     && ClaudeSessionProvider.hookAgents.last == .zcode && ClaudeSessionProvider.zcode.answersPermissions
                     && !ClaudeSessionProvider.zcode.takesRepliesDirectly && AgentCLI.command(.zcode) == "zcode"
                     && AgentCLI.startArguments(.zcode, prompt: "go") == [] && !AgentCLI.startsWithPrompt(.zcode)
                     && AgentCLI.startsWithPrompt(.grok) && !AgentHookInstaller.foldersByHand.contains(.zcode),
                     "named, listed last, answered here, and not offered by New session")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.zcode).contains("~/.zcode/cli/config.json")
                     && text.answerIn(.zcode) == "Answer this one in ZCode."
                     && NotchClaudeSessionsSupport.agentKey(.zcode) == DefaultsKey.notchSessionsAgentZCode,
                     "its hint, where it is answered, and its own switch")
        // A task started in ZCode's app without a project runs in a folder of the app's own.
        suite.expect(AgentHookMessage.zcodeProject("/Users/a/.zcode/workspace/draft-4e5688a1-61b1") == "ZCode task"
                     && AgentHookMessage.zcodeProject("/Users/a/.zcode/workspace/default") == "ZCode task"
                     && AgentHookMessage.zcodeProject("/Users/a/code/rocket") == "rocket"
                     && AgentHookMessage.zcodeProject("/Users/a/.zcode/workspace/default/sub") == "sub",
                     "the app's own folders read as a ZCode task")
        // ZCode's app runs its agent under a host process of its own: the app shows the session.
        let tree: [Int32: Int32] = [900: 800, 800: 700, 700: 1, 500: 400, 400: 300, 300: 1]
        let apps: [Int32: String] = [700: "dev.zcode.app", 300: "com.apple.Terminal"]
        suite.expect(ClaudeSessionHosts.agentApp(of: 900, bundle: { apps[$0] }, parent: { tree[$0] }) == "dev.zcode.app"
                     && ClaudeSessionHosts.agentApp(of: 500, bundle: { apps[$0] }, parent: { tree[$0] }) == nil,
                     "the desktop app's agent is told by its ancestor, a terminal's isn't")
        suite.expect(ClaudeTerminalApps.name(of: ClaudeSessionHosts.zcode) == "ZCode", "its app is called ZCode")
        suite.expect(ClaudeSessionProvider.zcode.character == .zcode && ClawdCharacter.zcode.favouredShell == .rose
                     && NotchHookAgentMarkPaths.mark(.zcode) != nil
                     && AgentHookInstaller.hookFile(.zcode, home: home, bundle: "b")?.wholeFile == false,
                     "its own figure, shell, mark and shared config file")
    }
}
