// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The made-up session onboarding's Try it shows in the island (#168): a
/// Claude Code session asking to run the tests, the request people meet
/// first. No agent stands behind it, so it has no process, no terminal and
/// no transcript, and answering it goes nowhere.
enum ClaudeSessionSample {
    static let id = "openbar-sample-session"
    static let project = "sample-app"
    static let command = "npm test"

    static func waiting(round: Int, home: URL, now: Date) -> ClaudeSession {
        let permission = ClaudePermission(requestID: "openbar-sample-\(round)", tool: "Bash", summary: command,
                                          detail: command, offersAlways: true, since: now)
        return ClaudeSession(id: id, pid: 0, cwd: home.appending(path: "Projects/\(project)").path,
                             project: project, name: project, branch: "main", model: nil, host: .unknown,
                             phase: .waitingPermission, phaseSince: now, turnStarted: now.addingTimeInterval(-12),
                             lastActive: now, waitingFor: nil, currentTool: nil, permission: permission,
                             timeline: [], subagents: [], tasks: nil, tokens: AgentTokens(), cost: 0,
                             title: nil, lastText: nil, hooked: true)
    }

    /// Right after the answer: an allowed command runs, a denied one leaves
    /// the turn over.
    static func answered(_ session: ClaudeSession, allowed: Bool, now: Date) -> ClaudeSession {
        var session = session
        session.permission = nil
        session.phaseSince = now
        session.lastActive = now
        if allowed {
            session.phase = .runningTool
            session.currentTool = ClaudeToolUse(id: "openbar-sample-tool", name: "Bash", summary: command, started: now)
        } else {
            session.phase = .finished
        }
        return session
    }

    static func finished(_ session: ClaudeSession, now: Date) -> ClaudeSession {
        var session = session
        if var tool = session.currentTool {
            tool.finished = now
            session.timeline.append(tool)
        }
        session.currentTool = nil
        session.phase = .finished
        session.phaseSince = now
        session.lastActive = now
        return session
    }
}
