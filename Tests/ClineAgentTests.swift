// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation
import SQLite3

/// Cline in the Sessions list (#241): its store and conversations (shapes
/// copied from Cline CLI 3.0.75 and the VS Code extension 4.1.27), the
/// sessions kept from them, the keys and hub frames that answer it, and
/// that nothing is installed into it.
enum ClineAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("cline: places") { places(suite) }
        suite.run("cline: store") { store(suite) }
        suite.run("cline: conversation") { conversation(suite) }
        suite.run("cline: tools and asks") { asks(suite) }
        suite.run("cline: watcher") { watcher(suite) }
        suite.run("cline: answers") { answers(suite) }
        suite.run("cline: keys") { keys(suite) }
        suite.run("cline: hub") { hub(suite) }
        suite.run("cline: nothing installed") { installed(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    // MARK: Fixtures

    static let cliSession = "1795622999301_iedbg"
    static let extensionSession = "1795623506687_zli8v"

    static func message(_ role: String, _ content: [[String: Any]], model: String? = nil, ts: Double = 1_795_622_999_400) -> [String: Any] {
        var json: [String: Any] = ["id": UUID().uuidString, "role": role, "content": content, "ts": ts]
        if let model { json["modelInfo"] = ["id": model, "provider": "openai-compatible"] }
        return json
    }

    static func typed(_ text: String) -> [String: Any] { ["type": "text", "text": "<user_input mode=\"act\">\(text)</user_input>"] }
    static func call(_ id: String, _ name: String, _ input: [String: Any]) -> [String: Any] {
        ["type": "tool_use", "id": id, "name": name, "input": input]
    }
    static func result(_ id: String, _ name: String, _ content: Any = "ok") -> [String: Any] {
        ["type": "tool_result", "tool_use_id": id, "name": name, "content": content]
    }

    static func file(_ messages: [[String: Any]]) -> Data {
        try! JSONSerialization.data(withJSONObject: ["version": 1, "sessionId": cliSession, "messages": messages,
                                                     "system_prompt": "You are Cline"])
    }

    static let approval = [
        message("user", [typed("tool echo two")]),
        message("assistant", [call("call_da0cd2e3", "run_commands", ["commands": ["echo two"]])], model: "mock-coder"),
    ]
    static let approved = approval + [
        message("user", [result("call_da0cd2e3", "run_commands", [["query": "echo two", "result": "two\n", "success": true]])]),
        message("assistant", [["type": "text", "text": "Done: two"]], model: "mock-coder"),
    ]
    static let question = approved + [
        message("user", [typed("ask")]),
        message("assistant", [call("call_79e96d2d", "ask_question",
                                   ["question": "Which color should the button be?", "options": ["Red", "Blue", "Green"]])],
                model: "mock-coder"),
    ]

    static func transcript(_ messages: [[String: Any]]) -> AgentCline.Transcript { AgentCline.transcript(data: file(messages))! }

    static func record(_ status: String, id: String = cliSession, source: String = "cli", pid: Int32 = 97843,
                       messages: String? = "/m/cli.json", title: String? = "tool echo two") -> AgentCline.Record {
        AgentCline.Record(id: id, source: source, pid: pid, status: status, cwd: "/Users/a/code/proj", model: "mock-coder",
                          title: title, messages: messages, started: now)
    }

    // MARK: Places

    static func places(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/a")
        suite.expect(AgentCline.dataFolder(home: home, environment: [:]).path == "/Users/a/.cline/data", "data in ~/.cline/data")
        suite.expect(AgentCline.dataFolder(home: home, environment: ["CLINE_DIR": "/x/c"]).path == "/x/c/data", "CLINE_DIR moves it")
        suite.expect(AgentCline.dataFolder(home: home, environment: ["CLINE_DIR": "/x/c", "CLINE_DATA_DIR": "/d"]).path == "/d",
                     "CLINE_DATA_DIR comes first")
        suite.expect(AgentCline.database(home: home, environment: [:]).path == "/Users/a/.cline/data/db/sessions.db", "the store")
        suite.expect(AgentCline.database(home: home, environment: ["CLINE_DB_DATA_DIR": "/db"]).path == "/db/sessions.db",
                     "CLINE_DB_DATA_DIR moves the store alone")
        suite.expect(AgentCline.folder(home: home, environment: [:]).path == "/Users/a/.cline", "the folder Cline was used from")
    }

    // MARK: Store

    /// A store with Cline's own table, as 3.0.75 makes it.
    static func makeStore(_ rows: [[String: Any?]]) -> URL {
        let folder = FileManager.default.temporaryDirectory.appending(path: "ob-cline-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appending(path: "sessions.db")
        var db: OpaquePointer?
        sqlite3_open(url.path, &db)
        sqlite3_exec(db, """
            CREATE TABLE sessions (session_id TEXT PRIMARY KEY, source TEXT NOT NULL, pid INTEGER NOT NULL,
              started_at TEXT NOT NULL, ended_at TEXT, exit_code INTEGER, status TEXT NOT NULL,
              status_lock INTEGER NOT NULL DEFAULT 0, interactive INTEGER NOT NULL, provider TEXT NOT NULL,
              model TEXT NOT NULL, cwd TEXT NOT NULL, workspace_root TEXT NOT NULL, team_name TEXT,
              enable_tools INTEGER NOT NULL, enable_spawn INTEGER NOT NULL, enable_teams INTEGER NOT NULL,
              parent_session_id TEXT, parent_agent_id TEXT, agent_id TEXT, conversation_id TEXT,
              is_subagent INTEGER NOT NULL DEFAULT 0, prompt TEXT, metadata_json TEXT,
              transcript_path TEXT NOT NULL DEFAULT '', hook_path TEXT NOT NULL, messages_path TEXT,
              updated_at TEXT NOT NULL);
            """, nil, nil, nil)
        for row in rows {
            func quote(_ value: Any?) -> String {
                guard let value else { return "NULL" }
                if let number = value as? Int { return String(number) }
                return "'" + "\(value)".replacingOccurrences(of: "'", with: "''") + "'"
            }
            let sql = """
                INSERT INTO sessions (session_id, source, pid, started_at, ended_at, status, interactive, provider, model, cwd,
                  workspace_root, enable_tools, enable_spawn, enable_teams, is_subagent, prompt, metadata_json, hook_path,
                  messages_path, updated_at) VALUES (\(quote(row["id"] ?? nil)), \(quote(row["source"] ?? "cli")),
                  \(quote(row["pid"] ?? 100)), '2026-11-25T16:09:59.304Z', \(quote(row["ended"] ?? nil)),
                  \(quote(row["status"] ?? "running")), 1, 'openai-compatible', 'mock-coder', '/w/proj', '/w/proj', 1, 1, 1,
                  \(quote(row["sub"] ?? 0)), 'secret prompt', \(quote(row["meta"] ?? nil)), '', \(quote(row["messages"] ?? nil)),
                  '2026-11-25T16:10:00.000Z')
                """
            sqlite3_exec(db, sql, nil, nil, nil)
        }
        sqlite3_close(db)
        return url
    }

    static func store(_ suite: TestSuite) {
        let url = makeStore([
            ["id": "a", "status": "pending", "meta": #"{"title":"tool echo two","modelId":"gpt-4o"}"#, "messages": "/m/a.json"],
            ["id": "b", "source": "vscode", "pid": 1139, "status": "idle"],
            ["id": "c", "status": "completed", "ended": "2026-11-25T16:20:24.658Z"],
            ["id": "d", "status": "running", "sub": 1],
        ])
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let records = AgentCline.records(database: url)
        suite.expect(records.map(\.id) == ["a", "b"], "only open sessions, no subagents: \(records.map(\.id))")
        let first = records.first
        suite.expect(first?.status == "pending" && first?.waitsForApproval == true && first?.isBusy == true, "pending waits")
        suite.expect(first?.title == "tool echo two" && first?.messages == "/m/a.json" && first?.cwd == "/w/proj",
                     "title, conversation and folder read")
        suite.expect(records.last?.isExtension == true && records.last?.pid == 1139 && records.last?.isBusy == false,
                     "the extension's session, idle")
        suite.expect(AgentCline.records(database: URL(fileURLWithPath: "/nonexistent/sessions.db")).isEmpty,
                     "no store, no sessions and no error")
    }

    // MARK: Conversation

    static func conversation(_ suite: TestSuite) {
        let waiting = transcript(approval)
        suite.expect(waiting.open?.id == "call_da0cd2e3" && waiting.open?.name == "run_commands", "the call without a result is open")
        suite.expect(waiting.lastUser == "tool echo two", "the person's words, out of Cline's wrapper")
        suite.expect(waiting.model == "mock-coder", "the model that answered")
        let done = transcript(approved)
        suite.expect(done.open == nil && done.finished == ["call_da0cd2e3"], "a result closes the call")
        suite.expect(done.lastText == "Done: two", "the last reply")
        suite.expect(AgentCline.typed("<hook_context source=\"ToolSuccess\">x</hook_context>") == nil,
                     "a note Cline adds isn't the person's")
        suite.expect(AgentCline.typed("plain words") == "plain words", "unwrapped text stays")
        suite.expect(AgentCline.transcript(data: Data("[]".utf8)) == nil, "not Cline's file")
        // The captured file of a real CLI session reads whole.
        let real = """
            {"version":1,"messages":[{"role":"user","content":[{"type":"text","text":"<user_input mode=\\"act\\">ask</user_input>"}],"ts":1795623087870},
            {"role":"assistant","content":[{"type":"tool_use","id":"call_79e96d2d","name":"ask_question","input":{"question":"Which color?","options":["Red","Blue"]}}],"ts":1795623087872,"modelInfo":{"id":"mock-coder","provider":"openai-compatible"}},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_79e96d2d","name":"ask_question","content":"Blue"}],"ts":1795623095794}]}
            """
        let read = AgentCline.transcript(data: Data(real.utf8))
        suite.expect(read?.calls.count == 1 && read?.open == nil && read?.lastUser == "ask", "a real conversation")
    }

    // MARK: Tools and asks

    static func asks(_ suite: TestSuite) {
        let bash = AgentCline.claudeTool(AgentCline.Call(id: "1", name: "run_commands", input: ["commands": ["npm test", "ls"]], at: nil))
        suite.expect(bash.name == "Bash" && bash.input["command"] as? String == "npm test\nls", "commands as Bash")
        let structured = AgentCline.commands(["commands": [["command": "git", "args": ["status"]]]])
        suite.expect(structured == ["git status"], "a command given with its arguments")
        let edit = AgentCline.claudeTool(AgentCline.Call(id: "2", name: "editor",
                                                         input: ["path": "/w/a.txt", "old_text": "a", "new_text": "b"], at: nil))
        suite.expect(edit.name == "Edit" && edit.input["old_string"] as? String == "a" && edit.input["file_path"] as? String == "/w/a.txt",
                     "a replacement is an edit")
        let write = AgentCline.claudeTool(AgentCline.Call(id: "3", name: "editor", input: ["path": "/w/n.txt", "new_text": "hi"], at: nil))
        suite.expect(write.name == "Write" && write.input["content"] as? String == "hi", "a new file is a write")
        suite.expect(AgentCline.patchFile("*** Begin Patch\n*** Update File: src/a.ts\n@@\n-x\n+y\n*** End Patch") == "src/a.ts",
                     "the file a patch changes")
        let read = AgentCline.claudeTool(AgentCline.Call(id: "4", name: "read_files", input: ["files": [["path": "/w/r.md"]]], at: nil))
        suite.expect(read.name == "Read" && read.input["file_path"] as? String == "/w/r.md", "a read")

        let asked = AgentCline.ask(AgentCline.Call(id: "call_x", name: "run_commands", input: ["commands": ["rm -rf ./scratch-dir"]], at: nil),
                                   cwd: "/w", at: now)
        suite.expect(asked?.requestID == "call_x" && asked?.tool == "Bash" && asked?.detail.contains("rm -rf ./scratch-dir") == true,
                     "the approval names the call and the whole command")
        let questionAsk = AgentCline.ask(AgentCline.Call(id: "call_q", name: "ask_question",
                                                         input: ["question": "Which color?", "options": ["Red", "Blue"]], at: nil),
                                         cwd: nil, at: now)
        suite.expect(questionAsk?.asksQuestions == true && questionAsk?.questions.first?.options.map(\.label) == ["Red", "Blue"],
                     "a question with its choices")
        suite.expect(AgentCline.ask(AgentCline.Call(id: "q", name: "ask_question", input: ["question": ""], at: nil),
                                    cwd: nil, at: now) == nil, "an empty question isn't shown")
    }

    // MARK: Watcher

    static func scan(_ watcher: AgentClineWatcher, _ records: [AgentCline.Record], _ messages: [[String: Any]],
                     policy: ClaudeContentPolicy = T.on, alive: Bool = true, client: String = AgentCline.cliClient,
                     pid: Int32? = nil) -> [AgentHookEvent] {
        watcher.scan(records, now: now, policy: policy, pid: { pid ?? $0.pid }, client: { _ in client },
                     alive: { _ in alive }, transcript: { _ in transcript(messages) })
    }

    static func kinds(_ events: [AgentHookEvent]) -> [String] {
        events.map { event in
            switch event.kind {
            case .started: return "started"
            case .ended: return "ended"
            case .prompt: return "prompt"
            case .toolStarted: return "tool"
            case .toolFinished: return "toolDone"
            case .permission(let p): return p.asksQuestions ? "question" : "permission"
            case .permissionSettled: return "settled"
            case .stopped: return "stopped"
            case .model: return "model"
            case .title: return "title"
            case .client: return "client"
            default: return "other"
            }
        }
    }

    static func watcher(_ suite: TestSuite) {
        let watcher = AgentClineWatcher()
        var events = scan(watcher, [record("pending")], approval)
        suite.expect(kinds(events) == ["started", "client", "model", "title", "prompt", "tool", "permission"],
                     "a session found waiting: \(kinds(events))")
        if case .prompt(let text) = events.first(where: { kinds([$0]) == ["prompt"] })?.kind {
            suite.expect(text == "tool echo two", "the prompt with previews")
        }
        // Nothing changed: nothing told again.
        suite.expect(scan(watcher, [record("pending")], approval).isEmpty, "a scan with no change tells nothing")
        events = scan(watcher, [record("running")], approved)
        suite.expect(kinds(events) == ["toolDone", "settled"], "allowed: the call ran: \(kinds(events))")
        events = scan(watcher, [record("idle")], approved)
        suite.expect(kinds(events) == ["stopped"], "the turn ended: \(kinds(events))")
        if case .stopped(let text, let failed) = events.first?.kind {
            suite.expect(text == "Done: two" && !failed, "the reply it ended on")
        }
        events = scan(watcher, [record("running")], question)
        suite.expect(kinds(events) == ["prompt", "tool", "question"], "a question waits while the turn runs: \(kinds(events))")
        events = scan(watcher, [record("idle")], question + [
            message("user", [result("call_79e96d2d", "ask_question", "Blue")]),
            message("assistant", [["type": "text", "text": "Blue it is."]], model: "mock-coder"),
        ])
        suite.expect(kinds(events) == ["toolDone", "settled", "stopped"], "answered: \(kinds(events))")
        // A whole turn between two readings: the reply came and went while the store said idle.
        let quick = question + [
            message("user", [result("call_79e96d2d", "ask_question", "Blue")]),
            message("assistant", [["type": "text", "text": "Blue it is."]], model: "mock-coder"),
            message("user", [typed("hello from the island")]),
            message("assistant", [["type": "text", "text": "Hi there."]], model: "mock-coder"),
        ]
        events = scan(watcher, [record("idle")], quick)
        suite.expect(kinds(events) == ["prompt", "stopped"], "a quick turn is still told: \(kinds(events))")
        if case .stopped(let text, _) = events.last?.kind { suite.expect(text == "Hi there.", "with its reply") }
        events = scan(watcher, [], quick)
        suite.expect(kinds(events) == ["ended"], "gone from the store: ended")

        // Previews off: no prompt text, title or reply.
        let quiet = AgentClineWatcher()
        events = scan(quiet, [record("running")], approval, policy: T.off)
        suite.expect(!kinds(events).contains("title"), "no title without previews")
        if case .prompt(let text) = events.first(where: { kinds([$0]) == ["prompt"] })?.kind {
            suite.expect(text == nil, "no prompt text without previews")
        }
        events = scan(quiet, [record("idle")], approved, policy: T.off)
        if case .stopped(let text, _) = events.last?.kind { suite.expect(text == nil, "no reply without previews") }

        // A process that quit ends its session, whatever the store says.
        let gone = AgentClineWatcher()
        _ = scan(gone, [record("idle")], approved)
        suite.expect(kinds(scan(gone, [record("idle")], approved, alive: false)) == ["ended"], "its process quit: ended")
        // A hub session nobody shows (its CLI quit) isn't listed.
        let orphan = AgentClineWatcher()
        suite.expect(orphan.scan([record("idle")], now: now, policy: T.on, pid: { _ in nil }, client: { _ in AgentCline.hubClient },
                                 alive: { _ in true }, transcript: { _ in transcript(approved) }).isEmpty,
                     "no CLI shows it: nothing listed")
        // What ended before the island looked is not told again.
        let late = AgentClineWatcher()
        events = scan(late, [record("idle")], approved)
        suite.expect(!kinds(events).contains("tool") && !kinds(events).contains("stopped"), "history isn't replayed: \(kinds(events))")
        // A failed turn.
        let failing = AgentClineWatcher()
        _ = scan(failing, [record("running")], approval)
        events = scan(failing, [record("failed")], approval)
        if case .stopped(_, let failed) = events.last?.kind { suite.expect(failed, "a failed turn") }
    }

    // MARK: Answers

    static func session(_ client: String, host: ClaudeSessionHost, _ messages: [[String: Any]] = approval,
                        status: String = "pending") -> ClaudeSession? {
        let tracker = AgentHookSessions()
        let watcher = AgentClineWatcher()
        for event in scan(watcher, [record(status)], messages, client: client) { tracker.ingest(event, now: now) }
        return T.snapshot(tracker, host: host).sessions.first
    }

    static func answers(_ suite: TestSuite) {
        let terminal = ClaudeSessionHost.terminal(tty: "/dev/ttys009")
        let cli = session(AgentCline.cliClient, host: terminal)
        suite.expect(cli?.phase == .waitingPermission && cli?.permission?.requestID == "call_da0cd2e3",
                     "the CLI's approval is answered on the island")
        suite.expect(cli?.provider == .cline && cli?.editor == nil, "a CLI session names no editor")
        let vscode = session(AgentCline.extensionClient, host: .ide(bundle: "com.microsoft.VSCode", port: nil))
        suite.expect(vscode?.permission == nil && vscode?.waitingFor == CodexSessionState.answerInTerminal,
                     "the extension's approval is answered in Cline")
        suite.expect(vscode?.editor == "VS Code", "the row names the editor: \(vscode?.editor ?? "nil")")
        let cursor = session(AgentCline.extensionClient, host: .ide(bundle: "com.todesktop.230313mzl4w4u92", port: nil))
        suite.expect(cursor?.editor == "Cursor", "Cline in Cursor")
        let hubOnly = session(AgentCline.hubClient, host: .unknown)
        suite.expect(hubOnly?.permission != nil, "a hub session's approval, with no terminal to type into, goes through the hub")
        let hubQuestion = session(AgentCline.hubClient, host: .unknown, question, status: "running")
        suite.expect(hubQuestion?.permission == nil && hubQuestion?.phase == .waitingPermission,
                     "a question the hub can't take is answered in Cline")
        let keyed = session(AgentCline.cliClient, host: terminal, question, status: "running")
        suite.expect(keyed?.permission?.asksQuestions == true, "a question in a terminal that takes keys")

        // Replies.
        if var extensionSession = vscode {
            suite.expect(!NotchClaudeSessionsSupport.repliesReach(extensionSession), "the extension takes replies only in its panel")
            extensionSession.editor = nil
        }
        let text = NotchClaudeSessionStrings.enUS
        if let vscode { suite.expect(text.replyUnavailable(for: vscode) == String(format: text.replyInAgentFormat, "Cline"),
                                     "Reply in Cline") }
        suite.expect(text.agentInEditor("Cline", "VS Code") == "Cline in VS Code", "Cline in VS Code")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        let approval = AgentCline.ask(AgentCline.Call(id: "c", name: "run_commands", input: ["commands": ["ls"]], at: nil), cwd: nil, at: now)!
        suite.expect(AgentCline.keys(.allow, approval) == [.keys([.letter("y")])], "y approves, never a Return a notice would take")
        suite.expect(AgentCline.keys(.deny, approval) == [.keys([.letter("n")])], "n denies")
        suite.expect(AgentCline.keys(.allowAlways, approval) == nil, "no Always in Cline's dialog")
        let asked = AgentCline.ask(AgentCline.Call(id: "q", name: "ask_question",
                                                   input: ["question": "Which color?", "options": ["Red", "Blue", "Green"]], at: nil),
                                   cwd: nil, at: now)!
        suite.expect(AgentCline.keys(.answer(["Which color?": ["Blue"]]), asked) == [.keys([.digit(2)])], "a choice's number")
        suite.expect(AgentCline.keys(.answer(["Which color?": ["Teal please"]]), asked)
                     == [.keys([.down, .down, .down]), .wait(0.2), .paste("Teal please"), .wait(0.2), .enter],
                     "own words on the row under the choices")
        suite.expect(AgentCline.keys(.answer(["Which color?": ["Red", "Blue"]]), asked) == nil, "one answer only")
        suite.expect(AgentCline.keys(.deny, asked) == [.keys([.escape])], "Escape leaves the question")
        suite.expect(AgentCline.keys(.deny, asked, oneBurst: true) == nil, "never Escape with a return after it")
        suite.expect(AgentCline.keys(.allow, asked) == nil, "a question isn't allowed")
        suite.expect(ClaudeKey.letter("n").bytes == "n" && ClaudeKey.letter("n").multiplexerName == "n"
                     && ClaudeKey.letter("n").appleScript == "\"n\"", "a letter key in each form")
    }

    // MARK: Hub

    static func hub(_ suite: TestSuite) {
        let file = #"{"hubId":"hub_1","protocolVersion":"v1","authToken":"0b12e9abc","host":"127.0.0.1","port":25463,"url":"ws://127.0.0.1:25463/hub","pid":97843}"#
        let found = AgentClineHub.discovery(Data(file.utf8))
        suite.expect(found?.url.absoluteString == "ws://127.0.0.1:25463/hub" && found?.token == "0b12e9abc" && found?.pid == 97843,
                     "the hub's discovery file")
        let remote = file.replacingOccurrences(of: "127.0.0.1:25463", with: "example.com:25463")
        suite.expect(AgentClineHub.discovery(Data(remote.utf8)) == nil, "only a hub on this computer")
        suite.expect(AgentClineHub.discovery(Data(#"{"url":"ws://127.0.0.1:1/hub","pid":3}"#.utf8)) == nil, "no token, no hub")
        // session.list and client.list as the hub answered them.
        let sessions: [[String: Any]] = [
            ["sessionId": "s1", "participants": [["clientId": "core-ijkcpmrb", "role": "creator"]]],
            ["sessionId": "s2", "participants": []],
        ]
        let clients: [[String: Any]] = [
            ["clientId": "core-ijkcpmrb", "clientType": "cli", "metadata": ["version": "0.2.1", "pid": 98185]],
            ["clientId": "openbar-1", "clientType": "openbar"],
        ]
        let shown = AgentClineHub.clients(sessions: sessions, clients: clients)
        suite.expect(shown == ["s1": 98185], "each session's CLI; one whose CLI quit has none: \(shown)")

        // A live hub on a folder of its own answers no unknown request.
        let data = FileManager.default.temporaryDirectory.appending(path: "ob-cline-hub-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: data.appending(path: "locks/hub"), withIntermediateDirectories: true)
        try? Data(file.utf8).write(to: data.appending(path: "locks/hub/production.json"))
        suite.expect(AgentClineHub.discovery(data: data, alive: { $0 == 97843 })?.pid == 97843, "found while its process runs")
        suite.expect(AgentClineHub.discovery(data: data, alive: { _ in false }) == nil, "a hub that quit is not used")
        try? FileManager.default.removeItem(at: data)
    }

    // MARK: Nothing installed

    static func installed(_ suite: TestSuite) {
        suite.expect(ClaudeSessionProvider.hookAgents.contains(.cline), "Cline has a switch, a mark and rows")
        suite.expect(!ClaudeSessionProvider.installedAgents.contains(.cline) && !ClaudeSessionProvider.cline.installsHooks,
                     "nothing is installed into Cline")
        suite.expect(ClaudeSessionProvider.cline.displayName == "Cline" && ClaudeSessionProvider.cline.answersPermissions,
                     "named Cline, answered where it can be")
        let home = URL(fileURLWithPath: "/tmp/ob-cline-none")
        suite.expect(AgentHookInstaller.file(.cline, home: home, bundle: "com.openbar.app") == nil, "no hook file")
        suite.expect(AgentHookInstaller.hookFile(.cline, home: home, bundle: "com.openbar.app") == nil, "no hook record")
        suite.expect(NotchClaudeSessionsSupport.agentKey(.cline) == DefaultsKey.notchSessionsAgentCline, "its own switch")
        suite.expect(AgentCLI.command(.cline) == "cline" && AgentCLI.startArguments(.cline, prompt: "hi") == ["--tui", "hi"],
                     "New session opens its terminal UI with the prompt")
        suite.expect(ClaudeSessionProvider.cline.character == .cline, "its own figure")
        suite.expect(ClaudeTyping.enterDelay(.cline) == nil,
                     "a reply sends on one return: a second would approve the tool Cline asks about next")
    }
}
