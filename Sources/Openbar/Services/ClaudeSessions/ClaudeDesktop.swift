// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Foundation

/// Where a Claude session runs when not in a terminal: the Claude desktop
/// app's Code tab, or its Cowork (issue #246).
enum ClaudeSessionOrigin: String, Equatable {
    /// The Code tab: Claude Code run by the app, heard through the same
    /// hooks and registry as one in a terminal.
    case desktop
    /// Cowork: Claude Code run inside the app's virtual machine with a
    /// config folder of its own, so no hook of the island's runs there.
    /// Read from the session files the app keeps, with its own switch.
    case cowork
}

/// The Claude desktop app, as far as Sessions knows it (docs/more-session-agents.md).
enum ClaudeDesktop {
    static let bundle = "com.anthropic.claudefordesktop"
    static let name = "Claude Desktop"

    /// The Code tab runs Claude Code with `CLAUDE_CODE_ENTRYPOINT` set to
    /// `claude-desktop` (signed in) or `claude-desktop-3p` (third-party
    /// inference), which the session's registry record carries as
    /// `entrypoint`. Cowork's is `local-agent`, inside its machine.
    static func isCodeTab(entrypoint: String?) -> Bool {
        guard let entrypoint else { return false }
        return entrypoint == "claude-desktop" || entrypoint.hasPrefix("claude-desktop-")
    }

    /// The app's data folders: the one it keeps signed in, and the one for
    /// third-party inference, which needs no account.
    static func dataFolders(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        let support = home.appending(path: "Library/Application Support", directoryHint: .isDirectory)
        return ["Claude", "Claude-3p"].map { support.appending(path: $0, directoryHint: .isDirectory) }
    }

    /// Where Cowork keeps a `local_<id>.json` and a folder for each session.
    static func coworkFolders(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        dataFolders(home: home).map { $0.appending(path: "local-agent-mode-sessions", directoryHint: .isDirectory) }
    }

    static func isInstalled(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) != nil
            || dataFolders(home: home).contains { FileManager.default.fileExists(atPath: $0.path) }
    }

    /// Whether the app runs: Cowork's sessions live in its machine, so
    /// none of them goes on once the app is gone.
    static func isRunning() -> Bool {
        !NSRunningApplication.runningApplications(withBundleIdentifier: bundle).isEmpty
    }

    /// The link that shows the session in the app. The Code tab opens a
    /// Claude Code session by its id with `claude://resume?session=`, the way
    /// `/desktop` hands one over from a terminal; Cowork has no link to one
    /// session, so the app comes forward.
    static func link(for session: ClaudeSession) -> URL? {
        guard session.origin == .desktop,
              session.id.range(of: #"^[0-9a-fA-F-]{8,64}$"#, options: .regularExpression) != nil else { return nil }
        return URL(string: "claude://resume?session=\(session.id)")
    }

    /// Brings the session's app forward, at the session when the app takes a link to it.
    static func open(_ session: ClaudeSession) -> Bool {
        if let link = link(for: session), NSWorkspace.shared.urlForApplication(toOpen: link) != nil {
            NSWorkspace.shared.open(link)
            return true
        }
        if let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first {
            return app.activate()
        }
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) else { return false }
        NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
        return true
    }
}

// MARK: Cowork

/// Cowork's sessions, read from the files the app keeps for each and never
/// written: `local_<id>.json` (title, folders, model, last activity) and the
/// `audit.jsonl` in the session's folder, where the app logs every message of
/// the session and each permission it asks for and gets. Queue-owned, like
/// the service that holds it.
final class ClaudeCoworkSessions {
    /// A session quiet this long leaves the list.
    static let listedFor: TimeInterval = ClaudeSessionEngine.finishedFor
    /// A turn heard of this long ago is taken as over: the app may have been
    /// stopped mid-turn, which writes nothing.
    static let quietFor: TimeInterval = AgentHookSessions.quietFor
    /// How much of a log first seen is read: its end tells the state.
    static let firstRead = 1 << 20
    static let timelineLength = ClaudeSessionEngine.timelineLength
    static let chatLength = 40

    struct Meta: Equatable {
        var id: String
        var title: String?
        var folder: String?
        var model: String?
        var created: Date?
        var lastActivity: Date?
        var archived = false
    }

    struct Entry {
        var meta: Meta
        var metaStamp: Date?
        var log: String
        var offset: UInt64 = 0
        var state = ClaudeCoworkLog.State()
        var acknowledged: Date?
        /// Read with previews on: turned on later, the log is read again for what was skipped.
        var previews = false
    }

    private var entries: [String: Entry] = [:]
    private var phases: [String: (phase: ClaudeSessionPhase, since: Date)] = [:]

    /// Reads what changed in `folders` and lists the sessions active lately.
    /// With the app not running, or reading turned off, there are none.
    func scan(_ folders: [URL], now: Date, previews: Bool, running: Bool) -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        guard running else {
            entries = [:]
            phases = [:]
            return ([], [])
        }
        var seen = Set<String>()
        for file in Self.sessionFiles(in: folders) {
            let id = ((file.path as NSString).lastPathComponent as NSString).deletingPathExtension
            guard seen.insert(id).inserted else { continue }
            let stamp = Self.modified(file.path)
            var entry = entries[id] ?? Entry(meta: Meta(id: id), log: file.deletingPathExtension().appending(path: "audit.jsonl").path)
            if entry.metaStamp != stamp, let data = try? Data(contentsOf: file), data.count < 4 << 20,
               let meta = Self.meta(data, id: id) {
                entry.meta = meta
                entry.metaStamp = stamp
            }
            entries[id] = entry
        }
        entries = entries.filter { seen.contains($0.key) }
        var sessions: [ClaudeSession] = []
        var events: [ClaudeSessionEvent] = []
        for id in entries.keys.sorted() {
            guard var entry = entries[id] else { continue }
            let logStamp = Self.modified(entry.log)
            // The app rewrites a session's file now and then, as when it
            // starts; the activity it records, and its log, tell when it worked.
            let heard = [entry.meta.lastActivity ?? entry.metaStamp, logStamp].compactMap { $0 }.max()
            guard !entry.meta.archived, let heard, now.timeIntervalSince(heard) < Self.quietFor else {
                phases[id] = nil
                continue
            }
            read(&entry, previews: previews)
            entries[id] = entry
            let phase = Self.phase(entry.state, acknowledged: entry.acknowledged, now: now)
            guard phase.isActive || phase.isWaiting || now.timeIntervalSince(heard) < Self.listedFor else {
                phases[id] = nil
                continue
            }
            let previous = phases[id]
            let since = previous?.phase == phase ? previous?.since ?? now : now
            phases[id] = (phase, since)
            if let previous, previous.phase != phase {
                switch phase {
                case .waitingPermission: events.append(.needsPermission(id: Self.rowID(id), tool: entry.state.pending.first?.tool ?? ""))
                case .waitingInput: events.append(.needsInput(id: Self.rowID(id)))
                case .finished where previous.phase.isActive || previous.phase.isWaiting:
                    let duration = entry.state.turnStarted.flatMap { start in entry.state.turnEnded.map { $0.timeIntervalSince(start) } }
                    events.append(.finished(id: Self.rowID(id), duration: duration))
                default: break
                }
            }
            sessions.append(Self.session(entry, phase: phase, since: since, heard: heard, previews: previews))
        }
        return (sessions, events)
    }

    func acknowledge(_ rowID: String, now: Date) {
        guard rowID.hasPrefix(Self.prefix) else { return }
        entries[String(rowID.dropFirst(Self.prefix.count))]?.acknowledged = now
    }

    /// Drops what previews let it keep, when they are turned off.
    func purgeContent() {
        for id in entries.keys {
            entries[id]?.state.purgeContent()
            entries[id]?.previews = false
            entries[id]?.meta.title = nil
            entries[id]?.metaStamp = nil
        }
    }

    // MARK: Rows

    static let prefix = "cowork:"
    static func rowID(_ id: String) -> String { prefix + id }

    static func session(_ entry: Entry, phase: ClaudeSessionPhase, since: Date, heard: Date, previews: Bool) -> ClaudeSession {
        let state = entry.state
        let folder = entry.meta.folder
        let project = folder.map(AgentLogParser.projectName) ?? "Cowork"
        var timeline = Array(state.tools.suffix(timelineLength))
        if !previews { for index in timeline.indices { timeline[index].summary = nil } }
        var open = state.openTool
        if !previews { open?.summary = nil }
        var session = ClaudeSession(
            id: rowID(entry.meta.id), pid: 0, cwd: folder ?? "", project: project,
            name: previews ? entry.meta.title ?? project : project,
            model: state.model ?? entry.meta.model,
            host: .app(ClaudeDesktop.bundle), phase: phase, phaseSince: since, turnStarted: state.turnStarted,
            lastActive: heard, lookedAt: entry.acknowledged,
            // Cowork asks inside the app: the island shows the wait, the answer stays there.
            waitingFor: state.pending.isEmpty ? nil : CodexSessionState.answerInTerminal,
            currentTool: phase == .runningTool || phase == .waitingPermission ? open : nil,
            timeline: timeline, subagents: [], tasks: nil,
            turnEndedAt: state.turnEnded, turnCompleted: phase == .finished,
            tokens: AgentTokens(), cost: 0,
            title: previews ? entry.meta.title : nil, lastText: previews ? state.lastText : nil,
            hooked: false)
        session.startedAt = entry.meta.created
        session.origin = .cowork
        if previews { session.chat = state.chat }
        return session
    }

    static func phase(_ state: ClaudeCoworkLog.State, acknowledged: Date?, now: Date) -> ClaudeSessionPhase {
        if let ask = state.pending.first { return ask.tool == "AskUserQuestion" ? .waitingInput : .waitingPermission }
        if state.turnOpen, let heard = state.heard, now.timeIntervalSince(heard) < quietFor {
            return state.openTool != nil ? .runningTool : .generating
        }
        if let ended = state.turnEnded, state.completed, now.timeIntervalSince(ended) < ClaudeSessionEngine.finishedFor,
           (acknowledged ?? .distantPast) < ended {
            return .finished
        }
        return .idle
    }

    // MARK: Files

    /// The `local_<id>.json` files under the folders: `<account>/<org>/`,
    /// and one level below for the kinds the app files apart. A session's
    /// own folder is never gone into.
    static func sessionFiles(in folders: [URL]) -> [URL] {
        var found: [URL] = []
        func walk(_ folder: URL, depth: Int) {
            guard depth <= 4, let names = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return }
            for name in names.sorted() {
                let url = folder.appending(path: name)
                if name.hasPrefix("local_") {
                    if name.hasSuffix(".json") { found.append(url) }
                    continue
                }
                guard !name.hasPrefix("."), name != "skills-plugin", isFolder(url.path) else { continue }
                walk(url, depth: depth + 1)
            }
        }
        folders.forEach { walk($0, depth: 0) }
        return found
    }

    static func meta(_ data: Data, id: String) -> Meta? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        var meta = Meta(id: (json["sessionId"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? id)
        meta.id = id
        meta.title = (json["title"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        meta.folder = (json["userSelectedFolders"] as? [Any])?.compactMap { item -> String? in
            if let path = item as? String { return path }
            return (item as? [String: Any])?["path"] as? String
        }.first { $0.hasPrefix("/") }
        meta.model = (json["model"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        meta.created = date(json["createdAt"])
        meta.lastActivity = date(json["lastActivityAt"])
        meta.archived = json["isArchived"] as? Bool ?? false
        return meta
    }

    /// Milliseconds since 1970, or an ISO 8601 date.
    static func date(_ value: Any?) -> Date? {
        if let number = value as? NSNumber, number.doubleValue > 0 {
            let seconds = number.doubleValue > 1e11 ? number.doubleValue / 1000 : number.doubleValue
            return Date(timeIntervalSince1970: seconds)
        }
        if let text = value as? String { return ClaudeCoworkLog.timestamp(text) }
        return nil
    }

    private func read(_ entry: inout Entry, previews: Bool) {
        if previews, !entry.previews {
            entry.offset = 0
            entry.state = ClaudeCoworkLog.State()
        }
        entry.previews = previews
        guard let handle = FileHandle(forReadingAtPath: entry.log) else { return }
        defer { try? handle.close() }
        guard let size = try? handle.seekToEnd() else { return }
        if size < entry.offset { entry.offset = 0; entry.state = ClaudeCoworkLog.State() }
        guard size > entry.offset else { return }
        var start = entry.offset
        var skipFirst = false
        if start == 0, size > UInt64(Self.firstRead) {
            start = size - UInt64(Self.firstRead)
            skipFirst = true
        }
        try? handle.seek(toOffset: start)
        guard let data = try? handle.readToEnd() else { return }
        // Only whole lines: one being written is read once it ends.
        guard let lastBreak = data.lastIndex(of: 0x0A) else { return }
        var lines = data[data.startIndex..<lastBreak].split(separator: 0x0A, omittingEmptySubsequences: true)
        if skipFirst, !lines.isEmpty { lines.removeFirst() }
        for line in lines {
            ClaudeCoworkLog.apply(Data(line), to: &entry.state, previews: previews)
        }
        entry.offset = start + UInt64(lastBreak - data.startIndex + 1)
    }

    private static func modified(_ path: String) -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date
    }

    private static func isFolder(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &directory) && directory.boolValue
    }
}

/// One line of a Cowork session's `audit.jsonl` at a time: the session's own
/// messages as the Agent SDK streams them (`user`, `assistant`, `result`,
/// `system`), and the app's `permission_request` and `permission_response`,
/// each stamped with `_audit_timestamp`.
enum ClaudeCoworkLog {
    struct Ask: Equatable {
        let id: String
        let tool: String
        let since: Date
    }

    struct State: Equatable {
        var turnOpen = false
        var turnStarted: Date?
        var turnEnded: Date?
        var completed = false
        var heard: Date?
        var model: String?
        var tools: [ClaudeToolUse] = []
        var pending: [Ask] = []
        var lastText: String?
        var chat: [ClaudeChatMessage] = []
        private var nextChat = 0

        var openTool: ClaudeToolUse? { tools.last { $0.isRunning } }

        mutating func purgeContent() {
            lastText = nil
            chat = []
            for index in tools.indices { tools[index].summary = nil }
        }

        mutating func say(_ text: String, as role: ClaudeChatMessage.Role, at date: Date, endsTurn: Bool = false) {
            chat.append(ClaudeChatMessage(id: nextChat, role: role, text: text, at: date, endsTurn: endsTurn))
            nextChat += 1
            if chat.count > ClaudeCoworkSessions.chatLength { chat.removeFirst(chat.count - ClaudeCoworkSessions.chatLength) }
        }
    }

    static func apply(_ line: Data, to state: inout State, previews: Bool) {
        guard let json = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else { return }
        let at = (json["_audit_timestamp"] as? String).flatMap(timestamp) ?? Date()
        state.heard = max(state.heard ?? at, at)
        let type = json["type"] as? String
        // A subagent's messages carry the tool call that runs it; its tools
        // are not the session's own.
        let subagent = (json["parent_tool_use_id"] as? String).map { !$0.isEmpty } ?? false
        switch type {
        case "system":
            switch json["subtype"] as? String {
            case "init":
                if let model = json["model"] as? String, !model.isEmpty { state.model = model }
            case "permission_request":
                let id = json["uuid"] as? String ?? UUID().uuidString
                guard !state.pending.contains(where: { $0.id == id }) else { return }
                state.pending.append(Ask(id: id, tool: json["tool_name"] as? String ?? "", since: at))
            case "permission_response":
                if let id = json["uuid"] as? String { state.pending.removeAll { $0.id == id } }
            default: break
            }
        case "user":
            guard let message = json["message"] as? [String: Any] else { return }
            if let text = message["content"] as? String {
                if !subagent { prompt(text, at: at, in: &state, previews: previews) }
                return
            }
            let blocks = message["content"] as? [[String: Any]] ?? []
            var finished = false
            for block in blocks where block["type"] as? String == "tool_result" {
                guard let id = block["tool_use_id"] as? String,
                      let index = state.tools.lastIndex(where: { $0.id == id }) else { continue }
                state.tools[index].finished = at
                state.tools[index].failed = block["is_error"] as? Bool ?? false
                finished = true
            }
            if !finished, !subagent {
                let text = blocks.filter { $0["type"] as? String == "text" }.compactMap { $0["text"] as? String }
                    .joined(separator: "\n")
                if !text.isEmpty { prompt(text, at: at, in: &state, previews: previews) }
            }
        case "assistant":
            guard let message = json["message"] as? [String: Any], !subagent else { return }
            if !state.turnOpen { begin(at: at, in: &state) }
            if let model = message["model"] as? String, !model.isEmpty, model != "<synthetic>" { state.model = model }
            for block in message["content"] as? [[String: Any]] ?? [] {
                switch block["type"] as? String {
                case "text":
                    guard let text = (block["text"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
                          !text.isEmpty, previews else { continue }
                    state.lastText = text
                    state.say(text, as: .assistant, at: at)
                case "tool_use":
                    guard let id = block["id"] as? String, let name = block["name"] as? String,
                          !state.tools.contains(where: { $0.id == id }) else { continue }
                    let input = block["input"] as? [String: Any] ?? [:]
                    state.tools.append(ClaudeToolUse(id: id, name: name,
                                                     summary: previews ? ClaudeToolSummary.summary(name: name, input: input) : nil,
                                                     started: at))
                    if state.tools.count > 200 { state.tools.removeFirst(state.tools.count - 200) }
                default: break
                }
            }
        case "result":
            guard !subagent else { return }
            state.turnOpen = false
            state.turnEnded = at
            state.completed = (json["subtype"] as? String) == "success" && (json["is_error"] as? Bool) != true
            state.pending = []
            for index in state.tools.indices where state.tools[index].isRunning { state.tools[index].finished = at }
            if let last = state.chat.indices.last, state.chat[last].role == .assistant { state.chat[last].endsTurn = true }
        default: break
        }
    }

    private static func prompt(_ text: String, at date: Date, in state: inout State, previews: Bool) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // Reminders and command output the app adds come wrapped in tags.
        guard !trimmed.isEmpty, !trimmed.hasPrefix("<") else { return }
        begin(at: date, in: &state)
        if previews { state.say(trimmed, as: .user, at: date) }
    }

    private static func begin(at date: Date, in state: inout State) {
        state.turnOpen = true
        state.turnStarted = date
        state.completed = false
    }

    private static let fractional: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
    private static let whole = ISO8601DateFormatter()

    static func timestamp(_ text: String) -> Date? {
        fractional.date(from: text) ?? whole.date(from: text)
    }
}
