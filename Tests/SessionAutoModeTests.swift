// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Sessions in their agent's no-ask mode and Always proceed (#234): what
/// counts as that mode, when a permission goes through, and that it is told
/// apart from questions and plans.
enum SessionAutoModeTests {
    static func run(_ suite: TestSuite) {
        suite.run("auto mode settings") { settings(suite) }
        suite.run("auto mode claude hooks") { claudeHooks(suite) }
        suite.run("auto mode claude engine") { claudeEngine(suite) }
        suite.run("auto mode codex state") { codexState(suite) }
        suite.run("auto mode codex client") { codexClient(suite) }
        suite.run("auto mode log") { log(suite) }
    }

    static let t0 = Date(timeIntervalSince1970: 1_792_600_000)
    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    // MARK: Settings

    static func settings(_ suite: TestSuite) {
        suite.expect(SessionAutoMode.isAuto(claudeMode: "bypassPermissions"), "bypass permissions is Claude's no-ask mode")
        for mode in ["default", "auto", "acceptEdits", "plan", "dontAsk", nil] {
            suite.expect(!SessionAutoMode.isAuto(claudeMode: mode), "\(mode ?? "nil") is not: the island still asks")
        }
        suite.expect(SessionAutoMode.isAuto(codexApproval: "never"), "approvals off is Codex's no-ask mode")
        for policy in ["on-request", "untrusted", "granular", nil] {
            suite.expect(!SessionAutoMode.isAuto(codexApproval: policy), "\(policy ?? "nil") is not")
        }

        let defaults = UserDefaults(suiteName: "com.openbar.tests.auto-mode")!
        defer { defaults.removePersistentDomain(forName: "com.openbar.tests.auto-mode") }
        defaults.removePersistentDomain(forName: "com.openbar.tests.auto-mode")
        let fresh = SessionAutoMode.Settings(defaults: defaults)
        suite.expect(fresh.autoRun == [.claude, .codex] && fresh.alwaysProceed.isEmpty,
                     "auto-mode sessions run on their own by default; Always proceed is off")
        suite.expect(fresh.passes(.claude, autoMode: true) == .autoMode && fresh.passes(.codex, autoMode: true) == .autoMode,
                     "a no-ask session goes through")
        suite.expect(fresh.passes(.claude, autoMode: false) == nil, "a normal session still asks")
        suite.expect(fresh.passes(.opencode, autoMode: true) == nil, "an agent whose mode is not read is never auto")

        defaults.set(false, forKey: DefaultsKey.notchSessionsAutoRunClaude)
        defaults.set(true, forKey: DefaultsKey.notchSessionsAlwaysProceedOpenCode)
        let changed = SessionAutoMode.Settings(defaults: defaults)
        suite.expect(changed.passes(.claude, autoMode: true) == nil, "with the switch off a bypass session asks again")
        suite.expect(changed.passes(.codex, autoMode: true) == .autoMode, "the switch is per agent")
        suite.expect(changed.passes(.opencode, autoMode: false) == .alwaysProceed, "Always proceed lets any session through")
        suite.expect(changed.passes(.vibe, autoMode: false) == nil, "only for the agents picked")
        defaults.set(true, forKey: DefaultsKey.notchSessionsAlwaysProceedClaude)
        suite.expect(SessionAutoMode.Settings(defaults: defaults).passes(.claude, autoMode: true) == .alwaysProceed,
                     "Always proceed covers a bypass session even with the auto switch off")
        suite.expect(SessionAutoMode.alwaysProceedKey(.gemini) == nil && !SessionAutoMode.proceedAgents.contains(.gemini),
                     "an agent the island cannot answer has no Always proceed")
    }

    // MARK: Claude Code

    static let base = #""session_id":"s1","transcript_path":"/tmp/t.jsonl","cwd":"/Users/me/code/app""#

    static func decode(_ json: String) -> ClaudeHookMessage? {
        ClaudeHookMessage.decode(Data(json.utf8), requestID: "r1", now: t0, policy: ClaudeContentPolicy(previews: false),
                                 planModes: { _, _ in [.auto, .acceptEdits, .manual] })
    }

    static func claudeHooks(_ suite: TestSuite) {
        // As Claude Code 2.1.318 sends it in a session started with
        // --dangerously-skip-permissions and a settings `ask` rule.
        let bash = decode(#"{\#(base),"permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"echo hi","description":"Say hi"},"permission_suggestions":[]}"#)
        suite.expect(bash?.fact.mode == "bypassPermissions", "the hook's mode is kept")
        suite.expect(bash?.letsThrough?.tool == "Bash" && bash?.letsThrough?.summary == "echo hi",
                     "a tool's request can be let through")
        let settled = bash?.settled()
        suite.expect(settled?.fact.event == .other("PermissionRequest") && settled?.isPermissionRequest == false
                         && settled?.fact.mode == "bypassPermissions" && settled?.fact.sessionID == "s1",
                     "let through, it waits on nobody but still tells the mode")
        let allow = (try? JSONSerialization.jsonObject(with: ClaudeHookMessage.reply(.allow, suggestions: nil))) as? [String: Any]
        let decision = (allow?["hookSpecificOutput"] as? [String: Any])?["decision"] as? [String: Any]
        suite.expect(decision?["behavior"] as? String == "allow" && decision?["updatedPermissions"] == nil,
                     "the answer allows this call only, adding no rule")

        let question = decode(#"{\#(base),"permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"r"},{"label":"Blue","description":"b"}],"multiSelect":false}]}}"#)
        suite.expect(question?.isPermissionRequest == true && question?.letsThrough == nil,
                     "a question is never let through: it still comes to the island")
        let plan = decode(##"{\##(base),"permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"ExitPlanMode","tool_input":{"plan":"# Plan"}}"##)
        suite.expect(plan?.isPermissionRequest == true && plan?.letsThrough == nil, "nor a plan, which the person approves")
        let start = decode(#"{\#(base),"hook_event_name":"SessionStart","source":"startup"}"#)
        suite.expect(start?.fact.mode == nil && start?.letsThrough == nil, "SessionStart names no mode")
        let prompt = decode(#"{\#(base),"permission_mode":"default","hook_event_name":"UserPromptSubmit","prompt":"again"}"#)
        suite.expect(prompt?.fact.mode == "default" && prompt?.letsThrough == nil, "every other event tells the mode too")

        var reader = ClaudeTranscriptReader(policy: ClaudeContentPolicy(previews: false), path: "/Users/me/.claude/projects/app/s1.jsonl")
        let line = #"{"type":"user","permissionMode":"bypassPermissions","timestamp":"2026-11-16T22:18:37.335Z","message":{"role":"user","content":"go"}}"#
        let facts = reader.parse(Data(line.utf8), now: t0)
        suite.expect(facts.contains { if case .permissionMode("bypassPermissions", _) = $0 { true } else { false } },
                     "a prompt in the transcript tells the mode it was typed in")
        let side = #"{"type":"user","isSidechain":true,"permissionMode":"default","message":{"role":"user","content":"x"}}"#
        suite.expect(!reader.parse(Data(side.utf8), now: t0).contains { if case .permissionMode = $0 { true } else { false } },
                     "a subagent's prompt does not")
    }

    static func claudeEngine(_ suite: TestSuite) {
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [ClaudeSessionEngineTests.record(status: .busy)], now: at(0))
        func only(_ seconds: TimeInterval) -> ClaudeSession? { engine.snapshot(now: at(seconds)).sessions.first }
        suite.expect(only(1)?.autoMode == false, "a session nothing told of yet is not auto")

        engine.ingest(sessionID: "s1", facts: [.permissionMode("bypassPermissions", at: at(1))], now: at(1))
        suite.expect(only(2)?.autoMode == true && engine.mode(of: "s1") == "bypassPermissions",
                     "the transcript's last prompt marks a bypass session, as after a restart")
        engine.ingest(hook: ClaudeHookFact(sessionID: "s1", date: at(5), event: .promptSubmitted, mode: "default"), now: at(5))
        suite.expect(only(6)?.autoMode == false, "switching back to the normal mode takes the mark off")
        engine.ingest(sessionID: "s1", facts: [.permissionMode("bypassPermissions", at: at(3))], now: at(6))
        suite.expect(only(7)?.autoMode == false, "a prompt read late never undoes a newer hook")
        engine.ingest(hook: ClaudeHookFact(sessionID: "s1", date: at(8), event: .toolStarted(
            ClaudeToolUse(id: "t1", name: "Bash", summary: nil, started: at(8))), agentID: "a1", mode: "bypassPermissions"), now: at(8))
        suite.expect(only(9)?.autoMode == false, "a subagent's hook speaks of its own mode, not the session's")
        engine.ingest(hook: ClaudeHookFact(sessionID: "s1", date: at(10), event: .promptSubmitted, mode: "bypassPermissions"), now: at(10))
        suite.expect(only(11)?.autoMode == true, "a switch into bypass marks it from the next event")

        // A let-through request never puts the session in wait.
        guard let message = decode(#"{\#(base),"permission_mode":"bypassPermissions","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf build"}}"#)
        else { return suite.expect(false, "the request decodes") }
        engine.ingest(hook: message.settled().fact, now: at(12))
        suite.expect(only(13)?.phase != .waitingPermission && only(13)?.permission == nil,
                     "a request let through shows no card and no wait")
    }

    // MARK: Codex

    static func codexState(_ suite: TestSuite) {
        let state = CodexSessionTests.stateWithThread()
        state.autoSettings = SessionAutoMode.Settings()
        func shown(_ seconds: TimeInterval) -> ClaudeSession? { state.snapshot(now: at(seconds), previews: false).sessions.first }
        suite.expect(shown(0)?.autoMode == false && state.letsThrough("t1") == nil, "before a resume the policy is unknown")
        // As Codex 0.164 answers thread/resume for a `--yolo` session.
        suite.expect(state.setApprovalPolicy("t1", "never"), "the resume's policy is kept")
        suite.expect(!state.setApprovalPolicy("t1", "never"), "and the same policy changes nothing")
        suite.expect(shown(1)?.autoMode == true && state.letsThrough("t1") == .autoMode, "approvals off mark it auto")
        suite.expect(!state.setApprovalPolicy("nope", "never"), "an unknown thread is left alone")

        // A /permissions switch reaches the island as the next turn starts.
        let changed = state.notify("thread/settings/updated", params: [
            "threadId": "t1", "threadSettings": ["approvalPolicy": "on-request", "approvalsReviewer": "user",
                                                 "sandboxPolicy": ["type": "workspaceWrite"], "model": "gpt-5.5-codex"],
        ], now: at(2))
        suite.expect(changed.changed && shown(3)?.autoMode == false && state.letsThrough("t1") == nil,
                     "switching back to asking takes the mark off")
        state.notify("thread/settings/updated", params: [
            "threadId": "t1", "threadSettings": ["approvalPolicy": ["granular": ["rules": true, "sandbox_approval": true,
                                                                                  "mcp_elicitations": true]]],
        ], now: at(3))
        suite.expect(state.threads["t1"]?.approvalPolicy == "granular" && shown(4)?.autoMode == false,
                     "a granular policy still asks")
        state.notify("thread/settings/updated", params: ["threadId": "t1", "threadSettings": ["approvalPolicy": "never"]], now: at(4))
        suite.expect(shown(5)?.autoMode == true, "and Full Access marks it again")

        // Codex flags the wait a moment before its request comes; for a
        // session let through, that moment is no wait.
        state.notify("thread/status/changed", params: [
            "threadId": "t1", "status": ["type": "active", "activeFlags": ["waitingOnApproval"]],
        ], now: at(10))
        suite.expect(shown(11)?.phase == .generating, "a no-ask session's flag alone does not wait")
        suite.expect(shown(14)?.phase == .waitingPermission, "but one left that long waits, as Codex asks in its terminal")
        state.autoSettings = SessionAutoMode.Settings(autoRun: [], alwaysProceed: [])
        suite.expect(shown(11)?.phase == .waitingPermission, "with the switch off the flag waits at once")
        state.autoSettings = SessionAutoMode.Settings()
        state.proceeded("t1", at: at(20))
        suite.expect(shown(21)?.phase == .generating, "just after an answer the flag that lingers is no wait either")
        state.notify("thread/status/changed", params: ["threadId": "t1", "status": ["type": "active", "activeFlags": []]],
                     now: at(22))
        suite.expect(state.threads["t1"]?.approvalWaitSince == nil, "the wait's start goes with the flag")
    }

    static func codexClient(_ suite: TestSuite) {
        guard let daemon = CodexSessionTests.FakeDaemon() else { return suite.expect(false, "temporary home made") }
        defer { daemon.finish() }
        suite.expect(daemon.listen(), "the fake daemon listens")
        let client = CodexSessionClient(home: daemon.home, clientVersion: "9.9", retry: 0.1)
        let box = SessionAutoMode.Box()
        let log = SessionAutoMode.Log()
        client.auto = box
        client.letThroughLog = log
        client.start {}
        defer { client.stop() }
        guard daemon.accept() else { return suite.expect(false, "the island connects") }
        func next(_ method: String) -> [String: Any]? {
            for _ in 0..<6 {
                guard let message = daemon.message() else { return nil }
                if message["method"] as? String == method { return message }
            }
            return nil
        }
        let initialize = next("initialize")
        daemon.send(["id": initialize?["id"] ?? 0, "result": ["userAgent": "codex_cli_rs/0.164.0"]])
        let list = next("thread/loaded/list")
        daemon.send(["id": list?["id"] ?? 0, "result": ["data": ["t1"]]])
        let read = next("thread/read")
        let thread: [String: Any] = ["id": "t1", "cwd": "/Users/me/code/app", "status": ["type": "active", "activeFlags": []]]
        daemon.send(["id": read?["id"] ?? 0, "result": ["thread": thread]])
        let resume = next("thread/resume")
        daemon.send(["id": resume?["id"] ?? 0, "result": ["thread": thread, "approvalPolicy": "never",
                                                          "sandbox": ["type": "dangerFullAccess"]]])
        suite.expect(CodexSessionTests.eventually { client.snapshot(now: Date(), previews: false).sessions.first?.autoMode == true },
                     "the resume marks the thread auto")

        // Always proceed for Codex: a request is answered at once.
        box.settings = SessionAutoMode.Settings(autoRun: [], alwaysProceed: [.codex])
        daemon.send(["id": 7, "method": "item/commandExecution/requestApproval",
                     "params": ["threadId": "t1", "turnId": "u1", "itemId": "i1", "command": "rm -rf build",
                                "cwd": "/Users/me/code/app"]])
        var answer: [String: Any]?
        for _ in 0..<4 {
            guard let message = daemon.message() else { break }
            if message["method"] == nil, message["id"] as? Int == 7 { answer = message; break }
        }
        suite.expect((answer?["result"] as? [String: Any])?["decision"] as? String == "accept",
                     "the approval is accepted for this call only: \(CodexSessionTests.json(answer))")
        let after = client.snapshot(now: Date(), previews: false).sessions.first
        suite.expect(after?.permission == nil && after?.phase != .waitingPermission, "no card waits")
        let listed = log.entries(of: "t1")
        suite.expect(listed.count == 1 && listed.first?.tool == "Command" && listed.first?.reason == .alwaysProceed,
                     "it is listed for the session: \(listed)")

        // A question is never answered for the person.
        daemon.send(["id": 8, "method": "item/tool/requestUserInput",
                     "params": ["threadId": "t1", "turnId": "u1", "itemId": "i2",
                                "questions": [["id": "q", "header": "Pick", "question": "Which?",
                                               "options": [["label": "A", "description": "a"]]]]]])
        suite.expect(CodexSessionTests.eventually { client.snapshot(now: Date(), previews: false).sessions.first?.phase == .waitingInput },
                     "a question still waits on the island")

        // Off again: the next approval waits for the person.
        box.settings = SessionAutoMode.Settings(autoRun: [], alwaysProceed: [])
        daemon.send(["id": 9, "method": "item/commandExecution/requestApproval",
                     "params": ["threadId": "t1", "turnId": "u1", "itemId": "i3", "command": "ls"]])
        suite.expect(CodexSessionTests.eventually {
            client.snapshot(now: Date(), previews: false).sessions.first?.permission?.requestID == "codex:8"
                || client.waits("t1", requestID: "codex:9")
        }, "with nothing let through the approval waits")
        suite.expect(log.entries(of: "t1").count == 1, "and nothing more is listed")
    }

    // MARK: Log

    static func log(_ suite: TestSuite) {
        let log = SessionAutoMode.Log()
        for index in 0..<(SessionAutoMode.Log.kept + 5) {
            log.add(ClaudeLetThrough(id: "r\(index)", tool: "Bash", summary: "echo \(index)", at: at(Double(index)),
                                     reason: .autoMode), to: "s1")
        }
        let kept = log.entries(of: "s1")
        suite.expect(kept.count == SessionAutoMode.Log.kept && kept.first?.id == "r5" && kept.last?.id == "r34",
                     "the newest are kept, oldest first")
        var session = NotchSessionAgentTests.session("s1", .generating, provider: .claude)
        session.letThrough = []
        let other = NotchSessionAgentTests.session("s2", .idle, provider: .claude)
        let decorated = log.decorate([session, other])
        suite.expect(decorated[0].letThrough.count == SessionAutoMode.Log.kept && decorated[1].letThrough.isEmpty,
                     "each session gets its own list")
        log.add(ClaudeLetThrough(id: "x", tool: "Bash", summary: nil, at: at(1000), reason: .autoMode), to: "gone")
        log.prune(listed: ["s1"], now: at(1100))
        suite.expect(!log.entries(of: "gone").isEmpty, "a session heard before it is listed keeps its list a while")
        log.prune(listed: ["s1"], now: at(2000))
        suite.expect(log.entries(of: "gone").isEmpty && !log.entries(of: "s1").isEmpty,
                     "and loses it once long gone; listed ones keep theirs")
        log.clear()
        suite.expect(log.entries(of: "s1").isEmpty, "cleared when the section stops")
    }
}
