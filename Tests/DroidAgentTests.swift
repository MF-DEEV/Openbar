// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Factory Droid in the Sessions list (#242): what its hooks send (payloads
/// copied from Droid 1.19.0 runs), the sessions kept from them, the
/// prompts, questions and specs answered with keys, the island's entries in
/// its two hook files, and how it is found and named.
enum DroidAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("droid: hooks") { hooks(suite) }
        suite.run("droid: permissions") { permissions(suite) }
        suite.run("droid: questions and specs") { questions(suite) }
        suite.run("droid: keys") { keys(suite) }
        suite.run("droid: installer") { installer(suite) }
        suite.run("droid: logs") { logs(suite) }
        suite.run("droid: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    static let session = "63741b7f-01fb-44b3-9855-9b3503848f54"
    static let transcript = "/Users/a/.factory/sessions/-Users-a-code-rocket/\(session).jsonl"

    /// The fields Droid's hook input carries for every event.
    static func common(_ event: String, _ extra: [String: Any] = [:]) -> [String: Any] {
        var json: [String: Any] = [
            "session_id": session, "transcript_path": transcript, "cwd": "/Users/a/code/rocket",
            "permission_mode": "auto-medium", "hook_event_name": event,
        ]
        for (key, value) in extra { json[key] = value }
        return json
    }

    static func envelope(_ event: String, _ extra: [String: Any] = [:]) -> Data {
        T.envelope("droid", hook: event, pid: 56083, common(event, extra))
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, _ extra: [String: Any] = [:],
                     policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, envelope(event, extra), policy: policy, at: date)
    }

    static func tool(_ name: String, _ input: [String: Any]) -> [String: Any] { ["tool_name": name, "tool_input": input] }

    static let danger = tool("Execute", ["summary": "Remove the scratch folder now", "command": "rm -rf ./scratch-dir",
                                         "riskLevel": "high"])
    static let asking = tool("AskUser", ["questionnaire":
        "1. [question] Which color should the button be?\n[topic] Color\n[option] Red\n[option] Blue\n[option] Green"])
    static let askingTwo = tool("AskUser", ["questionnaire":
        "1. [question] Which database?\n[topic] DB\n[option] Postgres\n[option] SQLite\n\n"
        + "2. [question] Which extras do you want? (multi)\n[topic] Extras\n[context] Each adds a day.\n"
        + "[option] Tests (Recommended)\n[option] Docs\n[option] CI"])
    static let spec = tool("ExitSpecMode", ["title": "Button plan", "plan": "## Plan\n\n1. Add a button.\n2. Test it."])

    static func notification(_ type: String, _ message: String) -> [String: Any] {
        ["notification_type": type, "message": message]
    }

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "SessionStart", ["source": "startup", "CLAUDE_ENV_FILE": "/Users/a/.factory/temp/x"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "droid:\(session)" && listed[0].provider == .droid
                     && listed[0].project == "rocket" && listed[0].phase == .idle,
                     "a session start lists it idle in its folder: \(listed)")
        send(tracker, "UserPromptSubmit", ["prompt": "tool echo hi > hi.txt", "has_images": false,
                                           "message_id": "a815d7aa-3c36-4139-a47b-5c9cb35f39c9"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["tool echo hi > hi.txt"],
                     "a prompt opens a turn, its text with previews")
        let hidden = AgentHookSessions()
        send(hidden, "UserPromptSubmit", ["prompt": "secret plan"], policy: T.off)
        suite.expect(T.snapshot(hidden, previews: false).sessions.first?.chat.isEmpty == true,
                     "without previews the prompt isn't kept")
        let echo = tool("Execute", ["summary": "Run the test command now", "command": "echo hi > hi.txt", "riskLevel": "medium"])
        send(tracker, "PreToolUse", echo)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "echo hi > hi.txt",
                     "Execute runs as Bash: \(String(describing: listed[0].currentTool))")
        var done = echo
        done["tool_response"] = "Command completed successfully\n\n[Process exited with code 0]"
        send(tracker, "PostToolUse", done)
        var failed = tool("Create", ["file_path": "/Users/a/code/rocket/a.txt", "content": "x"])
        send(tracker, "PreToolUse", failed)
        failed["tool_response"] = "Error: no room\n\n[Process exited with code 1]"
        send(tracker, "PostToolUse", failed)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 2 && listed[0].timeline[0].finished != nil
                     && !listed[0].timeline[0].failed && listed[0].timeline[1].name == "Write" && listed[0].timeline[1].failed,
                     "a tool that ran is done, a failed one says so: \(listed[0].timeline)")
        suite.expect(!AgentHookMessage.droidFailed("ok") && AgentHookMessage.droidFailed(["error": "x"])
                     && !AgentHookMessage.droidFailed("a [Process exited with code 0]")
                     && AgentHookMessage.droidFailed("[Process exited with code 127]"),
                     "a failure is a non-zero exit or an error")
        send(tracker, "Stop", ["message_id": "95597f2e-c545-4bf8-b78d-0d2d1bb3dced", "stop_hook_active": false,
                               "tool_execution_count": 2, "elapsed_time": 21])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .finished, "Stop ends the turn done")
        let stop = AgentHookMessage.decode(envelope("Stop", ["message_id": "m-1"]), now: now, policy: T.on)
        suite.expect(stop?.replyID == "m-1" && stop?.transcript == transcript, "Stop names its reply, for the transcript to tell")

        // Escape mid-turn: an idle note, no Stop.
        send(tracker, "UserPromptSubmit", ["prompt": "slow"])
        send(tracker, "Notification", notification("idle_prompt", "Agent stopped by user and is waiting for input"))
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .idle, "a turn the person stopped ends, not done: \(listed[0].phase)")
        send(tracker, "SessionEnd", ["reason": "other", "session_duration_ms": 289132.3, "message_count": 84])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "quitting takes the row away")
        let gone = AgentHookSessions()
        send(gone, "UserPromptSubmit", ["prompt": "hi"])
        gone.prune(now: now, alive: { _ in false })
        suite.expect(T.snapshot(gone).sessions.isEmpty, "no ghost row once Droid's process is gone")
    }

    // MARK: Permissions

    static func permissions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "danger"])
        suite.expect(!AgentHookMessage.waits(envelope("PreToolUse", danger)), "no hook of Droid's waits on the island")
        send(tracker, "PreToolUse", danger)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].permission == nil, "a tool alone asks nothing yet")
        send(tracker, "Notification", notification("permission_prompt", "Factory CLI needs permission to execute 1 tool(s)"))
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.tool == "Bash"
                     && listed[0].permission?.detail == "rm -rf ./scratch-dir" && listed[0].permission?.content != nil
                     && listed[0].waitingFor == nil,
                     "the prompt shows the command its PreToolUse named, answerable here: \(String(describing: listed[0].permission))")
        // In a terminal that takes no keys, it is answered in Droid.
        let elsewhere = T.snapshot(tracker, host: .ghostty(tty: "/dev/ttys009")).sessions
        suite.expect(elsewhere[0].permission == nil && elsewhere[0].waitingFor == CodexSessionState.answerInTerminal,
                     "where keys can't go, the row says to answer in Droid")
        var ran = danger
        ran["tool_response"] = "Command completed successfully\n\n[Process exited with code 0]"
        send(tracker, "PostToolUse", ran)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "a tool that ran was answered")
        // Denied: no PostToolUse, the turn stops.
        send(tracker, "PreToolUse", danger)
        send(tracker, "Notification", notification("permission_prompt", "Factory CLI needs permission to execute 1 tool(s)"))
        send(tracker, "Stop", ["message_id": "x"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .finished, "a denied call ends the turn")
        // An edit shows what it changes.
        let edit = tool("Edit", ["file_path": "/Users/a/code/rocket/a.txt", "old_str": "a", "new_str": "b"])
        send(tracker, "UserPromptSubmit", ["prompt": "edit"])
        send(tracker, "PreToolUse", edit)
        send(tracker, "Notification", notification("permission_prompt", "Factory CLI needs permission to execute 1 tool(s)"))
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission?.tool == "Edit", "an edit asks as Edit: \(String(describing: listed[0].permission))")
    }

    // MARK: Questions and specs

    static func questions(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "UserPromptSubmit", ["prompt": "ask"])
        send(tracker, "PreToolUse", asking)
        suite.expect(T.snapshot(tracker).sessions[0].currentTool == nil, "a question doesn't run as a tool")
        send(tracker, "Notification", notification("elicitation_dialog", "Factory CLI is asking the user 1 question(s)"))
        var listed = T.snapshot(tracker).sessions
        let asked = listed[0].permission
        suite.expect(listed[0].phase == .waitingPermission && asked?.asksQuestions == true
                     && asked?.questions.first?.question == "Which color should the button be?"
                     && asked?.questions.first?.header == "Color"
                     && asked?.questions.first?.options.map(\.label) == ["Red", "Blue", "Green"],
                     "its question shows on the island: \(String(describing: asked))")
        var answered = asking
        answered["tool_response"] = "1. [question] Which color should the button be?\n[answer] Blue"
        send(tracker, "PostToolUse", answered)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].timeline.isEmpty, "answered, it settles without a tool row")

        let two = AgentDroid.questions(in: askingTwo["tool_input"].flatMap { ($0 as? [String: Any])?["questionnaire"] as? String } ?? "")
        suite.expect(two?.count == 2 && two?[1].multiSelect == true && two?[1].question == "Which extras do you want?"
                     && two?[1].options.map(\.label) == ["Tests (Recommended)", "Docs", "CI"] && two?[1].header == "Extras",
                     "several questions, one of several picks: \(String(describing: two))")
        suite.expect(AgentDroid.questions(in: "1. [question] Pick?\n[option] Only") == nil
                     && AgentDroid.questions(in: "no questions here") == nil,
                     "a question with one choice, or none, isn't read")

        // A question the island can't read waits in Droid.
        let bare = AgentHookSessions()
        send(bare, "UserPromptSubmit", ["prompt": "ask"])
        send(bare, "PreToolUse", tool("AskUser", ["questionnaire": "What now?"]))
        send(bare, "Notification", notification("elicitation_dialog", "Factory CLI is asking the user 1 question(s)"))
        listed = T.snapshot(bare).sessions
        suite.expect(listed[0].phase == .waitingInput && listed[0].permission == nil
                     && listed[0].waitingFor == CodexSessionState.answerInTerminal,
                     "an unreadable question is answered in Droid: \(listed[0].phase)")

        let planning = AgentHookSessions()
        send(planning, "UserPromptSubmit", ["prompt": "plan"])
        send(planning, "PreToolUse", spec)
        send(planning, "Notification", notification("permission_prompt", "Factory CLI needs permission to execute 1 tool(s)"))
        let plan = T.snapshot(planning).sessions[0].permission
        suite.expect(plan?.tool == ClaudeToolSummary.planTool && plan?.planModes == [.auto, .acceptEdits, .manual]
                     && plan?.detail.contains("Add a button") == true,
                     "a spec shows as a plan, with the autonomy it goes on with: \(String(describing: plan))")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        let command = AgentHookMessage.grokAsk("Bash", input: ["command": "rm -rf x"], cwd: nil, at: now)
        let downs = Array(repeating: ClaudeKey.down, count: AgentDroid.denyDowns)
        suite.expect(AgentDroid.keys(.allow, command) == [.enter]
                     && AgentDroid.keys(.deny, command) == [.keys(downs), .enter]
                     && AgentDroid.keys(.deny, command, oneBurst: true) == [.keys(downs), .enter]
                     && AgentDroid.keys(.allowAlways, command) == nil,
                     "Return allows; arrows down to No, cancel and Return deny, in Terminal too")
        guard let one = AgentDroid.ask("AskUserQuestion", raw: "AskUser", input: asking["tool_input"] as? [String: Any] ?? [:],
                                       cwd: nil, at: now) else { return suite.expect(false, "a question") }
        let question = "Which color should the button be?"
        suite.expect(AgentDroid.keys(.answer([question: ["Blue"]]), one) == [.keys([.digit(2)])]
                     && AgentDroid.keys(.answer([question: ["Blue"]]), one, oneBurst: true) == [.keys([.digit(2)])],
                     "a single choice is its number, which answers it at once")
        suite.expect(AgentDroid.keys(.answer([question: ["Purple please"]]), one) == nil
                     && AgentDroid.keys(.deny, one) == [.keys([.escape])] && AgentDroid.keys(.deny, one, oneBurst: true) == nil
                     && AgentDroid.keys(.allow, one) == nil,
                     "a typed answer is given in Droid; Escape never goes with Terminal's return")
        let two = AgentDroid.questions(in: (askingTwo["tool_input"] as? [String: Any])?["questionnaire"] as? String ?? "") ?? []
        suite.expect(AgentDroid.questionKeys(["Which database?": ["SQLite"], "Which extras do you want?": ["CI", "Tests (Recommended)"]], two)
                        == [.keys([.digit(2), .digit(1), .digit(3), .tab]), .wait(0.3), .enter],
                     "numbers in order, Tab after a multiple choice, Return on the review")
        suite.expect(AgentDroid.questionKeys(["Which database?": ["SQLite"]], two) == nil, "every question needs an answer")
        guard let plan = AgentDroid.ask("ExitPlanMode", raw: "ExitSpecMode", input: spec["tool_input"] as? [String: Any] ?? [:],
                                        cwd: nil, at: now) else { return suite.expect(false, "a spec") }
        suite.expect(AgentDroid.keys(.allow, plan) == [.keys([.digit(1)]), .wait(0.4), .keys([.digit(3)])]
                     && AgentDroid.keys(.approvePlan(.acceptEdits), plan) == [.keys([.digit(1)]), .wait(0.4), .keys([.digit(2)])]
                     && AgentDroid.keys(.approvePlan(.manual), plan) == [.keys([.digit(1)]), .wait(0.4), .keys([.digit(1)])]
                     && AgentDroid.keys(.approvePlan(.bypass), plan) == nil
                     && AgentDroid.keys(.deny, plan) == [.keys([.escape])] && AgentDroid.keys(.deny, plan, oneBurst: true) == nil,
                     "a spec proceeds, then takes the autonomy's number; Escape keeps planning")
        suite.expect(ClaudeKey.tab.bytes == "\t" && ClaudeKey.tab.multiplexerName == "Tab" && ClaudeKey.tab.zellijName == "Tab"
                     && ClaudeKey.tab.kittyName == "tab" && ClaudeKey.tab.appleScript == "(character id 9)",
                     "Tab in every terminal's words")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "vs-droid-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let folder = home.appending(path: ".factory")
        let settingsURL = folder.appending(path: "settings.json")
        let hooksURL = folder.appending(path: "hooks.json")
        suite.expect(AgentDroid.settings(home: home, environment: [:]) == settingsURL
                     && AgentDroid.folder(home: home, environment: ["FACTORY_HOME_OVERRIDE": "/f"]).path == "/f/.factory"
                     && AgentDroid.folder(home: home, environment: ["FACTORY_HOME_OVERRIDE": ""]) == folder
                     && AgentHookInstaller.file(.droid, folder: URL(fileURLWithPath: "/f/.factory"), bundle: bundle)?.path
                        == "/f/.factory/settings.json",
                     "its folder, where FACTORY_HOME_OVERRIDE moves it")
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .missing, "no file, no hooks")

        try? manager.createDirectory(at: folder, withIntermediateDirectories: true)
        try? Data(#"{"logoAnimation":"off","customModels":[{"model":"m","displayName":"Mine"}]}"#.utf8).write(to: settingsURL)
        try? AgentHookInstaller.install(.droid, home: home, socket: socket, bundle: bundle, folder: folder)
        var settings = (try? ClaudeHookInstaller.read(settingsURL)) ?? [:]
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        let pre = (hooks["PreToolUse"] as? [[String: Any]])?.first
        let stop = (hooks["Stop"] as? [[String: Any]])?.first
        let handler = (pre?["hooks"] as? [[String: Any]])?.first
        suite.expect(Set(hooks.keys) == Set(AgentDroid.events) && pre?["matcher"] as? String == "*" && stop?["matcher"] == nil
                     && handler?["type"] as? String == "command" && handler?["timeout"] as? Int == AgentHookInstaller.eventTimeout
                     && (handler?["command"] as? String)?.contains(#"{"openbar":"droid","hook":"PreToolUse""#) == true
                     && (handler?["command"] as? String)?.hasSuffix("# openbar-hook com.openbar.app.dev") == true
                     && settings["logoAnimation"] as? String == "off" && settings["customModels"] != nil
                     && !manager.fileExists(atPath: hooksURL.path),
                     "every event in settings.json beside its other keys, no hooks.json made: \(hooks)")
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .installed, "it reads as installed")
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: "/tmp/other.sock", bundle: bundle) == .outdated,
                     "another socket is older")

        // The person's hooks.json shadows an event in settings.json: the island's entry goes beside theirs.
        try? Data(#"{"hooksDisabled":false,"Stop":[{"hooks":[{"type":"command","command":"say done"}]}],"PreCompact":[]}"#.utf8)
            .write(to: hooksURL)
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .outdated,
                     "an entry hooks.json shadows doesn't run, so it isn't installed")
        try? AgentHookInstaller.install(.droid, home: home, socket: socket, bundle: bundle, folder: folder)
        settings = (try? ClaudeHookInstaller.read(settingsURL)) ?? [:]
        hooks = settings["hooks"] as? [String: Any] ?? [:]
        var file = (try? ClaudeHookInstaller.read(hooksURL)) ?? [:]
        let stops = file["Stop"] as? [[String: Any]] ?? []
        suite.expect(hooks["Stop"] == nil && hooks.count == AgentDroid.events.count - 1 && stops.count == 2
                     && ((stops[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String) == "say done"
                     && file["hooksDisabled"] as? Bool == false && (file["PreCompact"] as? [Any])?.isEmpty == true
                     && file["PreToolUse"] == nil,
                     "Stop moves after the person's in hooks.json, the rest stays in settings.json: \(file) \(hooks)")
        let backups = ((try? manager.contentsOfDirectory(atPath: folder.path)) ?? []).filter { $0.contains(".openbar-") }
        suite.expect(backups.contains("hooks.json.openbar-original.bak") && backups.contains("settings.json.openbar-original.bak"),
                     "a copy of each file is kept first: \(backups)")
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .installed,
                     "installed across both files")

        try? AgentHookInstaller.remove(.droid, home: home, bundle: bundle)
        settings = (try? ClaudeHookInstaller.read(settingsURL)) ?? [:]
        file = (try? ClaudeHookInstaller.read(hooksURL)) ?? [:]
        suite.expect(settings["hooks"] == nil && settings["logoAnimation"] as? String == "off"
                     && (file["Stop"] as? [[String: Any]])?.count == 1 && (file["PreCompact"] as? [Any])?.isEmpty == true
                     && file["hooksDisabled"] as? Bool == false && !ClaudeHookInstaller.hasAny(in: ["hooks": file], bundle: bundle),
                     "Remove takes only the island's entries, the person's empty list stays: \(file) \(settings)")
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .missing, "gone")

        // The older hooks/hooks.json while there is no hooks.json.
        try? manager.removeItem(at: hooksURL)
        let legacy = folder.appending(path: "hooks/hooks.json")
        try? manager.createDirectory(at: legacy.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(#"{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}"#.utf8).write(to: legacy)
        suite.expect(AgentDroid.hooksFile(in: folder) == legacy, "the older file is read while there is no hooks.json")
        try? AgentHookInstaller.install(.droid, home: home, socket: socket, bundle: bundle, folder: folder)
        let old = (try? ClaudeHookInstaller.read(legacy)) ?? [:]
        suite.expect((old["SessionStart"] as? [[String: Any]])?.count == 2 && old.count == 1,
                     "the island's SessionStart joins the person's there: \(old)")

        // A file Droid would read with comments isn't the island's to rewrite.
        try? Data("{ // mine\n}".utf8).write(to: settingsURL)
        suite.expect(AgentHookInstaller.state(.droid, home: home, socket: socket, bundle: bundle) == .unreadable,
                     "settings with comments are left alone")
        let before = try? Data(contentsOf: legacy)
        try? AgentHookInstaller.install(.droid, home: home, socket: socket, bundle: bundle, folder: folder)
        suite.expect((try? Data(contentsOf: legacy)) == before, "and so is the other file")
    }

    // MARK: Logs

    static func logs(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "vs-droid-log-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        let sessions = home.appending(path: ".factory/sessions/-Users-a-code-rocket")
        try? manager.createDirectory(at: sessions, withIntermediateDirectories: true)
        let log = sessions.appending(path: "\(session).jsonl")
        let lines = [
            #"{"type":"session_start","id":"\#(session)","title":"hello there","cwd":"/Users/a/code/rocket"}"#,
            #"{"type":"message","id":"a815","message":{"role":"user","content":[{"type":"text","text":"hello there"}]}}"#,
            #"{"type":"message","id":"95597f2e","message":{"role":"assistant","content":[{"type":"text","text":"Mock reply"}],"modelId":"custom:Mock-Coder-0"}}"#,
        ]
        try? Data((lines.joined(separator: "\n") + "\n").utf8).write(to: log)
        suite.expect(AgentDroid.reply(transcript: log.path, message: "95597f2e") == "Mock reply"
                     && AgentDroid.reply(transcript: log.path, message: "a815") == nil
                     && AgentDroid.reply(transcript: "/nope", message: "x") == nil,
                     "a Stop's reply is read by its message id")
        try? Data(#"{"model":"custom:Mock-Coder-0","autonomyMode":"auto-medium"}"#.utf8)
            .write(to: sessions.appending(path: "\(session).settings.json"))
        try? Data(#"{"customModels":[{"model":"mock-coder","displayName":"Mock Coder","baseUrl":"http://x"}]}"#.utf8)
            .write(to: home.appending(path: ".factory/settings.json"))
        suite.expect(AgentDroid.model(transcript: log.path, home: home, environment: [:]) == "Mock Coder",
                     "a custom model goes by its name")
        try? Data(#"{"model":"claude-sonnet-4-5-20250929"}"#.utf8).write(to: sessions.appending(path: "\(session).settings.json"))
        suite.expect(AgentDroid.model(transcript: log.path, home: home, environment: [:]) == "claude-sonnet-4-5-20250929"
                     && AgentDroid.model(transcript: "/nope.jsonl", home: home, environment: [:]) == nil,
                     "Factory's own models by their id")
        suite.expect(AgentDroid.customName("custom:Mock-Coder-7", settings: Data(#"{"customModels":[]}"#.utf8)) == nil,
                     "a custom model that isn't listed keeps its id")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.droid, home: home, path: nil,
                                                  isExecutable: { $0 == "/Users/someone/.local/bin/droid" },
                                                  exists: { $0 == "/Users/someone/.factory" })
                     && !AgentHookInstaller.isPresent(.droid, home: home, path: "/usr/bin",
                                                      isExecutable: { $0 == "/usr/bin/droid" }, exists: { _ in false })
                     && !AgentHookInstaller.isPresent(.droid, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".factory") }),
                     "found by its command and its folder, never by one alone")
        suite.expect(ClaudeSessionProvider.droid.displayName == "Factory Droid" && ClaudeSessionProvider.droid.shortName == "Droid"
                     && ClaudeSessionProvider.installedAgents.contains(.droid) && ClaudeSessionProvider.droid.answersPermissions
                     && ClaudeSessionProvider.droid.reportsThroughHooks && !ClaudeSessionProvider.droid.takesRepliesDirectly
                     && AgentCLI.command(.droid) == "droid" && AgentCLI.startArguments(.droid, prompt: "go") == ["--", "go"]
                     && AgentCLI.startsWithPrompt(.droid) && AgentHookInstaller.foldersByHand.contains(.droid)
                     && ClaudeTyping.enterDelay(.droid) == 0.5,
                     "named, listed last, answered here, started with its first prompt")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        suite.expect(text.agentHint(.droid).contains("~/.factory")
                     && text.answerIn(.droid) == "Answer this one in Factory Droid."
                     && NotchClaudeSessionsSupport.agentKey(.droid) == DefaultsKey.notchSessionsAgentDroid,
                     "its hint, where it is answered, and its own switch")
        suite.expect(ClaudeSessionProvider.droid.character == .droid && ClawdCharacter.droid.favouredShell == .coral
                     && NotchHookAgentMarkPaths.mark(.droid) != nil
                     && AgentHookInstaller.hookFile(.droid, home: home, bundle: "b")?.wholeFile == false
                     && AgentHookInstaller.agentFolder(.droid, home: home, environment: [:])?.path == "/Users/someone/.factory",
                     "its own figure, shell, mark and settings file")
        suite.expect(["Execute": "Bash", "Read": "Read", "Create": "Write", "Edit": "Edit", "ApplyPatch": "Edit",
                      "Grep": "Grep", "Glob": "Glob", "LS": "LS", "FetchUrl": "WebFetch", "AskUser": "AskUserQuestion",
                      "ExitSpecMode": "ExitPlanMode", "TodoWrite": "TodoWrite", "mcp__github__search": "github__search"]
                        .allSatisfy { AgentHookTools.name(.droid, $0.key) == $0.value },
                     "its tools under Claude Code's names")
    }
}
