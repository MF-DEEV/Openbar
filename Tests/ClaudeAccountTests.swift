// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Several Claude Code accounts, one per config folder (issue #216).
enum ClaudeAccountTests {
    static func run(_ suite: TestSuite) {
        suite.run("claude accounts settings") { settings(suite) }
        suite.run("claude accounts found") { found(suite) }
        suite.run("claude accounts labels") { labels(suite) }
        suite.run("claude accounts hooks") { hooks(suite) }
        suite.run("claude accounts limits") { limits(suite) }
    }

    static let t0 = Date(timeIntervalSince1970: 1_800_000_000)
    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    static func folder() -> URL {
        let url = FileManager.default.temporaryDirectory.appending(path: "openbar-accounts-\(UUID().uuidString)", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url.resolvingSymlinksInPath()
    }

    static func session(_ id: String, account: String?, provider: ClaudeSessionProvider = .claude,
                        host: ClaudeSessionHost = .unknown) -> ClaudeSession {
        var session = ClaudeSession(id: id, pid: 0, cwd: "/p/\(id)", project: id, name: id, branch: nil, model: nil, host: host,
                                    phase: .generating, phaseSince: t0, timeline: [], subagents: [], tokens: AgentTokens(),
                                    cost: 0, hooked: true)
        session.provider = provider
        session.account = account
        return session
    }

    static func settings(_ suite: TestSuite) {
        let saved = ClaudeAccountSetting.encode([
            ClaudeAccountSetting(path: "/Users/me/.claude-work", name: "  Work  "),
            ClaudeAccountSetting(path: "/Users/me/.claude-work/", name: "Twice"),
            ClaudeAccountSetting(path: "relative", name: "Nope"),
            ClaudeAccountSetting(path: "/Users/me/x", name: String(repeating: "n", count: 80), hidden: true),
        ])
        let decoded = ClaudeAccountSetting.decode(saved)
        suite.expect(decoded.count == 2, "one setting per folder, only absolute paths: \(decoded)")
        suite.expect(decoded.first?.name == "Work", "names are trimmed")
        suite.expect(decoded.last?.name?.count == ClaudeAccountSetting.longestName && decoded.last?.hidden == true,
                     "a long name is cut, hiding is kept")
        suite.expect(ClaudeAccountSetting.decode("not json").isEmpty && ClaudeAccountSetting.decode("").isEmpty, "garbage reads as none")
        suite.expect(ClaudeAccountSetting(path: "/a").isEmpty && !ClaudeAccountSetting(path: "/a", hidden: true).isEmpty,
                     "a setting with nothing in it is not kept")

        let home = URL(fileURLWithPath: "/Users/me")
        suite.expect(ClaudeAccounts.name(for: "/Users/me/.claude", home: home) == ClaudeAccounts.defaultName, "~/.claude is the default")
        suite.expect(ClaudeAccounts.name(for: "/Users/me/.claude-work", home: home) == "Work", ".claude-work is Work")
        suite.expect(ClaudeAccounts.name(for: "/Users/me/.claude-2", home: home) == "2", ".claude-2 is 2")
        suite.expect(ClaudeAccounts.name(for: "/Users/me/.config/claude", home: home) == "Config", "the XDG folder")
        suite.expect(ClaudeAccounts.name(for: "/Volumes/x/claude-personal", home: home) == "Claude-personal",
                     "any other folder goes by its own name")

        let defaults = UserDefaults(suiteName: "com.openbar.tests.claude-accounts")!
        defaults.removePersistentDomain(forName: "com.openbar.tests.claude-accounts")
        defer { defaults.removePersistentDomain(forName: "com.openbar.tests.claude-accounts") }
        let root = folder()
        defer { try? FileManager.default.removeItem(at: root) }
        let work = root.appending(path: ".claude-work", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
        let account = ClaudeAccount(path: work.path, name: "Work", named: false, manual: false, hidden: false)
        ClaudeAccounts.rename(account, to: "Job", defaults: defaults)
        ClaudeAccounts.setHidden(account, true, defaults: defaults)
        var stored = ClaudeAccounts.settings(defaults)
        suite.expect(stored.count == 1 && stored[0].name == "Job" && stored[0].hidden == true, "a rename and a hide share one entry")
        ClaudeAccounts.rename(account, to: "  ", defaults: defaults)
        ClaudeAccounts.setHidden(account, false, defaults: defaults)
        suite.expect(defaults.string(forKey: DefaultsKey.notchSessionsClaudeAccounts) == nil,
                     "an empty name and showing again leave nothing behind")
        let manual = root.appending(path: "elsewhere", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: manual, withIntermediateDirectories: true)
        let added = ClaudeAccounts.add(manual, home: root, defaults: defaults)
        suite.expect(added?.manual == true && added?.name == "Elsewhere", "a folder added by hand is an account")
        suite.expect(ClaudeAccounts.add(manual, home: root, defaults: defaults) == nil, "the same folder twice is refused")
        suite.expect(ClaudeAccounts.add(root.appending(path: "missing"), home: root, defaults: defaults) == nil, "a missing folder is refused")
        suite.expect(ClaudeAccounts.manualFolders(defaults).map { ClaudeAccounts.canonical($0.path) } == [ClaudeAccounts.canonical(manual.path)],
                     "the session list reads the folder added by hand")
        if let added { ClaudeAccounts.remove(added, defaults: defaults) }
        stored = ClaudeAccounts.settings(defaults)
        suite.expect(stored.isEmpty, "removing forgets it")
    }

    static func found(_ suite: TestSuite) {
        let home = folder()
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let standard = home.appending(path: ".claude", directoryHint: .isDirectory)
        let work = home.appending(path: ".claude-work", directoryHint: .isDirectory)
        let stale = home.appending(path: ".claude-old", directoryHint: .isDirectory)
        for folder in [standard, work.appending(path: "sessions"), stale] {
            try? manager.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        let linked = home.appending(path: ".claude-link")
        try? manager.createSymbolicLink(at: linked, withDestinationURL: work)
        let homes = ClaudeSessionRegistryReader.homes(home)
        let accounts = ClaudeAccounts.all(found: homes, settings: [
            ClaudeAccountSetting(path: work.path, name: "Job", hidden: true),
        ], home: home)
        suite.expect(accounts.map(\.name) == [ClaudeAccounts.defaultName, "Job"],
                     "~/.claude and each folder with a session registry, once through a link, none missing: \(accounts.map(\.path))")
        suite.expect(accounts.last?.hidden == true && accounts.last?.named == true, "the person's name and hiding apply")
        suite.expect(!accounts.contains { $0.path.hasSuffix(".config/claude") }, "a standard folder that is not there is no account")
        suite.expect(ClaudeAccounts.account(for: linked.path, in: accounts)?.name == "Job", "a session found through a link is the same account")

        let shown = accounts.map { account -> ClaudeAccount in var account = account; account.hidden = false; return account }
        suite.expect(ClaudeAccounts.preferred(shown, last: work.path, folder: standard.path)?.name == "Job", "the last account used first")
        suite.expect(ClaudeAccounts.preferred(shown, last: "/gone", folder: work.path)?.name == "Job", "else the folder's own")
        suite.expect(ClaudeAccounts.preferred(accounts, last: work.path, folder: nil)?.name == ClaudeAccounts.defaultName,
                     "a hidden account is never picked")
    }

    static func labels(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/me")
        let accounts = [ClaudeAccount(path: "/Users/me/.claude", name: "Personal", named: true, manual: false, hidden: false)]
        let one = [session("a", account: "/Users/me/.claude"), session("b", account: "/Users/me/.claude"),
                   session("c", account: nil, provider: .codex)]
        suite.expect(ClaudeAccounts.labels(one, accounts: accounts, home: home).isEmpty, "with one account nothing is labelled")
        let remote = one + [session("r", account: "/home/dev/.claude-work", host: .remote(alias: "box", tmux: false))]
        suite.expect(ClaudeAccounts.labels(remote, accounts: accounts, home: home).isEmpty,
                     "a remote host's folder is not another account here")
        let two = one + [session("w", account: "/Users/me/.claude-work")]
        let labels = ClaudeAccounts.labels(two, accounts: accounts, home: home)
        suite.expect(labels == ["a": "Personal", "b": "Personal", "w": "Work"],
                     "with two accounts every Claude session says its own, named or made from the folder: \(labels)")
        suite.expect(ClaudeAccounts.inUse(two).count == 2, "two accounts in use")
    }

    static func hooks(_ suite: TestSuite) {
        let home = folder()
        defer { try? FileManager.default.removeItem(at: home) }
        let manager = FileManager.default
        let work = home.appending(path: ".claude-work", directoryHint: .isDirectory)
        try? manager.createDirectory(at: work, withIntermediateDirectories: true)
        let bundle = "com.openbar.app.dev"
        let command = ClaudeHookInstaller.command(socket: "/tmp/x.sock", bundle: bundle)
        let account = ClaudeAccount(path: work.path, name: "Work", named: false, manual: false, hidden: false)
        // Each account's settings are its own file, beside the person's hooks.
        try? Data(#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#.utf8).write(to: account.settingsURL)
        try? ClaudeHookInstaller.update(account.settingsURL) { ClaudeHookInstaller.installing(into: $0, command: command, bundle: bundle) }
        let settings = (try? ClaudeHookInstaller.read(account.settingsURL)) ?? [:]
        suite.expect(ClaudeHookInstaller.isInstalled(in: settings, command: command), "the account's settings hold the hooks")
        let stop = ((settings["hooks"] as? [String: Any])?["Stop"] as? [[String: Any]]) ?? []
        suite.expect(stop.count == 2, "the person's own hook stays")
        let other = ClaudeHookInstaller.command(socket: "/tmp/old.sock", bundle: bundle)
        suite.expect(ClaudeHookInstaller.hasAny(in: settings, bundle: bundle)
                     && !ClaudeHookInstaller.isInstalled(in: settings, command: other), "an older command reads as out of date")
        suite.expect(!manager.fileExists(atPath: home.appending(path: ".claude/settings.json").path),
                     "~/.claude is left alone")
    }

    static func limits(_ suite: TestSuite) {
        let standard = URL(fileURLWithPath: "/Users/me/.claude", isDirectory: true)
        let work = URL(fileURLWithPath: "/Users/me/.claude-work", isDirectory: true)
        func record(_ pid: Int32, _ folder: URL, changed: TimeInterval) -> ClaudeSessionRecord {
            var record = ClaudeSessionRecord(pid: pid, sessionID: "s\(pid)", cwd: "/p", name: "p", status: .busy,
                                             waitingFor: nil, statusUpdated: at(changed), started: at(0), version: "2")
            record.configHome = folder.path
            return record
        }
        let both = [record(1, standard, changed: 100), record(2, work, changed: 200)]
        suite.expect(AgentClaudeServer.accountFolder(both, previous: nil)?.path == work.path, "with none before, the latest")
        suite.expect(AgentClaudeServer.accountFolder(both, previous: standard)?.path == standard.path,
                     "while the account before still runs a session the reading stays with it")
        suite.expect(AgentClaudeServer.accountFolder([record(2, work, changed: 50)], previous: standard)?.path == work.path,
                     "once it runs none, the reading follows the account in use")

        func window(_ id: String, used: Double, resets: Date?) -> AgentLimitWindow {
            AgentLimitWindow(id: id, kind: .session, minutes: 300, scope: nil, usedPercent: used, resetsAt: resets)
        }
        var snapshot = AgentUsageSnapshot(loaded: true, now: at(0))
        snapshot.limits[.claude] = AgentLimits(provider: .claude, windows: [window("claude.fh", used: 40, resets: at(3600))],
                                               observedAt: at(0), source: .account)
        let other = AgentLimits(provider: .claude, windows: [window("claude.fh", used: 100, resets: at(1800))],
                                observedAt: at(0), source: .account)
        let strip = NotchSessionLimitsSupport.stripLimit(snapshot, accounts: [other], agents: [.claude], focus: .mostUsed,
                                                         threshold: 80, now: at(1))
        suite.expect(strip?.window.usedPercent == 100, "the strip warns about another account that is nearly spent")
        suite.expect(NotchSessionLimitsSupport.stripLimit(snapshot, accounts: [other], agents: [.codex], focus: .mostUsed,
                                                          threshold: 80, now: at(1)) == nil, "only while Claude sessions are listed")

        var stopped = session("w", account: work.path)
        stopped.phase = .limited
        stopped.limitStop = ClaudeLimitStop(at: at(0))
        var state = ClaudeAccountLimits()
        state.limits = other
        suite.expect(NotchSessionLimitsSupport.reset(of: stopped, snapshot: snapshot, now: at(1),
                                                     accounts: [ClaudeAccounts.canonical(work.path): state]) == at(1800),
                     "a stopped session's reset comes from its own account")
        suite.expect(NotchSessionLimitsSupport.reset(of: stopped, snapshot: snapshot, now: at(1)) == nil,
                     "the other account's windows are not this one's")
        suite.expect(state.current(at: at(2000)) == nil, "a renewed window is no longer read")
    }
}
