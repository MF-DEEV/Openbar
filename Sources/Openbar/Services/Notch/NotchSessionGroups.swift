// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// How the Sessions list is laid out (#233): one list, or grouped.
enum NotchSessionGrouping: String, CaseIterable {
    case none, project, agent
}

/// Sessions of one project, or of one agent, under one header (#233).
struct NotchSessionGroup: Equatable, Identifiable {
    enum Kind: Equatable { case project, agent }

    /// Stable across launches, so a folded group stays folded.
    let key: String
    let kind: Kind
    /// The project's or the agent's name.
    let title: String
    /// The agent, for a group of one agent's sessions.
    var provider: ClaudeSessionProvider? = nil
    /// The host, for a project on another machine.
    var remote: String? = nil
    /// In the list's order.
    var sessions: [ClaudeSession] = []
    /// How many of them wait on the person.
    var waiting = 0

    var id: String { key }
}

/// Grouping and the branch label, shared by the island and its tests
/// (windows/src/sessions/groups.ts on Windows and Linux).
enum NotchSessionGroups {
    /// How many folded groups are remembered; the oldest go first.
    static let collapsedLimit = 64

    /// The key a session's group goes by. A project on another machine is
    /// its own group, under its host.
    static func key(_ session: ClaudeSession, by grouping: NotchSessionGrouping) -> String {
        switch grouping {
        case .agent: return "agent:\(session.provider.rawValue)"
        case .project, .none: return "project:\(session.host.remoteAlias ?? ""):\(session.project)"
        }
    }

    /// The sessions in groups. `ordered` is the list as it shows ungrouped;
    /// groups come in the order their first session does there, so every
    /// group with someone waiting is on top, each keeping the list's order.
    static func groups(_ ordered: [ClaudeSession], by grouping: NotchSessionGrouping) -> [NotchSessionGroup] {
        guard grouping != .none else { return [] }
        var groups: [NotchSessionGroup] = []
        var index: [String: Int] = [:]
        for session in ordered {
            let key = key(session, by: grouping)
            if index[key] == nil {
                index[key] = groups.count
                groups.append(grouping == .agent
                    ? NotchSessionGroup(key: key, kind: .agent, title: session.provider.displayName, provider: session.provider)
                    : NotchSessionGroup(key: key, kind: .project, title: session.project, remote: session.host.remoteAlias))
            }
            guard let at = index[key] else { continue }
            groups[at].sessions.append(session)
            if session.phase.isWaiting { groups[at].waiting += 1 }
        }
        return groups
    }

    /// Folds a group or opens it again. Keys of groups that are gone stay,
    /// so they come back folded.
    static func toggled(_ collapsed: [String], key: String) -> [String] {
        if collapsed.contains(key) { return collapsed.filter { $0 != key } }
        return Array((collapsed + [key]).suffix(collapsedLimit))
    }

    /// What the row says of where a session works: its branch, and its
    /// worktree's name when the branch doesn't already carry it
    /// (`worktree-<name>` from Claude Code, `openbar/<name>` from the island).
    /// Nothing on a detached HEAD or outside git, unless a worktree names the
    /// checkout.
    struct BranchLabel: Equatable {
        /// Nil on a detached HEAD.
        var branch: String?
        /// The worktree's name, unless the branch already says it.
        var worktree: String?
        var inWorktree: Bool
        /// Everything, uncut, for the pointer and VoiceOver.
        var full: String
    }

    static func branchLabel(branch rawBranch: String?, cwd: String) -> BranchLabel? {
        let branch = rawBranch.map { $0.trimmingCharacters(in: .whitespaces) }.flatMap { $0.isEmpty ? nil : $0 }
        let worktree = SessionWorktrees.worktreeName(cwd)
        guard branch != nil || worktree != nil else { return nil }
        var named = false
        if let branch, let worktree {
            named = branch == worktree || branch.hasSuffix("-\(worktree)") || branch.hasSuffix("/\(worktree)")
        }
        let shown = named ? nil : worktree
        let full = [shown, branch].compactMap { $0 }.joined(separator: " · ")
        return BranchLabel(branch: branch, worktree: shown, inWorktree: worktree != nil, full: full)
    }

    static func branchLabel(_ session: ClaudeSession) -> BranchLabel? {
        branchLabel(branch: session.branch, cwd: session.cwd)
    }
}
