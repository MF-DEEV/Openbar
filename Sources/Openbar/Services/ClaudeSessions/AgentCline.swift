// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Foundation
import SQLite3

/// Cline (#241): its VS Code extension (and the editors built on VS Code)
/// and its CLI (`cline`) keep every session in one store,
/// `~/.cline/data/db/sessions.db`, with each conversation beside it in
/// `sessions/<id>/<id>.messages.json`. The store says what hooks would:
/// `running` while a turn works, `pending` while a tool waits for the
/// person's approval, `idle` once the turn ended, and `ended_at` once the
/// session is closed. The conversation is written as it goes, a tool call
/// before its result. So the island reads both and puts nothing into Cline:
/// a hook would show as a step of its own in the extension's chat.
///
/// The extension asks in its own panel, and nothing outside it can answer.
/// The CLI's terminal UI takes `y` for Approve and `n` for Deny, and a
/// question's number. A CLI session that runs in Cline's hub (its local
/// daemon) can be answered and sent replies through the hub too.
enum AgentCline {
    /// The tool Cline asks a question with: one question, two to five choices.
    static let questionTool = "ask_question"

    /// Where a session is answered: the extension's panel, the CLI's
    /// terminal, or the CLI's terminal and Cline's hub.
    static let extensionClient = "vscode"
    static let cliClient = "cli"
    static let hubClient = "hub"

    /// The editor an extension session runs in, by its short name.
    static func editorName(_ host: ClaudeSessionHost) -> String {
        let bundle: String?
        switch host {
        case .ide(let id, _): bundle = id
        case .app(let id): bundle = id
        default: bundle = nil
        }
        if let ide = ClaudeIDE.find(bundle) {
            return ide.name.replacingOccurrences(of: "Visual Studio Code", with: "VS Code")
        }
        if let bundle, let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) {
            return FileManager.default.displayName(atPath: app.path).replacingOccurrences(of: ".app", with: "")
        }
        return "VS Code"
    }

    // MARK: Places

    /// Cline's data folder: `CLINE_DATA_DIR`, else `data` in `CLINE_DIR`,
    /// else `~/.cline/data`.
    static func dataFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let data = nonEmpty(environment["CLINE_DATA_DIR"]) { return URL(fileURLWithPath: data) }
        if let dir = nonEmpty(environment["CLINE_DIR"]) { return URL(fileURLWithPath: dir).appending(path: "data") }
        return home.appending(path: ".cline/data")
    }

    /// The folder `cline` was used from: `CLINE_DIR`, or `~/.cline`.
    static func folder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        nonEmpty(environment["CLINE_DIR"]).map { URL(fileURLWithPath: $0) } ?? home.appending(path: ".cline")
    }

    static func database(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let db = nonEmpty(environment["CLINE_DB_DATA_DIR"]) { return URL(fileURLWithPath: db).appending(path: "sessions.db") }
        return dataFolder(home: home, environment: environment).appending(path: "db/sessions.db")
    }

    private static func nonEmpty(_ text: String?) -> String? { text.flatMap { $0.isEmpty ? nil : $0 } }

    /// Cline's hub, when one runs.
    static func hub(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> AgentClineHub.Discovery? {
        AgentClineHub.discovery(data: dataFolder(home: home, environment: environment))
    }

    // MARK: Store

    /// A session as Cline's store has it.
    struct Record: Equatable {
        let id: String
        /// `vscode` for the extension, `cli` for the CLI.
        let source: String
        /// The process that runs it: the editor's extension host, the CLI, or
        /// Cline's hub when the CLI runs its sessions there.
        let pid: Int32
        let status: String
        let cwd: String
        let model: String?
        let title: String?
        let messages: String?
        let started: Date?

        /// A turn works, or waits on the person in it.
        var isBusy: Bool { status == "running" || status == "pending" }
        var waitsForApproval: Bool { status == "pending" }
        var failed: Bool { status == "failed" }
        var isExtension: Bool { source == "vscode" }
    }

    /// The sessions still open: no end written, and not a subagent's.
    static func records(database: URL) -> [Record] {
        let sql = """
            SELECT session_id, source, pid, status, cwd, model, metadata_json, messages_path, started_at
            FROM sessions WHERE ended_at IS NULL AND is_subagent = 0 ORDER BY started_at
            """
        return rows(database, sql) { stmt in
            guard let id = text(stmt, 0), let source = text(stmt, 1) else { return nil }
            let metadata = text(stmt, 6).flatMap { $0.data(using: .utf8) }
                .flatMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
            return Record(id: id, source: source, pid: Int32(truncatingIfNeeded: sqlite3_column_int64(stmt, 2)),
                          status: text(stmt, 3) ?? "", cwd: text(stmt, 4) ?? "", model: text(stmt, 5),
                          title: (metadata?["title"] as? String).flatMap { $0.isEmpty ? nil : $0 },
                          messages: text(stmt, 7), started: text(stmt, 8).flatMap { AgentHookMessage.timestamp($0) })
        }
    }

    private static func rows<T>(_ database: URL, _ sql: String, _ row: (OpaquePointer) -> T?) -> [T] {
        let path = database.path
        guard FileManager.default.fileExists(atPath: path) else { return [] }
        var db: OpaquePointer?
        defer { sqlite3_close(db) }
        guard sqlite3_open_v2(path, &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, nil) == SQLITE_OK, let db else { return [] }
        sqlite3_busy_timeout(db, 300)
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK, let stmt else { return [] }
        defer { sqlite3_finalize(stmt) }
        var out: [T] = []
        while sqlite3_step(stmt) == SQLITE_ROW {
            if let value = row(stmt) { out.append(value) }
        }
        return out
    }

    private static func text(_ stmt: OpaquePointer, _ column: Int32) -> String? {
        guard let raw = sqlite3_column_text(stmt, column) else { return nil }
        let value = String(cString: raw)
        return value.isEmpty ? nil : value
    }

    // MARK: Conversation

    /// A tool call in the conversation.
    struct Call: Equatable {
        let id: String
        let name: String
        let input: [String: Any]
        let at: Date?

        static func == (a: Call, b: Call) -> Bool {
            a.id == b.id && a.name == b.name && NSDictionary(dictionary: a.input).isEqual(to: b.input)
        }
    }

    /// What the conversation says: its calls in order, those that have a
    /// result, the last thing the person and the agent wrote, and the model
    /// that answered last.
    struct Transcript: Equatable {
        var calls: [Call] = []
        var finished: Set<String> = []
        var failed: Set<String> = []
        var lastUser: String?
        var lastText: String?
        var model: String?
        /// How many messages the person sent.
        var prompts = 0

        /// The last call still without a result.
        var open: Call? { calls.last { !finished.contains($0.id) } }
    }

    /// The conversation file read whole; nil when it is too big or not Cline's.
    static func transcript(path: String, limit: Int = 16 << 20) -> Transcript? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: path),
              ((attributes[.size] as? NSNumber)?.intValue ?? 0) <= limit,
              let data = FileManager.default.contents(atPath: path) else { return nil }
        return transcript(data: data)
    }

    static func transcript(data: Data) -> Transcript? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let messages = json["messages"] as? [[String: Any]] else { return nil }
        var out = Transcript()
        for message in messages {
            let role = message["role"] as? String
            let at = AgentHookMessage.timestamp(message["ts"])
            let parts: [[String: Any]]
            if let list = message["content"] as? [[String: Any]] { parts = list }
            else if let text = message["content"] as? String { parts = [["type": "text", "text": text]] }
            else { parts = [] }
            if role == "assistant" {
                if let model = (message["modelInfo"] as? [String: Any])?["id"] as? String, !model.isEmpty { out.model = model }
                let texts = parts.filter { $0["type"] as? String == "text" }.compactMap { $0["text"] as? String }
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
                if !texts.isEmpty { out.lastText = texts.joined(separator: "\n") }
                for part in parts where part["type"] as? String == "tool_use" {
                    guard let id = part["id"] as? String, let name = part["name"] as? String else { continue }
                    out.calls.append(Call(id: id, name: name, input: AgentHookMessage.arguments(part["input"]), at: at))
                }
            } else if role == "user" {
                for part in parts {
                    switch part["type"] as? String {
                    case "tool_result":
                        guard let id = part["tool_use_id"] as? String else { continue }
                        out.finished.insert(id)
                        if part["is_error"] as? Bool == true { out.failed.insert(id) }
                    case "text":
                        if let typed = typed(part["text"] as? String) {
                            out.lastUser = typed
                            out.prompts += 1
                        }
                    default: break
                    }
                }
            }
        }
        return out
    }

    /// What the person typed, from Cline's `<user_input>` wrapper; nil for a
    /// note Cline adds itself (`<hook_context>`, reminders).
    static func typed(_ text: String?) -> String? {
        guard var text = text?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return nil }
        if let open = text.range(of: "<user_input"), let close = text.range(of: "</user_input>", range: open.upperBound..<text.endIndex),
           let end = text.range(of: ">", range: open.upperBound..<close.lowerBound) {
            text = String(text[end.upperBound..<close.lowerBound]).trimmingCharacters(in: .whitespacesAndNewlines)
            return text.isEmpty ? nil : text
        }
        return text.hasPrefix("<") ? nil : text
    }

    // MARK: Tools

    /// A call under the name Claude Code gives the same tool, with Claude
    /// Code's arguments, so it reads and sums up alike.
    static func claudeTool(_ call: Call) -> (name: String, input: [String: Any]) {
        let input = call.input
        switch call.name {
        case "run_commands":
            return ("Bash", ["command": commands(input).joined(separator: "\n")])
        case "editor":
            let path = input["path"] as? String ?? ""
            let new = input["new_text"] as? String ?? ""
            if let old = input["old_text"] as? String, !old.isEmpty {
                return ("Edit", ["file_path": path, "old_string": old, "new_string": new])
            }
            if input["insert_line"] is NSNumber { return ("Edit", ["file_path": path, "new_string": new]) }
            return ("Write", ["file_path": path, "content": new])
        case "apply_patch":
            let patch = input["input"] as? String ?? (call.input.isEmpty ? "" : "\(call.input)")
            return ("Edit", ["file_path": patchFile(patch) ?? "", "patch": patch])
        case "read_files":
            return ("Read", ["file_path": files(input).first ?? ""])
        case "search_codebase":
            let queries = input["queries"] as? [String] ?? (input["queries"] as? String).map { [$0] } ?? []
            return ("Grep", ["pattern": queries.joined(separator: " | ")])
        case "fetch_web_content":
            let first = (input["requests"] as? [[String: Any]])?.first
            return ("WebFetch", ["url": first?["url"] as? String ?? "", "prompt": first?["prompt"] as? String ?? ""])
        case "skills":
            return ("Skill", ["skill": input["skill"] as? String ?? ""])
        case questionTool:
            return (ClaudeQuestion.tool, input)
        default:
            return (AgentHookTools.name(.cline, call.name), input)
        }
    }

    /// The commands of a `run_commands` call, whichever form it took.
    static func commands(_ input: [String: Any]) -> [String] {
        func line(_ value: Any) -> String? {
            if let text = value as? String { return text }
            guard let entry = value as? [String: Any], let command = entry["command"] as? String else { return nil }
            let args = (entry["args"] as? [String]) ?? []
            return ([command] + args).joined(separator: " ")
        }
        for key in ["commands", "command", "cmd"] {
            if let list = input[key] as? [Any] { return list.compactMap(line) }
            if let one = input[key], let text = line(one) { return [text] }
        }
        return []
    }

    private static func files(_ input: [String: Any]) -> [String] {
        for key in ["files", "file_paths", "paths"] {
            if let text = input[key] as? String { return [text] }
            for item in input[key] as? [Any] ?? [] {
                if let text = item as? String { return [text] }
                if let path = (item as? [String: Any])?["path"] as? String { return [path] }
            }
            if let path = (input[key] as? [String: Any])?["path"] as? String { return [path] }
        }
        return []
    }

    /// The first file an apply_patch payload changes.
    static func patchFile(_ patch: String) -> String? {
        for line in patch.split(whereSeparator: \.isNewline) {
            for head in ["*** Update File:", "*** Add File:", "*** Delete File:"] where line.hasPrefix(head) {
                let path = line.dropFirst(head.count).trimmingCharacters(in: .whitespaces)
                if !path.isEmpty { return path }
            }
        }
        return nil
    }

    /// The call as the timeline shows it.
    static func use(_ call: Call, at date: Date, policy: ClaudeContentPolicy) -> ClaudeToolUse {
        let tool = claudeTool(call)
        var use = AgentHookMessage.toolUse(tool.name, input: tool.input, at: call.at.map { min($0, date) } ?? date, policy: policy)
        use = ClaudeToolUse(id: call.id, name: use.name, summary: use.summary, started: use.started, inputKey: use.inputKey)
        return use
    }

    // MARK: Asks

    /// What Cline asks about the call: the command or change it waits to
    /// run, or the question it asks. Shown whatever the previews, since the
    /// person decides on it. The call's id is the request's.
    static func ask(_ call: Call, cwd: String?, at date: Date) -> ClaudePermission? {
        if call.name == questionTool {
            guard let text = (call.input["question"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !text.isEmpty else { return nil }
            let options = (call.input["options"] as? [Any] ?? []).compactMap { $0 as? String }.filter { !$0.isEmpty }
            let question = ClaudeQuestion(question: text, options: options.map { ClaudeQuestion.Option(label: $0) })
            var permission = ClaudePermission(requestID: call.id, tool: ClaudeQuestion.tool,
                                              summary: ClaudeToolSummary.clip(text), offersAlways: false, since: date)
            permission.questions = [question]
            return permission
        }
        let tool = claudeTool(call)
        let asked = AgentHookMessage.grokAsk(tool.name, input: tool.input, cwd: cwd, at: date)
        return asked.renewed(requestID: call.id, since: date)
    }

    // MARK: Keys

    /// The keys that answer the CLI's dialog, nil for an answer it can't be
    /// given by keys. Its approval takes `y` for Approve and `n` for Deny;
    /// a question takes its choice's number, or arrows down to "Type a
    /// response..." for an answer of the person's own. `oneBurst` is a
    /// terminal that adds a return to what it sends (Terminal's `do
    /// script`): there nothing ends on Escape, which the return would join.
    static func keys(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission,
                     oneBurst: Bool = false) -> [ClaudeTypingStep]? {
        if let question = asked.questions.first {
            switch decision {
            // Escape leaves the question with an empty answer.
            case .deny: return oneBurst ? nil : [.keys([.escape])]
            case .answer(let answers):
                let given = (answers[question.question] ?? (answers.count == 1 ? answers.values.first : nil))?
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty } ?? []
                guard given.count == 1, let answer = given.first else { return nil }
                if let index = question.options.firstIndex(where: { $0.label == answer }), index < 9 {
                    return [.keys([.digit(index + 1)])]
                }
                // The person's own words go on the last row, under the choices.
                return [.keys(Array(repeating: .down, count: question.options.count)), .wait(0.2),
                        .paste(answer), .wait(0.2), .enter]
            default: return nil
            }
        }
        switch decision {
        // `y` rather than Return: a notice Cline shows over its dialog on a
        // first run opens a web page on Return, but closes on any other key.
        case .allow: return [.keys([.letter("y")])]
        case .deny: return [.keys([.letter("n")])]
        default: return nil
        }
    }
}

// MARK: Watching the store

/// Turns what Cline's store says into the events its hooks would have told,
/// for the island's list of hook agents. Not thread-safe: the service uses
/// it on its queue.
final class AgentClineWatcher {
    /// What was last told of a session.
    private struct Seen {
        var pid: Int32
        var turnOpen = false
        var prompts = 0
        var told: Set<String> = []
        var finished: Set<String> = []
        var asked: String?
        var model: String?
        var title: String?
        var client: String?
    }

    private var seen: [String: Seen] = [:]
    /// Each conversation's last reading, kept until its file changes.
    private var read: [String: (stamp: [AnyHashable], transcript: AgentCline.Transcript)] = [:]

    var sessions: Set<String> { Set(seen.keys) }

    /// The events since the last scan. `pid` gives the process that shows a
    /// session (nil while none does: a hub session whose CLI quit), `client`
    /// where it is answered (`vscode`, `cli` or `hub`).
    func scan(_ records: [AgentCline.Record], now: Date, policy: ClaudeContentPolicy,
              pid: (AgentCline.Record) -> Int32?, client: (AgentCline.Record) -> String,
              alive: (Int32) -> Bool = AgentHookSessions.isAlive,
              transcript: (String) -> AgentCline.Transcript? = { AgentCline.transcript(path: $0) }) -> [AgentHookEvent] {
        var events: [AgentHookEvent] = []
        var live: Set<String> = []
        for record in records {
            guard let shown = pid(record), shown > 0, alive(shown) else { continue }
            live.insert(record.id)
            var state = seen[record.id] ?? Seen(pid: shown)
            let fresh = seen[record.id] == nil
            state.pid = shown
            func add(_ kind: AgentHookEvent.Kind) {
                events.append(AgentHookEvent(provider: .cline, sessionID: record.id, pid: shown,
                                             cwd: record.cwd.isEmpty ? nil : record.cwd, date: now, kind: kind))
            }
            let talk = record.messages.flatMap { reading($0, transcript: transcript) } ?? AgentCline.Transcript()
            if fresh {
                add(.started)
                // What ended before the island looked isn't told again; a call still open is.
                state.told = Set(talk.calls.map(\.id)).intersection(talk.finished)
                state.finished = talk.finished
                state.prompts = talk.prompts
            }
            let place = client(record)
            if state.client != place { state.client = place; add(.client(place)) }
            if let model = talk.model ?? record.model, state.model != model { state.model = model; add(.model(model)) }
            if policy.previews, let title = record.title, state.title != title { state.title = title; add(.title(title)) }
            // A turn opens while the store says it works, or when the person
            // wrote again and the turn already ended between two readings.
            let wroteAgain = talk.prompts > state.prompts
            state.prompts = max(state.prompts, talk.prompts)
            if !state.turnOpen, record.isBusy || wroteAgain {
                state.turnOpen = true
                add(.prompt(policy.previews ? talk.lastUser.flatMap { AgentHookMessage.preview($0) } : nil))
            }
            for call in talk.calls where !state.told.contains(call.id) {
                state.told.insert(call.id)
                add(.toolStarted(AgentCline.use(call, at: now, policy: policy)))
            }
            for call in talk.calls where talk.finished.contains(call.id) && !state.finished.contains(call.id) {
                state.finished.insert(call.id)
                let tool = AgentCline.claudeTool(call)
                add(.toolFinished(id: call.id, name: tool.name, inputKey: ClaudeToolSummary.inputKey(name: tool.name, input: tool.input),
                                  failed: talk.failed.contains(call.id), started: call.at))
            }
            // A call waits on the person: Cline marks the session pending for
            // an approval; a question is a call that runs until answered.
            var waiting: AgentCline.Call?
            if record.isBusy, let open = talk.open {
                if open.name == AgentCline.questionTool || record.waitsForApproval { waiting = open }
            }
            if let waiting, state.asked != waiting.id, let asked = AgentCline.ask(waiting, cwd: record.cwd, at: now) {
                if let before = state.asked { add(.permissionSettled(before)) }
                state.asked = waiting.id
                add(.permission(asked))
            } else if waiting == nil, let before = state.asked {
                state.asked = nil
                add(.permissionSettled(before))
            }
            if !record.isBusy, state.turnOpen {
                state.turnOpen = false
                add(.stopped(lastText: policy.previews ? talk.lastText.flatMap { AgentHookMessage.preview($0) } : nil,
                             failed: record.failed))
            }
            seen[record.id] = state
        }
        for (id, state) in seen where !live.contains(id) {
            events.append(AgentHookEvent(provider: .cline, sessionID: id, pid: state.pid, cwd: nil, date: now, kind: .ended))
            seen[id] = nil
        }
        read = read.filter { path, _ in records.contains { $0.messages == path && live.contains($0.id) } }
        return events
    }

    /// Forgets the titles told, so they are told again once previews are back on.
    func forgetContent() {
        for id in seen.keys { seen[id]?.title = nil }
    }

    private func reading(_ path: String, transcript: (String) -> AgentCline.Transcript?) -> AgentCline.Transcript? {
        let attributes = try? FileManager.default.attributesOfItem(atPath: path)
        let stamp: [AnyHashable] = [(attributes?[.modificationDate] as? Date) ?? .distantPast,
                                    (attributes?[.size] as? NSNumber)?.intValue ?? -1]
        if let known = read[path], known.stamp == stamp, attributes != nil { return known.transcript }
        guard let fresh = transcript(path) else { return read[path]?.transcript }
        read[path] = (stamp, fresh)
        return fresh
    }
}

// MARK: Cline's hub

/// Cline's hub: the local daemon the CLI runs its sessions in once it is up
/// (`cline --cline-hub-daemon`). It takes a WebSocket on 127.0.0.1 whose
/// token sits in Cline's data folder, readable only by the person. Through
/// it the island learns which CLI shows a session, answers an approval
/// (the first answer wins, the CLI's own or the island's) and sends a reply.
/// A question is the CLI's own and can't be answered here.
enum AgentClineHub {
    struct Discovery: Equatable {
        let url: URL
        let token: String
        let pid: Int32
    }

    /// The hub that runs now, from the file it writes when it starts.
    static func discovery(data: URL, alive: (Int32) -> Bool = AgentHookSessions.isAlive) -> Discovery? {
        let hub = data.appending(path: "locks/hub")
        let owners = hub.appending(path: "owners")
        let manager = FileManager.default
        let files = ((try? manager.contentsOfDirectory(at: hub, includingPropertiesForKeys: nil)) ?? [])
            + ((try? manager.contentsOfDirectory(at: owners, includingPropertiesForKeys: nil)) ?? [])
        for file in files where file.pathExtension == "json" {
            guard let data = try? Data(contentsOf: file), let found = discovery(data), alive(found.pid) else { continue }
            return found
        }
        return nil
    }

    static func discovery(_ data: Data) -> Discovery? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let text = json["url"] as? String, let url = URL(string: text), url.scheme == "ws",
              let host = url.host, ["127.0.0.1", "localhost", "::1"].contains(host),
              let token = json["authToken"] as? String, !token.isEmpty,
              let pid = (json["pid"] as? NSNumber)?.int32Value, pid > 0 else { return nil }
        return Discovery(url: url, token: token, pid: pid)
    }

    /// Each hub session's CLI: the process of the client that took part in it
    /// last. A session with no client left was quit.
    static func clients(sessions: [[String: Any]], clients: [[String: Any]]) -> [String: Int32] {
        var pids: [String: Int32] = [:]
        for client in clients {
            guard let id = client["clientId"] as? String,
                  let pid = ((client["metadata"] as? [String: Any])?["pid"] as? NSNumber)?.int32Value, pid > 0 else { continue }
            pids[id] = pid
        }
        var out: [String: Int32] = [:]
        for session in sessions {
            guard let id = session["sessionId"] as? String else { continue }
            let participants = (session["participants"] as? [[String: Any]] ?? []).compactMap { $0["clientId"] as? String }
            if let pid = participants.reversed().lazy.compactMap({ pids[$0] }).first { out[id] = pid }
        }
        return out
    }

    /// The CLI showing each hub session, asked of the hub. Blocks a moment.
    static func shown(_ hub: Discovery, timeout: TimeInterval = 2) -> [String: Int32]? {
        guard let connection = Connection(hub, timeout: timeout) else { return nil }
        defer { connection.close() }
        guard let listed = connection.command("session.list", [:]),
              let known = connection.command("client.list", [:]) else { return nil }
        return clients(sessions: listed["sessions"] as? [[String: Any]] ?? [], clients: known["clients"] as? [[String: Any]] ?? [])
    }

    /// Answers the approval the session waits on for `call`. Blocks a few
    /// seconds at most: the hub sends a waiting approval again to a client
    /// that subscribes to its session.
    static func approve(_ approved: Bool, session: String, call: String, hub: Discovery, timeout: TimeInterval = 5) -> Bool {
        guard let connection = Connection(hub, timeout: timeout) else { return false }
        defer { connection.close() }
        guard connection.subscribe(session) else { return false }
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            guard let frame = connection.receive(until: deadline) else { return false }
            guard frame["kind"] as? String == "event", let envelope = frame["envelope"] as? [String: Any],
                  envelope["event"] as? String == "approval.requested",
                  let payload = envelope["payload"] as? [String: Any],
                  payload["sessionId"] as? String == session, payload["toolCallId"] as? String == call,
                  let approval = payload["approvalId"] as? String else { continue }
            var answer: [String: Any] = ["approvalId": approval, "approved": approved]
            if !approved { answer["reason"] = "Denied in Openbar" }
            return connection.command("approval.respond", answer) != nil
        }
        return false
    }

    /// Sends the person's reply to the session, queued after the turn under way.
    static func send(_ text: String, session: String, hub: Discovery, timeout: TimeInterval = 5) -> Bool {
        guard let connection = Connection(hub, timeout: timeout) else { return false }
        defer { connection.close() }
        return connection.command("session.send_input", ["prompt": text, "delivery": "queue"], session: session) != nil
    }

    /// One client's conversation with the hub.
    final class Connection {
        private let task: URLSessionWebSocketTask
        private let urlSession: URLSession
        private let timeout: TimeInterval
        private let client = "openbar-\(UUID().uuidString.prefix(8).lowercased())"
        private var counter = 0

        init?(_ hub: Discovery, timeout: TimeInterval) {
            let configuration = URLSessionConfiguration.ephemeral
            configuration.timeoutIntervalForRequest = timeout
            configuration.connectionProxyDictionary = [:]
            urlSession = URLSession(configuration: configuration)
            task = urlSession.webSocketTask(with: hub.url, protocols: ["cline-hub-auth.\(hub.token)"])
            self.timeout = timeout
            task.resume()
            let registration: [String: Any] = ["clientId": client, "clientType": "openbar", "displayName": "Openbar",
                                               "transport": "websocket", "capabilities": ["approval.respond"]]
            guard command("client.register", registration) != nil else { close(); return nil }
        }

        /// The reply's payload, skipping the frames between; nil on a refusal or no reply in time.
        func command(_ name: String, _ payload: [String: Any], session: String? = nil) -> [String: Any]? {
            counter += 1
            let request = "\(client)-\(counter)"
            var envelope: [String: Any] = ["version": "v1", "command": name, "requestId": request, "clientId": client,
                                           "payload": payload]
            if let session { envelope["sessionId"] = session }
            guard send(["kind": "command", "envelope": envelope]) else { return nil }
            let deadline = Date().addingTimeInterval(timeout)
            while Date() < deadline {
                guard let frame = receive(until: deadline) else { return nil }
                guard frame["kind"] as? String == "reply", let reply = frame["envelope"] as? [String: Any],
                      reply["requestId"] as? String == request else { continue }
                guard reply["ok"] as? Bool == true else { return nil }
                return reply["payload"] as? [String: Any] ?? [:]
            }
            return nil
        }

        func subscribe(_ session: String) -> Bool {
            send(["kind": "stream.subscribe", "clientId": client, "sessionId": session])
        }

        private func send(_ frame: [String: Any]) -> Bool {
            guard let data = try? JSONSerialization.data(withJSONObject: frame),
                  let text = String(data: data, encoding: .utf8) else { return false }
            let sent = DispatchSemaphore(value: 0)
            var failed = false
            task.send(.string(text)) { error in
                failed = error != nil
                sent.signal()
            }
            return sent.wait(timeout: .now() + timeout) == .success && !failed
        }

        func receive(until deadline: Date) -> [String: Any]? {
            let got = DispatchSemaphore(value: 0)
            var data: Data?
            task.receive { result in
                if case .success(let message) = result {
                    switch message {
                    case .string(let text): data = Data(text.utf8)
                    case .data(let bytes): data = bytes
                    @unknown default: break
                    }
                }
                got.signal()
            }
            let left = max(0.1, deadline.timeIntervalSinceNow)
            guard got.wait(timeout: .now() + left) == .success, let data else {
                task.cancel(with: .goingAway, reason: nil)
                return nil
            }
            return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        }

        func close() {
            task.cancel(with: .normalClosure, reason: nil)
            urlSession.invalidateAndCancel()
        }
    }
}
