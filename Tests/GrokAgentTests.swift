// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Grok CLI in the Sessions list (#243): what its hooks send, the sessions
/// kept from them, a permission filled in from the tool before it, the
/// keys that answer its prompt, the file of the island's own in its hooks
/// folder, Claude Code's hooks it runs, and the model read from its summary.
enum GrokAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("grok: hooks") { hooks(suite) }
        suite.run("grok: permissions") { permissions(suite) }
        suite.run("grok: questions and ends") { endings(suite) }
        suite.run("grok: keys") { keys(suite) }
        suite.run("grok: installer") { installer(suite) }
        suite.run("grok: claude hooks and summary") { compat(suite) }
        suite.run("grok: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    /// The fields every hook of grok 1.0.89 sent in a live run (macOS, 2026-11-18).
    static func common(_ event: String, _ extra: [String: Any] = [:]) -> [String: Any] {
        let pascal = event.split(separator: "_").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined()
        var json: [String: Any] = [
            "hookEventName": event, "hook_event_name": pascal, "sessionId": "01a1ef72", "session_id": "01a1ef72",
            "cwd": "/w/rocket", "workspaceRoot": "/w/rocket", "timestamp": "2026-11-18T16:17:01.781084+00:00",
            "permissionMode": "default", "permission_mode": "default",
            "transcriptPath": "/h/.grok/sessions/%2Fw%2Frocket/01a1ef72/updates.jsonl",
        ]
        for (key, value) in extra { json[key] = value }
        return json
    }

    static func send(_ tracker: AgentHookSessions, _ hook: String, _ event: String, _ extra: [String: Any] = [:],
                     policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, T.envelope("grok", hook: hook, pid: 9100, common(event, extra)), policy: policy, at: date)
    }

    static let shell: [String: Any] = ["toolName": "run_terminal_command", "toolUseId": "call_1",
                                       "toolInput": ["command": "rm -rf build", "description": "Clean the build"]]

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        let decoded = AgentHookMessage.decode(T.envelope("grok", hook: "Stop", common("stop")), now: now, policy: T.on)
        suite.expect(decoded?.provider == .grok && decoded?.transcript?.hasSuffix("/01a1ef72/updates.jsonl") == true,
                     "the envelope is Grok's and names its conversation")
        send(tracker, "SessionStart", "session_start", ["source": "new", "modelId": "grok-4.5"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "grok:01a1ef72" && listed[0].provider == .grok
                     && listed[0].project == "rocket" && listed[0].phase == .idle && listed[0].model == "grok-4.5",
                     "a session start lists it idle in its folder, with the model it names: \(listed)")
        send(tracker, "UserPromptSubmit", "user_prompt_submit", ["prompt": "clean up", "promptId": "p1"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["clean up"],
                     "a prompt opens a turn, its text with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", "user_prompt_submit", ["prompt": "secret plan"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first?.chat.isEmpty == true,
                     "without previews the prompt isn't kept")
        send(tracker, "PreToolUse", "pre_tool_use", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "rm -rf build",
                     "its shell runs as Bash, summed up by the command: \(listed[0].currentTool as Any)")
        send(tracker, "PostToolUse", "post_tool_use", shell.merging(["toolResult": "ok", "durationMs": 12]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed, "the tool ends by its id")
        send(tracker, "PreToolUse", "pre_tool_use", ["toolName": "search_replace", "toolUseId": "call_2",
                                                     "toolInput": ["file_path": "/w/rocket/a.ts", "old_string": "a", "new_string": "b"]])
        send(tracker, "PostToolUseFailure", "post_tool_use_failure", ["toolName": "search_replace", "toolUseId": "call_2"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 2 && listed[0].timeline[1].name == "Edit" && listed[0].timeline[1].failed,
                     "a failed edit shows as failed: \(listed[0].timeline)")
        send(tracker, "Stop", "stop", ["reason": "end_turn", "lastAssistantMessage": "Cleaned.", "stopHookActive": false],
             at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Cleaned.",
                     "Stop ends the turn on its last message: \(listed[0].phase)")
        // As Grok shuts down it runs Stop once more, beside SessionEnd.
        send(tracker, "Stop", "stop", ["reason": "shutdown"], at: now.addingTimeInterval(2))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(2)).sessions[0].lastText == "Cleaned.",
                     "the Stop of a shutdown ends no turn")
        send(tracker, "SessionEnd", "session_end", ["reason": "shutdown"], at: now.addingTimeInterval(3))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(3)).sessions.isEmpty, "SessionEnd takes the row away")
        // A session whose Grok quit without a word goes when its process does.
        send(tracker, "UserPromptSubmit", "user_prompt_submit", ["prompt": "again"])
        tracker.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "no ghost row once Grok's process is gone")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", "user_prompt_submit", ["prompt": "clean"])
        send(tracker, "PreToolUse", "pre_tool_use", shell)
        // The prompt's notification names no tool; the PreToolUse before it did.
        send(tracker, "Notification", "notification", ["notificationType": "permission_prompt",
                                                       "message": "Tool permission requested", "level": "info"])
        var listed = T.snapshot(tracker).sessions
        let asked = listed[0].permission
        suite.expect(listed[0].phase == .waitingPermission && asked?.tool == "Bash" && asked?.detail == "rm -rf build"
                     && asked?.summary == "rm -rf build" && asked?.offersAlways == false && listed[0].waitingFor == nil,
                     "the prompt waits on a card for the tool named before it: \(asked as Any)")
        let hidden = AgentHookSessions()
        send(hidden, "PreToolUse", "pre_tool_use", shell, policy: T.off)
        send(hidden, "Notification", "notification", ["notificationType": "permission_prompt"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions[0].permission?.detail == "rm -rf build",
                     "the command shows with previews off, as Claude Code's")
        // Answered in Grok's own terminal: the tool runs, and the wait ends.
        send(tracker, "PostToolUse", "post_tool_use", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "a tool that ran was let through")
        // Refused there: PermissionDenied, then the turn is cancelled.
        send(tracker, "PreToolUse", "pre_tool_use", shell)
        send(tracker, "Notification", "notification", ["notificationType": "permission_prompt"])
        let request = T.snapshot(tracker).sessions[0].permission?.requestID ?? ""
        suite.expect(!request.isEmpty && request != asked?.requestID, "every prompt is a request of its own")
        send(tracker, "PermissionDenied", "permission_denied", shell)
        send(tracker, "StopCancelled", "stop_cancelled", ["reason": "permission_rejected"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase != .waitingPermission, "a refusal ends the wait")
        // A file change shows what it would do.
        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-grok-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appending(path: "a.txt")
        try? Data("one\ntwo\n".utf8).write(to: file)
        let edit = AgentHookMessage.grokAsk("Edit", input: ["file_path": file.path, "old_string": "two", "new_string": "three"],
                                            cwd: folder.path, at: now)
        suite.expect(edit.detail == file.path && edit.diffs.count == 1, "an edit's card carries its diff: \(edit.diffs.count)")
        // The island allows only what it can show whole, as for Claude Code (#224).
        let whole = AgentHookMessage.grokAsk("Bash", input: ["command": "rm -rf build"], cwd: nil, at: now)
        let cut = AgentHookMessage.grokAsk("Bash", input: ["command": "rm -rf bu"], cwd: nil, truncated: true, at: now)
        suite.expect(whole.content?.complete == true && whole.content?.full == "rm -rf build" && cut.content?.complete == false,
                     "the card carries the whole command, and a cut one can't be allowed here")
        // A prompt with no tool before it still waits, as a plain tool.
        let bare = AgentHookSessions()
        send(bare, "Notification", "notification", ["notificationType": "permission_prompt"])
        suite.expect(T.snapshot(bare).sessions[0].permission?.tool == "Tool", "an unnamed tool waits as Tool")
        // Where the terminal takes no keys, the person answers in Grok.
        let ghostty = bare.snapshot(now: now, previews: true, host: { _ in .ghostty(tty: "/dev/ttys009") },
                                    branch: { _ in nil }).sessions[0]
        suite.expect(ghostty.permission == nil && ghostty.waitingFor == CodexSessionState.answerInTerminal,
                     "in Ghostty the row says to answer in Grok CLI")
    }

    // MARK: Questions and ends

    static func endings(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", "user_prompt_submit", ["prompt": "ask me"])
        send(tracker, "PreToolUse", "pre_tool_use", ["toolName": "ask_user_question", "toolUseId": "q1",
                                                     "toolInput": ["questions": [["question": "Which colour?"]]]])
        send(tracker, "Notification", "notification", ["notificationType": "elicitation_dialog"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingInput && listed[0].waitingFor == CodexSessionState.answerInTerminal
                     && listed[0].permission == nil && listed[0].timeline.isEmpty,
                     "a question waits in Grok, not as a tool: \(listed[0].phase)")
        send(tracker, "PostToolUse", "post_tool_use", ["toolName": "ask_user_question", "toolUseId": "q1"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating, "its answer moves the turn on")
        send(tracker, "StopFailure", "stop_failure", ["reason": "rate_limit"], at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .idle, "a failed turn ends without a done: \(listed[0].phase)")
        send(tracker, "UserPromptSubmit", "user_prompt_submit", ["prompt": "go"], at: now.addingTimeInterval(2))
        send(tracker, "StopCancelled", "stop_cancelled", ["reason": "user_interrupt"], at: now.addingTimeInterval(3))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(3)).sessions[0].phase == .idle,
                     "an interrupted turn ends with no done")
        send(tracker, "Notification", "notification", ["notificationType": "idle_prompt"], at: now.addingTimeInterval(4))
        suite.expect(T.snapshot(tracker, at: now.addingTimeInterval(4)).sessions[0].phase != .waitingInput,
                     "idle_prompt asks nothing")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        let pane = ClaudeTmuxPane(tmux: "/tmp/tmux-501/default,4,0", pane: "%3")!
        suite.expect(ClaudeTyping.takesKeys(.terminal(tty: "/dev/ttys001")) && ClaudeTyping.takesKeys(.iterm(tty: "/dev/ttys001"))
                     && ClaudeTyping.takesKeys(.tmux(pane))
                     && ClaudeTyping.takesKeys(.kitty(window: 1, socket: "unix:/tmp/kitty"))
                     && !ClaudeTyping.takesKeys(.kitty(window: 1, socket: nil))
                     && !ClaudeTyping.takesKeys(.ghostty(tty: "/dev/ttys001"))
                     && !ClaudeTyping.takesKeys(.unknown),
                     "keys go where the terminal can press them")
        let script = ClaudeTerminalScripts.press([.interrupt], tty: "/dev/ttys001")
        suite.expect(script.contains("do script (ASCII character 3) in theTab") && !script.contains("[200~"),
                     "Terminal gets Ctrl-C as a key, not a paste")
        suite.expect(ClaudeTerminalScripts.press([.enter], tty: "/dev/ttys001").contains("do script \"\" in theTab"),
                     "and a lone return for yes")
        suite.expect(ClaudeTTYTerminalScripts.itermType([.interrupt], tty: "/dev/ttys001").contains("(character id 3) newline no"),
                     "iTerm2 writes Ctrl-C with no newline")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-grok-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentHookInstaller.grokHooks(home: home, bundle: bundle, environment: [:])
        suite.expect(url.path.hasSuffix("/.grok/hooks/openbar-com.openbar.app.dev.json"), "a file of its own in ~/.grok/hooks")
        suite.expect(AgentHookInstaller.grokHooks(home: home, bundle: bundle, environment: ["GROK_HOME": "/g"]).path
                     == "/g/hooks/openbar-com.openbar.app.dev.json", "GROK_HOME moves it")
        let person = url.deletingLastPathComponent().appending(path: "mine.json")
        try? manager.createDirectory(at: person.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#.utf8).write(to: person)
        guard let file = AgentHookInstaller.file(.grok, home: home, bundle: bundle, folder: home.appending(path: ".grok")) else {
            return suite.expect(false, "a Grok folder has a hook file")
        }
        suite.expect(file == url, "the folder by hand names the same file")
        suite.expect(AgentHookInstaller.state(.grok, home: home, socket: socket, bundle: bundle, folder: home.appending(path: ".grok"))
                     == .missing, "no file, no hooks")
        try? AgentHookInstaller.install(.grok, home: home, socket: socket, bundle: bundle, folder: home.appending(path: ".grok"))
        let json = (try? ClaudeHookInstaller.read(url)) ?? [:]
        let hooks = json["hooks"] as? [String: Any] ?? [:]
        let pre = (hooks["PreToolUse"] as? [[String: Any]])?.first
        let handler = (pre?["hooks"] as? [[String: Any]])?.first
        suite.expect(Set(hooks.keys) == Set(AgentHookInstaller.grokEvents) && pre?["matcher"] == nil
                     && handler?["type"] as? String == "command" && handler?["timeout"] as? Int == AgentHookInstaller.eventTimeout
                     && (handler?["command"] as? String)?.contains(#"{"openbar":"grok","hook":"PreToolUse""#) == true
                     && (handler?["command"] as? String)?.hasSuffix("# openbar-hook com.openbar.app.dev") == true,
                     "every event's command, without a matcher: \(hooks.keys)")
        suite.expect(AgentHookInstaller.state(.grok, home: home, socket: socket, bundle: bundle, folder: home.appending(path: ".grok"))
                     == .installed, "it reads as installed")
        suite.expect(AgentHookInstaller.state(.grok, home: home, socket: "/tmp/other.sock", bundle: bundle,
                                              folder: home.appending(path: ".grok")) == .outdated, "another socket is older")
        try? AgentHookInstaller.remove(.grok, home: home, bundle: bundle, folder: home.appending(path: ".grok"))
        let left = (try? manager.contentsOfDirectory(atPath: url.deletingLastPathComponent().path)) ?? []
        suite.expect(!manager.fileExists(atPath: url.path) && left.contains("mine.json")
                     && left.filter { $0.hasSuffix(".json") } == ["mine.json"],
                     "Remove takes only the island's file, its copy kept as no .json Grok would read: \(left)")
        // A file of the same name that isn't the island's is never written over.
        try? Data(#"{"hooks":{}}"#.utf8).write(to: url)
        var refused = false
        do { try AgentHookInstaller.install(.grok, home: home, socket: socket, bundle: bundle, folder: home.appending(path: ".grok")) }
        catch { refused = true }
        suite.expect(refused, "someone else's file under that name stays")
    }

    // MARK: Claude Code's hooks and the summary

    static func compat(_ suite: TestSuite) {
        let grok = try! JSONSerialization.data(withJSONObject: common("user_prompt_submit", ["prompt": "hi"]))
        let claude = try! JSONSerialization.data(withJSONObject: [
            "hook_event_name": "UserPromptSubmit", "session_id": "s1", "cwd": "/w", "prompt": "hi",
            "transcript_path": "/h/.claude/projects/w/s1.jsonl",
        ])
        suite.expect(AgentGrokClaudeHooks.isGrok(grok) && !AgentGrokClaudeHooks.isGrok(claude),
                     "Claude Code's hooks run by Grok are told apart from Claude Code's own")
        let summary = Data(#"{"info":{"id":"x"},"current_model_id":"grok-code-fast","session_summary":"hi"}"#.utf8)
        suite.expect(AgentGrokLog.model(summary: summary) == "grok-code-fast"
                     && AgentGrokLog.model(summary: Data(#"{"current_model_id":" "}"#.utf8)) == nil,
                     "the model comes from the session's summary")
        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-grok-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        try? summary.write(to: folder.appending(path: "summary.json"))
        suite.expect(AgentGrokLog.model(transcript: folder.appending(path: "updates.jsonl").path) == "grok-code-fast"
                     && AgentGrokLog.model(transcript: "/nowhere/updates.jsonl") == nil,
                     "read beside the conversation its hooks name")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.grok, home: home, path: nil,
                                                  isExecutable: { $0 == "/Users/someone/.grok/bin/grok" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.grok, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".grok") }),
                     "found by its command where its installer puts it, never by ~/.grok alone")
        suite.expect(ClaudeSessionProvider.grok.displayName == "Grok CLI" && ClaudeSessionProvider.grok.shortName == "Grok"
                     && ClaudeSessionProvider.hookAgents.last == .grok && ClaudeSessionProvider.grok.answersPermissions
                     && !ClaudeSessionProvider.grok.takesRepliesDirectly
                     && AgentCLI.startArguments(.grok, prompt: "-go") == ["--", "-go"]
                     && AgentHookInstaller.foldersByHand.contains(.grok),
                     "named, listed last, answered with keys, and started with its first prompt")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.grok).contains("~/.grok/hooks")
                     && text.answerIn(.grok) == "Answer this one in Grok CLI."
                     && NotchClaudeSessionsSupport.agentKey(.grok) == DefaultsKey.notchSessionsAgentGrok,
                     "its hint, where it is answered, and its own switch")
        suite.expect(ClaudeSessionProvider.grok.character == .grok && ClawdCharacter.grok.favouredShell == .moss
                     && NotchHookAgentMarkPaths.mark(.grok) != nil, "its own figure, shell and mark")
    }
}
