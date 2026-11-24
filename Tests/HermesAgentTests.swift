// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Hermes in the Sessions list (#244): what its shell hooks send, the
/// sessions kept from them, its dangerous-command prompt and questions
/// answered with keys, the island's lines in its config.yaml, the hooks its
/// allowlist approves, and how a session's client and folder are found.
enum HermesAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("hermes: hooks") { hooks(suite) }
        suite.run("hermes: approvals and questions") { approvals(suite) }
        suite.run("hermes: keys") { keys(suite) }
        suite.run("hermes: config.yaml") { config(suite) }
        suite.run("hermes: installer") { installer(suite) }
        suite.run("hermes: approval of the hooks") { allowlist(suite) }
        suite.run("hermes: clients and places") { clients(suite) }
        suite.run("hermes: state.db") { store(suite) }
        suite.run("hermes: presence and names") { presence(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }
    static let session = "local-5bae1346b7aa551fff6b3e80"

    /// A shell hook's input as Hermes writes it (agent/shell_hooks.py): the
    /// gateway's own folder as `cwd`, the event's fields under `extra`.
    static func payload(_ event: String, tool: String? = nil, input: [String: Any]? = nil,
                        _ extra: [String: Any] = [:]) -> [String: Any] {
        var json: [String: Any] = ["hook_event_name": event, "session_id": session, "cwd": "/Users/me/.hermes/hermes-agent",
                                   "profile": "default", "extra": extra.merging(["telemetry_schema_version": "hermes.observer.v1",
                                                                                 "kanban_task_id": ""]) { a, _ in a }]
        json["tool_name"] = tool ?? NSNull()
        json["tool_input"] = input ?? NSNull()
        return json
    }

    static func send(_ tracker: AgentHookSessions, _ event: String, tool: String? = nil, input: [String: Any]? = nil,
                     _ extra: [String: Any] = [:], policy: ClaudeContentPolicy = T.on, at date: Date = now) {
        T.feed(tracker, T.envelope("hermes", hook: event, pid: 8152, payload(event, tool: tool, input: input, extra)),
               policy: policy, at: date)
    }

    static func events(_ event: String, tool: String? = nil, input: [String: Any]? = nil, _ extra: [String: Any] = [:],
                       policy: ClaudeContentPolicy = T.on) -> [AgentHookEvent.Kind] {
        T.decode(T.envelope("hermes", hook: event, pid: 8152, payload(event, tool: tool, input: input, extra)), policy: policy)
            .map(\.kind)
    }

    // MARK: Hooks

    static func hooks(_ suite: TestSuite) {
        let opened = T.decode(T.envelope("hermes", hook: "on_session_open", pid: 8152,
                                         payload("on_session_open", ["platform": "cli", "reason": "create"])))
        suite.expect(opened.map(\.kind) == [.client("cli"), .started] && opened.allSatisfy { $0.cwd == nil && $0.sessionID == session },
                     "a session opens in its client, and the gateway's folder is never taken for the session's: \(opened)")
        let tracker = AgentHookSessions()
        send(tracker, "on_session_open", ["platform": "cli", "reason": "create"])
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].id == "hermes:\(session)" && listed[0].provider == .hermes
                     && listed[0].phase == .idle, "listed idle as it opens: \(listed)")
        send(tracker, "pre_llm_call", ["user_message": "tidy the repo", "model": "anthropic/claude-sonnet-4.6",
                                       "platform": "cli", "is_first_turn": true])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].chat.map(\.text) == ["tidy the repo"]
                     && listed[0].model == "anthropic/claude-sonnet-4.6", "a prompt opens a turn, with its model")
        suite.expect(events("pre_llm_call", ["user_message": "secret", "platform": "cli"], policy: T.off).contains(.prompt(nil)),
                     "without previews the prompt isn't kept")
        send(tracker, "pre_tool_call", tool: "terminal", input: ["command": "git status"], ["tool_call_id": "call_1"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .runningTool && listed[0].currentTool?.name == "Bash"
                     && listed[0].currentTool?.summary == "git status" && listed[0].currentTool?.id == "call_1",
                     "its terminal tool runs as Bash: \(listed[0].currentTool as Any)")
        send(tracker, "post_tool_call", tool: "terminal", input: ["command": "git status"],
             ["tool_call_id": "call_1", "status": "ok", "duration_ms": 153])
        send(tracker, "pre_tool_call", tool: "patch", input: ["path": "/w/repo/a.py", "old_string": "a", "new_string": "b"],
             ["tool_call_id": "call_2"])
        send(tracker, "post_tool_call", tool: "patch", input: ["path": "/w/repo/a.py", "old_string": "a", "new_string": "b"],
             ["tool_call_id": "call_2", "status": "error", "error_type": "NoMatch"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .generating && listed[0].timeline.count == 2 && !listed[0].timeline[0].failed
                     && listed[0].timeline[1].name == "Edit" && listed[0].timeline[1].summary == "a.py" && listed[0].timeline[1].failed,
                     "tools end by their call id, a failed patch as a failed edit: \(listed[0].timeline)")
        send(tracker, "on_session_end", ["completed": true, "failed": false, "interrupted": false,
                                         "turn_exit_reason": "text_response(finish_reason=stop)", "model": "anthropic/claude-sonnet-4.6"],
             at: now.addingTimeInterval(1))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(1)).sessions
        suite.expect(listed[0].phase == .finished, "a completed turn ends done: \(listed[0].phase)")
        // A turn the person stopped is no notice of work done.
        send(tracker, "pre_llm_call", ["user_message": "again", "platform": "cli"], at: now.addingTimeInterval(2))
        send(tracker, "on_session_end", ["completed": "False", "failed": "False", "interrupted": "True"], at: now.addingTimeInterval(3))
        listed = T.snapshot(tracker, at: now.addingTimeInterval(3)).sessions
        suite.expect(listed[0].phase == .idle, "an interrupted turn ends idle, its flags read as text too: \(listed[0].phase)")
        send(tracker, "on_session_finalize", ["platform": "local", "reason": "cli_close"])
        suite.expect(T.snapshot(tracker).sessions.isEmpty, "the row goes when its client closes")
        // A session whose client wasn't found lives in the gateway: it goes once long quiet.
        let lost = AgentHookSessions()
        lost.ingest(AgentHookEvent(provider: .hermes, sessionID: "s2", pid: 0, cwd: "/w", date: now, kind: .started), now: now)
        lost.prune(now: now.addingTimeInterval(60), alive: { _ in false })
        suite.expect(lost.entries.count == 1, "a session with no client found stays while it is recent")
        lost.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor + 1), alive: { _ in false })
        suite.expect(lost.entries.isEmpty, "and goes once long quiet")
        let kept = AgentHookSessions()
        kept.ingest(AgentHookEvent(provider: .hermes, sessionID: "s3", pid: 777, cwd: "/w", date: now, kind: .started), now: now)
        kept.prune(now: now.addingTimeInterval(AgentHookSessions.quietFor + 1), alive: { $0 == 777 })
        suite.expect(kept.entries.count == 1, "one whose terminal client lives stays however quiet")
        kept.prune(now: now, alive: { _ in false })
        suite.expect(kept.entries.isEmpty, "and goes with that client")
    }

    // MARK: Approvals and questions

    static func approvals(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        send(tracker, "on_session_open", ["platform": "cli", "reason": "create"])
        send(tracker, "pre_llm_call", ["user_message": "clean", "platform": "cli"])
        send(tracker, "pre_tool_call", tool: "terminal", input: ["command": "rm -rf build"], ["tool_call_id": "call_9"])
        send(tracker, "on_human_input_request", ["kind": "approval", "request_id": "6630f540", "platform": "local",
                                                 "prompt": "rm -rf build"], policy: T.off)
        var listed = T.snapshot(tracker, previews: false).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.requestID == "6630f540"
                     && listed[0].permission?.tool == "Bash" && listed[0].permission?.detail == "rm -rf build"
                     && listed[0].permission?.content?.complete == true && listed[0].permission?.offersAlways == false
                     && listed[0].waitingFor == nil,
                     "a dangerous command waits on the island's card, shown whatever the previews: \(listed[0].permission as Any)")
        // Its Ink UI's and app's prompts are answered on the card too, through Hermes's gateway.
        let tui = AgentHookSessions()
        send(tui, "on_session_open", ["platform": "tui", "reason": "create"])
        send(tui, "on_human_input_request", ["kind": "approval", "request_id": "r1", "prompt": "rm -rf build"])
        listed = T.snapshot(tui).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.requestID == "r1"
                     && listed[0].waitingFor == nil && tui.entry("hermes:\(session)")?.client == "tui",
                     "its Ink UI's prompt is on the island's card: \(listed[0])")
        send(tui, "on_session_open", ["platform": "cli", "reason": "resume"])
        suite.expect(tui.entry("hermes:\(session)")?.client == "tui",
                     "the island joining the session to answer doesn't change where it is shown")
        let ghostty = tracker.snapshot(now: now, previews: true, host: { _ in .ghostty(tty: "/dev/ttys004") }, branch: { _ in nil })
        suite.expect(ghostty.sessions[0].permission != nil && ghostty.sessions[0].waitingFor == nil,
                     "and so is one in a terminal the island can't press keys in")
        send(tracker, "on_human_input_resolved", ["kind": "approval", "request_id": "6630f540", "outcome": "answered"])
        send(tracker, "post_tool_call", tool: "terminal", input: ["command": "rm -rf build"],
             ["tool_call_id": "call_9", "status": "ok"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase == .generating, "answered, here or in Hermes, the card goes")
        // A question with choices is answered with its number.
        let question: [String: Any] = ["questions": [["question": "Which color?", "choices": ["Red", "Blue"]]]]
        send(tracker, "pre_tool_call", tool: "clarify", input: question, ["tool_call_id": "call_q"])
        send(tracker, "on_human_input_request", ["kind": "clarify", "request_id": "q1", "prompt": "Which color?"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].phase == .waitingPermission && listed[0].permission?.requestID == "q1"
                     && listed[0].permission?.questions.first?.options.map(\.label) == ["Red", "Blue"]
                     && listed[0].timeline.allSatisfy { $0.name != "AskUserQuestion" },
                     "its choices are on the card, and the question isn't a tool: \(listed[0].permission as Any)")
        send(tracker, "on_human_input_resolved", ["kind": "clarify", "request_id": "q1", "outcome": "answered"])
        send(tracker, "post_tool_call", tool: "clarify", input: question, ["tool_call_id": "call_q", "status": "ok"])
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && !listed[0].timeline.contains { $0.name == "AskUserQuestion" },
                     "an answered question leaves no tool behind")
        // Several questions, a free answer and many picks are on the card too.
        let several = AgentHookSessions()
        send(several, "pre_tool_call", tool: "clarify", input: ["questions": [
            ["question": "A?", "choices": ["x", " "]], ["question": "Why?"],
            ["question": "Which?", "choices": ["a", "b"], "multi_select": true],
        ]])
        send(several, "on_human_input_request", ["kind": "clarify", "request_id": "q2", "prompt": "?"])
        let row = T.snapshot(several).sessions[0]
        suite.expect(row.phase == .waitingPermission && row.permission?.questions.map(\.question) == ["A?", "Why?", "Which?"]
                     && row.permission?.questions.map(\.options.count) == [1, 0, 2]
                     && row.permission?.questions.map(\.multiSelect) == [false, false, true],
                     "several questions, a free answer and many picks are on the card: \(row.permission as Any)")
        suite.expect(AgentHookMessage.hermesQuestion(["questions": [["question": " "]]], at: now) == nil
                     && AgentHookMessage.hermesQuestion(["questions": []], at: now) == nil,
                     "a question with no text isn't shown")
        // A password is only ever typed in Hermes.
        let sudo = AgentHookSessions()
        send(sudo, "on_human_input_request", ["kind": "sudo", "request_id": "p1", "prompt": "[sudo] password"])
        suite.expect(T.snapshot(sudo).sessions[0].phase == .waitingInput, "a sudo password waits on the person, in Hermes")
        send(sudo, "on_human_input_resolved", ["kind": "sudo", "request_id": "p1", "outcome": "answered"])
        suite.expect(T.snapshot(sudo).sessions[0].phase != .waitingInput, "and the wait ends when it is typed")
    }

    // MARK: Keys

    static func keys(_ suite: TestSuite) {
        let command = AgentHookMessage.hermesApproval("rm -rf build", request: "r", at: now)
        suite.expect(AgentHookKeys.hermes(.allow, command) == [.enter],
                     "Allow presses Return on the Allow once Hermes has picked")
        suite.expect(AgentHookKeys.hermes(.deny, command) == [.keys([.down, .down, .down]), .enter],
                     "Deny goes down to it: the arrows stop there whichever choices are offered")
        suite.expect(AgentHookKeys.hermes(.allowAlways, command) == nil, "nothing else is offered")
        if let question = AgentHookMessage.hermesQuestion(["questions": [["question": "Which color?", "choices": ["Red", "Blue"]]]],
                                                          at: now) {
            suite.expect(AgentHookKeys.hermes(.answer(["Which color?": ["Blue"]]), question) == nil
                         && AgentHookKeys.hermes(.deny, question) == nil,
                         "a question is never answered with keys: it goes through the gateway")
        }
        // Through the gateway: Hermes's own choice names, each question's answer.
        let asked = AgentHookMessage.hermesApproval("rm -rf build", request: "r", at: now)
        suite.expect(AgentHermesGateway.answer(.allow, asked) == .approval("once")
                     && AgentHermesGateway.answer(.deny, asked) == .approval("deny")
                     && AgentHermesGateway.answer(.allowAlways, asked) == nil,
                     "the gateway gets Allow once or Deny")
        if let card = AgentHookMessage.hermesQuestion(["questions": [["question": "Which color?", "choices": ["Red", "Blue"]]]], at: now) {
            suite.expect(AgentHermesGateway.answer(.answer(["Which color?": ["Blue"]]), card) == .clarify([["Blue"]])
                         && AgentHermesGateway.answer(.answer(["Which color?": ["Purple"]]), card) == .clarify([["Purple"]])
                         && AgentHermesGateway.answer(.deny, card) == .clarify([nil]) && AgentHermesGateway.answer(.allow, card) == nil,
                         "and a question's answer, one of its own too, or nothing for Skip")
        }
        if let card = AgentHookMessage.hermesQuestion(["questions": [["question": "A?", "choices": ["x", "y"]],
                                                                     ["question": "Which?", "choices": ["a", "b"], "multi_select": true]]],
                                                      at: now) {
            suite.expect(AgentHermesGateway.answer(.answer(["Which?": ["a", "b"]]), card) == .clarify([nil, ["a", "b"]])
                         && AgentHermesGateway.answer(.answer([:]), card) == nil,
                         "each question gets its own answer, several picks as a list, one left out skipped")
        }
        let printed = #"{"state":"ready","url":"ws://127.0.0.1:56033/api/ws","protocols":["hermes-gateway-v1","hermes-gateway-ticket.Al0c"],"ticket":"Al0c"}"#
        suite.expect(AgentHermesGateway.ticket(printed)?.url.absoluteString == "ws://127.0.0.1:56033/api/ws"
                     && AgentHermesGateway.ticket(printed)?.protocols == ["hermes-gateway-v1", "hermes-gateway-ticket.Al0c"],
                     "the ticket gives the gateway's socket and the subprotocols that carry it")
        suite.expect(AgentHermesGateway.ticket(#"{"state":"ready","url":"ws://example.com/api/ws","protocols":["x"]}"#) == nil
                     && AgentHermesGateway.ticket(#"{"state":"absent"}"#) == nil && AgentHermesGateway.ticket("nope") == nil,
                     "only a local gateway that is ready is joined")
        let resumed: [String: Any] = ["execution_generation": 3, "prompts": [
            ["kind": "clarify", "prompt_id": "c1", "execution_generation": 2],
            ["kind": "approval", "prompt_id": "a1", "execution_generation": 2, "command": "rm -rf build", "choices": ["once", "deny"]],
        ]]
        suite.expect(AgentHermesGateway.pending(resumed, kind: "approval").map { "\($0.prompt)@\($0.generation)" } == "a1@2"
                     && AgentHermesGateway.pending(resumed, kind: "clarify")?.prompt == "c1"
                     && AgentHermesGateway.pending(["prompts": []], kind: "approval") == nil,
                     "the prompt waiting is found in what joining the session returns")
        let asking: [String: Any] = ["open_requests": [
            ["id": "srq-1", "method": "sudo", "params": [:]],
            ["id": "srq-3c49", "method": "clarify", "params": ["questions": [["qid": "q0", "question": "Which color?",
                                                                                "choices": ["Red (Recommended)", "Blue"]]]]],
        ]]
        suite.expect(AgentHermesGateway.openQuestion(asking).map { "\($0.request):\($0.questions)" } == "srq-3c49:[\"q0\"]"
                     && AgentHermesGateway.openQuestion(["open_requests": [["id": "srq-1", "method": "sudo"]]]) == nil,
                     "its Ink UI's question is the gateway's own request, answered question by question")
        let terminal = ClaudeTerminalScripts.press([.keys([.down, .down, .down]), .enter], tty: "/dev/ttys001")
        suite.expect(terminal.contains("do script ((character id 27) & \"[B\" & (character id 27) & \"[B\" & (character id 27) & \"[B\") in theTab")
                     && terminal.components(separatedBy: "do script").count == 2,
                     "Terminal sends the arrows with the return it adds, in one go: \(terminal)")
        suite.expect(ClaudeTTYTerminalScripts.itermType([.keys([.down])], tty: "/dev/ttys001")
                        .contains("write text ((character id 27) & \"[B\") newline no"), "iTerm2 writes the arrow with no newline")
        suite.expect(ClaudeKey.down.multiplexerName == "Down" && ClaudeKey.down.kittyName == "down"
                     && ClaudeKey.down.bytes == "\u{1B}[B", "tmux, zellij and kitty get the keys by name, WezTerm as bytes")
    }

    // MARK: config.yaml

    static let marker = "# openbar-hook com.openbar.app.dev"
    static let entries: [(event: String, command: String)] = [
        ("on_session_open", "/bin/sh -c 'echo one # openbar-hook com.openbar.app.dev'"),
        ("pre_llm_call", "/bin/sh -c 'echo two # openbar-hook com.openbar.app.dev'"),
    ]

    static func config(_ suite: TestSuite) {
        let item = AgentHermesConfig.item(command: "/bin/sh -c 'printf \"{\\\"a\\\":1}\" | nc -U /tmp/s'", timeout: 5)
        suite.expect(item == #"- {command: "/bin/sh -c 'printf \"{\\\"a\\\":1}\" | nc -U /tmp/s'", timeout: 5}"#,
                     "an entry is one flow mapping with its command as a JSON string: \(item)")
        suite.expect(AgentHermesConfig.command(in: item) == "/bin/sh -c 'printf \"{\\\"a\\\":1}\" | nc -U /tmp/s'",
                     "and its command reads back whole")

        // No hooks yet: the island's block goes at the end.
        let plain = "model:\n  default: gpt\n# hooks:\n#   pre_tool_call: []\nhooks_auto_accept: false\n"
        let added = try? AgentHermesConfig.installing(plain, entries: entries, timeout: 5, marker: marker)
        suite.expect(added?.hasPrefix(plain) == true && added?.contains("hooks:  \(marker)\n  on_session_open:  \(marker)\n    - {command:") == true,
                     "with no hooks yet the island's block comes after everything, its keys marked: \(added ?? "nil")")
        suite.expect(added.flatMap { AgentHermesConfig.commands($0) }?["pre_llm_call"] == [entries[1].command],
                     "and reads back")
        suite.expect(added.flatMap { AgentHermesConfig.removing($0, marker: marker) } == plain,
                     "Remove leaves the file as it was")

        // The person's own hooks: the island's join them under the same keys.
        let mine = """
            model: gpt
            hooks:
              # my formatter
              post_tool_call:
                - matcher: "write_file|patch"
                  command: "~/.hermes/agent-hooks/fmt.sh"
              on_session_open:
              - command: ~/.hermes/agent-hooks/hello.sh
                timeout: 5

            # display settings
            display:
              interface: cli
            """
        guard let joined = try? AgentHermesConfig.installing(mine, entries: entries, timeout: 5, marker: marker) else {
            return suite.expect(false, "the island's hooks join the person's")
        }
        let read = AgentHermesConfig.commands(joined) ?? [:]
        suite.expect(read["post_tool_call"] == ["~/.hermes/agent-hooks/fmt.sh"]
                     && read["on_session_open"] == ["~/.hermes/agent-hooks/hello.sh", entries[0].command]
                     && read["pre_llm_call"] == [entries[1].command],
                     "after the person's own, under their key, a new key at the end of the block: \(read)")
        suite.expect(joined.contains("  - command: ~/.hermes/agent-hooks/hello.sh\n    timeout: 5\n  - {command: \"/bin/sh -c 'echo one")
                     && joined.contains("  pre_llm_call:  \(marker)\n    - {command:")
                     && joined.contains("\n\n# display settings\ndisplay:\n  interface: cli"),
                     "each at the indent already used there, before the comment of what follows:\n\(joined)")
        suite.expect(joined.components(separatedBy: "\nhooks:").count == 2, "never a second hooks: key, which Hermes refuses")
        suite.expect(AgentHermesConfig.removing(joined, marker: marker) == mine + "\n",
                     "Remove takes only the island's lines and the key it added:\n\(AgentHermesConfig.removing(joined, marker: marker) ?? "nil")")
        suite.expect(AgentHermesConfig.removing(mine, marker: marker) == nil, "nothing of the island's, nothing to change")
        let again = try? AgentHermesConfig.installing(joined, entries: entries, timeout: 5, marker: marker)
        suite.expect(again == joined, "installing again changes nothing")
        let moved = [(event: "on_session_open", command: "/bin/sh -c 'echo new # openbar-hook com.openbar.app.dev'")]
        let updated = (try? AgentHermesConfig.installing(joined, entries: moved, timeout: 5, marker: marker)) ?? ""
        suite.expect(AgentHermesConfig.commands(updated)?["on_session_open"] == ["~/.hermes/agent-hooks/hello.sh", moved[0].command]
                     && AgentHermesConfig.commands(updated)?["pre_llm_call"] == nil,
                     "an update replaces the island's old lines: \(AgentHermesConfig.commands(updated) ?? [:])")

        // Hermes may write the file back in block style: still the island's.
        let rewritten = joined.replacingOccurrences(of: "  pre_llm_call:  \(marker)\n    - {command: \"/bin/sh -c 'echo two \(marker)'\", timeout: 5}",
                                                    with: "  pre_llm_call:\n    - command: \"/bin/sh -c 'echo two \(marker)'\"\n      timeout: 5")
        suite.expect(rewritten != joined && AgentHermesConfig.removing(rewritten, marker: marker) == mine + "\n",
                     "a rewritten entry is still found by its marker, its emptied key goes")

        // Empty hooks, and lists, take the island's lines.
        let empty = "hooks: {}\nother: 1\n"
        let filled = try? AgentHermesConfig.installing(empty, entries: entries, timeout: 5, marker: marker)
        suite.expect(filled?.hasPrefix("hooks:  \(marker)\n  on_session_open:  \(marker)\n    - {command:") == true
                     && filled?.hasSuffix("other: 1\n") == true, "hooks: {} becomes the island's block: \(filled ?? "nil")")
        suite.expect(filled.flatMap { AgentHermesConfig.removing($0, marker: marker) } == "other: 1\n",
                     "and goes again, empty hooks meaning none")
        let emptyList = "hooks:\n  on_session_open: []\n"
        let list = try? AgentHermesConfig.installing(emptyList, entries: Array(entries.prefix(1)), timeout: 5, marker: marker)
        suite.expect(list.flatMap { AgentHermesConfig.commands($0) }?["on_session_open"] == [entries[0].command],
                     "an empty list takes the island's line: \(list ?? "nil")")

        // Hooks written inline are left to the person.
        for unsupported in ["hooks: {on_session_open: [{command: x}]}\n", "hooks:\n  on_session_open: [{command: x}]\n"] {
            suite.expect((try? AgentHermesConfig.installing(unsupported, entries: entries, timeout: 5, marker: marker)) == nil,
                         "\(unsupported) is not changed")
        }
        suite.expect(AgentHermesConfig.commands("model: x\n") == nil
                     && AgentHermesConfig.commands("hooks:\n  pre_tool_call:\n    - command: 'it''s'\n")?["pre_tool_call"] == ["it's"],
                     "no hooks block reads as none; a single-quoted command reads as YAML does")
        let crlf = "hooks:\r\n  on_session_open:\r\n    - command: a\r\n"
        suite.expect(AgentHermesConfig.commands(crlf)?["on_session_open"] == ["a"], "Windows line ends read too")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-hermes-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let socket = "/tmp/island's.sock"
        let bundle = "com.openbar.app.dev"
        let url = AgentHermes.config(home: home, environment: [:])
        suite.expect(url.path.hasSuffix("/.hermes/config.yaml")
                     && AgentHermes.config(home: home, environment: ["HERMES_HOME": "/h"]).path == "/h/config.yaml",
                     "config.yaml in ~/.hermes, or in HERMES_HOME")
        let command = AgentHermes.command(event: "pre_tool_call", socket: socket, bundle: bundle)
        suite.expect(command.hasPrefix("/bin/sh -c '") && command.contains("\"hook\":\"pre_tool_call\"")
                     && command.contains("# openbar-hook \(bundle)") && command.contains("'\"'\"'"),
                     "Hermes starts hooks with no shell, so the island's line goes to sh, quoted for its split: \(command)")
        suite.expect(AgentHookInstaller.state(.hermes, home: home, socket: socket, bundle: bundle) == .missing, "nothing yet")
        try? manager.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let mine = "model:\n  default: x\nhooks:\n  on_session_end:\n    - command: ~/mine.sh\n"
        try? Data(mine.utf8).write(to: url)
        do {
            try AgentHookInstaller.install(.hermes, home: home, socket: socket, bundle: bundle)
        } catch {
            return suite.expect(false, "install: \(error)")
        }
        let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        let read = AgentHermesConfig.commands(text) ?? [:]
        suite.expect(AgentHookInstaller.state(.hermes, home: home, socket: socket, bundle: bundle) == .installed
                     && read["on_session_end"]?.first == "~/mine.sh" && read["on_session_end"]?.count == 2
                     && AgentHermes.events.allSatisfy { read[$0]?.contains(AgentHermes.command(event: $0, socket: socket, bundle: bundle)) == true },
                     "every event holds the island's hook, beside the person's: \(read)")
        suite.expect(manager.fileExists(atPath: ClaudeHookInstaller.originalCopy(of: url).path), "the file was copied aside first")
        suite.expect(AgentHookInstaller.state(.hermes, home: home, socket: "/tmp/other.sock", bundle: bundle) == .outdated,
                     "another socket's hooks are outdated")
        try? AgentHookInstaller.remove(.hermes, home: home, bundle: bundle)
        suite.expect((try? String(contentsOf: url, encoding: .utf8)) == mine
                     && AgentHookInstaller.state(.hermes, home: home, socket: socket, bundle: bundle) == .missing,
                     "Remove leaves the person's hooks as they were")
        try? Data("hooks: {on_session_open: [{command: x}]}\n".utf8).write(to: url)
        suite.expect(AgentHookInstaller.state(.hermes, home: home, socket: socket, bundle: bundle) == .unreadable
                     && (try? AgentHookInstaller.install(.hermes, home: home, socket: socket, bundle: bundle)) == nil,
                     "hooks written inline are left alone, and said to be")
    }

    // MARK: Approval of the hooks

    static func allowlist(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "vs-hermes-allow-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let socket = "/tmp/s.sock", bundle = "com.openbar.app.dev"
        let hermes = AgentHermes.home(home: home, environment: [:])
        try? FileManager.default.createDirectory(at: hermes, withIntermediateDirectories: true)
        suite.expect(AgentHermes.approval(home: home, socket: socket, bundle: bundle, environment: [:])
                     == .waiting(count: AgentHermes.events.count), "no allowlist: every hook waits for Hermes's approval")
        let some: [[String: Any]] = AgentHermes.events.prefix(3).map {
            ["event": $0, "command": AgentHermes.command(event: $0, socket: socket, bundle: bundle), "approved_at": "2026-11-23T22:00:00Z"]
        }
        try? JSONSerialization.data(withJSONObject: ["approvals": some]).write(to: hermes.appending(path: "shell-hooks-allowlist.json"))
        suite.expect(AgentHermes.approval(home: home, socket: socket, bundle: bundle, environment: [:])
                     == .waiting(count: AgentHermes.events.count - 3), "those approved are counted")
        let all: [[String: Any]] = AgentHermes.events.map { ["event": $0, "command": AgentHermes.command(event: $0, socket: socket, bundle: bundle)] }
            + [["event": "pre_tool_call", "command": "/other"]]
        try? JSONSerialization.data(withJSONObject: ["approvals": all]).write(to: hermes.appending(path: "shell-hooks-allowlist.json"))
        suite.expect(AgentHermes.approval(home: home, socket: socket, bundle: bundle, environment: [:]) == .approved,
                     "all approved")
        suite.expect(AgentHermes.approval(home: home, socket: "/tmp/moved.sock", bundle: bundle, environment: [:])
                     == .waiting(count: AgentHermes.events.count), "a changed command needs approving again")
        suite.expect(AgentHermes.approvedPairs(Data("{\"approvals\": \"x\"}".utf8)).isEmpty
                     && AgentHermes.approvedPairs(Data("nope".utf8)).isEmpty, "a broken allowlist approves nothing")
    }

    // MARK: Clients and places

    static func clients(_ suite: TestSuite) {
        suite.expect(AgentHermesClients.device("tty-dev-ttys116") == "/dev/ttys116" && AgentHermesClients.device("tmux_pane-%3") == nil
                     && AgentHermesClients.device("tty-dev-../x") == nil, "a note's name gives its device, nothing else")
        suite.expect(AgentHermesClients.isClient(["/opt/homebrew/bin/python3.14", "/Users/me/.hermes/hermes-agent/venv/bin/hermes"])
                     && AgentHermesClients.isClient(["python", "-m", "hermes_cli.main", "chat"])
                     && AgentHermesClients.isClient(["/x/bin/hermes-light", "--tui"])
                     && !AgentHermesClients.isClient(["python", "-m", "hermes_cli.main", "--profile", "default", "gateway", "run"])
                     && !AgentHermesClients.isClient(["/x/bin/hermes", "acp"]) && !AgentHermesClients.isClient(["/x/bin/hermes-acp"])
                     && !AgentHermesClients.isClient(["/usr/bin/vim", "hermes.md"]),
                     "Hermes's terminal client is told from its gateway and from other programs")
        let running = [
            AgentHermesClients.Process(pid: 101, parent: 50, tty: "/dev/ttys001", cwd: "/w/a"),
            AgentHermesClients.Process(pid: 102, parent: 101, tty: "/dev/ttys001", cwd: "/w/a"),
            AgentHermesClients.Process(pid: 201, parent: 60, tty: "/dev/ttys002", cwd: "/w/b"),
            AgentHermesClients.Process(pid: 301, parent: 70, tty: "/dev/ttys003", cwd: "/w/c"),
            AgentHermesClients.Process(pid: 302, parent: 71, tty: "/dev/ttys004", cwd: "/w/c"),
        ]
        let notes = ["s1": ["/dev/ttys009", "/dev/ttys001"], "s2": ["/dev/ttys002"]]
        suite.expect(AgentHermesClients.client(of: "s1", cwd: "/w/x", notes: notes, running: running) == 101,
                     "the note's terminal that still runs Hermes, and the first Hermes process on it")
        suite.expect(AgentHermesClients.client(of: "s3", cwd: "/w/b", notes: notes, running: running, listed: ["s2"]) == nil,
                     "a terminal another listed session's note names is that session's")
        suite.expect(AgentHermesClients.client(of: "s3", cwd: "/w/b", notes: notes, running: running) == 201,
                     "a note of a session no longer listed says nothing: the terminal was used again")
        suite.expect(AgentHermesClients.client(of: "s4", cwd: "/w/c", notes: notes, running: running) == nil
                     && AgentHermesClients.client(of: "s4", cwd: "/w/c", notes: notes, running: running, claimed: [302]) == 301,
                     "without a note (its Ink UI writes none), the only client in the folder no other row claims")
        let places = AgentHermesPlaces(alive: { $0 == 101 })
        let local = AgentHermesRecord(source: "local", createdSource: "cli", cwd: "/w/a", title: "Tidy up")
        var place = places.place("s1", hermes: URL(fileURLWithPath: "/h"), now: now, record: { _ in local },
                                 notes: { notes }, running: { running })
        suite.expect(place?.pid == 101 && place?.record?.cwd == "/w/a", "a session is placed at its client: \(place as Any)")
        var looked = 0
        place = places.place("s1", hermes: URL(fileURLWithPath: "/h"), now: now.addingTimeInterval(1), record: { _ in local },
                             notes: { looked += 1; return notes }, running: { running })
        suite.expect(place?.pid == 101 && looked == 0, "and kept there while that client lives")
        let telegram = AgentHermesRecord(source: "telegram", createdSource: "telegram", cwd: nil)
        suite.expect(places.place("t1", hermes: URL(fileURLWithPath: "/h"), now: now, record: { _ in telegram }) == nil,
                     "a chat its gateway answers isn't listed")
        let desktop = AgentHermesRecord(source: "local", createdSource: "gui", cwd: "/w/d")
        place = places.place("d1", hermes: URL(fileURLWithPath: "/h"), now: now, record: { _ in desktop }, app: { 999 })
        suite.expect(place?.pid == 999 && desktop.client == "desktop", "a session of Hermes's app is the app's")
        let none = AgentHermesPlaces(alive: { _ in false })
        _ = none.place("n1", hermes: URL(fileURLWithPath: "/h"), now: now, record: { _ in local })
        looked = 0
        _ = none.place("n1", hermes: URL(fileURLWithPath: "/h"), now: now.addingTimeInterval(0.2), record: { _ in local },
                       notes: { looked += 1; return [:] })
        suite.expect(looked == 0, "a client not found is looked for again only after a while")
        _ = none.place("n1", hermes: URL(fileURLWithPath: "/h"), now: now.addingTimeInterval(AgentHermesPlaces.retryAfter + 1),
                       record: { _ in local }, notes: { looked += 1; return [:] })
        suite.expect(looked == 1, "and then looked for")
        suite.expect(AgentHermesRecord(source: "local", createdSource: "cli").isLocal
                     && AgentHermesRecord(source: "oneshot", createdSource: "oneshot").isLocal
                     && !AgentHermesRecord(source: "discord", createdSource: "discord").isLocal
                     && AgentHermesRecord(source: nil, createdSource: "tui").client == "tui",
                     "terminals, its terminal UI and app are local; chats aren't")
    }

    // MARK: state.db

    static func store(_ suite: TestSuite) {
        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-hermes-db-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let path = AgentHermesStore.database(folder).path
        let sql = """
            CREATE TABLE sessions (id TEXT PRIMARY KEY, source TEXT NOT NULL, created_source TEXT, cwd TEXT, title TEXT,
              model TEXT, started_at REAL NOT NULL, ended_at REAL);
            CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, role TEXT NOT NULL,
              content TEXT, timestamp REAL NOT NULL);
            INSERT INTO sessions VALUES ('s1', 'local', 'cli', '/w/repo', 'Tidy the repo', 'mock', 1, NULL);
            INSERT INTO sessions VALUES ('s2', 'telegram', 'telegram', NULL, NULL, NULL, 1, 5);
            INSERT INTO messages (session_id, role, content, timestamp) VALUES ('s1', 'user', 'tidy', 1);
            INSERT INTO messages (session_id, role, content, timestamp) VALUES ('s1', 'assistant', 'Done, 3 files.', 2);
            INSERT INTO messages (session_id, role, content, timestamp) VALUES ('s1', 'assistant', '', 3);
            """
        let made = Process()
        made.executableURL = URL(fileURLWithPath: "/usr/bin/sqlite3")
        made.arguments = [path, sql]
        try? made.run()
        made.waitUntilExit()
        let record = AgentHermesStore.record("s1", hermes: folder)
        suite.expect(record == AgentHermesRecord(source: "local", createdSource: "cli", cwd: "/w/repo", title: "Tidy the repo",
                                                 model: "mock", ended: false),
                     "a session's record: \(record as Any)")
        suite.expect(AgentHermesStore.record("s2", hermes: folder)?.ended == true && AgentHermesStore.record("s2", hermes: folder)?.isLocal == false
                     && AgentHermesStore.record("nope", hermes: folder) == nil, "an ended chat, and no record for an unknown id")
        suite.expect(AgentHermesStore.lastReply("s1", hermes: folder) == "Done, 3 files.", "the last thing the agent wrote")
        suite.expect(AgentHermesStore.record("s1", hermes: folder.appending(path: "missing")) == nil, "no database, no record")
    }

    // MARK: Presence and names

    static func presence(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/someone")
        suite.expect(AgentHookInstaller.isPresent(.hermes, home: home, path: nil,
                                                  isExecutable: { $0 == "/Users/someone/.local/bin/hermes" }, exists: { _ in false })
                     && AgentHookInstaller.isPresent(.hermes, home: home, path: nil, isExecutable: { _ in false },
                                                     exists: { $0 == "/Applications/Hermes.app" })
                     && !AgentHookInstaller.isPresent(.hermes, home: home, path: nil, isExecutable: { _ in false },
                                                      exists: { $0.hasSuffix(".hermes") }),
                     "found by its command or its app, never by ~/.hermes alone")
        suite.expect(ClaudeSessionProvider.hermes.displayName == "Hermes" && ClaudeSessionProvider.hookAgents.firstIndex(of: .hermes) == ClaudeSessionProvider.hookAgents.firstIndex(of: .zcode).map { $0 + 1 }
                     && ClaudeSessionProvider.hermes.answersPermissions && !ClaudeSessionProvider.hermes.takesRepliesDirectly
                     && AgentCLI.command(.hermes) == "hermes" && AgentCLI.startArguments(.hermes, prompt: "go") == ["chat", "-q", "go"]
                     && AgentCLI.startsWithPrompt(.hermes) && ClaudeSessionProvider.hermes.character == .hermes,
                     "named, listed last, answered with keys, and started with its first prompt")
        suite.expect(NotchClaudeSessionsSupport.agentKey(.hermes) == DefaultsKey.notchSessionsAgentHermes
                     && Defaults.registeredDefaults[DefaultsKey.notchSessionsAgentHermes] as? Bool == false,
                     "its switch starts off: Hermes asks the person to approve each new hook")
        let hint = NotchClaudeSessionStrings.enUS.agentHint(.hermes)
        suite.expect(hint.contains("--accept-hooks") && hint.contains("config.yaml"), "and its hint says how to approve them")
    }
}
