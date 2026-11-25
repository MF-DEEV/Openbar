// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Kimi Code in the Sessions list (#240): what its hooks send (payloads
/// copied from Kimi Code CLI 2.8.0 runs), the sessions kept from them, the
/// requests, questions and plans answered with keys or through its server,
/// the island's tables in its config.toml, and its session log.
enum KimiAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("kimi: hooks") { hooks(suite) }
        suite.run("kimi: permissions") { permissions(suite) }
        suite.run("kimi: questions and plans") { questions(suite) }
        suite.run("kimi: keys") { keys(suite) }
        suite.run("kimi: server") { server(suite) }
        suite.run("kimi: installer") { installer(suite) }
        suite.run("kimi: log") { log(suite) }
        suite.run("kimi: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    static let session = "session_f6fd8114-c8d8-4ae0-9158-5b3bd8c4a9e7"

    /// The fields Kimi's hook input carries for every event.
    static func common(_ event: String, _ extra: [String: Any] = [:], client: String = "kimi_code_cli") -> [String: Any] {
        var json: [String: Any] = ["hook_event_name": event, "session_id": session, "cwd": "/Users/a/code/rocket",
                                   "client_type": client, "session_title": "tool echo hi"]
        for (key, value) in extra { json[key] = value }
        return json
    }

    static func envelope(_ event: String, _ extra: [String: Any] = [:], client: String = "kimi_code_cli") -> Data {
        T.envelope("kimi", hook: event, pid: 59299, common(event, extra, client: client))
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, _ extra: [String: Any] = [:],
                     client: String = "kimi_code_cli", policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, envelope(event, extra, client: client), policy: policy, at: date)
    }

    static let danger: [String: Any] = [
        "id": "approval_ad3eed77-8444-4cec-95ff-230728626ef9", "agent_id": "main", "turn_id": 3,
        "tool_call_id": "call_df5a55a4", "tool_name": "Bash", "action": "Running: rm -rf ./scratch-dir",
        "display": ["kind": "command", "command": "rm -rf ./scratch-dir", "cwd": "/Users/a/code/rocket",
                    "description": "Remove scratch", "language": "bash"],
        "tool_input": ["command": "rm -rf ./scratch-dir", "description": "Remove scratch"],
    ]

    static let planned: [String: Any] = [
        "id": "approval_5e585559-06a4-4894-8bc0-4a12ce3ae1c0", "agent_id": "main", "turn_id": 6,
        "tool_call_id": "call_e56075f5", "tool_name": "ExitPlanMode", "action": "Presenting plan and exiting plan mode",
        "display": ["kind": "plan_review", "plan": "## Plan\n\n1. Add a button.\n2. Test it.\n",
                    "path": "/Users/a/.kimi-code/sessions/wd_rocket_1/\(session)/agents/main/plans/aqualad.md"],
        "tool_input": [String: Any](),
    ]

    static let askingTwo: [String: Any] = [
        "tool_name": "AskUserQuestion", "tool_call_id": "call_f587936f",
        "tool_input": ["questions": [
            ["question": "Which database?", "header": "DB", "multi_select": false,
             "options": [["label": "Postgres", "description": "SQL"], ["label": "SQLite", "description": "File"]]],
            ["question": "Which extras do you want?", "header": "Extras", "multi_select": true,
             "options": [["label": "Tests", "description": "Unit tests"], ["label": "Docs", "description": "README"],
                         ["label": "CI", "description": "Workflow"]]],
        ]],
    ]

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "SessionStart", ["source": "startup", "model": "kimi-code/k3", "profile": "agent"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "kimi:\(session)" && listed[0].provider == .kimi
                     && listed[0].project == "rocket" && listed[0].phase == .idle && listed[0].model == "k3",
                     "a session start lists it idle in its folder, its model by name: \(listed)")
        send(tracker, "UserPromptSubmit", ["prompt": [["type": "text", "text": "tool echo hi"]], "is_steer": false])
        send(tracker, "TurnStarted", ["turn_id": 0, "origin_kind": "user", "prompt": "tool echo hi"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["tool echo hi"]
                     && listed[0].title == "tool echo hi",
                     "a prompt opens a turn, its text and the session's title with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", ["prompt": [["type": "text", "text": "secret plan"]]], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first.map { $0.chat.isEmpty && $0.title == nil } == true,
                     "without previews neither the prompt nor the title is kept")
        let echo: [String: Any] = ["tool_name": "Bash", "tool_call_id": "call_ebf3f382",
                                   "tool_input": ["command": "echo hi", "description": "Run echo hi"]]
        send(tracker, "PreToolUse", echo)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "echo hi",
                     "a tool runs: \(String(describing: listed[0].currentTool))")
        var done = echo
        done["tool_output"] = "hi\n"
        send(tracker, "PostToolUse", done)
        let write: [String: Any] = ["tool_name": "Write", "tool_call_id": "call_beb57973",
                                    "tool_input": ["path": "note.txt", "content": "hello\n"]]
        send(tracker, "PreToolUse", write)
        var failed = write
        failed["error"] = ["code": "internal", "message": "no room", "retryable": false]
        send(tracker, "PostToolUseFailure", failed)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 2 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed && listed[0].timeline[1].name == "Write" && listed[0].timeline[1].failed,
                     "a tool that ran is done, one that failed says so: \(listed[0].timeline)")
        send(tracker, "Stop", ["turn_id": 0, "step": 2, "first_step_of_turn": false, "finish_reason": "completed",
                               "usage": ["inputOther": 100, "output": 10], "stop_hook_active": false])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .finished, "Stop ends the turn done")

        // Escape mid-turn: Interrupt, no Stop.
        send(tracker, "UserPromptSubmit", ["prompt": [["type": "text", "text": "slow"]]])
        send(tracker, "Interrupt", ["turn_id": 7, "reason": "cancelled"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .idle, "a turn the person stopped ends, not done: \(listed[0].phase)")
        send(tracker, "UserPromptSubmit", ["prompt": [["type": "text", "text": "x"]]])
        send(tracker, "StopFailure", ["error_type": "provider"])
        suite.expect(T.snapshot(tracker).sessions[0].phase == .idle, "a failed turn ends, not done")
        // A turn a background task begins works without a prompt.
        send(tracker, "TurnStarted", ["turn_id": 9, "origin_kind": "task", "prompt": ""])
        suite.expect(T.snapshot(tracker).sessions[0].phase == .generating, "a turn begun by a task works")
        send(tracker, "SessionEnd", ["reason": "exit"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "quitting takes the row away")
        let gone = AgentHookSessions()
        send(gone, "UserPromptSubmit", ["prompt": [["type": "text", "text": "hi"]]])
        gone.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(gone).sessions.isEmpty, "no ghost row once kimi's process is gone")
        // A session Kimi's server holds stays while the server runs, until long quiet.
        let served = AgentHookSessions()
        send(served, "SessionStart", ["source": "startup"], client: "kimi_code_desktop")
        served.prune(now: now.addingTimeInterval(60), alive: { _ in true })
        suite.expect(T.snapshot(served).sessions.count == 1, "a served session stays while it's heard from")
        served.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor + 1), alive: { _ in true })
        suite.expect(T.snapshot(served).sessions.isEmpty, "a served session long quiet leaves the list")
        suite.expect(!AgentHookMessage.waits(envelope("PermissionRequest", danger)), "no hook of Kimi's waits on the island")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": [["type": "text", "text": "danger"]]])
        send(tracker, "PreToolUse", ["tool_name": "Bash", "tool_call_id": "call_df5a55a4",
                                     "tool_input": ["command": "rm -rf ./scratch-dir", "description": "Remove scratch"]])
        send(tracker, "PermissionRequest", danger)
        var listed = T.snapshot(tracker).sessions
        let asked = listed[0].permission
        suite.expect(listed[0].phase == .waitingPermission && asked?.tool == "Bash" && asked?.detail == "rm -rf ./scratch-dir"
                     && asked?.requestID == danger["id"] as? String && asked?.offersAlways == true
                     && asked?.content != nil && listed[0].waitingFor == nil,
                     "the request shows its command, named by Kimi's own id, with Always: \(String(describing: asked))")
        // Where keys can't go, it is answered in Kimi.
        let elsewhere = T.snapshot(tracker, host: .ghostty(tty: "/dev/ttys009")).sessions
        suite.expect(elsewhere[0].permission == nil && elsewhere[0].waitingFor == CodexSessionState.answerInTerminal,
                     "where keys can't go, the row says to answer in Kimi Code")
        var result = danger
        result["decision"] = "rejected"
        send(tracker, "PermissionResult", result)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase != .waitingPermission,
                     "its result settles the request, answered anywhere")

        // An older request's result leaves a newer one alone.
        send(tracker, "PermissionRequest", danger)
        var other = danger
        other["id"] = "approval_older"
        send(tracker, "PermissionResult", other)
        suite.expect(T.snapshot(tracker).sessions[0].permission != nil, "another request's result settles nothing")

        // A file write reads as Claude Code's, its path its file.
        let write = AgentHookSessions()
        send(write, "PermissionRequest", ["id": "approval_w", "tool_call_id": "call_w", "tool_name": "Write",
                                          "action": "Write note.txt",
                                          "display": ["kind": "generic", "summary": "Write note.txt"],
                                          "tool_input": ["path": "/Users/a/code/rocket/note.txt", "content": "hello\n"]])
        let written = T.snapshot(write).sessions[0].permission
        suite.expect(written?.tool == "Write" && written?.detail.contains("note.txt") == true && written?.diffs.isEmpty == false,
                     "a write shows its file and change: \(String(describing: written))")

        // The VS Code extension asks in its own panel.
        let vscode = AgentHookSessions()
        send(vscode, "PermissionRequest", danger, client: "kimi_code_vscode")
        let panel = T.snapshot(vscode, host: .app("com.microsoft.VSCode")).sessions[0]
        suite.expect(panel.permission == nil && panel.waitingFor == CodexSessionState.answerInTerminal && !panel.served,
                     "an extension's request is answered in its panel")

        // The Kimi Code app's server answers, wherever its window is.
        let app = AgentHookSessions()
        send(app, "PermissionRequest", danger, client: "kimi_code_desktop")
        let shown = T.snapshot(app, host: .app("com.kimi.code.desktop")).sessions[0]
        suite.expect(shown.permission != nil && shown.waitingFor == nil && shown.served,
                     "a served session's request is answered on the island")
        suite.expect(NotchClaudeSessionsSupport.repliesReach(shown), "a served session takes replies")
        let web = AgentHookSessions()
        send(web, "SessionStart", ["source": "startup"])
        web.ingest(AgentHookEvent(provider: .kimi, sessionID: session, pid: 60964, cwd: nil, date: now,
                                  kind: .client(AgentKimi.kapClient)), now: now)
        send(web, "PermissionRequest", danger)
        suite.expect(T.snapshot(web, host: .ghostty(tty: "/dev/ttys004")).sessions[0].permission != nil,
                     "`kimi web`'s server answers, whatever terminal runs it")
    }

    // MARK: Questions and plans

    static func questions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": [["type": "text", "text": "ask2"]]])
        send(tracker, "PreToolUse", askingTwo)
        var listed = T.snapshot(tracker).sessions
        let asked = listed[0].permission
        suite.expect(listed[0].phase == .waitingPermission && asked?.asksQuestions == true && asked?.questions.count == 2
                     && asked?.questions[1].multiSelect == true && asked?.requestID == "call_f587936f"
                     && asked?.questions[0].header == "DB",
                     "a question shows by its tool call: \(String(describing: asked))")
        var answered = askingTwo
        answered["tool_output"] = #"{"answers":{"Which database?":"SQLite","Which extras do you want?":"Tests, CI"}}"#
        send(tracker, "PostToolUse", answered)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].timeline.isEmpty, "the answer settles it, no tool row for it")
        // A question the island can't read is answered in Kimi.
        let odd = AgentHookSessions()
        send(odd, "PreToolUse", ["tool_name": "AskUserQuestion", "tool_call_id": "call_x", "tool_input": ["questions": "?"]])
        suite.expect(T.snapshot(odd).sessions[0].phase == .waitingInput, "an unreadable question waits in Kimi")

        let plan = AgentHookSessions()
        send(plan, "PreToolUse", ["tool_name": "ExitPlanMode", "tool_call_id": "call_e56075f5", "tool_input": [String: Any]()])
        suite.expect(T.snapshot(plan).sessions[0].timeline.isEmpty, "a plan's tool isn't a step of its own")
        send(plan, "PermissionRequest", planned)
        let shown = T.snapshot(plan).sessions[0].permission
        suite.expect(shown?.tool == ClaudeToolSummary.planTool && shown?.planModes.isEmpty == true && shown?.offersAlways == false
                     && shown?.content?.full.contains("1. Add a button.") == true && shown?.requestID == planned["id"] as? String,
                     "a plan shows its text, approved or not: \(String(describing: shown))")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        let tool = ClaudePermission(requestID: "a", tool: "Bash", summary: nil, offersAlways: true, since: now)
        suite.expect(AgentKimi.keys(.allow, tool) == [.keys([.digit(1)])] && AgentKimi.keys(.allowAlways, tool) == [.keys([.digit(2)])]
                     && AgentKimi.keys(.deny, tool) == [.keys([.digit(3)])] && AgentKimi.keys(.answer([:]), tool) == nil,
                     "a request: 1 once, 2 for the session, 3 reject")
        let plan = AgentKimi.ask("ExitPlanMode", raw: "ExitPlanMode", request: "p", input: [:],
                                 display: planned["display"] as? [String: Any], cwd: nil, at: now)
        suite.expect(AgentKimi.keys(.allow, plan) == [.keys([.digit(1)])] && AgentKimi.keys(.deny, plan) == [.keys([.digit(2)])],
                     "a plan: 1 approves, 2 rejects")
        let input = askingTwo["tool_input"] as? [String: Any] ?? [:]
        guard let two = AgentKimi.question(call: "c", input: input, at: now) else { return suite.expect(false, "questions read") }
        let steps = AgentKimi.keys(.answer(["Which database?": ["SQLite"], "Which extras do you want?": ["CI", "Tests"]]), two)
        suite.expect(steps == [.keys([.digit(2)]), .wait(0.3), .keys([.digit(1)]), .wait(0.15), .keys([.digit(3)]), .wait(0.15),
                               .keys([.tab]), .wait(0.3), .keys([.digit(1)])],
                     "each answer's number, a multiple choice ticked then Tab, then 1 submits: \(String(describing: steps))")
        suite.expect(AgentKimi.keys(.answer(["Which database?": ["MySQL"], "Which extras do you want?": ["CI"]]), two) == nil
                     && AgentKimi.keys(.answer(["Which database?": ["SQLite"]]), two) == nil
                     && AgentKimi.keys(.deny, two) == [.keys([.escape])],
                     "an answer not listed or missing types nothing; Deny cancels")
        let single = ClaudeQuestion(question: "Color?", header: nil,
                                    options: ["Red", "Blue"].map { ClaudeQuestion.Option(label: $0, description: nil, preview: nil) },
                                    multiSelect: false)
        suite.expect(AgentKimi.questionKeys(["Color?": ["Red", "Blue"]], [single]) == nil
                     && AgentKimi.questionKeys(["x": ["Blue"]], [single]) == [.keys([.digit(2)]), .wait(0.3), .keys([.digit(1)])],
                     "one choice for a single question, matched alone when it's the only one")
    }

    // MARK: Server

    static func server(_ suite: TestSuite) {
        let instance = AgentKimiServer.instance(Data(#"{"server_id":"01M8","pid":60964,"host":"127.0.0.1","port":58627,"started_at":1795631803172,"heartbeat_at":1795631803389,"host_version":"2.8.0","product":"kimi-code-cli"}"#.utf8))
        suite.expect(instance == AgentKimiServer.Instance(pid: 60964, host: "127.0.0.1", port: 58627, product: "kimi-code-cli")
                     && instance?.origin == "http://127.0.0.1:58627",
                     "an instance file names the server's process and address")
        suite.expect(AgentKimiServer.Instance(pid: 1, host: "0.0.0.0", port: 9, product: "").origin == "http://127.0.0.1:9"
                     && AgentKimiServer.Instance(pid: 1, host: "::", port: 9, product: "").origin == "http://[::1]:9"
                     && AgentKimiServer.instance(Data(#"{"pid":0,"port":1}"#.utf8)) == nil,
                     "an address for every host, nothing without a process")
        suite.expect(instance.flatMap { AgentKimiServer.page($0, session: session, token: "t-1") }?.absoluteString
                        == "http://127.0.0.1:58627/sessions/\(session)#token=t-1",
                     "`kimi web` shows a session at its own page")

        func same(_ a: [String: Any]?, _ b: [String: Any]) -> Bool { a.map { NSDictionary(dictionary: $0).isEqual(to: b) } ?? false }
        suite.expect(same(AgentKimiServer.approval(.allow, plan: false), ["kind": "approval", "decision": "approved"])
                     && same(AgentKimiServer.approval(.allowAlways, plan: false),
                             ["kind": "approval", "decision": "approved", "scope": "session"])
                     && same(AgentKimiServer.approval(.deny, plan: false),
                             ["kind": "approval", "decision": "rejected", "feedback": ClaudeHookMessage.denyMessage])
                     && same(AgentKimiServer.approval(.deny, plan: true),
                             ["kind": "approval", "decision": "rejected", "feedback": ClaudeHookMessage.planDenyMessage])
                     && AgentKimiServer.approval(.answer([:]), plan: false) == nil,
                     "approvals as the server takes them")
        let listed = try? JSONSerialization.jsonObject(with: Data(#"""
        {"interaction_id":"question_77b9","tool_call_id":"call_f587936f","kind":"question","request":{"questions":[
        {"id":"q_0","question":"Which database?","options":[{"id":"opt_0_0","label":"Postgres"},{"id":"opt_0_1","label":"SQLite"}],"header":"DB","multi_select":false,"allow_other":true},
        {"id":"q_1","question":"Which extras do you want?","options":[{"id":"opt_1_0","label":"Tests"},{"id":"opt_1_1","label":"Docs"},{"id":"opt_1_2","label":"CI"}],"header":"Extras","multi_select":true,"allow_other":true}]}}
        """#.utf8)) as? [String: Any]
        let input = askingTwo["tool_input"] as? [String: Any] ?? [:]
        let questions = ClaudeQuestion.questions(in: input) ?? []
        let body = AgentKimiServer.answers(.answer(["Which database?": ["SQLite"], "Which extras do you want?": ["Tests", "CI"]]),
                                           interaction: listed ?? [:], questions: questions)
        suite.expect(same(body, ["kind": "question", "answers": [
            "q_0": ["kind": "single", "option_id": "opt_0_1"],
            "q_1": ["kind": "multi", "option_ids": ["opt_1_0", "opt_1_2"]]]]),
                     "answers go by the server's own ids: \(String(describing: body))")
        let typed = AgentKimiServer.answers(.answer(["Which database?": ["MySQL"], "Which extras do you want?": ["Docs", "Lint"]]),
                                            interaction: listed ?? [:], questions: questions)
        suite.expect(same(typed, ["kind": "question", "answers": [
            "q_0": ["kind": "other", "text": "MySQL"],
            "q_1": ["kind": "multi_with_other", "option_ids": ["opt_1_1"], "other_text": "Lint"]]]),
                     "a typed answer is the question's other: \(String(describing: typed))")
        suite.expect(same(AgentKimiServer.answers(.deny, interaction: listed ?? [:], questions: questions),
                          ["kind": "question", "dismissed": true])
                     && AgentKimiServer.answers(.answer(["Which database?": ["SQLite"]]), interaction: listed ?? [:],
                                                questions: questions) == nil,
                     "Deny dismisses it; an unanswered question sends nothing")
        let prompt = AgentKimiServer.prompt("hello")
        suite.expect((prompt["prompt_id"] as? String)?.hasPrefix("openbar-") == true
                     && (prompt["content"] as? [[String: Any]])?.first?["text"] as? String == "hello",
                     "a reply is a prompt of its own")

        // The process a session's hooks run in is its server's.
        let manager = FileManager.default
        let folder = manager.temporaryDirectory.appending(path: "vs-kimi-server-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: folder) }
        let instances = folder.appending(path: "server/instances")
        try? manager.createDirectory(at: instances, withIntermediateDirectories: true)
        try? Data(#"{"pid":60964,"host":"127.0.0.1","port":58627,"product":"kimi-code-cli"}"#.utf8)
            .write(to: instances.appending(path: "01A.json"))
        try? Data(#"{"pid":70000,"host":"127.0.0.1","port":58700,"product":"kimi-code-desktop"}"#.utf8)
            .write(to: instances.appending(path: "01B.json"))
        try? Data("  tok-1\n".utf8).write(to: folder.appending(path: "server.token"))
        suite.expect(AgentKimiServer.serving(60964, folder: folder, alive: { _ in true })?.port == 58627
                     && AgentKimiServer.serving(59299, folder: folder, alive: { _ in true }) == nil
                     && AgentKimiServer.serving(60964, folder: folder, alive: { _ in false }) == nil
                     && AgentKimiServer.token(folder: folder) == "tok-1",
                     "a session's server is its hooks' process, while it runs; its token trimmed")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "vs-kimi-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let folder = home.appending(path: ".kimi-code")
        let config = folder.appending(path: "config.toml")
        suite.expect(AgentKimi.config(home: home, environment: [:]) == config
                     && AgentKimi.folder(home: home, environment: ["KIMI_CODE_HOME": "/k"]).path == "/k"
                     && AgentKimi.folder(home: home, environment: ["KIMI_CODE_HOME": ""]) == folder
                     && AgentHookInstaller.file(.kimi, folder: URL(fileURLWithPath: "/k"), bundle: bundle)?.path == "/k/config.toml",
                     "its folder, where KIMI_CODE_HOME moves it")
        suite.expect(AgentHookInstaller.state(.kimi, home: home, socket: socket, bundle: bundle) == .missing, "no file, no hooks")

        let own = """
        default_model = "kimi-code/k3"
        # my notes stay

        [providers."managed:kimi-code"]
        type = "kimi"

        [[hooks]]
        event = "Notification"
        matcher = "task\\\\.completed"
        command = "terminal-notifier -message done"

        """
        try? manager.createDirectory(at: folder, withIntermediateDirectories: true)
        try? Data(own.utf8).write(to: config)
        try? AgentHookInstaller.install(.kimi, home: home, socket: socket, bundle: bundle, folder: folder)
        let text = (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        suite.expect(text.hasPrefix(own + "\n" + AgentHookInstaller.vibeStart(bundle: bundle)) && text.hasSuffix(AgentHookInstaller.vibeEnd(bundle: bundle) + "\n"),
                     "the island's tables go after the person's own, which stay as they were")
        let added = String(text.dropFirst(own.count))
        let keys = Set(added.components(separatedBy: "\n").compactMap { line -> String? in
            guard let equals = line.firstIndex(of: "="), !line.hasPrefix("#") else { return nil }
            return line[..<equals].trimmingCharacters(in: .whitespaces)
        })
        suite.expect(keys == ["event", "command", "timeout"]
                     && added.components(separatedBy: "[[hooks]]").count - 1 == AgentKimi.events.count
                     && added.contains("timeout = \(AgentHookInstaller.eventTimeout)\n")
                     && !added.contains("timeout = \(AgentHookInstaller.eventTimeout).0"),
                     "one table per event, with only the keys Kimi takes and a whole-number timeout: \(keys)")
        suite.expect(added.contains(#"\"openbar\":\"kimi\",\"hook\":\"PermissionRequest\""#)
                     && added.contains(">/dev/null") && added.contains("# openbar-hook com.openbar.app.dev"),
                     "each command sends its event, prints nothing and carries the marker")
        suite.expect(AgentHookInstaller.state(.kimi, home: home, socket: socket, bundle: bundle, folder: folder) == .installed,
                     "it reads as installed")
        suite.expect(AgentHookInstaller.state(.kimi, home: home, socket: "/tmp/other.sock", bundle: bundle, folder: folder) == .outdated,
                     "another socket is older")
        try? AgentHookInstaller.install(.kimi, home: home, socket: socket, bundle: bundle, folder: folder)
        suite.expect((try? String(contentsOf: config, encoding: .utf8)) == text, "installing again changes nothing")

        // Kimi adds a table after the island's while saving its own settings.
        try? Data((text + "\n[models.extra]\nprovider = \"x\"\n").utf8).write(to: config)
        try? AgentHookInstaller.remove(.kimi, home: home, bundle: bundle, folder: folder)
        let removed = (try? String(contentsOf: config, encoding: .utf8)) ?? ""
        suite.expect(!removed.contains("openbar-hook") && removed.hasPrefix(own) && removed.contains("[models.extra]")
                     && removed.contains("terminal-notifier"),
                     "Remove takes only the island's tables, whatever Kimi added after: \(removed)")
        try? Data(own.utf8).write(to: config)
        try? AgentHookInstaller.install(.kimi, home: home, socket: socket, bundle: bundle, folder: folder)
        try? AgentHookInstaller.remove(.kimi, home: home, bundle: bundle, folder: folder)
        suite.expect((try? String(contentsOf: config, encoding: .utf8)) == own, "Install then Remove leaves the file as it was")

        try? Data("hooks = [{ event = \"Stop\", command = \"say hi\" }]\n".utf8).write(to: config)
        suite.expect(AgentHookInstaller.state(.kimi, home: home, socket: socket, bundle: bundle, folder: folder) == .unreadable,
                     "inline hooks can't take tables beside them")
        do {
            try AgentHookInstaller.install(.kimi, home: home, socket: socket, bundle: bundle, folder: folder)
            suite.expect(false, "inline hooks are left alone")
        } catch {
            suite.expect((try? String(contentsOf: config, encoding: .utf8))?.hasPrefix("hooks = [") == true,
                         "inline hooks are left alone")
        }
    }

    // MARK: Log

    static func log(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "vs-kimi-log-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        let folder = home.appending(path: ".kimi-code")
        let dir = folder.appending(path: "sessions/wd_rocket_1/\(session)")
        try? manager.createDirectory(at: dir.appending(path: "agents/main"), withIntermediateDirectories: true)
        try? Data("""
        {"sessionId":"session_other","sessionDir":"/x","workDir":"/w"}
        {"sessionId":"\(session)","sessionDir":"\(dir.path)","workDir":"/Users/a/code/rocket"}

        """.utf8).write(to: folder.appending(path: "session_index.jsonl"))
        suite.expect(AgentKimi.sessionFolder(session, home: home, environment: [:]) == dir.path
                     && AgentKimi.sessionFolder("session_none", home: home, environment: [:]) == nil,
                     "a session's folder from Kimi's index")
        let wire = AgentKimi.wire(sessionFolder: dir.path)
        let lines = [
            #"{"type":"turn.prompt","agentId":"main","input":[{"type":"text","text":"tool echo hi"}],"turnId":1}"#,
            #"{"type":"context.append_loop_event","agentId":"main","event":{"type":"content.part","turnId":"1","step":1,"stepUuid":"s1","part":{"type":"text","text":"Let me run it."}}}"#,
            #"{"type":"context.append_loop_event","agentId":"main","event":{"type":"tool.call","turnId":"1","toolCallId":"c1","name":"Bash","args":{"command":"echo hi"}}}"#,
            #"{"type":"context.append_loop_event","agentId":"main","event":{"type":"content.part","turnId":"1","step":2,"stepUuid":"s2","part":{"type":"text","text":"Done: "}}}"#,
            #"{"type":"context.append_loop_event","agentId":"main","event":{"type":"content.part","turnId":"1","step":2,"stepUuid":"s2","part":{"type":"text","text":"hi"}}}"#,
            #"{"type":"turn.ended","agentId":"main","turnId":1,"reason":"completed","replyModel":{"provider":"openai","model":"kimi-for-coding"}}"#,
            #"{"type":"prompt.completed","agentId":"main","reason":"completed"}"#,
        ]
        try? Data((lines.joined(separator: "\n") + "\n").utf8).write(to: URL(fileURLWithPath: wire))
        let last = AgentKimi.lastTurn(wire: wire, text: true)
        suite.expect(last.model == "kimi-for-coding" && last.text == "Done: hi",
                     "the last turn's model and the answer it ended on: \(last)")
        let quiet = AgentKimi.lastTurn(wire: wire, text: false)
        suite.expect(quiet.model == "kimi-for-coding" && quiet.text == nil, "without previews no text is read")
        suite.expect(AgentKimi.lastTurn(wire: "/nonexistent/wire.jsonl", text: true).model == nil, "no log, nothing")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        suite.expect(AgentCLI.command(.kimi) == "kimi" && !AgentCLI.startsWithPrompt(.kimi)
                     && AgentCLI.startArguments(.kimi, prompt: "hi").isEmpty,
                     "found as kimi; its terminal UI takes no first prompt")
        let home = URL(fileURLWithPath: "/Users/a")
        suite.expect(AgentCLI.locate(.kimi, path: "", home: home, isExecutable: { $0 == "/Users/a/.kimi-code/bin/kimi" })
                        == "/Users/a/.kimi-code/bin/kimi",
                     "found where its install script puts it")
        suite.expect(ClaudeSessionProvider.kimi.displayName == "Kimi Code" && ClaudeSessionProvider.kimi.shortName == "Kimi"
                     && ClaudeSessionProvider.kimi.answersPermissions && ClaudeSessionProvider.kimi.reportsThroughHooks
                     && ClaudeSessionProvider.kimi.character == .kimi && ClaudeSessionProvider.installedAgents.last == .kimi
                     && AgentHookInstaller.foldersByHand.contains(.kimi),
                     "named Kimi Code, answered on the island")
        suite.expect(AgentKimi.modelName("kimi-code/kimi-for-coding") == "kimi-for-coding" && AgentKimi.modelName("mock") == "mock",
                     "a model alias by its model's name")
        suite.expect(NotchHookAgentMarkPaths.mark(.kimi)?.layers.count == 2
                     && NotchHookAgentMarkPaths.kimiHead.hasPrefix("M"),
                     "its mark draws: the head with its dot, and the bar")
    }
}
