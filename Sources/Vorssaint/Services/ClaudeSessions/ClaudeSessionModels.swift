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

    var isRunning: Bool { finished == nil }
}

/// A tool waiting for the person's permission, asked of the island by the hook.
struct ClaudePermission: Equatable {
    let requestID: String
    let tool: String
    /// What the tool would do. Shown even with previews off, since nobody
    /// should allow a command they cannot read, and dropped once answered.
    let summary: String?
    /// Claude Code offered rules that would stop it asking again.
    let offersAlways: Bool
    let since: Date
}

struct ClaudeSubagent: Equatable, Identifiable {
    let id: String
    var type: String
    /// The task it was given; nil unless content previews are on.
    var description: String?
    let started: Date
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

    static func clip(_ text: String) -> String {
        text.count > limit ? String(text.prefix(limit - 1)) + "…" : text
    }
}
