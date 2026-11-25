// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Moonshot AI's Kimi Code (`kimi`, #240): its terminal UI, `kimi web` and
/// the Kimi Code app. Its hooks are `[[hooks]]` tables in
/// `$KIMI_CODE_HOME/config.toml` (`~/.kimi-code` by default), each with only
/// `event`, `matcher`, `command` and `timeout`: any other key stops Kimi from
/// loading the file at all. Kimi writes that file back line by line, so the
/// island's tables, between its two marker lines at the end, stay where they
/// are when Kimi saves its own settings.
///
/// Kimi's hooks can't approve anything: `PermissionRequest` and
/// `PermissionResult` only tell of a request, and what `UserPromptSubmit`
/// prints goes into the model's context, so the island's hook prints
/// nothing. Where the session runs decides how the island answers:
/// - In the terminal UI, which holds the session in its own process, with
///   keys: `1` approves once, `2` for the session, `3` rejects; a plan takes
///   `1` to approve and `2` to reject; a question takes each answer's
///   number (a multiple choice its numbers and Tab), then `1` submits.
/// - In `kimi web` and the Kimi Code app, a local server (KAP) holds it and
///   takes every answer and reply over HTTP. That server runs the hooks, so
///   the hook's process is the one `server/instances/*.json` names.
/// - In Kimi Code for VS Code nothing outside can answer: the person does.
enum AgentKimi {
    /// Every event the island hears. A turn the person stops ends with
    /// `Interrupt` instead of `Stop`; one that fails with `StopFailure`.
    static let events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "TurnStarted", "PreToolUse", "PostToolUse",
                         "PostToolUseFailure", "PermissionRequest", "PermissionResult", "Stop", "StopFailure", "Interrupt"]

    static let questionTool = "AskUserQuestion"
    static let planTool = "ExitPlanMode"

    /// Where a session is shown, as the island keeps it: a KAP server
    /// (`kimi web`, the app) or VS Code. A terminal session has none.
    static let kapClient = "kap"
    static let vscodeClient = "vscode"

    // MARK: Places

    /// Kimi Code's folder: `KIMI_CODE_HOME`, or `.kimi-code` in the home folder.
    static func folder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["KIMI_CODE_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath) }
            ?? home.appending(path: ".kimi-code")
    }

    static func config(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        folder(home: home, environment: environment).appending(path: "config.toml")
    }

    // MARK: Hooks

    /// The island's `[[hooks]]` tables, between its two marker lines. One
    /// per event, matching everything; the timeout a whole number, as Kimi
    /// wants it.
    static func block(socket: String, bundle: String) -> String {
        var lines = [AgentHookInstaller.vibeStart(bundle: bundle)]
        for event in events {
            lines += ["[[hooks]]",
                      "event = \(AgentHookInstaller.tomlString(event))",
                      "command = \(AgentHookInstaller.tomlString(AgentHookInstaller.command(.kimi, event: event, socket: socket, bundle: bundle)))",
                      "timeout = \(AgentHookInstaller.eventTimeout)"]
        }
        lines.append(AgentHookInstaller.vibeEnd(bundle: bundle))
        return lines.joined(separator: "\n") + "\n"
    }

    /// The file with the island's tables at its end, after the person's own.
    static func installing(_ text: String?, socket: String, bundle: String) -> String {
        var base = text.map { AgentHookInstaller.vibeRemoving($0, bundle: bundle) ?? $0 } ?? ""
        if !base.isEmpty, !base.hasSuffix("\n") { base += "\n" }
        if !base.isEmpty { base += "\n" }
        return base + block(socket: socket, bundle: bundle)
    }

    static func state(_ text: String?, socket: String, bundle: String) -> AgentHookInstaller.State {
        guard let text else { return .missing }
        if text.contains(block(socket: socket, bundle: bundle)) { return .installed }
        if text.contains(AgentHookInstaller.marker(bundle: bundle)) { return .outdated }
        // `hooks = [...]` can't take tables beside it: the file is the person's.
        return AgentHookInstaller.vibeInline(text) ? .unreadable : .missing
    }

    // MARK: Asks

    /// The card for a request Kimi's `PermissionRequest` tells of. A plan
    /// carries its text; a command or a file change reads as Claude Code's.
    /// Kimi's own `id` names it, the same id its server answers it by.
    static func ask(_ tool: String, raw: String, request: String, input: [String: Any], display: [String: Any]?,
                    cwd: String?, at date: Date) -> ClaudePermission {
        if raw == planTool || display?["kind"] as? String == "plan_review" {
            let plan = display?["plan"] as? String ?? ""
            let content = PermissionContent(full: plan, complete: true, originalLength: plan.unicodeScalars.count)
            var permission = ClaudePermission(requestID: request, tool: ClaudeToolSummary.planTool,
                                              summary: ClaudeToolSummary.clip(plan.split(whereSeparator: \.isNewline)
                                                .first.map(String.init) ?? ""),
                                              detail: content.preview, content: content, offersAlways: false, since: date)
            permission.planModes = []
            return permission
        }
        // Kimi's tools take Claude Code's arguments but for a file's `path`,
        // so the request reads as Claude Code's, whatever the previews.
        let input = claudeInput(input)
        let full = ClaudeToolSummary.permissionDetail(name: tool, input: input)
        let complete = tool != "Bash" || (input["command"] as? String).map { !$0.isEmpty } == true
        let content = PermissionContent(full: full, complete: complete, originalLength: complete ? full.unicodeScalars.count : nil)
        // "Approve for this session" is Kimi's own second choice.
        var permission = ClaudePermission(
            requestID: request, tool: tool.isEmpty ? "Tool" : tool,
            summary: ClaudeToolSummary.clip(full.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
            detail: content.preview, content: content, offersAlways: true, since: date)
        permission.inputKey = ClaudeToolSummary.inputKey(name: tool, input: input)
        permission.diffs = ClaudeFileDiffs.diffs(tool: tool, input: input, cwd: cwd)
        return permission
    }

    /// The card for a question its `PreToolUse` names: its tool call's id
    /// names it, the same id its server's interaction carries.
    static func question(call: String, input: [String: Any], at date: Date) -> ClaudePermission? {
        guard let questions = ClaudeQuestion.questions(in: input) else { return nil }
        var permission = ClaudePermission(requestID: call, tool: ClaudeQuestion.tool,
                                          summary: ClaudeToolSummary.clip(questions[0].question), offersAlways: false, since: date)
        permission.questions = questions
        return permission
    }

    /// Kimi's file tools name the file `path`; Claude Code's `file_path`.
    static func claudeInput(_ input: [String: Any]) -> [String: Any] {
        guard input["file_path"] == nil, let path = input["path"] as? String else { return input }
        var input = input
        input["file_path"] = path
        return input
    }

    // MARK: Keys

    /// The keys that answer Kimi's terminal dialog, nil for an answer it
    /// can't be given by keys. Each choice takes its number at once.
    static func keys(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission) -> [ClaudeTypingStep]? {
        if asked.tool == ClaudeToolSummary.planTool {
            switch decision {
            case .allow, .allowAlways, .approvePlan: return [.keys([.digit(1)])]
            case .deny: return [.keys([.digit(2)])]
            case .answer: return nil
            }
        }
        if asked.asksQuestions {
            switch decision {
            // Escape cancels the question; Kimi goes on without an answer.
            case .deny: return [.keys([.escape])]
            case .answer(let answers): return questionKeys(answers, asked.questions)
            default: return nil
            }
        }
        switch decision {
        case .allow: return [.keys([.digit(1)])]
        case .allowAlways: return [.keys([.digit(2)])]
        case .deny: return [.keys([.digit(3)])]
        default: return nil
        }
    }

    /// Each question's answer in the dialog's order: a single choice is its
    /// number, which moves on by itself; a multiple choice ticks each number,
    /// then Tab moves on. The review that follows takes `1` to submit. Only
    /// listed choices, nine at most.
    static func questionKeys(_ answers: [String: [String]], _ questions: [ClaudeQuestion]) -> [ClaudeTypingStep]? {
        guard let picks = picks(answers, questions) else { return nil }
        var steps: [ClaudeTypingStep] = []
        for (question, numbers) in zip(questions, picks) {
            if question.multiSelect {
                for number in numbers { steps += [.keys([.digit(number)]), .wait(0.15)] }
                steps += [.keys([.tab]), .wait(0.3)]
            } else {
                steps += [.keys([.digit(numbers[0])]), .wait(0.3)]
            }
        }
        return steps + [.keys([.digit(1)])]
    }

    /// The number of each picked choice, question by question; nil when one
    /// is unanswered, not listed, past nine, or more than one for a single choice.
    static func picks(_ answers: [String: [String]], _ questions: [ClaudeQuestion]) -> [[Int]]? {
        guard !questions.isEmpty else { return nil }
        var result: [[Int]] = []
        for question in questions {
            let picked = (answers[question.question] ?? (questions.count == 1 && answers.count == 1 ? answers.values.first : nil))?
                .filter { !$0.isEmpty } ?? []
            guard !picked.isEmpty else { return nil }
            let labels = question.options.map(\.label)
            var numbers: [Int] = []
            for label in picked {
                guard let index = labels.firstIndex(of: label), index < 9 else { return nil }
                numbers.append(index + 1)
            }
            numbers = Array(Set(numbers)).sorted()
            guard question.multiSelect || numbers.count == 1 else { return nil }
            result.append(numbers)
        }
        return result
    }

    /// The name a model alias is shown by: Kimi names its models
    /// `<provider>/<model>`, as `kimi-code/k3`.
    static func modelName(_ alias: String) -> String {
        let name = alias.split(separator: "/").last.map(String.init) ?? alias
        return name.isEmpty ? alias : name
    }

    // MARK: Log

    /// A session's folder, from Kimi's session index.
    static func sessionFolder(_ session: String, home: URL,
                              environment: [String: String] = ProcessInfo.processInfo.environment) -> String? {
        let index = folder(home: home, environment: environment).appending(path: "session_index.jsonl").path
        guard let data = tail(index, limit: 4_194_304) else { return nil }
        let needle = Data(session.utf8)
        for line in data.split(separator: 0x0A).reversed() where line.range(of: needle) != nil {
            guard let json = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any],
                  json["sessionId"] as? String == session, let folder = json["sessionDir"] as? String else { continue }
            return folder
        }
        return nil
    }

    static func wire(sessionFolder: String) -> String {
        (sessionFolder as NSString).appendingPathComponent("agents/main/wire.jsonl")
    }

    /// What the last turn of the main agent's log ended on: the model that
    /// replied and, read only when asked, the text of its last answer.
    static func lastTurn(wire path: String, text wanted: Bool) -> (model: String?, text: String?) {
        guard let data = tail(path, limit: 1_048_576) else { return (nil, nil) }
        var model: String?
        var turn: String?
        var step: String?
        var parts: [String] = []
        for line in data.split(separator: 0x0A).reversed() {
            guard let json = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any],
                  let type = json["type"] as? String else { continue }
            if type == "turn.ended", turn == nil {
                if let reply = json["replyModel"] as? [String: Any] { model = reply["model"] as? String }
                turn = (json["turnId"] as? NSNumber)?.stringValue ?? json["turnId"] as? String
                if !wanted { break }
                continue
            }
            if type == "turn.prompt", turn != nil { break }
            guard wanted, let turn, type == "context.append_loop_event", let event = json["event"] as? [String: Any],
                  event["type"] as? String == "content.part", (event["turnId"] as? String) == turn,
                  let part = event["part"] as? [String: Any], part["type"] as? String == "text",
                  let text = part["text"] as? String else { continue }
            // Only the step the turn ended on: earlier ones led to its tools.
            let own = event["stepUuid"] as? String
            if step == nil { step = own }
            guard own == step else { break }
            parts.insert(text, at: 0)
        }
        let text = parts.joined()
        return (model, wanted && !text.isEmpty ? AgentHookMessage.preview(text) : nil)
    }

    /// The end of a file, at most `limit` bytes.
    static func tail(_ path: String, limit: UInt64) -> Data? {
        guard let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: size > limit ? size - limit : 0)
        return try? handle.readToEnd()
    }
}

/// Kimi's local server (KAP, `kimi web` and the Kimi Code app): where it
/// listens, and the answers and replies it takes for the sessions it holds.
enum AgentKimiServer {
    struct Instance: Equatable {
        let pid: Int32
        let host: String
        let port: Int
        let product: String

        var origin: String {
            let name = host == "0.0.0.0" || host.isEmpty ? "127.0.0.1" : host == "::" || host == "::1" ? "[::1]" : host
            return "http://\(name):\(port)"
        }
    }

    /// The servers Kimi says are running.
    static func instances(folder: URL) -> [Instance] {
        let directory = folder.appending(path: "server/instances")
        let names = (try? FileManager.default.contentsOfDirectory(atPath: directory.path)) ?? []
        return names.filter { $0.hasSuffix(".json") }.compactMap { name in
            guard let data = FileManager.default.contents(atPath: directory.appending(path: name).path) else { return nil }
            return instance(data)
        }
    }

    static func instance(_ data: Data) -> Instance? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let pid = (json["pid"] as? NSNumber)?.int32Value, pid > 0,
              let port = (json["port"] as? NSNumber)?.intValue, port > 0 else { return nil }
        return Instance(pid: pid, host: json["host"] as? String ?? "127.0.0.1", port: port,
                        product: json["product"] as? String ?? "")
    }

    /// The server that runs as `pid`, which then holds the sessions whose hooks it runs.
    static func serving(_ pid: Int32, folder: URL, alive: (Int32) -> Bool = AgentHookSessions.isAlive) -> Instance? {
        guard pid > 0 else { return nil }
        return instances(folder: folder).first { $0.pid == pid && alive(pid) }
    }

    /// The server that holds a session: the one its hooks' process is, or
    /// for the Kimi Code app (whose hooks may run in a helper) the app's.
    static func server(for pid: Int32, client: String?, folder: URL) -> Instance? {
        if let own = serving(pid, folder: folder) { return own }
        guard client == AgentKimi.kapClient else { return nil }
        return instances(folder: folder).filter { $0.product == appProduct && AgentHookSessions.isAlive($0.pid) }.first
    }

    /// The Kimi Code app's server, as its instance file names it.
    static let appProduct = "kimi-code-desktop"

    /// A session's own page in `kimi web`, signed in by the server's token.
    static func page(_ server: Instance, session: String, token: String?) -> URL? {
        let base = server.origin + "/sessions/" + escaped(session)
        return URL(string: token.map { base + "#token=" + ($0.addingPercentEncoding(withAllowedCharacters: .urlFragmentAllowed) ?? $0) } ?? base)
    }

    static func token(folder: URL) -> String? {
        guard let data = FileManager.default.contents(atPath: folder.appending(path: "server.token").path),
              let text = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
              !text.isEmpty else { return nil }
        return text
    }

    // MARK: Bodies

    /// The answer to an approval or a plan, nil for one it doesn't take.
    static func approval(_ decision: ClaudeHookMessage.Decision, plan: Bool) -> [String: Any]? {
        switch decision {
        case .allow, .approvePlan: return ["kind": "approval", "decision": "approved"]
        case .allowAlways: return plan ? ["kind": "approval", "decision": "approved"]
            : ["kind": "approval", "decision": "approved", "scope": "session"]
        case .deny:
            return ["kind": "approval", "decision": "rejected",
                    "feedback": plan ? ClaudeHookMessage.planDenyMessage : ClaudeHookMessage.denyMessage]
        case .answer: return nil
        }
    }

    /// The answers to a question the server lists (`request.questions`),
    /// matched to them by text and choice label. A skipped question is
    /// dismissed; nil when an answer doesn't match a choice.
    static func answers(_ decision: ClaudeHookMessage.Decision, interaction: [String: Any],
                        questions: [ClaudeQuestion]) -> [String: Any]? {
        switch decision {
        case .deny: return ["kind": "question", "dismissed": true]
        case .answer(let answers):
            guard let listed = (interaction["request"] as? [String: Any])?["questions"] as? [[String: Any]],
                  !listed.isEmpty else { return nil }
            var out: [String: Any] = [:]
            for (index, item) in listed.enumerated() {
                guard let id = item["id"] as? String, let text = item["question"] as? String else { return nil }
                let picked = (answers[text] ?? (listed.count == 1 && answers.count == 1 ? answers.values.first : nil))?
                    .filter { !$0.isEmpty } ?? []
                guard !picked.isEmpty else { return nil }
                let options = item["options"] as? [[String: Any]] ?? []
                var ids: [String] = []
                var other: [String] = []
                for label in picked {
                    if let option = options.first(where: { $0["label"] as? String == label }), let id = option["id"] as? String {
                        ids.append(id)
                    } else {
                        other.append(label)
                    }
                }
                let multi = item["multi_select"] as? Bool ?? questions[safe: index]?.multiSelect ?? false
                if !other.isEmpty, item["allow_other"] as? Bool == false { return nil }
                if multi {
                    out[id] = other.isEmpty ? ["kind": "multi", "option_ids": ids]
                        : ["kind": "multi_with_other", "option_ids": ids, "other_text": other.joined(separator: ", ")]
                } else if let first = ids.first, other.isEmpty, ids.count == 1 {
                    out[id] = ["kind": "single", "option_id": first]
                } else if ids.isEmpty, other.count == 1 {
                    out[id] = ["kind": "other", "text": other[0]]
                } else {
                    return nil
                }
            }
            return ["kind": "question", "answers": out]
        default: return nil
        }
    }

    static func prompt(_ text: String) -> [String: Any] {
        ["prompt_id": "openbar-\(UUID().uuidString.lowercased())", "content": [["type": "text", "text": text]]]
    }

    // MARK: Calls

    /// Answers a request the server holds: an approval by its id, a
    /// question by its tool call's.
    static func answer(_ decision: ClaudeHookMessage.Decision, asked: ClaudePermission, session: String,
                       server: Instance, token: String) -> Bool {
        let base = "/api/v1/sessions/\(escaped(session))"
        if asked.asksQuestions {
            guard let listed = call(server, token: token, "GET", base + "/interactions", body: nil),
                  let items = (listed["data"] as? [String: Any])?["items"] as? [[String: Any]],
                  let item = items.first(where: { $0["tool_call_id"] as? String == asked.requestID && $0["kind"] as? String == "question" }),
                  let id = item["interaction_id"] as? String,
                  let body = answers(decision, interaction: item, questions: asked.questions) else { return false }
            return ok(call(server, token: token, "POST", base + "/interactions/\(escaped(id))", body: body))
        }
        guard let body = approval(decision, plan: asked.tool == ClaudeToolSummary.planTool) else { return false }
        return ok(call(server, token: token, "POST", base + "/interactions/\(escaped(asked.requestID))", body: body))
    }

    /// Sends a reply as a new prompt: queued after the turn under way.
    static func send(_ text: String, session: String, server: Instance, token: String) -> Bool {
        ok(call(server, token: token, "POST", "/api/v1/sessions/\(escaped(session))/prompts", body: prompt(text)))
    }

    private static func ok(_ json: [String: Any]?) -> Bool { (json?["code"] as? NSNumber)?.intValue == 0 }

    private static func escaped(_ part: String) -> String {
        part.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(CharacterSet(charactersIn: "/:"))) ?? part
    }

    /// One request to a loopback server, waited on for at most a few seconds.
    private static func call(_ server: Instance, token: String, _ method: String, _ path: String,
                             body: [String: Any]?, timeout: TimeInterval = 4) -> [String: Any]? {
        guard let url = URL(string: server.origin + path), let host = url.host,
              ["127.0.0.1", "localhost", "::1", "[::1]"].contains(host) else { return nil }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = method
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try? JSONSerialization.data(withJSONObject: body)
        }
        let done = DispatchSemaphore(value: 0)
        var result: [String: Any]?
        let configuration = URLSessionConfiguration.ephemeral
        configuration.connectionProxyDictionary = [:]
        let session = URLSession(configuration: configuration)
        session.dataTask(with: request) { data, _, _ in
            result = data.flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
            done.signal()
        }.resume()
        _ = done.wait(timeout: .now() + timeout + 1)
        session.invalidateAndCancel()
        return result
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? { indices.contains(index) ? self[index] : nil }
}
