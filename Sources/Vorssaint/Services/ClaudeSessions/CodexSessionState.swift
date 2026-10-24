// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// What Codex's app-server says about its threads, joined into the same
/// sessions the island shows for Claude Code (windows/docs/spec/07-codex-sessions.md
/// §3.4, §3.5, §5). Pure: messages and times go in, sessions and the answers
/// to send come out. The owner serializes every call.
final class CodexSessionState {
    /// How long an unread finished turn stays marked, as for Claude.
    static let finishedFor: TimeInterval = ClaudeSessionEngine.finishedFor
    static let timelineLength = ClaudeSessionEngine.timelineLength
    static let toolsKept = 24
    static let detailLimit = ClaudeToolSummary.detailLimit

    /// Item types that run something, so the session reads as running a tool.
    static let toolItems: Set<String> = [
        "commandExecution", "fileChange", "mcpToolCall", "dynamicToolCall",
        "webSearch", "imageGeneration", "collabAgentToolCall",
    ]

    /// The requests the island keeps; any other is left to the terminal.
    static let knownRequests: Set<String> = [
        "item/commandExecution/requestApproval", "item/fileChange/requestApproval",
        "item/permissions/requestApproval", "item/tool/requestUserInput", "mcpServer/elicitation/request",
    ]

    enum Status: Equatable {
        case notLoaded, idle, systemError
        case active(waitingOnApproval: Bool, waitingOnInput: Bool)

        init(json value: Any?) {
            let object = value as? [String: Any]
            switch object?["type"] as? String {
            case "active":
                let flags = (object?["activeFlags"] as? [Any] ?? []).compactMap { $0 as? String }
                self = .active(waitingOnApproval: flags.contains("waitingOnApproval"),
                               waitingOnInput: flags.contains("waitingOnUserInput"))
            case "idle": self = .idle
            case "systemError": self = .systemError
            default: self = .notLoaded
            }
        }

        var isActive: Bool {
            if case .active = self { return true }
            return false
        }
    }

    /// A request Codex is waiting on, as it came, with the JSON-RPC id to answer.
    struct Pending {
        let rpcID: CodexRPCID
        let method: String
        let params: [String: Any]
        let since: Date

        /// The id the island knows it by.
        var requestID: String { "codex:\(rpcID)" }

        var asks: Bool { method == "item/tool/requestUserInput" || method == "mcpServer/elicitation/request" }

        /// Only these can be answered from the island; elicitations show and wait.
        var answerable: Bool {
            [
                "item/commandExecution/requestApproval", "item/fileChange/requestApproval",
                "item/permissions/requestApproval", "item/tool/requestUserInput",
            ].contains(method)
        }
    }

    struct Thread {
        let id: String
        var cwd = ""
        var name: String?
        var preview: String?
        var model: String?
        var branch: String?
        var created: Date?
        var updated: Date?
        var status: Status?
        var subscribed = false
        var tools: [ClaudeToolUse] = []
        var tokens = AgentTokens()
        var turnStarted: Date?
        /// The last turn's end and whether it completed.
        var turnEnded: (at: Date, completed: Bool)?
        var acknowledged: Date?
        var pending: [Pending] = []
        var phase: (phase: ClaudeSessionPhase, since: Date)?
        var parent: String?
        /// Codex's own helper threads (titles and the like), never a person's session.
        var ephemeral = false

        init(id: String) { self.id = id }

        var isActive: Bool { status?.isActive == true }
    }

    private(set) var threads: [String: Thread] = [:]

    init() {}

    /// Forgets every thread, as when the daemon goes away.
    func reset() { threads.removeAll() }

    /// Sets a thread's subscription mark; the client joins and leaves threads.
    func setSubscribed(_ id: String, _ subscribed: Bool) {
        threads[id]?.subscribed = subscribed
    }

    /// Removes a pending request the island answered, returning it.
    func takePending(thread id: String, requestID: String) -> Pending? {
        guard let index = threads[id]?.pending.firstIndex(where: { $0.requestID == requestID && $0.answerable })
        else { return nil }
        return threads[id]?.pending.remove(at: index)
    }

    // MARK: Input

    /// `thread/read`'s thread, or a thread from `thread/list`.
    func upsertThread(_ thread: [String: Any]) {
        guard let id = Self.string(thread, "id") else { return }
        var entry = threads[id] ?? Thread(id: id)
        if let cwd = Self.string(thread, "cwd") { entry.cwd = cwd }
        entry.name = Self.string(thread, "name") ?? entry.name
        entry.preview = Self.string(thread, "preview") ?? entry.preview
        entry.model = Self.string(thread, "model") ?? entry.model
        entry.branch = (thread["gitInfo"] as? [String: Any]).flatMap { Self.string($0, "branch") } ?? entry.branch
        entry.created = Self.time(thread, "createdAt") ?? entry.created
        entry.updated = Self.time(thread, "updatedAt") ?? entry.updated
        entry.parent = Self.string(thread, "parentThreadId") ?? entry.parent
        if let ephemeral = Self.bool(thread["ephemeral"]) { entry.ephemeral = ephemeral }
        if let status = thread["status"] { entry.status = Status(json: status) }
        threads[id] = entry
    }

    /// A notification from the daemon. Returns a thread id worth reading
    /// (one the island has not seen), if any.
    @discardableResult
    func notify(_ method: String, params: [String: Any], now: Date) -> String? {
        let threadID = Self.string(params, "threadId")
        switch method {
        case "thread/status/changed":
            guard let id = threadID else { return nil }
            let status = Status(json: params["status"])
            let known = threads[id] != nil
            if status == .notLoaded {
                threads[id] = nil
                return nil
            }
            var thread = threads[id] ?? Thread(id: id)
            // A new turn starts now; turn/started, if it comes, has the exact time.
            if status.isActive, !thread.isActive { thread.turnStarted = now }
            thread.status = status
            threads[id] = thread
            return known ? nil : id
        case "thread/closed":
            if let id = threadID { threads[id] = nil }
        case "turn/started":
            guard let id = threadID, threads[id] != nil else { return nil }
            let turn = params["turn"] as? [String: Any] ?? params
            threads[id]?.turnStarted = Self.time(turn, "startedAt") ?? now
        case "turn/completed":
            guard let id = threadID, var thread = threads[id] else { return nil }
            let turn = params["turn"] as? [String: Any] ?? params
            let status = Self.string(turn, "status")
            let completed = status == "completed" || status == "failed"
            let at = Self.time(turn, "completedAt") ?? now
            thread.turnEnded = (at, completed)
            // A turn that ends takes its questions with it.
            thread.pending.removeAll()
            for index in thread.tools.indices where thread.tools[index].finished == nil {
                thread.tools[index].finished = at
            }
            threads[id] = thread
        case "item/started", "item/completed":
            guard let id = threadID, var thread = threads[id],
                  let item = params["item"] as? [String: Any], let kind = Self.string(item, "type"),
                  Self.toolItems.contains(kind) else { return nil }
            let itemID = Self.string(item, "id") ?? ""
            if method == "item/started" {
                let started = Self.time(params, "startedAtMs") ?? now
                thread.tools.append(ClaudeToolUse(id: itemID, name: Self.toolName(kind),
                                                  summary: Self.toolSummary(kind, item), started: started))
                if thread.tools.count > Self.toolsKept {
                    thread.tools.removeFirst(thread.tools.count - Self.timelineLength)
                }
            } else if let index = thread.tools.lastIndex(where: { $0.id == itemID }) {
                thread.tools[index].finished = max(now, thread.tools[index].started)
                let status = Self.string(item, "status")
                thread.tools[index].failed = status == "failed" || status == "declined"
            }
            threads[id] = thread
        case "thread/tokenUsage/updated":
            guard let id = threadID, threads[id] != nil,
                  let total = (params["tokenUsage"] as? [String: Any])?["total"] as? [String: Any] else { return nil }
            func count(_ key: String) -> Int { Self.integer(total[key]) ?? 0 }
            let cached = count("cachedInputTokens")
            let write = count("cacheWriteInputTokens")
            // Codex counts cache reads and writes inside its input.
            threads[id]?.tokens = AgentTokens(input: max(count("inputTokens") - cached - write, 0),
                                              cacheWrite: write, cacheRead: cached,
                                              output: count("outputTokens"), reasoning: count("reasoningOutputTokens"))
        case "serverRequest/resolved":
            guard let id = threadID, threads[id] != nil else { return nil }
            if let request = CodexRPCID(json: params["requestId"]) {
                threads[id]?.pending.removeAll { $0.rpcID == request }
            }
        default: break
        }
        return nil
    }

    /// A request from the daemon. True when the island keeps it; requests it
    /// does not understand are left to the terminal and never answered.
    @discardableResult
    func request(_ rpcID: CodexRPCID, method: String, params: [String: Any], now: Date) -> Bool {
        guard let id = Self.string(params, "threadId"), threads[id] != nil, Self.knownRequests.contains(method)
        else { return false }
        threads[id]?.pending.removeAll { $0.rpcID == rpcID }
        threads[id]?.pending.append(Pending(rpcID: rpcID, method: method, params: params, since: now))
        return true
    }

    func acknowledge(_ id: String, now: Date) {
        threads[id]?.acknowledged = now
    }

    // MARK: Output

    /// The sessions to show, and the changes worth a notice since last time.
    func snapshot(now: Date, previews: Bool) -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        var sessions: [ClaudeSession] = []
        var events: [ClaudeSessionEvent] = []
        for id in threads.keys.sorted() {
            guard var thread = threads[id] else { continue }
            // A subagent's thread belongs to its parent's row.
            if thread.parent != nil || thread.ephemeral || thread.cwd.isEmpty { continue }
            let phase = Self.phase(thread, now: now)
            let since: Date
            if let previous = thread.phase, previous.phase == phase {
                since = previous.since
            } else {
                if let previous = thread.phase,
                   let event = Self.event(thread, previous: previous.phase, previousSince: previous.since, phase: phase) {
                    events.append(event)
                }
                since = now
            }
            thread.phase = (phase, since)
            threads[id] = thread
            sessions.append(Self.session(thread, phase: phase, since: since, previews: previews))
        }
        return (sessions, events)
    }

    /// Phase rules in order (spec 07 §5.1).
    static func phase(_ thread: Thread, now: Date) -> ClaudeSessionPhase {
        if thread.pending.contains(where: { !$0.asks }) { return .waitingPermission }
        if thread.pending.contains(where: \.asks) { return .waitingInput }
        switch thread.status {
        case .active(waitingOnApproval: true, _)?: return .waitingPermission
        case .active(_, waitingOnInput: true)?: return .waitingInput
        case .active?: return thread.tools.contains { $0.finished == nil } ? .runningTool : .generating
        default:
            if let ended = thread.turnEnded, ended.completed, now.timeIntervalSince(ended.at) < finishedFor,
               (thread.acknowledged ?? .distantPast) < ended.at {
                return .finished
            }
            return .idle
        }
    }

    private static func event(_ thread: Thread, previous: ClaudeSessionPhase, previousSince: Date,
                              phase: ClaudeSessionPhase) -> ClaudeSessionEvent? {
        switch phase {
        case .waitingPermission: return .needsPermission(id: thread.id, tool: "Codex")
        case .waitingInput: return .needsInput(id: thread.id)
        case .finished where previous.isActive || previous.isWaiting:
            guard let ended = thread.turnEnded?.at, ended >= previousSince else { return nil }
            return .finished(id: thread.id, duration: thread.turnStarted.map { ended.timeIntervalSince($0) })
        default: return nil
        }
    }

    private static func session(_ thread: Thread, phase: ClaudeSessionPhase, since: Date,
                                previews: Bool) -> ClaudeSession {
        var timeline = Array(thread.tools.suffix(timelineLength))
        if !previews {
            for index in timeline.indices { timeline[index].summary = nil }
        }
        let current = timeline.last { $0.finished == nil }
        let project = AgentLogParser.projectName(thread.cwd)
        let lastActive = [thread.updated, thread.turnEnded?.at].compactMap { $0 }.max()
        return ClaudeSession(
            id: thread.id, pid: 0, cwd: thread.cwd, project: project,
            name: thread.name ?? project, nameSource: thread.name == nil ? nil : "user",
            branch: thread.branch, model: thread.model,
            // Its terminal is found when Open is pressed: no pid ties it to one.
            // Replies go through Codex's own server, whatever terminal shows it.
            host: .unknown,
            phase: phase, phaseSince: since, turnStarted: thread.turnStarted, lastActive: lastActive,
            lookedAt: thread.acknowledged, waitingFor: nil,
            currentTool: phase == .runningTool || phase == .waitingPermission ? current : nil,
            permission: thread.pending.first.map(permission),
            timeline: timeline, subagents: [], tasks: nil, tokens: thread.tokens, cost: 0,
            title: previews ? thread.preview : nil, lastText: nil, hooked: true, provider: .codex)
    }

    // MARK: Tools

    static func toolName(_ kind: String) -> String {
        switch kind {
        case "commandExecution": return "Command"
        case "fileChange": return "Edit"
        case "mcpToolCall": return "MCP"
        case "dynamicToolCall": return "Tool"
        case "webSearch": return "Search"
        case "imageGeneration": return "Image"
        default: return "Agent"
        }
    }

    /// What the tool works on; kept only with previews (the session strips it otherwise).
    static func toolSummary(_ kind: String, _ item: [String: Any]) -> String? {
        let text: String?
        switch kind {
        case "commandExecution":
            text = string(item, "command").map(readableCommand)
        case "fileChange":
            let names = (item["changes"] as? [Any] ?? []).compactMap { change -> String? in
                guard let path = (change as? [String: Any]).flatMap({ string($0, "path") }) else { return nil }
                return path.split(omittingEmptySubsequences: false, whereSeparator: { $0 == "/" || $0 == "\\" })
                    .last.map(String.init) ?? path
            }
            text = names.isEmpty ? nil : names.joined(separator: ", ")
        case "mcpToolCall":
            if let server = string(item, "server"), let tool = string(item, "tool") {
                text = "\(server).\(tool)"
            } else {
                text = nil
            }
        case "webSearch":
            text = string(item, "query")
        default:
            text = nil
        }
        guard let text else { return nil }
        let line = firstLine(text).trimmingCharacters(in: .whitespacesAndNewlines)
        return line.count > ClaudeToolSummary.limit ? String(line.prefix(ClaudeToolSummary.limit - 1)) + "…" : line
    }

    // MARK: Commands

    /// POSIX shells Codex wraps a command in on macOS and Linux, as
    /// `/bin/zsh -lc '<command>'`.
    static let posixShells: Set<String> = ["sh", "bash", "zsh", "dash", "ksh", "fish"]

    /// The command a person would recognise. Codex runs shell commands as
    /// `/bin/zsh -lc '<command>'` on macOS (shell-quoted), and as
    /// `"…\powershell.exe" -Command '<command>'` (or `pwsh`, `cmd /c`) on
    /// Windows, and its own terminal shows only the inner command, so the
    /// island does too.
    static func readableCommand(_ command: String) -> String {
        let trimmed = command.trimmingCharacters(in: .whitespacesAndNewlines)
        if let inner = posixInner(trimmed) { return inner }
        for shell in ["powershell.exe\"", "powershell\"", "pwsh.exe\"", "pwsh\"", "powershell.exe", "pwsh.exe"] {
            guard let found = trimmed.range(of: shell, options: [.caseInsensitive]) else { continue }
            var rest = Substring(trimmed[found.upperBound...]).trimmingLeadingWhitespace()
            if rest.hasPrefix("-NoProfile") { rest = rest.dropFirst("-NoProfile".count).trimmingLeadingWhitespace() }
            guard rest.hasPrefix("-Command") else { continue }
            let inner = String(rest.dropFirst("-Command".count)).trimmingCharacters(in: .whitespacesAndNewlines)
            if inner.count >= 2, inner.hasPrefix("'"), inner.hasSuffix("'") {
                return String(inner.dropFirst().dropLast()).replacingOccurrences(of: "''", with: "'")
            }
            if inner.count >= 2, inner.hasPrefix("\""), inner.hasSuffix("\"") {
                return String(inner.dropFirst().dropLast())
            }
            return inner
        }
        for prefix in ["cmd.exe /c ", "cmd /c "] {
            if let found = trimmed.range(of: prefix, options: [.caseInsensitive]) {
                return String(trimmed[found.upperBound...]).trimmingCharacters(in: .whitespacesAndNewlines)
            }
        }
        return trimmed
    }

    /// The script of `<shell> [-l] -c '<script>'`, or nil when the command is
    /// not wrapped that way.
    static func posixInner(_ command: String) -> String? {
        guard let words = shellWords(command), words.count >= 3,
              let shell = words.first.map({ ($0 as NSString).lastPathComponent }), posixShells.contains(shell)
        else { return nil }
        // Options up to the one with `c`: `-lc`, `-c`, `-l -c`, `--login -c`.
        var index = 1
        while index < words.count, words[index].hasPrefix("-"), words[index].count > 1 {
            let option = words[index]
            index += 1
            if !option.hasPrefix("--"), option.dropFirst().contains("c") {
                return index < words.count ? words[index] : nil
            }
        }
        return nil
    }

    /// Splits a command line the way a POSIX shell reads its words: quotes
    /// and backslashes as `sh` treats them. Nil when a quote is left open.
    static func shellWords(_ line: String) -> [String]? {
        var words: [String] = []
        var word = ""
        var inWord = false
        var characters = Array(line)[...]
        while let character = characters.popFirst() {
            switch character {
            case " ", "\t", "\n":
                if inWord { words.append(word) }
                word = ""
                inWord = false
            case "'":
                inWord = true
                guard let end = characters.firstIndex(of: "'") else { return nil }
                word += String(characters[..<end])
                characters = characters[(end + 1)...]
            case "\"":
                inWord = true
                var closed = false
                while let next = characters.popFirst() {
                    if next == "\"" { closed = true; break }
                    if next == "\\", let escaped = characters.first, "$`\"\\\n".contains(escaped) {
                        characters.removeFirst()
                        if escaped != "\n" { word.append(escaped) }
                    } else {
                        word.append(next)
                    }
                }
                if !closed { return nil }
            case "\\":
                guard let escaped = characters.popFirst() else { return nil }
                if escaped != "\n" {
                    inWord = true
                    word.append(escaped)
                }
            default:
                inWord = true
                word.append(character)
            }
        }
        if inWord { words.append(word) }
        return words
    }

    // MARK: Requests

    /// The card the island shows for the oldest request Codex waits on.
    static func permission(_ pending: Pending) -> ClaudePermission {
        let params = pending.params
        let tool: String
        var summary: String?
        let detail: String
        let always: Bool
        switch pending.method {
        case "item/commandExecution/requestApproval":
            let command = readableCommand(string(params, "command") ?? "")
            var text = command
            if let cwd = string(params, "cwd") { text += "\n\nin \(cwd)" }
            if let reason = string(params, "reason") { text += "\n\n\(reason)" }
            (tool, summary, detail, always) = ("Command", firstLine(command), text, true)
        case "item/fileChange/requestApproval":
            (tool, detail, always) = ("Edit", string(params, "reason") ?? "Codex wants to change files.", true)
        case "item/permissions/requestApproval":
            let reason = string(params, "reason") ?? "Codex asks for more access."
            let asked = params["permissions"].map(prettyJSON) ?? ""
            (tool, detail, always) = ("Permissions", "\(reason)\n\n\(asked)", true)
        case "mcpServer/elicitation/request":
            let server = string(params, "serverName") ?? "MCP"
            let message = (params["mode"] as? [String: Any]).flatMap { string($0, "message") }
                ?? string(params, "message") ?? ""
            (tool, detail, always) = (server, message, false)
        default:
            (tool, detail, always) = ("Codex", "", false)
        }
        var card = ClaudePermission(requestID: pending.requestID, tool: tool, summary: summary,
                                    offersAlways: always, since: pending.since)
        card.detail = clip(detail.trimmingCharacters(in: .whitespacesAndNewlines), limit: detailLimit)
        if pending.method == "item/tool/requestUserInput" {
            card.questions = questions(params).map(\.question)
        }
        return card
    }

    /// Codex's questions with their own ids, which the answer is keyed by.
    static func questions(_ params: [String: Any]) -> [(id: String, question: ClaudeQuestion)] {
        (params["questions"] as? [Any] ?? []).compactMap { value in
            guard let item = value as? [String: Any], let id = string(item, "id"),
                  let text = string(item, "question") else { return nil }
            let options = (item["options"] as? [Any] ?? []).compactMap { option -> ClaudeQuestion.Option? in
                if let label = option as? String { return ClaudeQuestion.Option(label: label) }
                guard let object = option as? [String: Any], let label = string(object, "label") else { return nil }
                return ClaudeQuestion.Option(label: label, description: string(object, "description"))
            }
            return (id, ClaudeQuestion(question: text, header: string(item, "header"), options: options,
                                       multiSelect: false))
        }
    }

    /// The JSON-RPC result that answers `pending`, or nil when it cannot be answered that way.
    static func result(_ pending: Pending, _ answer: CodexAnswer) -> [String: Any]? {
        switch (pending.method, answer) {
        case ("item/commandExecution/requestApproval", .allow), ("item/fileChange/requestApproval", .allow):
            return ["decision": "accept"]
        case ("item/commandExecution/requestApproval", .allowForSession),
             ("item/fileChange/requestApproval", .allowForSession):
            return ["decision": "acceptForSession"]
        // Decline keeps the turn going; the terminal's own n/Esc would stop it.
        case ("item/commandExecution/requestApproval", .deny), ("item/fileChange/requestApproval", .deny):
            return ["decision": "decline"]
        case ("item/permissions/requestApproval", .allow), ("item/permissions/requestApproval", .allowForSession):
            let asked = pending.params["permissions"] ?? [String: Any]()
            return ["permissions": asked, "scope": answer == .allow ? "turn" : "session"]
        case ("item/permissions/requestApproval", .deny):
            return ["permissions": [String: Any]()]
        case ("item/tool/requestUserInput", .questions(let byText)):
            var answers: [String: Any] = [:]
            for (id, question) in questions(pending.params) {
                if let picked = byText[question.question] { answers[id] = ["answers": picked] }
            }
            return ["answers": answers]
        case ("item/tool/requestUserInput", .deny):
            return ["answers": [String: Any]()]
        default:
            return nil
        }
    }

    // MARK: JSON

    /// A non-empty string at `key`.
    static func string(_ object: [String: Any], _ key: String) -> String? {
        guard let text = object[key] as? String, !text.isEmpty else { return nil }
        return text
    }

    /// Seconds or milliseconds since the epoch, whichever Codex sent.
    static func time(_ object: [String: Any], _ key: String) -> Date? {
        guard let number = number(object[key]) else { return nil }
        return Date(timeIntervalSince1970: number > 1e11 ? number / 1000 : number)
    }

    static func number(_ value: Any?) -> Double? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        return number.doubleValue
    }

    static func integer(_ value: Any?) -> Int? {
        guard let number = number(value), number.rounded() == number, abs(number) < 9.0e18 else { return nil }
        return Int(number)
    }

    static func bool(_ value: Any?) -> Bool? {
        guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { return nil }
        return number.boolValue
    }

    static func prettyJSON(_ value: Any) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: value,
                                                     options: [.prettyPrinted, .sortedKeys, .fragmentsAllowed,
                                                               .withoutEscapingSlashes])
        else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    static func firstLine(_ text: String) -> String {
        guard let line = text.split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: false).first
        else { return "" }
        return line.hasSuffix("\r") ? String(line.dropLast()) : String(line)
    }

    static func clip(_ text: String, limit: Int) -> String {
        guard text.count > limit else { return text }
        return String(text.prefix(limit)) + "\n… \(text.count - limit) more characters"
    }
}

/// A JSON-RPC id, a number or a string, kept as it came so the answer
/// carries the same one.
enum CodexRPCID: Hashable, CustomStringConvertible {
    case number(Int)
    case string(String)

    init?(json value: Any?) {
        if let text = value as? String {
            self = .string(text)
        } else if let number = CodexSessionState.integer(value) {
            self = .number(number)
        } else {
            return nil
        }
    }

    var json: Any {
        switch self {
        case .number(let number): return number
        case .string(let text): return text
        }
    }

    /// As JSON writes it: `7`, or `"r2"` with its quotes.
    var description: String {
        switch self {
        case .number(let number): return String(number)
        case .string(let text):
            guard let data = try? JSONSerialization.data(withJSONObject: text, options: [.fragmentsAllowed,
                                                                                       .withoutEscapingSlashes])
            else { return "\"\(text)\"" }
            return String(decoding: data, as: UTF8.self)
        }
    }
}

/// How the person answered a Codex request.
enum CodexAnswer: Equatable {
    case allow
    case allowForSession
    case deny
    /// What the person picked or typed for each question, by its text.
    case questions([String: [String]])

    init(_ decision: ClaudeHookMessage.Decision) {
        switch decision {
        case .allow: self = .allow
        case .allowAlways: self = .allowForSession
        case .deny: self = .deny
        case .answer(let answers): self = .questions(answers)
        }
    }
}

private extension Substring {
    func trimmingLeadingWhitespace() -> Substring {
        drop { $0.isWhitespace }
    }
}
