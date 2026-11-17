// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// A session's own git worktree: names, paths, arguments, and real `git`
/// in a throwaway repository.
enum SessionWorktreeTests {
    static func run(_ suite: TestSuite) {
        suite.run("worktree names") { names(suite) }
        suite.run("worktree paths") { paths(suite) }
        suite.run("worktree start arguments") { arguments(suite) }
        suite.run("claude rm keeping a worktree") { claudeKept(suite) }
        suite.run("worktree in a real repository") { repository(suite) }
    }

    static func names(_ suite: TestSuite) {
        suite.expect(SessionWorktrees.slug("Fix the login bug!") == "fix-the-login-bug", "a prompt becomes a slug")
        suite.expect(SessionWorktrees.slug("  Zażółć gęślą jaźń  ") == "zazolc-gesla-jazn", "accents fold to ASCII")
        suite.expect(SessionWorktrees.slug("Straße Øre") == "strasse-ore", "letters of their own become ASCII")
        suite.expect(SessionWorktrees.slug("日本語だけ") == "", "a prompt with no ASCII letters gives nothing")
        suite.expect(SessionWorktrees.slug("Please refactor the payments worker so retries back off exponentially")
            == "please-refactor-the-payments-worker-so", "a long prompt stops at a whole word")
        suite.expect(SessionWorktrees.slug(String(repeating: "a", count: 60)).count == 40, "one long word is cut")
        suite.expect(SessionWorktrees.slug("--rm -rf /") == "rm-rf", "no leading dash or slash survives")
        suite.expect(SessionWorktrees.unique("fix", taken: []) == "fix", "a free name stays")
        suite.expect(SessionWorktrees.unique("fix", taken: ["fix", "fix-2"]) == "fix-3", "a taken name gets a number")
        suite.expect(SessionWorktrees.name(custom: nil, prompt: "Fix login", taken: ["fix-login"]) == "fix-login-2",
                     "the prompt's name is deduplicated")
        suite.expect(SessionWorktrees.name(custom: "My Branch", prompt: "Fix login", taken: []) == "my-branch",
                     "a typed name wins and is made safe")
        suite.expect(SessionWorktrees.name(custom: "  ", prompt: "", taken: []) == "session", "nothing at all names it session")
        suite.expect(SessionWorktrees.name(custom: "!!!", prompt: "日本", taken: ["session"]) == "session-2",
                     "the fallback is deduplicated too")
    }

    static func paths(_ suite: TestSuite) {
        suite.expect(SessionWorktrees.repository(of: "/Users/me/app/.claude/worktrees/fix") == "/Users/me/app",
                     "Claude's worktree belongs to its repository")
        suite.expect(SessionWorktrees.repository(of: "/Users/me/app/.openbar/worktrees/fix/web") == "/Users/me/app",
                     "the island's worktree belongs to its repository")
        suite.expect(SessionWorktrees.repository(of: "/Users/me/app/.gemini/worktrees/x") == "/Users/me/app",
                     "Gemini's worktree belongs to its repository")
        suite.expect(SessionWorktrees.repository(of: "/Users/me/app") == "/Users/me/app", "a plain folder is itself")
        suite.expect(AgentLogParser.projectName("/Users/me/app/.openbar/worktrees/fix") == "app", "the project is the repository")
        suite.expect(AgentLogParser.projectName("/Users/me/app/.claude/worktrees/fix/") == "app", "Claude's too")
        suite.expect(SessionWorktrees.worktreeName("/Users/me/app/.openbar/worktrees/fix-login/web") == "fix-login",
                     "a session deeper in the worktree names it")
        suite.expect(SessionWorktrees.worktreeName("/Users/me/app/.claude/worktrees/fix") == "fix", "Claude's worktree has a name")
        suite.expect(SessionWorktrees.worktreeName("/Users/me/app") == nil, "a plain folder has no worktree")
        suite.expect(SessionWorktrees.worktreeName("/Users/me/app/.openbar/worktrees/") == nil, "an empty name is none")
        suite.expect(SessionWorktrees.worktreeFolder("/r/.openbar/worktrees/fix/web/src") == "/r/.openbar/worktrees/fix",
                     "the worktree's own folder")
        suite.expect(SessionWorktrees.isOwn("/r/.openbar/worktrees/fix"), "the island's worktree is its own")
        suite.expect(!SessionWorktrees.isOwn("/r/.claude/worktrees/fix"), "Claude's is not the island's")
        suite.expect(SessionWorktrees.inside("/r/.openbar/worktrees/a", for: "/r/web", root: "/r") == "/r/.openbar/worktrees/a/web",
                     "a folder deeper in the repository is found in the worktree")
        suite.expect(SessionWorktrees.inside("/r/.openbar/worktrees/a", for: "/r", root: "/r") == "/r/.openbar/worktrees/a",
                     "the repository's root is the worktree's")
        suite.expect(SessionWorktrees.inside("/private/tmp/r/.openbar/worktrees/a", for: "/tmp/r/web", root: "/private/tmp/r")
            == "/private/tmp/r/.openbar/worktrees/a/web", "/tmp and /private/tmp are one folder")
        suite.expect(SessionWorktrees.inside("/r/.openbar/worktrees/a", for: "/rest", root: "/r") == "/r/.openbar/worktrees/a",
                     "a folder that only starts with the root's name is not inside it")
    }

    static func arguments(_ suite: TestSuite) {
        suite.expect(ClaudeCLI.startArguments(prompt: "go", background: true, model: "opus", worktree: "fix")
            == ["--bg", "--model", "opus", "--worktree", "fix", "--", "go"], "Claude makes its own worktree")
        suite.expect(ClaudeCLI.startArguments(prompt: "go", background: false, worktree: "fix")
            == ["--worktree", "fix", "--", "go"], "in a terminal too")
        suite.expect(ClaudeCLI.startArguments(prompt: "-x", background: true, worktree: "")
            == ["--bg", "--", "-x"], "an empty name adds nothing")
        suite.expect(ClaudeCLI.startArguments(prompt: nil, background: false, worktree: nil) == [], "no worktree, no prompt")
    }

    static func claudeKept(_ suite: TestSuite) {
        // As Claude Code 2.1.318 prints them.
        let unsaved = """
        kept f9db0992 — its worktree is still at “/private/tmp/r/.claude/worktrees/wt-dirty”
          The worktree has uncommitted changes. Deleting it would lose them.
          commit or stash them and run 'claude rm f9db0992' again, or delete the session from 'claude agents' (ctrl+x twice) to discard them
        """
        suite.expect(SessionWorktrees.claudeKept(unsaved) == .init(discard: nil), "uncommitted changes keep it, with no flag")
        let unpushed = """
        kept f9db0992 — its worktree is still at “/private/tmp/r/.claude/worktrees/wt-dirty”
          2 unpushed commits on “worktree-wt-dirty”: e2e9c7e “wip” and 1 more. They exist on no remote, so deleting the worktree would lose them.
          push them and run 'claude rm f9db0992' again, or discard the worktree and its commits: claude rm f9db0992 --discard-unpushed e2e9c7e505907d26dfded67443f63316996c7da1@ab7ce62461ef70f6394d434955c2c51b
        """
        suite.expect(SessionWorktrees.claudeKept(unpushed)?.discard
            == "e2e9c7e505907d26dfded67443f63316996c7da1@ab7ce62461ef70f6394d434955c2c51b", "unpushed commits give the flag's value")
        suite.expect(SessionWorktrees.claudeKept("removed f9db0992") == nil, "a removal kept nothing")

        // Remove, as the session's page runs it, against what claude rm answers in turn.
        func removal(_ answers: [String?], anyway: Bool, discards: Bool = true) -> (WorktreeRemoval, [[String]], [String]) {
            var answers = answers
            var calls: [[String]] = []
            var discarded: [String] = []
            let outcome = SessionWorktrees.removeClaudeJob("f9db0992", configHome: nil, cwd: "/r/.claude/worktrees/w/src", anyway: anyway,
                                                            run: { arguments in
                calls.append(arguments)
                let said = answers.isEmpty ? nil : answers.removeFirst()
                // Claude Code 2.1.318 prints what it kept on standard output.
                return ClaudeCLI.Output(status: said == nil ? 0 : 1, out: Data((said ?? "").utf8), error: "")
            }, discard: { folder in discarded.append(folder); return discards })
            return (outcome, calls, discarded)
        }
        suite.expect(removal([nil], anyway: false).0 == .done, "a session with a clean worktree goes")
        let asked = removal([unsaved], anyway: false)
        suite.expect(asked.0 == .unsaved && asked.2.isEmpty, "unsaved changes ask first and lose nothing")
        let dropped = removal([unsaved, nil], anyway: true)
        suite.expect(dropped.0 == .done && dropped.2 == ["/r/.claude/worktrees/w"] && dropped.1.count == 2,
                     "anyway drops the changes, then removes")
        let both = removal([unsaved, unpushed, nil], anyway: true)
        suite.expect(both.0 == .done && both.1.last?.contains("--discard-unpushed") == true, "then unpushed commits go with the flag")
        let pushedOnly = removal([unpushed, nil], anyway: true)
        suite.expect(pushedOnly.0 == .done && pushedOnly.2.isEmpty && pushedOnly.1.count == 2, "commits alone need only the flag")
        suite.expect(removal([unsaved], anyway: true, discards: false).0 == .unsaved, "changes that can't be dropped keep it")
        suite.expect(removal(["no such session"], anyway: true).0 == .failed("no such session"), "another failure says so")
        suite.expect(SessionWorktrees.claudeKept("no such session") == nil, "another failure is not about the worktree")
    }

    static func repository(_ suite: TestSuite) {
        let manager = FileManager.default
        let folder = manager.temporaryDirectory.appending(path: "openbar-worktree-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: folder) }
        let root = folder.appending(path: "repo")
        try? manager.createDirectory(at: root.appending(path: "web"), withIntermediateDirectories: true)
        guard SessionWorktrees.gitExecutable(path: ClaudeCLI.shell().environment["PATH"]) != nil else {
            suite.expect(true, "git is not installed here")
            return
        }
        suite.expect(SessionWorktrees.state(of: root.path) == .notRepo, "a folder that isn't a repository")
        let git = { (arguments: [String]) in
            _ = SessionWorktrees.git(["-c", "user.name=t", "-c", "user.email=t@t"] + arguments, in: root.path)
        }
        try? "one\n".write(to: root.appending(path: "web/a.txt"), atomically: true, encoding: .utf8)
        git(["init", "-q", "-b", "main"])
        git(["add", "."])
        git(["commit", "-qm", "init"])
        guard let realRoot = SessionWorktrees.state(of: root.appending(path: "web").path).root else {
            suite.expect(false, "a folder inside the repository is in it")
            return
        }
        suite.expect(SessionWorktrees.state(of: realRoot) == .clean(root: realRoot), "a clean checkout")
        try? "untracked\n".write(to: root.appending(path: "new.txt"), atomically: true, encoding: .utf8)
        suite.expect(SessionWorktrees.state(of: realRoot) == .clean(root: realRoot), "an untracked file isn't a change from HEAD")
        try? "two\n".write(to: root.appending(path: "web/a.txt"), atomically: true, encoding: .utf8)
        suite.expect(SessionWorktrees.state(of: realRoot) == .dirty(root: realRoot), "an edited file is")

        let first = SessionWorktrees.reserveName(custom: nil, prompt: "Fix login", root: realRoot)
        let second = SessionWorktrees.reserveName(custom: nil, prompt: "Fix login", root: realRoot)
        suite.expect(first == "fix-login" && second == "fix-login-2", "two sessions started at once get two names")
        guard case .success(let path) = SessionWorktrees.create(root: realRoot, name: first) else {
            suite.expect(false, "the worktree is made")
            return
        }
        suite.expect(path.hasSuffix("/.openbar/worktrees/fix-login"), "in the island's folder")
        suite.expect((try? String(contentsOfFile: path + "/web/a.txt", encoding: .utf8)) == "one\n", "from HEAD, not the working copy")
        suite.expect(SessionWorktrees.branch(path) == "openbar/fix-login", "on its own branch")
        let status = SessionWorktrees.git(["status", "--porcelain"], in: realRoot)
        let listed = status.map { String(decoding: $0.out, as: UTF8.self) } ?? ""
        suite.expect(!listed.contains(".openbar"), "git status of the repository doesn't list it")
        SessionWorktrees.excludeOwnFolder(root: realRoot)
        let exclude = (try? String(contentsOfFile: realRoot + "/.git/info/exclude", encoding: .utf8)) ?? ""
        suite.expect(exclude.components(separatedBy: "/.openbar/").count == 2, "excluded once, however often it's asked")
        guard case .success(let again) = SessionWorktrees.create(root: realRoot, name: first) else {
            suite.expect(false, "a name taken meanwhile still gets a worktree")
            return
        }
        suite.expect(again.hasSuffix("/fix-login-3"), "under the next free name, past the one held for the other session")
        suite.expect(SessionWorktrees.takenNames(root: realRoot).isSuperset(of: ["fix-login", "fix-login-2", "fix-login-3"]), "every name is taken")

        try? "edit\n".write(to: URL(fileURLWithPath: path + "/web/a.txt"), atomically: true, encoding: .utf8)
        suite.expect(SessionWorktrees.remove(path + "/web", force: false) == .unsaved, "unsaved changes keep it")
        suite.expect(manager.fileExists(atPath: path), "nothing was removed")
        suite.expect(SessionWorktrees.remove(path + "/web", force: true) == .removed, "removed when the person says so")
        suite.expect(!manager.fileExists(atPath: path), "its folder is gone")
        let branches = SessionWorktrees.git(["branch", "--list", "openbar/*"], in: realRoot)
        suite.expect((branches.map { String(decoding: $0.out, as: UTF8.self) } ?? "").contains("openbar/fix-login"),
                     "its branch stays")
        suite.expect(SessionWorktrees.remove(again, force: false) == .removed, "a clean worktree goes without asking")
        suite.expect(SessionWorktrees.remove(again, force: false) == .removed, "one already gone is pruned")
        suite.expect(SessionWorktrees.remove(realRoot + "/.claude/worktrees/x", force: true) == .failed(""),
                     "never one the island didn't make")
    }
}
