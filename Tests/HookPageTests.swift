// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The Hooks page (#236): each config file's state and why it is skipped,
/// config folders and tools added by hand, Codex's hook review, Reinstall
/// and Remove with copies kept, putting lost hooks back, and Remove
/// everything leaving every file as it was before Openbar.
enum HookPageTests {
    static func run(_ suite: TestSuite) {
        suite.run("hooks page: states and reasons") { states(suite) }
        suite.run("hooks page: extra folders") { folders(suite) }
        suite.run("hooks page: codex hooks and review") { codex(suite) }
        suite.run("hooks page: tools with claude style hooks") { tools(suite) }
        suite.run("hooks page: rows on disk") { rows(suite) }
        suite.run("hooks page: putting lost hooks back") { repairs(suite) }
        suite.run("hooks page: remove everything") { removeEverything(suite) }
    }

    static let socket = "/tmp/ob.sock"
    static let bundle = "com.openbar.app.dev"
    static var now: Date { AgentHookSessionTests.now }

    final class Sandbox {
        let home: URL
        let defaults: UserDefaults
        let suiteName = "com.openbar.tests.hooks-\(UUID().uuidString)"
        let previousStore = HookFolders.store

        init() {
            home = URL(fileURLWithPath: "/tmp/ob236t-\(UUID().uuidString.prefix(8))", isDirectory: true)
            try? FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
            defaults = UserDefaults(suiteName: suiteName)!
            HookFolders.store = defaults
        }

        deinit {
            try? FileManager.default.removeItem(at: home)
            defaults.removePersistentDomain(forName: suiteName)
            HookFolders.store = previousStore
        }

        var env: HookMaintenance.Environment {
            HookMaintenance.Environment(home: home, socket: HookPageTests.socket, bundle: HookPageTests.bundle, defaults: defaults)
        }

        func folder(_ path: String) -> URL {
            let url = home.appending(path: path, directoryHint: .isDirectory)
            try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
            return url
        }

        func write(_ path: String, _ text: String) {
            let url = home.appending(path: path)
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? Data(text.utf8).write(to: url)
        }

        func read(_ path: String) -> String? {
            FileManager.default.contents(atPath: home.appending(path: path).path).map { String(decoding: $0, as: UTF8.self) }
        }

        /// A command on the sandbox's PATH, so the agent counts as installed.
        func install(_ command: String) {
            let bin = folder("bin")
            let file = bin.appending(path: command)
            FileManager.default.createFile(atPath: file.path, contents: Data("#!/bin/sh\n".utf8),
                                           attributes: [.posixPermissions: 0o755])
        }

        func context(shown: Set<ClaudeSessionProvider> = Set(ClaudeSessionProvider.allCases),
                     folders: [AgentFolderSetting] = [], tools: [CustomHookTool] = [],
                     codexPresent: Bool = true) -> HookTargets.Context {
            HookTargets.Context(home: home, socket: HookPageTests.socket, bundle: HookPageTests.bundle, environment: [:],
                                path: home.appending(path: "bin").path,
                                accounts: ClaudeAccounts.all(found: [home.appending(path: ".claude")], settings: [], home: home),
                                shown: shown, folders: folders, tools: tools, optOut: HookMaintenance.optOut(defaults),
                                records: HookRecords.all(in: defaults), codexPresent: codexPresent)
        }
    }

    // MARK: States

    static func states(_ suite: TestSuite) {
        func state(_ derived: HookRecords.State, present: Bool = true, folder: Bool = true, enabled: Bool = true,
                   out: Bool = false, hidden: Bool = false, approval: Bool = false) -> HookTargetState {
            HookTargets.shownState(derived: derived, present: present, folderExists: folder, enabled: enabled,
                                   optedOut: out, hiddenAccount: hidden, needsApproval: approval)
        }
        suite.expect(state(.installed) == .installed && state(.outdated) == .outdated
                     && state(.changedByHand) == .changedByHand && state(.unreadable) == .unreadable,
                     "what the file holds shows as it is")
        suite.expect(state(.removedByHand) == .missing && state(.missing) == .missing,
                     "hooks deleted by hand read as missing")
        suite.expect(state(.missing, present: false) == .skipped(.agentNotInstalled)
                     && state(.missing, folder: false) == .skipped(.folderMissing)
                     && state(.missing, enabled: false) == .skipped(.switchedOff)
                     && state(.missing, out: true) == .skipped(.removedByYou)
                     && state(.missing, hidden: true) == .skipped(.accountHidden),
                     "a file left without hooks says why")
        suite.expect(state(.installed, approval: true) == .skipped(.needsApproval),
                     "hooks the agent still has to trust say so")
        suite.expect(state(.changedByHand, enabled: false) == .changedByHand,
                     "hooks changed by hand show whatever the switch")
        suite.expect(HookTargetState.skipped(.needsApproval).needsAttention && HookTargetState.missing.needsAttention
                     && !HookTargetState.skipped(.agentNotInstalled).needsAttention && !HookTargetState.installed.needsAttention,
                     "only what the person can fix counts as needing a look")
        suite.expect(HooksPageStrings.state(.skipped(.folderMissing)) == "Skipped: the config folder doesn’t exist yet"
                     && HooksPageStrings.state(.skipped(.agentNotInstalled)) == "Skipped: the agent isn’t installed"
                     && HooksPageStrings.state(.skipped(.needsApproval)) == "Skipped: the hooks need approval inside the agent",
                     "the reasons read as the issue asks")
        suite.expect(HooksPageStrings.linkCaption(working: 7, attention: 1) == "7 configs work, 1 needs a look"
                     && HooksPageStrings.linkCaption(working: 1, attention: 0) == "1 config works",
                     "Settings › Sessions sums the page up in a line")
    }

    // MARK: Folders added by hand

    static func folders(_ suite: TestSuite) {
        let box = Sandbox()
        let gemini = box.folder("work/.gemini")
        let picked = gemini.deletingLastPathComponent()
        suite.expect(AgentFolders.configFolder(.gemini, picked: picked).lastPathComponent == ".gemini"
                     && AgentFolders.configFolder(.gemini, picked: gemini).path == gemini.path,
                     "a folder holding .gemini stands for it, as GEMINI_CLI_HOME does")
        guard case .success(let setting) = AgentFolders.add(.gemini, picked: picked, defaultFolder: box.home.appending(path: ".gemini"),
                                                            defaults: box.defaults) else {
            return suite.expect(false, "the folder is added")
        }
        suite.expect(AgentFolders.add(.gemini, picked: gemini, defaultFolder: nil, defaults: box.defaults) == .failure(.alreadyListed)
                     && AgentFolders.add(.gemini, picked: box.home.appending(path: "nope"), defaultFolder: nil, defaults: box.defaults) == .failure(.notFolder),
                     "a folder is listed once, and only a folder")
        let own = box.folder(".codex")
        suite.expect(AgentFolders.add(.codex, picked: own, defaultFolder: own, defaults: box.defaults) == .failure(.alreadyListed),
                     "the agent's own folder needs no second row")
        suite.expect(AgentFolders.all(box.defaults) == [setting], "the folder is kept: \(AgentFolders.all(box.defaults))")

        // The hooks go into the added folder's settings.json, beside the person's.
        box.write("work/.gemini/settings.json", #"{"theme":"dark"}"#)
        try? AgentHookInstaller.install(.gemini, home: box.home, socket: socket, bundle: bundle, folder: gemini)
        suite.expect(AgentHookInstaller.state(.gemini, home: box.home, socket: socket, bundle: bundle, folder: gemini) == .installed
                     && AgentHookInstaller.state(.gemini, home: box.home, socket: socket, bundle: bundle) == .missing,
                     "the added folder has the hooks; the default one is untouched")
        let settings = (try? ClaudeHookInstaller.read(gemini.appending(path: "settings.json"))) ?? [:]
        suite.expect(settings["theme"] as? String == "dark", "the person's settings stay")

        // A file of the island's own in an added Copilot folder, with a copy kept when it changes.
        let copilot = box.folder("copilot-2")
        try? AgentHookInstaller.install(.copilot, home: box.home, socket: socket, bundle: bundle, folder: copilot)
        try? AgentHookInstaller.install(.copilot, home: box.home, socket: "/other.sock", bundle: bundle, folder: copilot)
        let hooks = copilot.appending(path: "hooks")
        let copies = ((try? FileManager.default.contentsOfDirectory(atPath: hooks.path)) ?? []).filter { $0.hasSuffix(".bak") && !$0.contains("original") }
        suite.expect(AgentHookInstaller.state(.copilot, home: box.home, socket: "/other.sock", bundle: bundle, folder: copilot) == .installed
                     && copies.count == 1, "a file of the island's own is copied aside before it changes: \(copies)")
        try? AgentHookInstaller.remove(.copilot, home: box.home, bundle: bundle, folder: copilot)
        let after = ((try? FileManager.default.contentsOfDirectory(atPath: hooks.path)) ?? []).filter { $0.hasSuffix(".bak") && !$0.contains("original") }
        suite.expect(AgentHookInstaller.state(.copilot, home: box.home, socket: socket, bundle: bundle, folder: copilot) == .missing
                     && after.count == 2, "and before it goes: \(after)")

        AgentFolders.remove(setting, defaults: box.defaults)
        suite.expect(AgentFolders.all(box.defaults).isEmpty, "a folder can be taken off the page")
        suite.expect(AgentHookInstaller.file(.opencode, folder: box.home, bundle: bundle) == nil
                     && AgentHookInstaller.foldersByHand == [.codex, .gemini, .copilot, .pi, .vibe],
                     "only agents with a home of their own take folders by hand")
    }

    // MARK: Codex

    static func codex(_ suite: TestSuite) {
        // Codex's own trust hashes, read from `codex app-server` hooks/list (0.164).
        let command = "/bin/cat >> /tmp/ob236/hooklog.txt # openbar-hook test"
        suite.expect(CodexHookTrust.hash(event: "PreToolUse", matcher: "*", command: command, timeout: 5)
                     == "sha256:9306a10457ff8b14c7c628dd8734747317142cb0e96aebe2e5642673a1bf7e83"
                     && CodexHookTrust.hash(event: "Stop", matcher: nil, command: command, timeout: nil)
                     == "sha256:4f420d4bacda7a630e3bdc7e5164be1b8c068b64accb2fa7b1c5c81e322d4ddb",
                     "the island's hash is Codex's own")
        suite.expect(CodexHookTrust.snake("PermissionRequest") == "permission_request" && CodexHookTrust.snake("Stop") == "stop"
                     && CodexHookTrust.timeout("SessionEnd", 5) == 3 && CodexHookTrust.timeout("SessionEnd", nil) == 1
                     && CodexHookTrust.timeout("PreToolUse", nil) == 600,
                     "event keys and timeouts are written as Codex runs them")
        suite.expect(CodexHookTrust.json("a\"b\\c/d\u{1}é") == "\"a\\\"b\\\\c/d\\u0001é\"", "strings are written as serde_json does")
        let toml = """
        model = "gpt-5"
        [hooks.state."/x/hooks.json:stop:0:0"]
        trusted_hash = "sha256:aa" # trusted in the review
        [hooks.state]
        "/x/hooks.json:pre_tool_use:1:0" = { enabled = true, trusted_hash = "sha256:bb" }
        [projects."/x"]
        trust_level = "trusted"
        """
        let hashes = CodexHookTrust.trustedHashes(toml)
        suite.expect(hashes == ["/x/hooks.json:stop:0:0": "sha256:aa", "/x/hooks.json:pre_tool_use:1:0": "sha256:bb"],
                     "trusted hashes are read from tables and inline tables: \(hashes)")

        let box = Sandbox()
        let home2 = box.folder("codex-2")
        box.write("codex-2/hooks.json", #"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#)
        try? AgentHookInstaller.install(.codex, home: box.home, socket: socket, bundle: bundle, folder: home2)
        let settings = (try? ClaudeHookInstaller.read(home2.appending(path: "hooks.json"))) ?? [:]
        let groups = (settings["hooks"] as? [String: Any])?["PreToolUse"] as? [[String: Any]] ?? []
        let tag = AgentHookInstaller.folderTag(home2)
        suite.expect(groups.count == 1 && groups[0]["matcher"] as? String == "*"
                     && ((groups[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String)?.contains("\"via\":\"\(tag)\"") == true,
                     "Codex's hooks name the folder they were written for: \(groups)")
        let stop = (settings["hooks"] as? [String: Any])?["Stop"] as? [[String: Any]] ?? []
        suite.expect(stop.count == 2 && ((stop[0]["hooks"] as? [[String: Any]])?.first?["command"] as? String) == "say done",
                     "the person's own Codex hooks stay first")
        suite.expect(AgentHookInstaller.state(.codex, home: box.home, socket: socket, bundle: bundle, folder: home2) == .installed
                     && AgentHookInstaller.state(.codex, home: box.home, socket: socket, bundle: bundle) == .missing,
                     "an added Codex home has hooks; Codex's own needs none")
        suite.expect(CodexHookTrust.trusted(folder: home2, bundle: bundle) == false, "new hooks wait for Codex's review")
        // The review keeps a hash for each hook; the island reads the same keys.
        let expected = CodexHookTrust.expected(home2.appending(path: "hooks.json"), settings: settings,
                                               marker: ClaudeHookInstaller.marker(bundle: bundle))
        suite.expect(expected.count == AgentHookInstaller.codexEvents.count
                     && expected.keys.allSatisfy { $0.hasPrefix(CodexHookTrust.realPath(home2.appending(path: "hooks.json")) + ":") }
                     && expected.keys.contains { $0.hasSuffix(":stop:1:0") },
                     "each of the island's hooks has Codex's key: \(expected.keys.sorted())")
        let trust = expected.map { "[hooks.state.\"\($0.key)\"]\ntrusted_hash = \"\($0.value)\"" }.joined(separator: "\n")
        box.write("codex-2/config.toml", trust)
        suite.expect(CodexHookTrust.trusted(folder: home2, bundle: bundle) == true, "trusted in the review, they run")
        try? AgentHookInstaller.install(.codex, home: box.home, socket: "/other.sock", bundle: bundle, folder: home2)
        suite.expect(CodexHookTrust.trusted(folder: home2, bundle: bundle) == false, "rewritten hooks need the review again")

        // What they send is Codex's session, apart from Codex's server's.
        let payload: [String: Any] = ["session_id": "t1", "turn_id": "u1", "cwd": "/w/app", "hook_event_name": "PermissionRequest",
                                      "model": "gpt-5-codex", "permission_mode": "default", "tool_name": "Bash",
                                      "tool_input": ["command": "rm -rf build"], "transcript_path": NSNull()]
        let data = try! JSONSerialization.data(withJSONObject: ["openbar": "codex", "hook": "PermissionRequest", "pid": 9,
                                                                "via": tag, "event": payload])
        let decoded = AgentHookMessage.decode(data, now: now, policy: ClaudeContentPolicy(previews: true))
        suite.expect(decoded?.provider == .codex && decoded?.via == tag
                     && decoded?.events.first?.id == "codex:\(tag):t1",
                     "Codex through hooks is a session of its own: \(decoded?.events.map(\.id) ?? [])")
        let unnamed = try! JSONSerialization.data(withJSONObject: ["openbar": "codex", "hook": "Stop", "event": payload])
        suite.expect(AgentHookMessage.decode(unnamed, now: now, policy: ClaudeContentPolicy(previews: true)) == nil,
                     "a Codex envelope that names no folder is not taken")
        let tracker = AgentHookSessions()
        for event in decoded?.events ?? [] { tracker.ingest(event, now: now) }
        let session = tracker.snapshot(now: now, previews: true, host: { _ in .unknown }, branch: { _ in nil }).sessions.first
        suite.expect(session?.provider == .codex && session?.via == tag && session?.codexServer == false
                     && session?.heardThroughHooks == true && session?.permission == nil
                     && session?.waitingFor == CodexSessionState.answerInTerminal && session?.model == "gpt-5-codex",
                     "its requests are answered in Codex's terminal: \(String(describing: session))")
    }

    // MARK: Tools with Claude Code style hooks

    static func tools(_ suite: TestSuite) {
        let box = Sandbox()
        let accounts = [ClaudeAccount(path: box.folder(".claude").path, name: "Default", named: false, manual: false, hidden: false)]
        suite.expect(CustomHookTools.add(name: " ", file: box.home.appending(path: "x.json"), accounts: accounts,
                                         defaults: box.defaults) == .failure(.noName)
                     && CustomHookTools.add(name: "Kit", file: box.home.appending(path: "no/where.json"), accounts: accounts,
                                            defaults: box.defaults) == .failure(.notInFolder)
                     && CustomHookTools.add(name: "Kit", file: accounts[0].settingsURL, accounts: accounts,
                                            defaults: box.defaults) == .failure(.claudeAccount),
                     "a tool needs a name and a file in a folder, and not a Claude Code account's")
        box.write(".kit/settings.json", #"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo bye"}]}]},"model":"x"}"#)
        guard case .success(let tool) = CustomHookTools.add(name: "Kit", file: box.home.appending(path: ".kit/settings.json"),
                                                           accounts: accounts, defaults: box.defaults) else {
            return suite.expect(false, "the tool is added")
        }
        suite.expect(CustomHookTools.all(box.defaults) == [tool] && AgentHookInstaller.isTag(tool.id), "the tool is kept by its id")
        try? AgentHookInstaller.installCustom(tool.file, tool: tool.id, socket: socket, bundle: bundle)
        suite.expect(AgentHookInstaller.customState(tool.file, tool: tool.id, socket: socket, bundle: bundle) == .installed,
                     "the tool gets Claude Code's hooks")
        let settings = (try? ClaudeHookInstaller.read(tool.file)) ?? [:]
        let events = Set(((settings["hooks"] as? [String: Any]) ?? [:]).keys)
        suite.expect(events == Set(AgentHookInstaller.customEvents) && settings["model"] as? String == "x",
                     "for every Claude Code event, the person's settings kept: \(events)")
        let data = try! JSONSerialization.data(withJSONObject: ["openbar": "custom", "hook": "UserPromptSubmit", "pid": 3, "via": tool.id,
                                                                "event": ["session_id": "k1", "cwd": "/w/kit", "prompt": "hi",
                                                                          "hook_event_name": "UserPromptSubmit"]])
        let tracker = AgentHookSessions()
        for event in AgentHookMessage.decode(data, now: now, policy: ClaudeContentPolicy(previews: true))?.events ?? [] {
            tracker.ingest(event, now: now)
        }
        let sessions = tracker.snapshot(now: now, previews: true, host: { _ in .unknown }, branch: { _ in nil }).sessions
        suite.expect(sessions.count == 1 && sessions[0].provider == .custom && sessions[0].via == tool.id
                     && sessions[0].id == "custom:\(tool.id):k1" && sessions[0].phase == .generating
                     && sessions[0].chat.map(\.text) == ["hi"],
                     "its sessions are listed: \(sessions)")
        suite.expect(HookTargets.labels(sessions, defaults: box.defaults) == [sessions[0].id: "Kit"],
                     "and carry the tool's name")
        suite.expect(NotchHookAgentMarkPaths.mark(.custom) != nil && ClaudeSessionProvider.custom.displayName == "Other tool"
                     && !ClaudeSessionProvider.agents.contains(.custom),
                     "a plain mark, and no place among the named agents")
        try? AgentHookInstaller.removeCustom(tool.file, bundle: bundle)
        let left = (try? ClaudeHookInstaller.read(tool.file)) ?? [:]
        suite.expect(AgentHookInstaller.customState(tool.file, tool: tool.id, socket: socket, bundle: bundle) == .missing
                     && ((left["hooks"] as? [String: Any])?["Stop"] as? [[String: Any]])?.count == 1,
                     "Remove leaves the tool's own hooks")
    }

    // MARK: Rows

    static func rows(_ suite: TestSuite) {
        let box = Sandbox()
        box.install("gemini")
        box.write(".claude/settings.json", #"{"model":"opus"}"#)
        box.folder(".gemini")
        var targets = HookTargets.list(box.context())
        let claude = targets.first { $0.provider == .claude }
        suite.expect(claude?.state == .missing && claude?.canReinstall == true && claude?.canRemove == false,
                     "Claude Code's account without hooks: \(String(describing: claude?.state))")
        let codex = targets.first { $0.provider == .codex }
        suite.expect(codex?.state == .skipped(.nothingNeeded) && codex?.file == nil && codex?.canReinstall == false,
                     "Codex's own home needs nothing")
        let gemini = targets.first { $0.id == HookRecords.agentID(.gemini) }
        suite.expect(gemini?.state == .missing && gemini?.present == true, "Gemini found, its hooks not in yet")
        let cursor = targets.first { $0.id == HookRecords.agentID(.cursor) }
        suite.expect(cursor?.state == .skipped(.agentNotInstalled), "an agent not installed is skipped")
        suite.expect(HookTargets.list(box.context(shown: [.claude])).first { $0.id == HookRecords.agentID(.gemini) }?.state
                     == .skipped(.switchedOff), "an agent switched off is skipped")

        // Reinstall, by hand edits, deletion, Remove.
        for target in targets where target.provider == .claude || target.id == HookRecords.agentID(.gemini) {
            try? HookMaintenance.reinstall(target, env: box.env)
        }
        targets = HookTargets.list(box.context())
        suite.expect(targets.first { $0.provider == .claude }?.state == .installed
                     && targets.first { $0.id == HookRecords.agentID(.gemini) }?.state == .installed,
                     "Reinstall puts the hooks in")
        // Someone edits the island's command.
        let settingsFile = box.home.appending(path: ".claude/settings.json")
        var text = box.read(".claude/settings.json") ?? ""
        text = text.replacingOccurrences(of: "2>/dev/null", with: "2>/tmp/log")
        try? Data(text.utf8).write(to: settingsFile)
        targets = HookTargets.list(box.context())
        suite.expect(targets.first { $0.provider == .claude }?.state == .changedByHand, "edited entries read as changed by hand")
        // Someone deletes them.
        try? Data(#"{"model":"opus"}"#.utf8).write(to: settingsFile)
        targets = HookTargets.list(box.context())
        let lost = targets.first { $0.provider == .claude }
        suite.expect(lost?.state == .missing && lost?.derived == .removedByHand, "deleted entries read as missing")
        if let lost { try? HookMaintenance.reinstall(lost, env: box.env) }
        suite.expect(HookTargets.list(box.context()).first { $0.provider == .claude }?.state == .installed,
                     "Reinstall brings them back")
        if let row = HookTargets.list(box.context()).first(where: { $0.provider == .claude }) {
            try? HookMaintenance.takeOut(row, env: box.env)
        }
        let removed = HookTargets.list(box.context()).first { $0.provider == .claude }
        suite.expect(removed?.state == .skipped(.removedByYou) && removed?.canReinstall == true
                     && (try? ClaudeHookInstaller.read(settingsFile))?["model"] as? String == "opus",
                     "Remove takes them out and the row stays out: \(String(describing: removed?.state))")
        let copies = ((try? FileManager.default.contentsOfDirectory(atPath: settingsFile.deletingLastPathComponent().path)) ?? [])
            .filter { $0.hasPrefix("settings.json.openbar-") }
        suite.expect(copies.contains("settings.json.openbar-original.bak") && copies.count >= 3,
                     "every change kept a copy, and the file from before Openbar: \(copies)")
        suite.expect(HookTargets.summary(HookTargets.list(box.context())).working == 1, "the summary counts the working rows")
    }

    // MARK: Repairs

    static func repairs(_ suite: TestSuite) {
        let record = HookRecords.Record(installedAt: now, fingerprint: "f")
        func target(_ id: String, derived: HookRecords.State, state: HookTargetState,
                    place: HookTarget.Place = .claude(URL(fileURLWithPath: "/x/settings.json"))) -> HookTarget {
            HookTarget(id: id, provider: .claude, place: place, name: id, folder: URL(fileURLWithPath: "/x"),
                       file: URL(fileURLWithPath: "/x/settings.json"), wholeFile: false, manual: false, present: true,
                       derived: derived, state: state)
        }
        let rows = [
            target("lost", derived: .removedByHand, state: .missing),
            target("old", derived: .outdated, state: .outdated),
            target("edited", derived: .changedByHand, state: .changedByHand),
            target("never", derived: .missing, state: .missing),
            target("out", derived: .removedByHand, state: .skipped(.removedByYou)),
            target("agent", derived: .removedByHand, state: .missing, place: .agent(.gemini, folder: nil)),
        ]
        let records = Dictionary(uniqueKeysWithValues: ["lost", "old", "edited", "out", "agent"].map { ($0, record) })
        suite.expect(HookMaintenance.repairs(rows, records: records, autoRepair: true).map(\.id) == ["lost", "old"],
                     "only hooks the island put in and lost come back; edits and Remove are the person's")
        suite.expect(HookMaintenance.repairs(rows, records: records, autoRepair: false).isEmpty, "and only when allowed")

        let found: Set<ClaudeSessionProvider> = [.gemini, .cursor, .pi, .vibe]
        let states: [ClaudeSessionProvider: HookRecords.State] = [.gemini: .removedByHand, .cursor: .changedByHand,
                                                                   .pi: .missing, .vibe: .outdated]
        let on = AgentHookInstaller.plan(found: found, shown: found, states: states, optedOut: [], repair: true)
        suite.expect(Set(on.install) == [.gemini, .pi, .vibe] && on.remove.isEmpty,
                     "agents' hooks come back by themselves, hand edits stay: \(on)")
        let off = AgentHookInstaller.plan(found: found, shown: found, states: states, optedOut: [.pi], repair: false)
        suite.expect(Set(off.install) == [.vibe], "without putting back, and with Pi removed by the person: \(off)")
        let hidden = AgentHookInstaller.plan(found: found, shown: [], states: states, optedOut: [], repair: true)
        suite.expect(Set(hidden.remove) == [.cursor, .vibe], "a switch turned off takes the island's out: \(hidden)")
    }

    // MARK: Remove everything

    static func removeEverything(_ suite: TestSuite) {
        let box = Sandbox()
        box.install("gemini")
        box.install("copilot")
        box.install("cursor-agent")
        box.install("vibe")
        // What the person had before Openbar, byte for byte, odd spacing and all.
        let claude = "{\n  \"model\" : \"opus\",\n  \"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"say hi\"}]}]}\n}\n"
        let gemini = "{ \"theme\": \"dark\" }"
        let vibe = "# mine\n[[hooks]]\nname = \"lint\"\ntype = \"post_agent\"\ncommand = \"eslint .\"\n"
        box.write(".claude/settings.json", claude)
        box.write(".gemini/settings.json", gemini)
        box.write(".vibe/hooks.toml", vibe)
        box.folder(".copilot")
        box.folder(".cursor")
        let codexHome = box.folder("codex-2")
        let folders = [AgentFolderSetting(agent: "codex", path: codexHome.path)]
        box.write(".kit/settings.json", "{}")
        let tool = CustomHookTool(id: UUID().uuidString.lowercased(), name: "Kit", path: box.home.appending(path: ".kit/settings.json").path)

        for target in HookTargets.list(box.context(folders: folders, tools: [tool])) where target.state == .missing {
            try? HookMaintenance.reinstall(target, env: box.env)
        }
        let installed = HookTargets.list(box.context(folders: folders, tools: [tool]))
        let ids = Set(installed.filter { $0.state == .installed || $0.state == .skipped(.needsApproval) }.map(\.id))
        suite.expect(ids.isSuperset(of: [HookRecords.claudeID(box.home.appending(path: ".claude/settings.json")),
                                         HookRecords.agentID(.gemini), HookRecords.agentID(.copilot), HookRecords.agentID(.cursor),
                                         HookRecords.agentID(.vibe), folders[0].targetID, tool.targetID]),
                     "everything is in first: \(installed.map { "\($0.id)=\($0.state)" })")
        suite.expect(box.read(".copilot/hooks/openbar-\(bundle).json") != nil && box.read(".cursor/hooks.json") != nil
                     && box.read("codex-2/hooks.json") != nil, "files the island made are there")

        let failures = HookMaintenance.removeEverything(installed, env: box.env)
        suite.expect(failures.isEmpty, "nothing failed: \(failures)")
        suite.expect(box.read(".claude/settings.json") == claude && box.read(".gemini/settings.json") == gemini
                     && box.read(".vibe/hooks.toml") == vibe && box.read(".kit/settings.json") == "{}",
                     "every file is exactly as it was before Openbar:\n\(box.read(".claude/settings.json") ?? "")")
        let manager = FileManager.default
        suite.expect(box.read(".cursor/hooks.json") == nil && box.read("codex-2/hooks.json") == nil
                     && !manager.fileExists(atPath: box.home.appending(path: ".copilot/hooks").path),
                     "files and folders the island made are gone: cursor=\(box.read(".cursor/hooks.json") ?? "-") codex=\(box.read("codex-2/hooks.json") ?? "-") copilot=\(((try? manager.contentsOfDirectory(atPath: box.home.appending(path: ".copilot/hooks").path)) ?? []))")
        let copies = ((try? manager.contentsOfDirectory(atPath: box.home.appending(path: ".claude").path)) ?? [])
            .filter { $0.hasPrefix("settings.json.openbar-") && $0.hasSuffix(".bak") }
        let cursorCopies = ((try? manager.contentsOfDirectory(atPath: box.home.appending(path: ".cursor").path)) ?? [])
            .filter { $0.hasSuffix(".bak") }
        suite.expect(!copies.isEmpty && !copies.contains { $0.contains("original") } && !cursorCopies.isEmpty,
                     "copies of what was there stay; the notes from before Openbar go: \(copies) \(cursorCopies)")
        let after = HookTargets.list(box.context(folders: folders, tools: [tool]))
        suite.expect(after.allSatisfy { !$0.canRemove } && HookMaintenance.repairs(after, records: HookRecords.all(in: box.defaults),
                                                                                  autoRepair: true).isEmpty,
                     "nothing is left, and nothing comes back by itself")
        suite.expect(Set(HookMaintenance.optedOutAgents(box.defaults)).isSuperset(of: [.gemini, .copilot, .cursor, .vibe]),
                     "and the agents' switches don't put the hooks back either")

        // A file the person changed since keeps the change.
        let other = Sandbox()
        other.write(".claude/settings.json", #"{"model":"opus"}"#)
        let rows = HookTargets.list(other.context())
        if let row = rows.first(where: { $0.provider == .claude }) { try? HookMaintenance.reinstall(row, env: other.env) }
        var edited = (try? ClaudeHookInstaller.read(other.home.appending(path: ".claude/settings.json"))) ?? [:]
        edited["theme"] = "light"
        try? JSONSerialization.data(withJSONObject: edited).write(to: other.home.appending(path: ".claude/settings.json"))
        _ = HookMaintenance.removeEverything(HookTargets.list(other.context()), env: other.env)
        let kept = (try? ClaudeHookInstaller.read(other.home.appending(path: ".claude/settings.json"))) ?? [:]
        suite.expect(kept["theme"] as? String == "light" && kept["model"] as? String == "opus" && kept["hooks"] == nil,
                     "a later change of the person's stays: \(kept)")
    }
}
