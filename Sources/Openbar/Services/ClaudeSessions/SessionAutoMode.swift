// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Sessions the island lets run without asking (#234): one started in its
/// agent's no-ask mode (Claude Code's bypass permissions, Codex with
/// approvals off) and, for whoever opts in, every session of an agent.
/// A permission such a session asks for is allowed at once, with no card,
/// sound or wait, and listed on its page; questions still come to the island.
enum SessionAutoMode {
    /// Claude Code's mode that skips its permission prompts. It still asks
    /// for questions and for commands a settings `ask` rule names.
    static let claudeBypass = "bypassPermissions"
    /// Codex's approval policy that never asks.
    static let codexNever = "never"

    /// Whether Claude Code's `permission_mode` is its no-ask mode. Its auto
    /// mode is not: it asks on purpose when its classifier holds back.
    static func isAuto(claudeMode: String?) -> Bool { claudeMode == claudeBypass }

    /// Whether a Codex thread's approval policy never asks.
    static func isAuto(codexApproval policy: String?) -> Bool { policy == codexNever }

    /// The agents whose no-ask mode the island can tell.
    static let modeAgents: [ClaudeSessionProvider] = [.claude, .codex]
    /// The agents whose permissions the island answers itself.
    static let proceedAgents: [ClaudeSessionProvider] = [.claude, .codex, .opencode, .vibe]

    static func autoRunKey(_ provider: ClaudeSessionProvider) -> String? {
        switch provider {
        case .claude: return DefaultsKey.notchSessionsAutoRunClaude
        case .codex: return DefaultsKey.notchSessionsAutoRunCodex
        default: return nil
        }
    }

    static func alwaysProceedKey(_ provider: ClaudeSessionProvider) -> String? {
        switch provider {
        case .claude: return DefaultsKey.notchSessionsAlwaysProceedClaude
        case .codex: return DefaultsKey.notchSessionsAlwaysProceedCodex
        case .opencode: return DefaultsKey.notchSessionsAlwaysProceedOpenCode
        case .vibe: return DefaultsKey.notchSessionsAlwaysProceedVibe
        default: return nil
        }
    }

    struct Settings: Equatable {
        /// Agents whose no-ask sessions run without the island; on by default.
        var autoRun: Set<ClaudeSessionProvider> = Set(SessionAutoMode.modeAgents)
        /// Agents whose every permission is allowed; off unless chosen.
        var alwaysProceed: Set<ClaudeSessionProvider> = []

        init(autoRun: Set<ClaudeSessionProvider> = Set(SessionAutoMode.modeAgents),
             alwaysProceed: Set<ClaudeSessionProvider> = []) {
            self.autoRun = autoRun
            self.alwaysProceed = alwaysProceed
        }

        init(defaults: UserDefaults) {
            autoRun = Set(SessionAutoMode.modeAgents.filter { provider in
                autoRunKey(provider).map { defaults.object(forKey: $0) as? Bool ?? true } ?? false
            })
            alwaysProceed = Set(SessionAutoMode.proceedAgents.filter { provider in
                alwaysProceedKey(provider).map { defaults.object(forKey: $0) as? Bool ?? false } ?? false
            })
        }

        /// Why a permission of this agent's session goes through without the
        /// island, or nil when the island asks.
        func passes(_ provider: ClaudeSessionProvider, autoMode: Bool) -> ClaudeLetThrough.Reason? {
            if autoMode, autoRun.contains(provider) { return .autoMode }
            if alwaysProceed.contains(provider) { return .alwaysProceed }
            return nil
        }
    }

    /// The settings as last read, for the queues that answer requests.
    final class Box: @unchecked Sendable {
        private let lock = NSLock()
        private var value = Settings()

        var settings: Settings {
            get { lock.withLock { value } }
            set { lock.withLock { value = newValue } }
        }
    }

    static let shared = Box()

    /// What the island let through, by session, newest last; written from the
    /// hook queue and Codex's connections alike.
    final class Log: @unchecked Sendable {
        static let kept = 30
        private let lock = NSLock()
        private var entries: [String: [ClaudeLetThrough]] = [:]

        func add(_ entry: ClaudeLetThrough, to session: String) {
            lock.withLock {
                var list = entries[session] ?? []
                list.append(entry)
                if list.count > Self.kept { list.removeFirst(list.count - Self.kept) }
                entries[session] = list
            }
        }

        func entries(of session: String) -> [ClaudeLetThrough] { lock.withLock { entries[session] ?? [] } }

        /// Gives each session what was let through for it.
        func decorate(_ sessions: [ClaudeSession]) -> [ClaudeSession] {
            lock.withLock {
                sessions.map { session in
                    guard let list = entries[session.id], list != session.letThrough else { return session }
                    var session = session
                    session.letThrough = list
                    return session
                }
            }
        }

        /// Forgets sessions no longer listed, once a while passed since their
        /// last entry: a session may be heard before it is listed.
        func prune(listed ids: Set<String>, now: Date) {
            lock.withLock {
                entries = entries.filter { id, list in
                    ids.contains(id) || (list.last.map { now.timeIntervalSince($0.at) < 600 } ?? false)
                }
            }
        }

        func clear() { lock.withLock { entries = [:] } }
    }

    static let log = Log()
}

/// One permission the island allowed without asking, as the session's page lists it.
struct ClaudeLetThrough: Equatable, Identifiable {
    enum Reason: Equatable {
        /// The session runs in its agent's no-ask mode.
        case autoMode
        /// The person chose Always proceed for the agent.
        case alwaysProceed
    }

    let id: String
    let tool: String
    let summary: String?
    let at: Date
    let reason: Reason

    init(id: String, tool: String, summary: String?, at: Date, reason: Reason) {
        self.id = id
        self.tool = tool
        self.summary = summary
        self.at = at
        self.reason = reason
    }

    init(_ permission: ClaudePermission, at: Date, reason: Reason) {
        self.init(id: permission.requestID, tool: permission.tool, summary: permission.summary, at: at, reason: reason)
    }
}
