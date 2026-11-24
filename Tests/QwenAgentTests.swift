// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Qwen Code in the Sessions list (#239): what its hooks send (payloads
/// copied from Qwen Code 0.28.4 runs), the sessions kept from them, the
/// tool request its hook holds for the island, the question and plan its
/// own dialog takes by keys, the turns told apart from what it runs after
/// them, the island's entries in its settings, and how it is found and named.
enum QwenAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("qwen: hooks") { hooks(suite) }
        suite.run("qwen: permissions") { permissions(suite) }
        suite.run("qwen: questions and plans") { questions(suite) }
        suite.run("qwen: keys") { keys(suite) }
        suite.run("qwen: turns") { turns(suite) }
        suite.run("qwen: installer") { installer(suite) }
        suite.run("qwen: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    static let session = "605a7d68-27ab-48cb-a29d-56d22f8e8804"
    static let transcript = "/Users/a/.qwen/projects/-Users-a-code-rocket/chats/\(session).jsonl"

    /// The fields Qwen Code's hook input carries for every event.
    static func common(_ event: String, _ extra: [String: Any] = [:]) -> [String: Any] {
        var json: [String: Any] = [
            "session_id": session, "transcript_path": transcript, "cwd": "/Users/a/code/rocket",
            "hook_event_name": event, "timestamp": "2026-11-24T13:41:18.995Z", "permission_mode": "default",
            "prompt_id": "\(session)########1",
        ]
        for (key, value) in extra { json[key] = value }
        return json
    }

    static func tool(_ name: String, _ input: [String: Any], call: String = "call_f0deb657") -> [String: Any] {
        ["tool_name": name, "tool_input": input, "tool_use_id": "toolu_1795527694307_\(call)", "tool_call_id": call]
    }

    static func envelope(_ event: String, _ extra: [String: Any] = [:]) -> Data {
        T.envelope("qwen", hook: event, pid: 3184, common(event, extra))
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, _ extra: [String: Any] = [:],
                     policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, envelope(event, extra), policy: policy, at: date)
    }

    static let shell = tool("run_shell_command", ["command": "touch a.txt", "description": "Run touch a.txt"])

    static let asking: [String: Any] = tool(AgentQwen.questionTool, ["questions": [[
        "question": "Which color should the button be?", "header": "Color", "multiSelect": false,
        "options": [["label": "Red", "description": "Warm"], ["label": "Blue", "description": "Cool"],
                    ["label": "Green", "description": "Calm"]],
    ]]])

    static let planning: [String: Any] = tool(AgentQwen.planTool, ["plan": "## Plan\n\n1. Add a button.\n2. Test it."])

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "SessionStart", ["source": "startup", "model": "qwen3-coder-plus", "permission_mode": "auto"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "qwen:\(session)" && listed[0].provider == .qwen
                     && listed[0].project == "rocket" && listed[0].phase == .idle && listed[0].model == "qwen3-coder-plus",
                     "a session start lists it idle in its folder, with its model: \(listed)")
        send(tracker, "UserPromptSubmit", ["prompt": "tool touch a.txt", "submitted_prompt": "tool touch a.txt"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["tool touch a.txt"],
                     "a prompt opens a turn, its text with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", ["prompt": "secret plan"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first?.chat.isEmpty == true,
                     "without previews the prompt isn't kept")
        send(tracker, "PreToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "touch a.txt",
                     "its shell runs as Bash, summed up by the command: \(listed[0].currentTool as Any)")
        send(tracker, "PostToolUse", shell.merging(["tool_response": ["llmContent": "ok"]]) { $1 })
        // The same prompt goes on after the tool, with no text of its own.
        let going = AgentHookMessage.decode(envelope("UserPromptSubmit", ["prompt": ""]), now: now, policy: T.on)
        suite.expect(going?.events.isEmpty == true && going?.qwen == nil, "the prompt going on after a tool isn't a new one")
        send(tracker, "UserPromptSubmit", ["prompt": ""])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 1 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed && listed[0].chat.count == 1, "the tool ends by its call id")
        let write = tool("write_file", ["file_path": "/Users/a/code/rocket/b.txt", "content": "x"], call: "call_2")
        send(tracker, "PreToolUse", write)
        send(tracker, "PostToolUseFailure", write.merging(["error": "denied"]) { $1 })
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].timeline.count == 2 && listed[0].timeline[1].name == "Write" && listed[0].timeline[1].failed,
                     "a failed write shows as a failed Write: \(listed[0].timeline)")
        send(tracker, "Stop", ["stop_hook_active": false, "last_assistant_message": "Done.", "background_tasks": [], "crons": []],
             at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished && listed[0].lastText == "Done.",
                     "Stop ends the turn on its last message: \(listed[0].phase)")
        // An error ends the turn as failed.
        let failed = AgentHookMessage.decode(envelope("StopFailure", ["error": "rate_limit"]), now: now, policy: T.on)
        suite.expect(failed?.events.first?.kind == .stopped(lastText: nil, failed: true) && failed?.qwen == .stopped,
                     "StopFailure ends a turn as failed")
        send(tracker, "SessionEnd", ["reason": "prompt_input_exit"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "/quit takes the row away")
        // A session closed another way goes with its process.
        let gone = AgentHookSessions()
        send(gone, "SessionStart", ["source": "startup", "model": "m"])
        gone.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(gone).sessions.isEmpty, "no ghost row once Qwen Code's process is gone")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let request = envelope("PermissionRequest", shell)
        suite.expect(AgentHookMessage.waits(request), "its permission hook waits for the island's answer")
        suite.expect(!AgentHookMessage.waits(envelope("PreToolUse", shell)), "every other hook is let go at once")
        let decoded = AgentHookMessage.decode(request, now: now, policy: T.off)
        guard let decoded, case .heldPermission(let asked)? = decoded.events.first?.kind else {
            return suite.expect(false, "a tool's request is held: \(decoded?.events as Any)")
        }
        suite.expect(decoded.permission?.session == session && decoded.permission?.request == asked.requestID
                     && asked.tool == "Bash" && asked.detail == "touch a.txt" && asked.content?.complete == true
                     && !asked.offersAlways && asked.planModes.isEmpty && decoded.qwen == .asked,
                     "the request shows its command whatever the previews: \(asked)")
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "go"])
        send(tracker, "PreToolUse", shell)
        T.feed(tracker, request)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.tool == "Bash" && listed[0].waitingFor == nil,
                     "the session waits on the island's card: \(listed[0].phase)")
        send(tracker, "PostToolUse", shell)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "a tool that ran was answered")
        // An edit's request carries its diff, as Claude Code's does.
        let edit = AgentHookMessage.decode(envelope("PermissionRequest", tool("edit", [
            "file_path": "/Users/a/code/rocket/a.ts", "old_string": "let a = 1", "new_string": "let a = 2",
        ])), now: now, policy: T.on)
        if case .heldPermission(let change)? = edit?.events.first?.kind {
            suite.expect(change.tool == "Edit" && !change.diffs.isEmpty, "an edit shows what it changes: \(change.diffs)")
        } else {
            suite.expect(false, "an edit's request is held")
        }
        // Qwen Code's own dialog, once the island let the hook go, waits in Qwen Code.
        let prompt = AgentHookMessage.decode(envelope("Notification", ["notification_type": "permission_prompt",
                                                                       "message": "Qwen Code needs your permission to use run_shell_command",
                                                                       "title": "Permission needed"]), now: now, policy: T.on)
        suite.expect(prompt?.events.first?.kind == .question && prompt?.qwen == .asked, "its dialog shown waits in Qwen Code")
        let waiting = AgentHookSessions()
        send(waiting, "UserPromptSubmit", ["prompt": "go"])
        send(waiting, "Notification", ["notification_type": "permission_prompt"])
        suite.expect(T.snapshot(waiting).sessions[0].waitingFor == CodexSessionState.answerInTerminal,
                     "a dialog only Qwen Code shows says to answer there")
        // The replies the hook prints.
        func decision(_ data: Data?) -> [String: Any] {
            let json = data.flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
            let output = json["hookSpecificOutput"] as? [String: Any]
            return output?["hookEventName"] as? String == "PermissionRequest" ? output?["decision"] as? [String: Any] ?? [:] : [:]
        }
        suite.expect(decision(AgentQwen.reply(.allow))["behavior"] as? String == "allow"
                     && decision(AgentQwen.reply(.allowAlways))["behavior"] as? String == "allow"
                     && decision(AgentQwen.reply(.allowAlways))["updatedPermissions"] == nil
                     && decision(AgentQwen.reply(.deny))["behavior"] as? String == "deny"
                     && decision(AgentQwen.reply(.deny))["message"] as? String == ClaudeHookMessage.denyMessage
                     && AgentQwen.reply(.answer(["q": ["a"]])) == nil && AgentQwen.reply(.approvePlan(.manual)) == nil,
                     "Allow and Deny print the decision Qwen Code takes; answers and plans aren't the hook's")
    }

    // MARK: Questions and plans

    static func questions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "ask"])
        // A question's PreToolUse isn't a tool run: its request tells it.
        send(tracker, "PreToolUse", asking)
        suite.expect(T.snapshot(tracker).sessions[0].currentTool == nil, "a question doesn't run as a tool")
        let request = envelope("PermissionRequest", asking)
        suite.expect(!AgentHookMessage.waits(request), "a question's hook is let go: only Qwen Code's dialog takes it")
        let decoded = AgentHookMessage.decode(request, now: now, policy: T.on)
        guard let decoded, case .permission(let asked)? = decoded.events.first?.kind else {
            return suite.expect(false, "a question shows on the island")
        }
        suite.expect(decoded.permission == nil && asked.asksQuestions && AgentQwen.answeredByKeys(asked)
                     && asked.questions.first?.options.map(\.label) == ["Red", "Blue", "Green"],
                     "the question shows with its options: \(asked)")
        T.feed(tracker, request)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.asksQuestions == true,
                     "the session waits on the question's card")
        send(tracker, "PostToolUse", asking)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].timeline.isEmpty, "answered, it settles without a tool row")
        // A question the island can't show is answered in Qwen Code.
        let bare = AgentHookMessage.decode(envelope("PermissionRequest", tool(AgentQwen.questionTool, ["questions": "?"])),
                                           now: now, policy: T.on)
        suite.expect(bare?.events.first?.kind == .question && bare?.permission == nil, "an unreadable question waits in Qwen Code")
        // A plan comes in the modes Qwen Code's dialog offers.
        let plan = envelope("PermissionRequest", planning)
        suite.expect(!AgentHookMessage.waits(plan), "a plan's hook is let go too")
        if case .permission(let asked)? = AgentHookMessage.decode(plan, now: now, policy: T.on)?.events.first?.kind {
            suite.expect(asked.tool == ClaudeToolSummary.planTool && asked.planModes == [.acceptEdits, .manual]
                         && AgentQwen.answeredByKeys(asked) && asked.detail.contains("Add a button"),
                         "the plan shows whole, approved into auto-accept edits or each edit: \(asked)")
        } else {
            suite.expect(false, "a plan shows on the island")
        }
    }

    // MARK: Keys

    /// Keys one by one, a moment apart, as the island sends them; nil is Return.
    static func apart(_ keys: [ClaudeKey?]) -> [ClaudeTypingStep] {
        keys.flatMap { [$0.map { ClaudeTypingStep.keys([$0]) } ?? .enter, .wait(AgentQwen.keyGap)] }
    }

    static func keys(_ suite: TestSuite) {
        guard case .permission(let asked)? = AgentHookMessage.decode(envelope("PermissionRequest", asking), now: now,
                                                                      policy: T.on)?.events.first?.kind else {
            return suite.expect(false, "a question")
        }
        let question = "Which color should the button be?"
        suite.expect(AgentQwen.keys(.answer([question: ["Red"]]), asked) == apart([nil]) + [.wait(0.3)]
                     && AgentQwen.keys(.answer([question: ["Green"]]), asked) == apart([.down, .down, nil]) + [.wait(0.3)],
                     "a choice is reached with arrows and taken with Return, each key on its own")
        suite.expect(AgentQwen.keys(.answer([question: ["Purple please"]]), asked)
                     == apart([.down, .down, .down]) + [.paste("Purple please"), .wait(0.1), .enter, .wait(0.3)],
                     "a typed answer goes on Type something, after the choices")
        suite.expect(AgentQwen.keys(.deny, asked) == apart([.escape]), "Skip is Escape")
        suite.expect(AgentQwen.keys(.allow, asked) == nil && AgentQwen.keys(.answer([:]), asked) == nil,
                     "a question is answered, never allowed or left empty")
        let two = [
            ClaudeQuestion(question: "Which database?", header: "DB",
                           options: [.init(label: "Postgres"), .init(label: "SQLite")]),
            ClaudeQuestion(question: "Which extras do you want?", header: "Extras",
                           options: [.init(label: "Tests"), .init(label: "Docs"), .init(label: "CI")], multiSelect: true),
        ]
        suite.expect(AgentQwen.questionKeys(["Which database?": ["SQLite"], "Which extras do you want?": ["CI", "Tests"]], two)
                     == apart([.down, nil]) + [.wait(0.3)] + apart([.space, .down, .down, .space, nil]) + [.wait(0.3), .enter],
                     "several questions: each answered in turn, ticks with Space, then Submit")
        suite.expect(AgentQwen.questionKeys(["Which database?": ["SQLite"], "Which extras do you want?": ["Mine"]], two) == nil
                     && AgentQwen.questionKeys(["Which database?": ["SQLite"]], two) == nil,
                     "a typed answer among ticks, or a question left out, isn't one the keys can give")
        guard case .permission(let plan)? = AgentHookMessage.decode(envelope("PermissionRequest", planning), now: now,
                                                                     policy: T.on)?.events.first?.kind else {
            return suite.expect(false, "a plan")
        }
        suite.expect(AgentQwen.keys(.approvePlan(.acceptEdits), plan) == apart([.down, nil])
                     && AgentQwen.keys(.approvePlan(.manual), plan) == apart([.down, .down, nil])
                     && AgentQwen.keys(.allow, plan) == apart([.down, nil])
                     && AgentQwen.keys(.approvePlan(.auto), plan) == nil
                     && AgentQwen.keys(.deny, plan) == apart([.down, .down, .down, nil]),
                     "a plan is approved into auto-accept edits or each edit, or kept planning")
        // Terminal types its keys in one burst, of which Qwen Code reads one: a number.
        suite.expect(AgentQwen.keys(.approvePlan(.manual), plan, oneBurst: true) == [.keys([.digit(3)])]
                     && AgentQwen.keys(.deny, plan, oneBurst: true) == [.keys([.digit(4)])]
                     && AgentQwen.keys(.answer([question: ["Blue"]]), asked, oneBurst: true) == [.keys([.digit(2)])]
                     && AgentQwen.keys(.answer([question: ["Purple"]]), asked, oneBurst: true) == nil
                     && AgentQwen.keys(.deny, asked, oneBurst: true) == nil,
                     "in Terminal a number picks a plan's mode or a question's one choice")
        let several = ClaudePermission(requestID: "r", tool: "AskUserQuestion", summary: nil, offersAlways: false, since: now,
                                       questions: two)
        suite.expect(AgentQwen.keyable(asked, oneBurst: true) && AgentQwen.keyable(plan, oneBurst: true)
                     && !AgentQwen.keyable(several, oneBurst: true) && AgentQwen.keyable(several, oneBurst: false)
                     && AgentQwen.oneBurst(.terminal(tty: "/dev/ttys001")) && !AgentQwen.oneBurst(.iterm(tty: "/dev/ttys001")),
                     "several questions are answered in Qwen Code when its terminal is Terminal")
        guard case .heldPermission(let held)? = AgentHookMessage.decode(envelope("PermissionRequest", shell), now: now,
                                                                         policy: T.on)?.events.first?.kind else {
            return suite.expect(false, "a tool's request")
        }
        suite.expect(AgentQwen.keys(.allow, held) == nil && !AgentQwen.answeredByKeys(held) && !AgentQwen.keyable(held, oneBurst: false),
                     "a tool's request is answered through the hook, never by keys")
        // Once the hook is let go (after Qwen Code's minute, or on Open) its own dialog takes keys.
        var shown = held
        shown.inAgentDialog = true
        suite.expect(AgentQwen.answeredByKeys(shown) && AgentQwen.keys(.allow, shown) == apart([nil])
                     && AgentQwen.keys(.deny, shown) == apart([.escape]) && AgentQwen.keys(.allowAlways, shown) == nil
                     && AgentQwen.keys(.allow, shown, oneBurst: true) == [.keys([.digit(1)])]
                     && AgentQwen.keys(.deny, shown, oneBurst: true) == nil && AgentQwen.keyable(shown, oneBurst: true)
                     && AgentQwen.holdLimit < 60,
                     "its dialog's Allow once is picked first and Escape says no; the island lets go within Qwen Code's minute")
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "go"])
        T.feed(tracker, envelope("PermissionRequest", shell))
        tracker.moveToDialog("qwen:\(session)", request: held.requestID)
        let moved = T.snapshot(tracker).sessions[0]
        tracker.moveToDialog("qwen:\(session)", request: moved.permission?.requestID ?? "")
        let after = T.snapshot(tracker).sessions[0]
        suite.expect(after.phase == .waitingPermission && after.permission?.inAgentDialog == true && after.waitingFor == nil,
                     "the card stays when the dialog shows, answered there by keys")
        // The keys reach a terminal as each of them names them.
        suite.expect(ClaudeKey.escape.multiplexerName == "Escape" && ClaudeKey.escape.zellijName == "Esc"
                     && ClaudeKey.space.multiplexerName == "Space" && ClaudeKey.space.kittyName == "space"
                     && ClaudeKey.escape.bytes == "\u{1B}" && ClaudeKey.space.bytes == " " && ClaudeKey.digit(3).bytes == "3"
                     && ClaudeKey.appleScript([.space, .escape, .digit(2)]) == "\" \" & (character id 27) & \"2\"",
                     "Escape, Space and numbers as tmux, zellij, kitty, WezTerm and Terminal take them")
    }

    // MARK: Turns

    static func turns(_ suite: TestSuite) {
        var turns = AgentQwen.Turns()
        let turn = "\(session)########1"
        suite.expect(turns.keep(session, turn: turn, fact: .prompt, at: now), "a prompt goes on")
        suite.expect(turns.keep(session, turn: turn, fact: .tool, at: now), "a tool goes on")
        suite.expect(!turns.keep(session, turn: turn, fact: .idle, at: now.addingTimeInterval(0.01)),
                     "idle just as a tool starts is the model done, not the turn")
        suite.expect(turns.keep(session, turn: turn, fact: .tool, at: now.addingTimeInterval(8)), "the tool ends")
        suite.expect(turns.keep(session, turn: turn, fact: .stopped, at: now.addingTimeInterval(9)), "Stop ends the turn")
        suite.expect(turns.keep(session, turn: turn, fact: .idle, at: now.addingTimeInterval(9)), "idle after Stop goes on")
        suite.expect(!turns.keep(session, turn: turn, fact: .tool, at: now.addingTimeInterval(9.1)),
                     "the memory helper's tool after the turn isn't the session's")
        suite.expect(turns.keep(session, turn: "\(session)########2", fact: .prompt, at: now.addingTimeInterval(20))
                     && turns.keep(session, turn: "\(session)########2", fact: .tool, at: now.addingTimeInterval(20)),
                     "the next prompt's tools are")
        // Escape at a dialog: no Stop, only idle.
        suite.expect(turns.keep(session, turn: "\(session)########2", fact: .asked, at: now.addingTimeInterval(20.05))
                     && turns.keep(session, turn: "\(session)########2", fact: .idle, at: now.addingTimeInterval(21)),
                     "idle after a dialog ends the turn the person cancelled")
        // Escape during a long tool.
        suite.expect(turns.keep(session, turn: "\(session)########3", fact: .prompt, at: now.addingTimeInterval(30))
                     && turns.keep(session, turn: "\(session)########3", fact: .tool, at: now.addingTimeInterval(30))
                     && turns.keep(session, turn: "\(session)########3", fact: .idle, at: now.addingTimeInterval(40)),
                     "idle well after a tool started ends the turn")
        // A dialog answered: the tool that runs clears it, so idle before the next tool waits again.
        suite.expect(turns.keep(session, turn: "\(session)########4", fact: .asked, at: now.addingTimeInterval(50))
                     && turns.keep(session, turn: "\(session)########4", fact: .tool, at: now.addingTimeInterval(51))
                     && turns.keep(session, turn: "\(session)########4", fact: .tool, at: now.addingTimeInterval(55))
                     && !turns.keep(session, turn: "\(session)########4", fact: .idle, at: now.addingTimeInterval(55.01)),
                     "an answered dialog doesn't end the turn at the next idle")
        turns.forget(session)
        suite.expect(turns.keep(session, turn: turn, fact: .tool, at: now.addingTimeInterval(60)),
                     "a session forgotten starts afresh")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-qwen-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentQwen.settings(home: home, environment: [:])
        suite.expect(url.path.hasSuffix("/.qwen/settings.json")
                     && AgentQwen.settings(home: home, environment: ["QWEN_HOME": "/q"]).path == "/q/settings.json"
                     && AgentQwen.settings(home: home, environment: ["QWEN_HOME": ""]) == url
                     && AgentHookInstaller.file(.qwen, folder: URL(fileURLWithPath: "/q"), bundle: bundle)?.path == "/q/settings.json",
                     "its settings, where QWEN_HOME moves them")
        suite.expect(AgentHookInstaller.state(.qwen, home: home, socket: socket, bundle: bundle) == .missing, "no file, no hooks")

        try? AgentHookInstaller.install(.qwen, home: home, socket: socket, bundle: bundle, folder: home.appending(path: ".qwen"))
        var settings = (try? ClaudeHookInstaller.read(url)) ?? [:]
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        let pre = (hooks["PreToolUse"] as? [[String: Any]])?.first
        let start = (hooks["SessionStart"] as? [[String: Any]])?.first
        let permission = ((hooks["PermissionRequest"] as? [[String: Any]])?.first?["hooks"] as? [[String: Any]])?.first
        let handler = (pre?["hooks"] as? [[String: Any]])?.first
        suite.expect(Set(hooks.keys) == Set(AgentQwen.events) && pre?["matcher"] as? String == "*" && start?["matcher"] == nil
                     && handler?["type"] as? String == "command" && handler?["timeout"] as? Int == AgentHookInstaller.eventTimeout
                     && handler?["name"] as? String == "openbar-com.openbar.app.dev"
                     && permission?["timeout"] as? Int == ClaudeHookInstaller.permissionTimeout * 1000
                     && (handler?["command"] as? String)?.contains(#"{"openbar":"qwen","hook":"PreToolUse""#) == true
                     && (handler?["command"] as? String)?.hasSuffix("# openbar-hook com.openbar.app.dev") == true,
                     "every event, tools matched, the permission's wait in milliseconds: \(hooks)")
        suite.expect(AgentHookInstaller.state(.qwen, home: home, socket: socket, bundle: bundle) == .installed, "it reads as installed")
        suite.expect(AgentHookInstaller.state(.qwen, home: home, socket: "/tmp/other.sock", bundle: bundle) == .outdated,
                     "another socket is older")
        try? AgentHookInstaller.remove(.qwen, home: home, bundle: bundle)
        settings = (try? ClaudeHookInstaller.read(url)) ?? [:]
        suite.expect((settings["hooks"] as? [String: Any] ?? [:]).isEmpty, "Remove takes the island's entries: \(settings)")

        // The person's own settings: their keys and hooks stay.
        let other = home.appending(path: "other")
        let mine = AgentQwen.settings(home: other, environment: [:])
        try? manager.createDirectory(at: mine.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(#"{"model":{"name":"qwen3-coder-plus"},"security":{"auth":{"selectedType":"qwen-oauth"}},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done","name":"mine"}]}]}}"#.utf8)
            .write(to: mine)
        try? AgentHookInstaller.install(.qwen, home: other, socket: socket, bundle: bundle, folder: mine.deletingLastPathComponent())
        settings = (try? ClaudeHookInstaller.read(mine)) ?? [:]
        hooks = settings["hooks"] as? [String: Any] ?? [:]
        let stops = hooks["Stop"] as? [[String: Any]] ?? []
        let backups = ((try? manager.contentsOfDirectory(atPath: mine.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasPrefix("settings.json.openbar-") }
        suite.expect(settings["model"] != nil && settings["security"] != nil && stops.count == 2
                     && ((stops[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String) == "say done"
                     && backups.contains("settings.json.openbar-original.bak"),
                     "the island's entries come after the person's, a copy kept first: \(hooks) \(backups)")
        try? AgentHookInstaller.remove(.qwen, home: other, bundle: bundle, folder: mine.deletingLastPathComponent())
        settings = (try? ClaudeHookInstaller.read(mine)) ?? [:]
        hooks = settings["hooks"] as? [String: Any] ?? [:]
        suite.expect(settings["model"] != nil && (hooks["Stop"] as? [[String: Any]])?.count == 1 && hooks.count == 1
                     && !ClaudeHookInstaller.hasAny(in: settings, bundle: bundle),
                     "Remove leaves the person's hook: \(hooks)")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.qwen, home: home, path: "/opt/node/bin",
                                                  isExecutable: { $0 == "/opt/node/bin/qwen" }, exists: { _ in false })
                     && AgentHookInstaller.isPresent(.qwen, home: home, path: nil,
                                                     isExecutable: { $0 == "/Users/someone/.npm-global/bin/qwen" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.qwen, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".qwen") }),
                     "found by its command, never by ~/.qwen alone")
        suite.expect(ClaudeSessionProvider.qwen.displayName == "Qwen Code" && ClaudeSessionProvider.qwen.shortName == "Qwen"
                     && ClaudeSessionProvider.hookAgents.last == .qwen && ClaudeSessionProvider.qwen.answersPermissions
                     && ClaudeSessionProvider.qwen.reportsThroughHooks && !ClaudeSessionProvider.qwen.takesRepliesDirectly
                     && AgentCLI.command(.qwen) == "qwen" && AgentCLI.startArguments(.qwen, prompt: "go") == ["-i", "go"]
                     && AgentCLI.startsWithPrompt(.qwen) && AgentHookInstaller.foldersByHand.contains(.qwen)
                     && ClaudeTyping.enterDelay(.qwen) == nil && ClaudeTyping.enterDelay(.gemini) == 0.5,
                     "named, listed last, answered here, started with its first prompt")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.qwen).contains("~/.qwen/settings.json")
                     && text.answerIn(.qwen) == "Answer this one in Qwen Code."
                     && NotchClaudeSessionsSupport.agentKey(.qwen) == DefaultsKey.notchSessionsAgentQwen,
                     "its hint, where it is answered, and its own switch")
        suite.expect(ClaudeSessionProvider.qwen.character == .qwen && ClawdCharacter.qwen.favouredShell == .iris
                     && NotchHookAgentMarkPaths.mark(.qwen) != nil
                     && AgentHookInstaller.hookFile(.qwen, home: home, bundle: "b")?.wholeFile == false
                     && AgentHookInstaller.agentFolder(.qwen, home: home, environment: [:])?.path == "/Users/someone/.qwen",
                     "its own figure, shell, mark and shared settings file")
        let log = FileManager.default.temporaryDirectory.appending(path: "qwen-\(UUID().uuidString).jsonl")
        defer { try? FileManager.default.removeItem(at: log) }
        try? Data((#"{"type":"user","message":{"role":"user","parts":[{"text":"hi"}]}}"# + "\n"
                   + #"{"type":"assistant","model":"qwen3-coder-plus","message":{"role":"model","parts":[{"text":"x"}]}}"# + "\n"
                   + #"{"type":"system","subtype":"ui_telemetry"}"# + "\n").utf8).write(to: log)
        suite.expect(AgentQwen.model(transcript: log.path) == "qwen3-coder-plus" && AgentQwen.model(transcript: "/nope") == nil,
                     "a session heard late takes its model from its log")
        suite.expect(["run_shell_command": "Bash", "read_file": "Read", "write_file": "Write", "edit": "Edit",
                      "grep_search": "Grep", "glob": "Glob", "list_directory": "LS", "ask_user_question": "AskUserQuestion",
                      "exit_plan_mode": "ExitPlanMode", "mcp__github__search": "github__search"]
                        .allSatisfy { AgentHookTools.name(.qwen, $0.key) == $0.value },
                     "its tools under Claude Code's names")
    }
}
