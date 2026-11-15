// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Mistral Vibe's approvals and questions answered from the island (#251):
/// the callbacks Vibe's app-server asks of its client, as Vibe 3.1.2 wrote
/// them against a stand-in model, what the proxy tells the island and what it
/// sends back, the session the island keeps from it, and the line New session runs.
enum VibeAppServerTests {
    static func run(_ suite: TestSuite) {
        suite.run("vibe app-server: callbacks read") { callbacks(suite) }
        suite.run("vibe app-server: settled elsewhere") { settled(suite) }
        suite.run("vibe app-server: callback/result") { results(suite) }
        suite.run("vibe app-server: late answers") { acknowledgements(suite) }
        suite.run("vibe app-server: envelopes") { envelopes(suite) }
        suite.run("vibe app-server: island session") { islandSession(suite) }
        suite.run("vibe app-server: replies") { replies(suite) }
        suite.run("vibe app-server: launcher") { launcher(suite) }
    }

    typealias T = AgentHookSessionTests
    static var now: Date { T.now }

    // As Vibe 3.1.2 sent them (trimmed of what the proxy does not read).
    static let approval = #"{"jsonrpc":"2.0","id":1,"method":"callback/call","params":{"callback":{"id":"approval-call_1-da12e44f","sessionId":"6267026e-5700-b9cb-3d47-f84eb25621f1","turnId":"turn-aee2","type":"callback","callbackId":"approval-call_1-da12e44f","title":"Approve file_system.bash","detail":{"kind":"approval","effect":{"toolName":"file_system.bash","display":{"summary":"bash: touch approved.txt","message":"touch approved.txt"},"kind":"shell","input":{"command":"touch approved.txt"}},"requiredPermissions":[{"scope":"command_pattern","invocationPattern":"touch approved.txt","sessionPattern":"touch *","label":"touch *","pathScopeRoot":null}],"choices":["approve","approve_for_session","approve_permanently","deny","cancel_turn"],"pathScopeChoices":[],"relatedEntryId":"effect-dc76","reason":null},"state":{"status":"open"}}}}"#
    static let question = #"{"jsonrpc":"2.0","id":2,"method":"callback/call","params":{"callback":{"id":"user-input-call_q-716120f1","sessionId":"30f956f5","type":"callback","callbackId":"user-input-call_q-716120f1","title":"Question","detail":{"kind":"user_input","request":{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multiSelect":false,"hideOther":false}],"footerNote":null}},"state":{"status":"open"}}}}"#
    static let session = "6267026e-5700-b9cb-3d47-f84eb25621f1"

    static func data(_ text: String) -> Data { Data(text.utf8) }
    static func json(_ data: Data?) -> [String: Any] {
        data.flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] } ?? [:]
    }

    static func callbacks(_ suite: TestSuite) {
        let asked = VibeWire.callback(inServerLine: data(approval))
        suite.expect(asked?.kind == .approval && asked?.id == "approval-call_1-da12e44f" && asked?.session == session
                     && asked?.tool == "file_system.bash" && asked?.input == ["command": "touch approved.txt"]
                     && asked?.offersSession == true, "an approval is read with its tool, input and choices: \(String(describing: asked))")
        let input = VibeWire.callback(inServerLine: data(question))
        suite.expect(input?.kind == .question && input?.questions.count == 1 && input?.questions[0].question == "Which color?"
                     && input?.questions[0].options.map(\.label) == ["Red", "Blue"] && input?.questions[0].multiSelect == false
                     && input?.id == "user-input-call_q-716120f1", "a question is read with its options: \(String(describing: input))")
        let connector = approval.replacingOccurrences(of: #""kind":"approval","effect""#, with: #""kind":"connector_auth","effect""#)
        suite.expect(VibeWire.callback(inServerLine: data(connector)) == nil, "a callback of another kind is the terminal's")
        let noApprove = approval.replacingOccurrences(of: #""choices":["approve","approve_for_session","approve_permanently","deny","cancel_turn"]"#,
                                                      with: #""choices":["deny","cancel_turn"]"#)
        suite.expect(VibeWire.callback(inServerLine: data(noApprove)) == nil, "an approval that cannot be given is the terminal's")
        let once = approval.replacingOccurrences(of: #""approve_for_session","#, with: "")
        suite.expect(VibeWire.callback(inServerLine: data(once))?.offersSession == false, "for the session is offered only when the server offers it")
        suite.expect(VibeWire.callback(inServerLine: data(#"{"jsonrpc":"2.0","method":"session/updated","params":{"sessionId":"x"}}"#)) == nil
                     && VibeWire.callback(inServerLine: data("not json callback/call")) == nil
                     && VibeWire.callback(inServerLine: data("")) == nil, "other lines and garbage are no callback")
        let noSession = approval.replacingOccurrences(of: #""sessionId":"6267026e-5700-b9cb-3d47-f84eb25621f1","#, with: "")
        suite.expect(VibeWire.callback(inServerLine: data(noSession)) == nil, "a callback with no session is dropped")
    }

    static func settled(_ suite: TestSuite) {
        let answered = #"{"jsonrpc":"2.0","method":"history/entryUpdated","params":{"eventId":12,"sessionId":"s","turnId":"t","entryId":"approval-call_1-da12e44f","patch":[{"op":"replace","path":"/generationStatus","value":"completed"},{"op":"replace","path":"/state","value":{"status":"answered","output":{"type":"approval","decision":{"type":"approve"}}}}]}}"#
        suite.expect(VibeWire.settledCallback(inServerLine: data(answered)) == "approval-call_1-da12e44f",
                     "an entry answered anywhere settles its callback")
        let cancelled = answered.replacingOccurrences(of: #""status":"answered""#, with: #""status":"cancelled""#)
        suite.expect(VibeWire.settledCallback(inServerLine: data(cancelled)) == "approval-call_1-da12e44f", "so does one cancelled")
        let open = answered.replacingOccurrences(of: #""status":"answered""#, with: #""status":"open""#)
        suite.expect(VibeWire.settledCallback(inServerLine: data(open)) == nil, "an entry still open settles nothing")
        let effect = #"{"jsonrpc":"2.0","method":"history/entryUpdated","params":{"entryId":"effect-dc76","patch":[{"op":"replace","path":"/generationStatus","value":"completed"},{"op":"add","path":"/state/decision","value":"execute"}]}}"#
        suite.expect(VibeWire.settledCallback(inServerLine: data(effect)) == nil, "a tool finishing is not a settled callback")
        let terminal = #"{"jsonrpc":"2.0","id":17,"method":"callback/result","params":{"sessionId":"s","result":{"callbackId":"approval-call_1-da12e44f","output":{"type":"approval","decision":{"type":"approve"},"feedback":null}}}}"#
        suite.expect(VibeWire.answeredCallback(inClientLine: data(terminal)) == "approval-call_1-da12e44f",
                     "the terminal's own callback/result names the callback it answered")
        suite.expect(VibeWire.answeredCallback(inClientLine: data(#"{"jsonrpc":"2.0","id":3,"method":"turn/start","params":{}}"#)) == nil,
                     "other requests of the terminal answer nothing")
        let started = #"{"jsonrpc":"2.0","id":3,"result":{"lastEventId":0,"state":{"session":{"id":"abc","cwd":"/w/app","status":{"type":"idle"}}}}}"#
        let seen = VibeWire.session(inServerLine: data(started))
        suite.expect(seen?.id == "abc" && seen?.cwd == "/w/app", "a session the server opens is told with its folder")
        suite.expect(VibeWire.session(inServerLine: data(#"{"jsonrpc":"2.0","method":"turn/started","params":{"sessionId":"def"}}"#))?.id == "def",
                     "a session is also heard from its events")
    }

    static func results(_ suite: TestSuite) {
        let callback = VibeWire.callback(inServerLine: data(approval))!
        let approve = json(VibeWire.result(id: "openbar-1", callback: callback, reply: ["decision": "approve"]))
        let params = approve["params"] as? [String: Any]
        let result = params?["result"] as? [String: Any]
        let output = result?["output"] as? [String: Any]
        suite.expect(approve["method"] as? String == "callback/result" && approve["id"] as? String == "openbar-1"
                     && params?["sessionId"] as? String == session && result?["callbackId"] as? String == "approval-call_1-da12e44f"
                     && output?["type"] as? String == "approval" && (output?["decision"] as? [String: Any])?["type"] as? String == "approve",
                     "an approval is answered with the callback/result Vibe's own client sends: \(approve)")
        let always = json(VibeWire.result(id: "openbar-2", callback: callback, reply: ["decision": "approve_for_session"]))
        suite.expect((((always["params"] as? [String: Any])?["result"] as? [String: Any])?["output"] as? [String: Any])
                        .flatMap { $0["decision"] as? [String: Any] }?["type"] as? String == "approve_for_session", "for the session is sent as it is")
        let deny = json(VibeWire.result(id: "openbar-3", callback: callback, reply: ["decision": "deny", "feedback": "Denied from the Dynamic Island."]))
        let denied = (((deny["params"] as? [String: Any])?["result"] as? [String: Any])?["output"] as? [String: Any])
        suite.expect((denied?["decision"] as? [String: Any])?["type"] as? String == "deny"
                     && denied?["feedback"] as? String == "Denied from the Dynamic Island.", "a denial carries what the model reads")
        suite.expect(VibeWire.result(id: "x", callback: callback, reply: ["decision": "approve_permanently"]) == nil
                     && VibeWire.result(id: "x", callback: callback, reply: ["decision": "maybe"]) == nil
                     && VibeWire.result(id: "x", callback: callback, reply: [:]) == nil,
                     "only the three decisions the island offers are sent")
        let once = VibeWire.callback(inServerLine: data(approval.replacingOccurrences(of: #""approve_for_session","#, with: "")))!
        suite.expect(VibeWire.result(id: "x", callback: once, reply: ["decision": "approve_for_session"]) == nil,
                     "for the session is not sent where the server did not offer it")
        let asked = VibeWire.callback(inServerLine: data(question))!
        let picked = json(VibeWire.result(id: "openbar-4", callback: asked, reply: [
            "answers": [["question": "Which color?", "answer": "Red", "isOther": false]]]))
        let answers = ((((picked["params"] as? [String: Any])?["result"] as? [String: Any])?["output"] as? [String: Any])?["result"]) as? [String: Any]
        suite.expect(((answers?["answers"] as? [[String: Any]])?.first?["answer"] as? String) == "Red" && answers?["cancelled"] as? Bool == false,
                     "an answer goes back as the user_input result: \(picked)")
        let skipped = json(VibeWire.result(id: "openbar-5", callback: asked, reply: ["cancelled": true]))
        let skippedResult = ((((skipped["params"] as? [String: Any])?["result"] as? [String: Any])?["output"] as? [String: Any])?["result"]) as? [String: Any]
        suite.expect(skippedResult?["cancelled"] as? Bool == true, "a skipped question is cancelled")
        suite.expect(VibeWire.result(id: "x", callback: asked, reply: ["answers": [["question": "Another?", "answer": "Red"]]]) == nil
                     && VibeWire.result(id: "x", callback: asked, reply: ["decision": "approve"]) == nil
                     && VibeWire.result(id: "x", callback: asked, reply: ["answers": []]) == nil,
                     "an answer to a question that was not asked, or none, is not sent")
    }

    static func acknowledgements(_ suite: TestSuite) {
        let own = data(#"{"jsonrpc":"2.0","id":"openbar-3","result":{"lastEventId":9,"accepted":true}}"#)
        let taken = VibeWire.response(inServerLine: own, idPrefix: "openbar-")
        suite.expect(taken?.id == "openbar-3" && taken?.accepted == true && taken?.lastEventId == 9, "the server's answer to a request of the proxy's is told")
        let conflict = data(#"{"jsonrpc":"2.0","id":"openbar-4","error":{"code":"conflict","message":"x","data":null}}"#)
        suite.expect(VibeWire.response(inServerLine: conflict, idPrefix: "openbar-")?.accepted == false
                     && VibeWire.response(inServerLine: data(#"{"jsonrpc":"2.0","id":17,"result":{"accepted":true}}"#), idPrefix: "openbar-") == nil
                     && VibeWire.response(inServerLine: data(#"{"jsonrpc":"2.0","id":"ob-1","result":{}}"#), idPrefix: "openbar-") == nil,
                     "a conflict is not a taken answer, and the terminal's own answers are not the proxy's")
        let terminal = data(#"{"jsonrpc":"2.0","id":17,"method":"callback/result","params":{"sessionId":"s","result":{"callbackId":"c"}}}"#)
        let reply = json(VibeWire.acknowledgement(forClientLine: terminal, lastEventId: 9))
        suite.expect(reply["id"] as? Int == 17 && (reply["result"] as? [String: Any])?["accepted"] as? Bool == true
                     && (reply["result"] as? [String: Any])?["lastEventId"] as? Int == 9,
                     "a late answer of the terminal is told it was taken: \(reply)")
        suite.expect(VibeWire.acknowledgement(forClientLine: data(#"{"jsonrpc":"2.0","id":1,"method":"turn/start","params":{}}"#), lastEventId: 0) == nil,
                     "only a callback/result is acknowledged")
    }

    static func envelopes(_ suite: TestSuite) {
        let callback = VibeWire.callback(inServerLine: data(approval))!
        let sent = VibeWire.envelope(VibeWire.event(for: callback), pid: 4242)!
        suite.expect(AgentHookMessage.isEnvelope(sent) && AgentHookMessage.waits(sent),
                     "an approval's connection waits for the island's answer")
        let decoded = AgentHookMessage.decode(sent, now: now, policy: ClaudeContentPolicy(previews: true))
        suite.expect(decoded?.fromProxy == true && decoded?.permission?.session == session
                     && decoded?.permission?.request == "approval-call_1-da12e44f" && decoded?.events.count == 1,
                     "it names the request the connection holds: \(String(describing: decoded))")
        if case .heldPermission(let permission)? = decoded?.events.first?.kind {
            suite.expect(permission.tool == "Bash" && permission.detail == "touch approved.txt" && permission.offersAlways
                         && permission.requestID == "approval-call_1-da12e44f" && permission.content?.complete == true,
                         "the approval reads as Claude Code's Bash: \(permission)")
        } else {
            suite.expect(false, "an approval is a request the island holds: \(String(describing: decoded?.events))")
        }
        suite.expect(decoded?.events.first?.pid == 0, "the proxy's pid is not the agent's")
        let asked = VibeWire.callback(inServerLine: data(question))!
        let questionSent = VibeWire.envelope(VibeWire.event(for: asked), pid: 4242)!
        suite.expect(AgentHookMessage.waits(questionSent), "so does a question's")
        if case .heldPermission(let permission)? = AgentHookMessage.decode(questionSent, now: now, policy: ClaudeContentPolicy(previews: true))?.events.first?.kind {
            suite.expect(permission.asksQuestions && permission.questions.first?.question == "Which color?"
                         && permission.questions.first?.options.map(\.label) == ["Red", "Blue"]
                         && permission.questions.first?.options.first?.description == "warm" && permission.tool == ClaudeQuestion.tool,
                         "a question is asked as Claude Code's AskUserQuestion: \(permission)")
        } else {
            suite.expect(false, "a question is a request the island holds")
        }
        let attach = VibeWire.envelope(["kind": "attach", "session": session, "cwd": "/w/app"], pid: 4242)!
        let attached = AgentHookMessage.decode(attach, now: now, policy: ClaudeContentPolicy(previews: true))
        suite.expect(!AgentHookMessage.waits(attach) && attached?.attached == session && attached?.events.isEmpty == true,
                     "a session the proxy serves is told at once and holds nothing")
        let settled = VibeWire.envelope(["kind": "settled", "session": session, "request": "approval-call_1-da12e44f"], pid: 4242)!
        let gone = AgentHookMessage.decode(settled, now: now, policy: ClaudeContentPolicy(previews: true))
        suite.expect(!AgentHookMessage.waits(settled), "one answered elsewhere waits for nothing")
        if case .permissionSettled(let request)? = gone?.events.first?.kind {
            suite.expect(request == "approval-call_1-da12e44f", "it settles the request")
        } else {
            suite.expect(false, "it settles the request: \(String(describing: gone?.events))")
        }
        // Vibe's hooks are not the proxy.
        let hook = T.envelope("vibe", hook: "notification", ["hook_event_name": "notification", "session_id": "v1",
                                                             "notification_type": "approval_requested"])
        suite.expect(!AgentHookMessage.waits(hook) && AgentHookMessage.decode(hook, now: now, policy: ClaudeContentPolicy(previews: true))?.fromProxy == false,
                     "a hook of Vibe's holds nothing")
        let hookNamedProxy = T.envelope("vibe", hook: "proxy", ["kind": "approval", "session": "v1", "request": "r1"])
        let forged = AgentHookMessage.decode(hookNamedProxy, now: now, policy: ClaudeContentPolicy(previews: true))
        suite.expect(forged?.events.count == 1 || forged?.events.isEmpty == true, "a proxy message with no input is read without a crash")
    }

    static func islandSession(_ suite: TestSuite) {
        let tracker = AgentHookSessions()
        let hook: [String: Any] = ["hook_event_name": "session_start", "session_id": session, "cwd": "/w/vibe-app", "model": "devstral"]
        T.feed(tracker, T.envelope("vibe", hook: "session_start", pid: 7101, hook))
        let callback = VibeWire.callback(inServerLine: data(approval))!
        T.feed(tracker, VibeWire.envelope(VibeWire.event(for: callback), pid: 4242)!)
        var listed = T.snapshot(tracker).sessions
        suite.expect(listed.count == 1 && listed[0].phase == .waitingPermission && listed[0].permission?.tool == "Bash"
                     && listed[0].permission?.detail == "touch approved.txt" && listed[0].waitingFor == nil,
                     "an approval the proxy holds is answered here, not in the terminal: \(listed)")
        suite.expect(listed[0].cwd == "/w/vibe-app" && listed[0].pid == 7101, "the proxy leaves the session's folder and process alone")
        // Vibe's hook for the same approval does not replace it: the island drops that one itself (a hook sets no answer here).
        T.feed(tracker, VibeWire.envelope(["kind": "settled", "session": session, "request": callback.id], pid: 4242)!)
        listed = T.snapshot(tracker).sessions
        suite.expect(listed[0].permission == nil && listed[0].phase != .waitingPermission, "answered in the terminal, the card goes")
        // A session the person started by hand is still answered in the terminal.
        let other = AgentHookSessions()
        T.feed(other, T.envelope("vibe", hook: "notification", pid: 7102, ["hook_event_name": "notification", "session_id": "hand", "cwd": "/w/x",
                                                                          "notification_type": "approval_requested", "tool_name": "file_system.bash",
                                                                          "message": "Vibe needs your approval"]))
        let byHand = T.snapshot(other).sessions
        suite.expect(byHand[0].permission == nil && byHand[0].waitingFor == CodexSessionState.answerInTerminal,
                     "a session started by hand still says to answer in the terminal")
        // A question the island holds.
        let asked = VibeWire.callback(inServerLine: data(question))!
        let questions = AgentHookSessions()
        T.feed(questions, T.envelope("vibe", hook: "session_start", pid: 7103, ["hook_event_name": "session_start", "session_id": "30f956f5", "cwd": "/w/q"]))
        T.feed(questions, VibeWire.envelope(VibeWire.event(for: asked), pid: 4242)!)
        let waiting = T.snapshot(questions).sessions
        suite.expect(waiting[0].permission?.asksQuestions == true && waiting[0].waitingFor == nil, "a question is answered here too")
    }

    static func replies(_ suite: TestSuite) {
        typealias Reply = ClaudeHookMessage.Decision
        func reply(_ decision: Reply, _ questions: [ClaudeQuestion] = []) -> [String: Any] {
            json(AgentHookMessage.vibeReply(decision, questions: questions))
        }
        suite.expect(reply(.allow)["decision"] as? String == "approve" && reply(.allowAlways)["decision"] as? String == "approve_for_session",
                     "Allow and Always are Vibe's approve and approve for the session")
        let denied = reply(.deny)
        suite.expect(denied["decision"] as? String == "deny" && denied["feedback"] as? String == ClaudeHookMessage.denyMessage,
                     "Deny tells the model who denied")
        let color = ClaudeQuestion(question: "Which color?", header: "Color",
                                   options: [.init(label: "Red", description: "warm"), .init(label: "Blue", description: "cool")], multiSelect: false)
        let picked = reply(.answer(["Which color?": ["Blue"]]), [color])
        suite.expect((picked["answers"] as? [[String: Any]])?.first?["answer"] as? String == "Blue"
                     && (picked["answers"] as? [[String: Any]])?.first?["isOther"] as? Bool == false, "a listed option is an answer")
        let typed = reply(.answer(["Which color?": ["green"]]), [color])
        suite.expect((typed["answers"] as? [[String: Any]])?.first?["isOther"] as? Bool == true, "one typed by hand is Other")
        let several = ClaudeQuestion(question: "Which?", options: [.init(label: "A"), .init(label: "B")], multiSelect: true)
        let both = reply(.answer(["Which?": ["A", "B"]]), [several])
        suite.expect((both["answers"] as? [[String: Any]])?.first?["answer"] as? String == "A, B"
                     && (both["answers"] as? [[String: Any]])?.first?["isOther"] as? Bool == false, "several picked are joined")
        suite.expect(AgentHookMessage.vibeReply(.answer(["Which color?": ["Red"]]), questions: []) == nil
                     && AgentHookMessage.vibeReply(.answer([:]), questions: [color]) == nil
                     && AgentHookMessage.vibeReply(.answer(["Other?": ["x"]]), questions: [color]) == nil,
                     "an answer with nothing to match is not sent")
        suite.expect(reply(.deny, [color])["cancelled"] as? Bool == true, "skipping a question cancels it")
    }

    static func launcher(_ suite: TestSuite) {
        let uv = "#!/bin/sh\n'''exec' '/Users/me/.local/share/uv/tools/mistral-vibe/bin/python' \"$0\" \"$@\"\n' '''\n# -*- coding: utf-8 -*-\n"
        suite.expect(VibeLauncher.python(inScript: uv) == ["/Users/me/.local/share/uv/tools/mistral-vibe/bin/python"],
                     "uv's console script names its Python: \(VibeLauncher.python(inScript: uv))")
        suite.expect(VibeLauncher.python(inScript: "#!/opt/homebrew/opt/python@3.13/bin/python3.13\nimport sys\n") == ["/opt/homebrew/opt/python@3.13/bin/python3.13"],
                     "so does a shebang")
        suite.expect(VibeLauncher.python(inScript: "#!/bin/sh\necho hi\n").isEmpty && VibeLauncher.python(inScript: "").isEmpty,
                     "a shell script names none")
        let tool = "/Users/me/.local/share/uv/tools/mistral-vibe/bin"
        let present: Set<String> = ["\(tool)/vibe", "\(tool)/vibe-app-server", "\(tool)/python"]
        let install = VibeLauncher.install(vibe: "\(tool)/vibe", isExecutable: { present.contains($0) },
                                           read: { _ in uv })
        suite.expect(install == VibeLauncher.Install(python: "\(tool)/python", server: "\(tool)/vibe-app-server"),
                     "Vibe's Python and server are found beside the console script: \(String(describing: install))")
        suite.expect(VibeLauncher.install(vibe: "\(tool)/vibe", isExecutable: { $0 != "\(tool)/vibe-app-server" && present.contains($0) }, read: { _ in uv }) == nil
                     && VibeLauncher.install(vibe: "\(tool)/vibe", isExecutable: { $0 == "\(tool)/vibe-app-server" }, read: { _ in uv }) == nil,
                     "with no server or no Python there is nothing to run the proxy with: plain vibe runs")
        let line = VibeLauncher.line(install: install!, proxy: "/Applications/Openbar (Developer).app/Contents/MacOS/Openbar",
                                     socket: "/Users/me/Library/Containers/x/claude-hooks.sock", prompt: "it's a prompt", folder: "/w/my app")
        suite.expect(line?.hasPrefix("cd '/w/my app' && '\(tool)/python' '-I' '-c' ") == true
                     && line?.contains("exec_rust_cli") == true && line?.contains("--vibe-proxy") == true
                     && line?.contains("Openbar (Developer).app") == true && line?.hasSuffix("'it'\\''s a prompt'") == true,
                     "New session runs Vibe's Python with the proxy as its server and the prompt last: \(String(describing: line))")
        let noPrompt = VibeLauncher.line(install: install!, proxy: "/p/Openbar", socket: "/s", prompt: "", folder: nil)
        suite.expect(noPrompt?.hasSuffix("]'") == true, "with no prompt, vibe opens empty: \(String(describing: noPrompt))")
        suite.expect(VibeProxy.isRequested(by: ["Openbar", "--vibe-proxy", "/s", "/server"]) && !VibeProxy.isRequested(by: ["Openbar", "--selftest"])
                     && !VibeProxy.isRequested(by: ["Openbar"]), "the proxy runs only when asked for as the first argument")
    }
}
