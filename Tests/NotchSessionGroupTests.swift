// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Sessions grouped by project or agent, and the branch label (#233).
enum NotchSessionGroupTests {
    static func run(_ suite: TestSuite) {
        suite.run("session groups") { groups(suite) }
        suite.run("session group folds") { folds(suite) }
        suite.run("session branch label") { branches(suite) }
        suite.run("session live branch") { live(suite) }
    }

    static let at = Date(timeIntervalSince1970: 1000)

    static func session(_ id: String, _ project: String, _ provider: ClaudeSessionProvider, _ phase: ClaudeSessionPhase,
                        since: TimeInterval = -100, remote: String? = nil) -> ClaudeSession {
        var session = ClaudeSession(id: id, pid: 7, cwd: "/x/\(project)", project: project, name: project,
                                    host: remote.map { .remote(alias: $0, tmux: true) } ?? .terminal(tty: "/dev/ttys001"),
                                    phase: phase, phaseSince: at.addingTimeInterval(since), turnStarted: nil,
                                    timeline: [], subagents: [], tokens: AgentTokens(), cost: 0, hooked: false)
        session.provider = provider
        return session
    }

    static func groups(_ suite: TestSuite) {
        let list = NotchClaudeSessionsSupport.sorted([
            session("1", "web", .claude, .generating, since: -10),
            session("2", "api", .codex, .finished, since: -50),
            session("3", "api", .claude, .waitingPermission, since: -40),
            session("4", "web", .gemini, .idle),
            session("5", "api", .claude, .generating, since: -20, remote: "devbox"),
        ])
        let byProject = NotchSessionGroups.groups(list, by: .project)
        let shape = byProject.map { "\($0.title)@\($0.remote ?? ""):\($0.sessions.map(\.id).joined(separator: ","))" }
        suite.expect(shape == ["api@:3,2", "api@devbox:5", "web@:1,4"],
                     "a waiting group leads, a host's project stands apart, rows keep their order: \(shape)")
        suite.expect(byProject.map(\.waiting) == [1, 0, 0], "only the group with a request counts one waiting")
        suite.expect(Set(byProject.map(\.key)).count == 3, "every project group has its own key")
        suite.expect(byProject.contains { $0.key == "project:devbox:api" }, "a host's key names the host")

        let byAgent = NotchSessionGroups.groups(list, by: .agent)
        let agents = byAgent.map { "\($0.title):\($0.sessions.count)" }
        suite.expect(agents == ["Claude Code:3", "Codex:1", "Gemini CLI:1"], "agents by their own names: \(agents)")
        suite.expect(byAgent.first?.sessions.first?.id == list.first?.id, "groups and the list agree on who comes first")
        suite.expect(NotchSessionGroups.groups(list, by: .none).isEmpty, "None keeps one list")
    }

    static func folds(_ suite: TestSuite) {
        suite.expect(NotchSessionGroups.toggled([], key: "a") == ["a"], "a group folds")
        suite.expect(NotchSessionGroups.toggled(["a", "b"], key: "a") == ["b"], "and opens again")
        let many = (0..<NotchSessionGroups.collapsedLimit).map { "k\($0)" }
        let next = NotchSessionGroups.toggled(many, key: "new")
        suite.expect(next.count == NotchSessionGroups.collapsedLimit && next.last == "new" && !next.contains("k0"),
                     "only the latest folds are kept")
    }

    static func branches(_ suite: TestSuite) {
        typealias L = NotchSessionGroups.BranchLabel
        suite.expect(NotchSessionGroups.branchLabel(branch: "feat/login", cwd: "/r")
                     == L(branch: "feat/login", worktree: nil, inWorktree: false, full: "feat/login"),
                     "a branch alone outside a worktree")
        suite.expect(NotchSessionGroups.branchLabel(branch: nil, cwd: "/r") == nil
                     && NotchSessionGroups.branchLabel(branch: "  ", cwd: "/notes") == nil,
                     "nothing on a detached HEAD or outside git")
        suite.expect(NotchSessionGroups.branchLabel(branch: "worktree-fix", cwd: "/r/.claude/worktrees/fix")
                     == L(branch: "worktree-fix", worktree: nil, inWorktree: true, full: "worktree-fix"),
                     "a worktree named once when its branch carries the name")
        suite.expect(NotchSessionGroups.branchLabel(branch: "openbar/docs", cwd: "/r/.openbar/worktrees/docs")?.worktree == nil,
                     "the island's own worktree branch too")
        suite.expect(NotchSessionGroups.branchLabel(branch: "main", cwd: "/r/.claude/worktrees/fix/src")
                     == L(branch: "main", worktree: "fix", inWorktree: true, full: "fix · main"),
                     "another branch checked out in a worktree names both")
        suite.expect(NotchSessionGroups.branchLabel(branch: nil, cwd: "/r/.openbar/worktrees/spike")?.full == "spike",
                     "a detached worktree goes by its folder")
        let a = NotchSessionGroups.branchLabel(branch: "worktree-a", cwd: "/r/.claude/worktrees/a")?.full
        let b = NotchSessionGroups.branchLabel(branch: "worktree-b", cwd: "/r/.claude/worktrees/b")?.full
        suite.expect(a != nil && a != b, "two worktrees of one repository read apart")
    }

    static func live(_ suite: TestSuite) {
        let root = FileManager.default.temporaryDirectory.appending(path: "ob-head-\(UUID().uuidString)")
        let repo = root.appending(path: "repo")
        let gitdir = repo.appending(path: ".git/worktrees/wt")
        try? FileManager.default.createDirectory(at: gitdir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let head = repo.appending(path: ".git/HEAD")
        func write(_ text: String, _ url: URL) { try? text.write(to: url, atomically: true, encoding: .utf8) }

        write("ref: refs/heads/main\n", head)
        suite.expect(AgentGitBranch.head(repo.path) == .branch("main"), "a checkout reads its branch")
        write("ref: refs/heads/feat/y\n", head)
        suite.expect(AgentGitBranch.head(repo.path) == .branch("feat/y"), "a checkout inside the session shows")
        write("3f2c1a9e8d7b6c5a4f3e2d1c0b9a8f7e6d5c4b3a\n", head)
        suite.expect(AgentGitBranch.head(repo.path) == .detached, "a detached HEAD reads as such")
        let wt = root.appending(path: "wt")
        try? FileManager.default.createDirectory(at: wt, withIntermediateDirectories: true)
        write("gitdir: \(gitdir.path)\n", wt.appending(path: ".git"))
        write("ref: refs/heads/worktree-wt\n", gitdir.appending(path: "HEAD"))
        suite.expect(AgentGitBranch.head(wt.path) == .branch("worktree-wt"), "a linked worktree reads its own HEAD")
        suite.expect(AgentGitBranch.head("") == .unknown && AgentGitBranch.head(root.appending(path: "none").path) == .unknown,
                     "no repository reads as unknown")

        suite.expect(AgentGitBranch.live(reported: "a", head: .branch("b")) == "b", "the checkout wins")
        suite.expect(AgentGitBranch.live(reported: "a", head: .detached) == nil, "detached shows nothing")
        suite.expect(AgentGitBranch.live(reported: "a", head: .unknown) == "a", "an unreadable folder keeps the agent's word")

        // The cache reads again once stale, so a checkout follows within seconds.
        let cache = AgentGitBranch()
        write("ref: refs/heads/one\n", head)
        suite.expect(cache.head(of: repo.path, now: at) == .branch("one"), "first read")
        write("ref: refs/heads/two\n", head)
        suite.expect(cache.head(of: repo.path, now: at.addingTimeInterval(1)) == .branch("one"), "fresh reads are cached")
        suite.expect(cache.head(of: repo.path, now: at.addingTimeInterval(AgentGitBranch.freshFor + 0.1)) == .branch("two"),
                     "a stale read looks again")
    }
}
