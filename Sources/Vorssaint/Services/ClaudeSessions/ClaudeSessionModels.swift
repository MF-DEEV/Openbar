// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Where a running Claude Code session is in its work, as the Sessions
/// section shows it.
enum ClaudeSessionPhase: String, Equatable {
    /// Writing a reply or thinking.
    case generating
    /// A tool the model called is running, like a shell command.
    case runningTool
    /// A tool waits for the person to allow it.
    case waitingPermission
    /// A question, dialog or other prompt waits for the person.
    case waitingInput
    /// The turn ended and the person has not looked at it yet.
    case finished
    case idle

    var isWorking: Bool { self == .generating || self == .runningTool }
    var isWaiting: Bool { self == .waitingPermission || self == .waitingInput }
}

/// The state a session writes to `~/.claude/sessions/<pid>.json`.
enum ClaudeRegistryStatus: String, Equatable {
    case idle, busy, waiting, shell
}

/// One running session, read from Claude Code's own registry of processes.
struct ClaudeSessionRecord: Equatable {
    let pid: Int32
    let sessionID: String
    let cwd: String
    let name: String
    let status: ClaudeRegistryStatus?
    /// Why a waiting session waits, like "permission prompt" or "input needed".
    let waitingFor: String?
    /// When `status` last changed.
    let statusUpdated: Date?
    let started: Date?
    let version: String
    var host: ClaudeSessionHost = .unknown

    static let permissionPrompt = "permission prompt"
    /// Any dialog in the terminal, including one the person opened, like
    /// /usage or /config.
    static let dialogOpen = "dialog open"
}

/// The app showing a session's terminal, which Open brings forward and a
/// reply is typed into.
enum ClaudeSessionHost: Equatable {
    /// A tab of Terminal, found by its device.
    case terminal(tty: String)
    /// The integrated terminal of Antigravity, with the port of the IDE
    /// connection when the session has one.
    case antigravity(port: Int?)
    /// Any other app, by bundle identifier.
    case app(String)
    case unknown

    var acceptsReplies: Bool {
        switch self {
        case .terminal, .antigravity: return true
        case .app, .unknown: return false
        }
    }
}

struct ClaudeToolUse: Equatable, Identifiable {
    let id: String
    let name: String
    /// A short reading of what the tool works on, like a command or a file
    /// name; nil unless content previews are on.
    var summary: String?
    let started: Date
    var finished: Date?
    var failed = false
    /// Tells this call apart from another of the same tool by what it was
    /// given, without keeping what it was given.
    var inputKey: Int? = nil

    var isRunning: Bool { finished == nil }
}

/// A tool waiting for the person's permission, asked of the island by the hook.
struct ClaudePermission: Equatable {
    let requestID: String
    let tool: String
    /// One line on what the tool would do, for the list and notices.
    let summary: String?
    /// Everything the tool would do: the whole command, path or arguments.
    /// Shown even with previews off, since nobody should allow what they
    /// cannot read, and dropped once answered.
    var detail: String = ""
    /// Claude Code offered rules that would stop it asking again.
    let offersAlways: Bool
    let since: Date
    /// The tool call the request is for; its result answers it. Empty for a
    /// subagent's, whose tools are not followed.
    var toolID: String? = nil
    /// Matches the tool call whose input this is.
    var inputKey: Int? = nil

    /// A request short enough to read whole on one line of the list.
    var fitsOnALine: Bool { !detail.contains("\n") && detail.count <= ClaudeToolSummary.quickDecisionLimit }
}

struct ClaudeSubagent: Equatable, Identifiable {
    /// The type of a subagent whose own description was not read yet.
    static let untyped = "Agent"

    let id: String
    var type: String
    /// The task it was given; nil unless content previews are on.
    var description: String?
    var started: Date
    var finished: Date?

    var isRunning: Bool { finished == nil }
}

struct ClaudeTaskProgress: Equatable {
    var done = 0
    var total = 0
    /// The task under way; nil unless content previews are on.
    var current: String?

    var fraction: Double { total > 0 ? Double(done) / Double(total) : 0 }
}

/// Everything the island shows about one session.
struct ClaudeSession: Equatable, Identifiable {
    let id: String
    let pid: Int32
    let cwd: String
    let project: String
    var name: String
    var branch: String?
    var model: String?
    var host: ClaudeSessionHost
    var phase: ClaudeSessionPhase
    var phaseSince: Date
    /// When the turn under way, or the last one, began.
    var turnStarted: Date?
    var waitingFor: String?
    var currentTool: ClaudeToolUse?
    var permission: ClaudePermission?
    /// The latest tools of the main agent, oldest first.
    var timeline: [ClaudeToolUse]
    var subagents: [ClaudeSubagent]
    var tasks: ClaudeTaskProgress?
    var tokens: AgentTokens
    var cost: Double
    /// Gated by content previews.
    var title: String?
    var lastText: String?
    /// The hooks report this session, so permissions can be answered here.
    var hooked: Bool
    /// What was typed and answered, oldest first; gated by content previews.
    var chat: [ClaudeChatMessage] = []
}

/// One message of a session's conversation.
struct ClaudeChatMessage: Equatable, Identifiable {
    enum Role: Equatable { case user, assistant }
    let id: Int
    let role: Role
    let text: String
    let at: Date
}

struct ClaudeSessionCounts: Equatable {
    var working = 0
    var waiting = 0
    var finished = 0

    var isEmpty: Bool { working + waiting + finished == 0 }

    init(working: Int = 0, waiting: Int = 0, finished: Int = 0) {
        self.working = working
        self.waiting = waiting
        self.finished = finished
    }

    init(_ sessions: [ClaudeSession]) {
        for session in sessions {
            if session.phase.isWorking { working += 1 }
            if session.phase.isWaiting { waiting += 1 }
            if session.phase == .finished { finished += 1 }
        }
    }
}

/// A change worth a notice in the island.
enum ClaudeSessionEvent: Equatable {
    case needsPermission(id: String, tool: String)
    case needsInput(id: String)
    case finished(id: String, duration: TimeInterval?)
}

/// Whether what sessions say and do may be decoded for the island.
struct ClaudeContentPolicy: Equatable {
    var previews = false
}

// MARK: Facts

/// What one transcript line tells about its session. Text, titles and tool
/// arguments appear only when the content policy allows them.
enum ClaudeTranscriptFact: Equatable {
    case turnBegan(Date)
    case turnEnded(Date, completed: Bool)
    case toolStarted(ClaudeToolUse)
    case toolFinished(id: String, at: Date, failed: Bool)
    case text(String, at: Date)
    /// A prompt the person typed, read only when previews are allowed.
    case prompt(String, at: Date)
    case title(String)
    case branch(String)
    case model(String)
    /// Streamed replies repeat their usage per block; the key merges them.
    case usage(key: String, tokens: AgentTokens, cost: Double)
    case subagentActivity(id: String, at: Date)
    case subagentEnded(id: String, at: Date)
    case subagentInfo(id: String, type: String, description: String?)
    case tasks(ClaudeTaskProgress)
}

/// One hook event Claude Code sent while the session ran.
struct ClaudeHookFact: Equatable {
    enum Event: Equatable {
        case sessionStarted
        case sessionEnded
        case promptSubmitted
        case toolStarted(ClaudeToolUse)
        case toolFinished(id: String, failed: Bool)
        case permissionRequested(ClaudePermission)
        case notification(type: String)
        case stopped(lastText: String?)
        case subagentStarted(id: String, type: String)
        case subagentStopped(id: String)
        case other(String)
    }

    let sessionID: String
    let date: Date
    let event: Event
    /// The subagent the event comes from; nil for the main agent.
    var agentID: String? = nil
}

// MARK: Tool summaries

enum ClaudeToolSummary {
    static let limit = 80

    /// A one-line reading of a tool call, built only from the fields that say
    /// what it touches.
    static func summary(name: String, input: [String: Any]) -> String? {
        func text(_ key: String) -> String? {
            guard let value = input[key] as? String else { return nil }
            let line = value.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            return trimmed.isEmpty ? nil : clip(trimmed)
        }
        switch name {
        case "Bash": return text("command")
        case "Read", "Edit", "Write", "MultiEdit", "NotebookEdit":
            return (text("file_path") ?? text("notebook_path")).map { ($0 as NSString).lastPathComponent }
        case "Grep", "Glob": return text("pattern")
        case "WebFetch":
            return (input["url"] as? String).flatMap { URL(string: $0)?.host } ?? text("url")
        case "WebSearch": return text("query")
        case "Task", "Agent": return text("description")
        case "Skill": return text("skill")
        default: return text("description")
        }
    }

    static let quickDecisionLimit = 60

    /// The same for the same tool and input within one run of the app.
    static func inputKey(name: String, input: [String: Any]) -> Int {
        var hasher = Hasher()
        hasher.combine(name)
        hasher.combine(permissionDetail(name: name, input: input))
        return hasher.finalize()
    }
    static let detailLimit = 8000

    /// All of what a tool asks to do, for the person to read before allowing
    /// it: every line of a command, a whole path or address, or each
    /// argument. Only text past `detailLimit` is left out, and that says so.
    static func permissionDetail(name: String, input: [String: Any]) -> String {
        let text: String
        switch name {
        case "Bash": text = input["command"] as? String ?? arguments(input)
        case "Read", "Edit", "Write", "MultiEdit", "NotebookEdit":
            text = (input["file_path"] as? String) ?? (input["notebook_path"] as? String) ?? arguments(input)
        case "WebFetch": text = input["url"] as? String ?? arguments(input)
        default: text = arguments(input)
        }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > detailLimit else { return trimmed }
        return String(trimmed.prefix(detailLimit)) + "\n… \(trimmed.count - detailLimit) more characters"
    }

    /// Each argument on its own line, in a steady order.
    static func arguments(_ input: [String: Any]) -> String {
        input.keys.sorted().map { key in
            let value = input[key]
            let shown: String
            if let text = value as? String {
                shown = text
            } else if let value, JSONSerialization.isValidJSONObject(value),
                      let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
                shown = String(decoding: data, as: UTF8.self)
            } else {
                shown = value.map { "\($0)" } ?? ""
            }
            return "\(key): \(shown)"
        }.joined(separator: "\n")
    }

    static func clip(_ text: String) -> String {
        text.count > limit ? String(text.prefix(limit - 1)) + "…" : text
    }
}
