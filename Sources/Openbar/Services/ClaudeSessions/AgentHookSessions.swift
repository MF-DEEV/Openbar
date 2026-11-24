// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// The sessions of Gemini CLI, Copilot CLI, Cursor and OpenCode, kept from
/// the events their hooks and plugin send. None of these agents keeps a
/// registry the island could read, so a session is listed from its first
/// event until it ends, its process quits, or (for Cursor's app, whose
/// process outlives its conversations) it has been quiet a while.
/// Not thread-safe: the service uses it on its queue.
final class AgentHookSessions {
    /// How long an unread finished turn stays marked, as for Claude Code.
    static let finishedFor = ClaudeSessionEngine.finishedFor
    /// A Cursor or Antigravity conversation nothing was heard from for this
    /// long leaves the list.
    static let quietFor: TimeInterval = 30 * 60
    /// The agents whose process outlives its conversations: Cursor's app, and
    /// the language server under Antigravity's app, IDE window or `agy`.
    static let outlived: Set<ClaudeSessionProvider> = [.cursor, .antigravity]
    static let timelineLength = 12
    static let chatLength = 40

    struct Entry {
        let provider: ClaudeSessionProvider
        let sessionID: String
        var pid: Int32
        var cwd: String
        let firstSeen: Date
        var heard: Date
        var turnOpen = false
        var turnStarted: Date?
        var turnEnded: Date?
        var completed = true
        var acknowledged: Date?
        var tools: [ClaudeToolUse] = []
        var permission: ClaudePermission?
        /// What the tool about to run would ask, for an agent that names it
        /// only before it asks (Grok CLI).
        var nextAsk: ClaudePermission?
        /// Whether the island can answer `permission`.
        var answerable = false
        var asking = false
        var model: String?
        var title: String?
        var lastText: String?
        var chat: [ClaudeChatMessage] = []
        var usage: [String: (tokens: AgentTokens, cost: Double)] = [:]
        /// Where the person sees a Hermes session: `cli`, `tui` or `desktop`.
        var client: String?

        var openTool: ClaudeToolUse? { tools.last { $0.finished == nil } }

        /// The config folder's tag or tool id of a Codex session or tool
        /// heard through hooks the Hooks page wrote (#236).
        var via: String? {
            guard provider == .codex || provider == .custom, let colon = sessionID.firstIndex(of: ":") else { return nil }
            return String(sessionID[..<colon])
        }
    }

    private(set) var entries: [String: Entry] = [:]
    private var phases: [String: (phase: ClaudeSessionPhase, since: Date)] = [:]
    private var messageID = 0

    var isEmpty: Bool { entries.isEmpty }

    func owns(_ id: String) -> Bool { entries[id] != nil }

    func entry(_ id: String) -> Entry? { entries[id] }

    // MARK: Input

    func ingest(_ event: AgentHookEvent, now: Date) {
        let id = event.id
        if case .ended = event.kind {
            entries[id] = nil
            phases[id] = nil
            return
        }
        var entry = entries[id] ?? Entry(provider: event.provider, sessionID: event.sessionID, pid: event.pid,
                                         cwd: event.cwd ?? "", firstSeen: now, heard: now)
        if event.pid > 0 { entry.pid = event.pid }
        if let cwd = event.cwd, !cwd.isEmpty { entry.cwd = cwd }
        entry.heard = now
        let date = min(event.date, now)
        switch event.kind {
        case .started, .ended:
            break
        case .prompt(let text):
            entry.turnOpen = true
            entry.turnStarted = date
            entry.turnEnded = nil
            entry.completed = false
            entry.clearWait()
            if let text { entry.append(.user, text, at: date, id: &messageID) }
        case .working:
            if !entry.turnOpen {
                entry.turnOpen = true
                entry.turnStarted = date
                entry.turnEnded = nil
                entry.completed = false
            }
        case .toolStarted(let use):
            guard !entry.tools.contains(where: { $0.id == use.id }) else { break }
            entry.openTurn(at: date)
            // A tool that runs was allowed, wherever.
            if !entry.answerable { entry.clearWait() }
            entry.tools.append(use)
            entry.tools = Array(entry.tools.suffix(Self.timelineLength))
        case .toolFinished(let toolID, let name, let key, let failed, let started):
            entry.openTurn(at: date)
            entry.nextAsk = nil
            if !entry.answerable { entry.clearWait() }
            let index = toolID.flatMap { id in entry.tools.lastIndex { $0.id == id } }
                ?? entry.tools.firstIndex { $0.finished == nil && $0.name == name && (key == nil || $0.inputKey == key) }
                ?? entry.tools.firstIndex { $0.finished == nil && $0.name == name }
            if let index {
                entry.tools[index].finished = max(date, entry.tools[index].started)
                entry.tools[index].failed = failed
            } else {
                // Cursor tells of a tool only once it ran.
                var use = ClaudeToolUse(id: toolID ?? UUID().uuidString, name: name.isEmpty ? "Tool" : name, summary: nil,
                                        started: started ?? date)
                use.finished = date
                use.failed = failed
                use.inputKey = key
                entry.tools.append(use)
                entry.tools = Array(entry.tools.suffix(Self.timelineLength))
            }
        case .permission(var permission):
            entry.openTurn(at: date)
            // Hermes asks a question the island can't answer with a key: in Hermes.
            if permission.tool.isEmpty, event.provider == .hermes, entry.nextAsk == nil {
                entry.asking = true
                break
            }
            // Grok's prompt names no tool: the one its last PreToolUse named waits.
            if permission.tool.isEmpty {
                let asked = permission
                permission = entry.nextAsk ?? ClaudePermission(requestID: asked.requestID, tool: "Tool", summary: nil,
                                                               offersAlways: false, since: asked.since)
                permission = permission.renewed(requestID: asked.requestID, since: asked.since)
            }
            entry.permission = permission
            // Codex through hooks only hears of a request: Codex asks in its terminal.
            entry.answerable = event.provider.answersPermissions && entry.via == nil
            entry.asking = false
        case .heldPermission(let permission):
            entry.openTurn(at: date)
            entry.permission = permission
            entry.answerable = true
            entry.asking = false
        case .permissionSettled(let request):
            if request == nil || entry.permission?.requestID == request { entry.clearWait() }
        case .question:
            entry.openTurn(at: date)
            entry.asking = true
        case .reply(let text):
            entry.lastText = text
            entry.append(.assistant, text, at: date, id: &messageID)
        case .stopped(let text, let failed):
            guard entry.turnOpen || entry.permission != nil || entry.asking else { break }
            entry.turnOpen = false
            entry.turnEnded = date
            entry.completed = !failed
            entry.nextAsk = nil
            entry.clearWait()
            for index in entry.tools.indices where entry.tools[index].finished == nil {
                entry.tools[index].finished = max(date, entry.tools[index].started)
            }
            if let text {
                entry.lastText = text
                if entry.chat.last?.role != .assistant || entry.chat.last?.text != text {
                    entry.append(.assistant, text, at: date, id: &messageID)
                }
                if let last = entry.chat.indices.last, entry.chat[last].role == .assistant { entry.chat[last].endsTurn = true }
            } else if let last = entry.chat.indices.last, entry.chat[last].role == .assistant {
                entry.chat[last].endsTurn = true
            }
        case .model(let model):
            entry.model = model
        case .mayAsk(let permission):
            entry.nextAsk = permission
        case .title(let title):
            entry.title = title
        case .usage(let key, let tokens, let cost):
            entry.usage[key] = (tokens, cost)
        case .client(let name):
            entry.client = name
        }
        entries[id] = entry
    }

    /// The person answered the request here; the agent tells when it moves on.
    func resolve(_ id: String, request: String) {
        guard entries[id]?.permission?.requestID == request else { return }
        entries[id]?.clearWait()
    }

    /// The agent shows its own dialog for the request now: the island
    /// answers it there, with keys (Qwen Code, #239).
    func moveToDialog(_ id: String, request: String) {
        guard entries[id]?.permission?.requestID == request else { return }
        entries[id]?.permission?.inAgentDialog = true
    }

    func acknowledge(_ id: String, now: Date) {
        entries[id]?.acknowledged = now
    }

    /// Forgets every text and title read so far.
    func purgeContent() {
        for id in entries.keys {
            entries[id]?.title = nil
            entries[id]?.lastText = nil
            entries[id]?.chat = []
            if let tools = entries[id]?.tools { entries[id]?.tools = tools.map { var tool = $0; tool.summary = nil; return tool } }
        }
    }

    /// Drops sessions whose process is gone, and Cursor and Antigravity
    /// conversations long quiet.
    func prune(now: Date, alive: (Int32) -> Bool = AgentHookSessions.isAlive) {
        for (id, entry) in entries {
            let gone = entry.pid > 0 && !alive(entry.pid)
            // A Hermes session whose client wasn't found lives in its gateway:
            // it goes when its client closes, or once long quiet.
            let outlived = Self.outlived.contains(entry.provider) || (entry.provider == .hermes && entry.pid <= 0)
            let quiet = outlived && !entry.turnOpen && entry.permission == nil
                && now.timeIntervalSince(entry.heard) > Self.quietFor
            if gone || quiet {
                entries[id] = nil
                phases[id] = nil
            }
        }
    }

    static func isAlive(_ pid: Int32) -> Bool { kill(pid, 0) == 0 || errno == EPERM }

    /// Whether a Hermes session's prompts take the island's keys: its
    /// classic terminal UI's do; its Ink UI and its app are answered there.
    static func hermesTakesKeys(_ client: String?) -> Bool { client == nil || client == "cli" }

    // MARK: Output

    static func phase(_ entry: Entry, now: Date) -> ClaudeSessionPhase {
        if entry.permission != nil { return .waitingPermission }
        if entry.asking { return .waitingInput }
        if entry.turnOpen { return entry.openTool != nil ? .runningTool : .generating }
        if let ended = entry.turnEnded, entry.completed, now.timeIntervalSince(ended) < finishedFor,
           (entry.acknowledged ?? .distantPast) < ended {
            return .finished
        }
        return .idle
    }

    /// The sessions as the island shows them, with the changes worth a notice.
    /// `host` finds the terminal or app of a process.
    func snapshot(now: Date, previews: Bool, host: (Int32) -> ClaudeSessionHost,
                  branch: (String) -> String?) -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        var sessions: [ClaudeSession] = []
        var events: [ClaudeSessionEvent] = []
        for (id, entry) in entries {
            let phase = Self.phase(entry, now: now)
            let previous = phases[id]
            let since = previous?.phase == phase ? previous?.since ?? now : now
            phases[id] = (phase, since)
            if let previous, previous.phase != phase {
                switch phase {
                case .waitingPermission:
                    events.append(.needsPermission(id: id, tool: entry.permission?.tool ?? ""))
                case .waitingInput:
                    events.append(.needsInput(id: id))
                case .finished where previous.phase.isActive || previous.phase.isWaiting:
                    let duration = entry.turnStarted.flatMap { start in entry.turnEnded.map { $0.timeIntervalSince(start) } }
                    events.append(.finished(id: id, duration: duration))
                default: break
                }
            }
            sessions.append(session(id: id, entry, phase: phase, since: since, previews: previews,
                                    host: host(entry.pid), branch: branch(entry.cwd)))
        }
        return (sessions, events)
    }

    private func session(id: String, _ entry: Entry, phase: ClaudeSessionPhase, since: Date, previews: Bool,
                         host: ClaudeSessionHost, branch: String?) -> ClaudeSession {
        var timeline = entry.tools
        if !previews { for index in timeline.indices { timeline[index].summary = nil } }
        let project = entry.provider == .zcode ? AgentHookMessage.zcodeProject(entry.cwd) : AgentLogParser.projectName(entry.cwd)
        let tokens = entry.usage.values.reduce(AgentTokens()) { total, part in
            AgentTokens(input: total.input + part.tokens.input, cacheWrite: total.cacheWrite + part.tokens.cacheWrite,
                        cacheRead: total.cacheRead + part.tokens.cacheRead, output: total.output + part.tokens.output,
                        reasoning: total.reasoning + part.tokens.reasoning)
        }
        let cost = entry.usage.values.reduce(0) { $0 + $1.cost }
        // A request only the agent can answer is shown as waiting there.
        // Grok's is answered with keys, where its terminal takes them.
        // Hermes's with keys in its classic terminal UI, through its gateway
        // anywhere else. Qwen Code's question or plan with keys, in its own
        // dialog; its tools' through the hook the island holds.
        let qwenKeys = entry.provider == .qwen && entry.permission.map(AgentQwen.answeredByKeys) == true
        let keyed = entry.provider == .grok || qwenKeys
        let inAgent = entry.permission != nil
            && (!entry.answerable || (keyed && !ClaudeTyping.takesKeys(host))
                || (qwenKeys && entry.permission.map { !AgentQwen.keyable($0, oneBurst: AgentQwen.oneBurst(host)) } == true))
        var session = ClaudeSession(
            id: id, pid: entry.pid, cwd: entry.cwd, project: project, name: project, branch: branch, model: entry.model,
            host: host, phase: phase, phaseSince: since, turnStarted: entry.turnStarted,
            lastActive: [entry.heard, entry.turnEnded].compactMap { $0 }.max(), lookedAt: entry.acknowledged,
            waitingFor: inAgent ? CodexSessionState.answerInTerminal : (entry.asking ? CodexSessionState.answerInTerminal : nil),
            currentTool: phase == .runningTool || phase == .waitingPermission ? entry.openTool : nil,
            permission: inAgent ? nil : entry.permission.map { permission in
                var permission = permission
                permission.inputKey = permission.inputKey ?? entry.openTool?.inputKey
                return permission
            },
            timeline: timeline, subagents: [], tasks: nil,
            // The same turn facts as Windows' hook_agents.rs, for the session sounds (#232).
            turnEndedAt: entry.turnEnded, turnCompleted: phase == .finished,
            tokens: tokens, cost: cost,
            title: previews ? entry.title : nil, lastText: previews ? entry.lastText : nil,
            hooked: true, provider: entry.provider)
        if previews { session.chat = entry.chat }
        session.via = entry.via
        if inAgent { session.waitingDiffs = entry.permission?.diffs ?? [] }
        return session
    }
}

private extension AgentHookSessions.Entry {
    mutating func clearWait() {
        permission = nil
        answerable = false
        asking = false
    }

    /// An event of a turn the island did not see begin opens it.
    mutating func openTurn(at date: Date) {
        guard !turnOpen else { return }
        turnOpen = true
        turnStarted = turnStarted.flatMap { started in turnEnded == nil ? started : nil } ?? date
        turnEnded = nil
        completed = false
    }

    mutating func append(_ role: ClaudeChatMessage.Role, _ text: String, at date: Date, id: inout Int) {
        id += 1
        chat.append(ClaudeChatMessage(id: id, role: role, text: text, at: date))
        if chat.count > AgentHookSessions.chatLength { chat.removeFirst(chat.count - AgentHookSessions.chatLength) }
    }
}

/// The agent process a hook came from: above the socket's peer, the `nc`
/// the hook runs, and the shells the agent started it in, the first process
/// that is none of these. It outlives the hook, so the island can tell when
/// the agent quits and find its terminal.
enum AgentHookProcess {
    static let shells: Set<String> = ["nc", "sh", "bash", "zsh", "dash", "fish", "ksh", "tcsh", "csh", "env", "cat",
                                      "timeout", "script", "login"]

    static func agent(fromHook peer: Int32, hint: Int32) -> Int32 {
        var pid = peer
        for _ in 0..<8 {
            guard pid > 1 else { break }
            guard let name = name(of: pid) else { break }
            if !shells.contains(name) { return pid }
            guard let parent = parent(of: pid) else { break }
            pid = parent
        }
        return hint
    }

    static func name(of pid: Int32) -> String? {
        var buffer = [CChar](repeating: 0, count: 2 * Int(MAXCOMLEN) + 1)
        guard proc_name(pid, &buffer, UInt32(buffer.count)) > 0 else { return nil }
        return String(cString: buffer)
    }

    static func parent(of pid: Int32) -> Int32? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size, info.pbi_ppid > 1 else { return nil }
        return Int32(info.pbi_ppid)
    }
}

/// The branch a folder's repository is on, read from its HEAD: for the
/// agents that do not say, and for every session on this Mac so a checkout
/// inside it shows on its row (#233). Cached a little, since every tick asks.
final class AgentGitBranch {
    /// What a checkout is on.
    enum Head: Equatable {
        case branch(String)
        /// HEAD names a commit, not a branch.
        case detached
        /// No repository found, or its HEAD couldn't be read.
        case unknown

        var branch: String? { if case .branch(let name) = self { return name } else { return nil } }
    }

    private var cache: [String: (head: Head, at: Date)] = [:]
    static let freshFor: TimeInterval = 3

    func head(of folder: String, now: Date = Date()) -> Head {
        guard !folder.isEmpty else { return .unknown }
        if let known = cache[folder], now.timeIntervalSince(known.at) < Self.freshFor { return known.head }
        let head = Self.head(folder)
        cache[folder] = (head, now)
        if cache.count > 64 { cache = cache.filter { now.timeIntervalSince($0.value.at) < Self.freshFor } }
        return head
    }

    func branch(of folder: String, now: Date = Date()) -> String? {
        head(of: folder, now: now).branch
    }

    static func read(_ folder: String) -> String? { head(folder).branch }

    /// A session's branch as its checkout has it now: a detached HEAD shows
    /// none, and a folder this Mac can't read keeps what the agent said.
    static func live(reported: String?, head: Head) -> String? {
        switch head {
        case .branch(let name): return name
        case .detached: return nil
        case .unknown: return reported
        }
    }

    static func head(_ folder: String) -> Head {
        guard !folder.isEmpty else { return .unknown }
        var url = URL(fileURLWithPath: folder, isDirectory: true)
        for _ in 0..<32 {
            let git = url.appending(path: ".git")
            var isFolder: ObjCBool = false
            if FileManager.default.fileExists(atPath: git.path, isDirectory: &isFolder) {
                var head = git.appending(path: "HEAD")
                if !isFolder.boolValue {
                    // A worktree's .git file names its own folder.
                    guard let text = try? String(contentsOf: git, encoding: .utf8),
                          let line = text.split(whereSeparator: \.isNewline).first, line.hasPrefix("gitdir:") else { return .unknown }
                    let path = line.dropFirst("gitdir:".count).trimmingCharacters(in: .whitespaces)
                    let gitdir = path.hasPrefix("/") ? URL(fileURLWithPath: path) : url.appending(path: path)
                    head = gitdir.appending(path: "HEAD")
                }
                guard let text = try? String(contentsOf: head, encoding: .utf8) else { return .unknown }
                let line = text.trimmingCharacters(in: .whitespacesAndNewlines)
                if line.hasPrefix("ref: refs/heads/") {
                    let name = String(line.dropFirst("ref: refs/heads/".count))
                    return name.isEmpty ? .unknown : .branch(name)
                }
                // An odd ref still isn't a commit: say nothing new.
                return line.hasPrefix("ref:") ? .unknown : .detached
            }
            let parent = url.deletingLastPathComponent()
            if parent.path == url.path { return .unknown }
            url = parent
        }
        return .unknown
    }
}
