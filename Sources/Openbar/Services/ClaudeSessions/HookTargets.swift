// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CryptoKit
import Foundation

// The Hooks page (#236): every config file the island writes its hooks into,
// what each holds now, and why one is left alone.

/// Why the island leaves a config file without its hooks.
enum HookSkip: String, Equatable {
    /// The agent never made its config folder: it was not run yet.
    case folderMissing
    case agentNotInstalled
    /// The hooks are in, and the agent waits for the person to trust them
    /// (Codex's hook review).
    case needsApproval
    /// The agent's switch in Settings › Sessions is off.
    case switchedOff
    /// The Claude account is hidden.
    case accountHidden
    /// Codex's own home: the island hears it through Codex's app-server.
    case nothingNeeded
    /// The person pressed Remove; Reinstall brings the hooks back.
    case removedByYou
}

/// A config file's state as the Hooks page shows it.
enum HookTargetState: Equatable {
    case installed
    /// The island's hooks from an older version, or pointing elsewhere.
    case outdated
    case changedByHand
    case missing
    /// The file can't be read as the agent's format; nothing is changed in it.
    case unreadable
    case skipped(HookSkip)

    /// Something the person may want to fix.
    var needsAttention: Bool {
        switch self {
        case .outdated, .changedByHand, .missing, .unreadable: return true
        case .skipped(let reason): return reason == .needsApproval
        case .installed: return false
        }
    }
}

/// One config file the island writes its hooks into.
struct HookTarget: Identifiable, Equatable {
    enum Place: Equatable {
        /// A Claude Code account's settings.json.
        case claude(URL)
        /// A hook agent where it reads its hooks by default (folder nil), or
        /// in a config folder the person added.
        case agent(ClaudeSessionProvider, folder: URL?)
        /// A tool with Claude Code style hooks the person added.
        case custom(CustomHookTool)
    }

    let id: String
    let provider: ClaudeSessionProvider
    let place: Place
    /// The account, folder or tool name.
    let name: String
    /// The config folder, shown and opened in Finder.
    let folder: URL
    /// The file the hooks live in; nil for Codex's own home.
    let file: URL?
    /// The whole file is the island's own (Copilot, OpenCode, Pi).
    let wholeFile: Bool
    /// Added by hand, so it can be taken off the page.
    let manual: Bool
    /// The agent itself is installed.
    let present: Bool
    /// The state before the reasons to skip are weighed.
    let derived: HookRecords.State
    let state: HookTargetState

    /// Whether Reinstall can do anything here.
    var canReinstall: Bool {
        guard file != nil else { return false }
        switch state {
        case .skipped(let reason):
            return reason == .removedByYou || reason == .needsApproval || reason == .folderMissing && manual
        case .unreadable: return false
        default: return true
        }
    }

    /// Whether the file holds any of the island's hooks to take out.
    var canRemove: Bool {
        guard file != nil else { return false }
        switch derived {
        case .installed, .outdated, .changedByHand: return true
        default: return false
        }
    }
}

enum HookTargets {
    /// The page's state of one file, from what it holds and why it may be left alone.
    static func shownState(derived: HookRecords.State, present: Bool, folderExists: Bool, enabled: Bool,
                           optedOut: Bool, hiddenAccount: Bool = false, needsApproval: Bool = false) -> HookTargetState {
        switch derived {
        case .unreadable: return .unreadable
        case .installed: return needsApproval ? .skipped(.needsApproval) : .installed
        case .outdated: return .outdated
        case .changedByHand: return .changedByHand
        case .missing, .removedByHand:
            if hiddenAccount { return .skipped(.accountHidden) }
            if !enabled { return .skipped(.switchedOff) }
            if optedOut { return .skipped(.removedByYou) }
            if !present { return .skipped(.agentNotInstalled) }
            if !folderExists { return .skipped(.folderMissing) }
            return .missing
        }
    }

    /// Everything the targets are read from.
    struct Context {
        var home: URL
        var socket: String
        var bundle: String
        var environment: [String: String] = ProcessInfo.processInfo.environment
        var path: String?
        var accounts: [ClaudeAccount]
        var shown: Set<ClaudeSessionProvider>
        var folders: [AgentFolderSetting]
        var tools: [CustomHookTool]
        var optOut: Set<String>
        var records: [String: HookRecords.Record]
        /// Whether Codex is installed, for the Codex rows.
        var codexPresent: Bool
    }

    /// Every target, in the page's order: Claude Code's accounts, Codex,
    /// each hook agent with its added folders, then the tools.
    static func list(_ context: Context) -> [HookTarget] {
        var targets: [HookTarget] = []
        let marker = ClaudeHookInstaller.marker(bundle: context.bundle)
        let command = ClaudeHookInstaller.command(socket: context.socket, bundle: context.bundle)
        var seen = Set<String>()
        for account in context.accounts {
            let file = account.settingsURL
            // Two folders whose settings are one linked file are one row.
            guard seen.insert(file.resolvingSymlinksInPath().path).inserted else { continue }
            let id = HookRecords.claudeID(file)
            let base = claudeState(file, command: command, bundle: context.bundle)
            let derived = HookRecords.derive(base, record: context.records[id],
                                             current: currentPrint(file, marker: marker))
            let state = shownState(derived: derived, present: true, folderExists: true, enabled: true,
                                   optedOut: context.optOut.contains(id), hiddenAccount: account.hidden)
            targets.append(HookTarget(id: id, provider: .claude, place: .claude(file), name: account.name,
                                      folder: URL(fileURLWithPath: account.path, isDirectory: true), file: file,
                                      wholeFile: false, manual: account.manual, present: true, derived: derived,
                                      state: state))
        }
        // Codex's own home needs nothing; folders added by hand get hooks.
        if let own = AgentHookInstaller.agentFolder(.codex, home: context.home, environment: context.environment) {
            targets.append(HookTarget(id: HookRecords.agentID(.codex), provider: .codex, place: .agent(.codex, folder: nil),
                                      name: "Codex", folder: own, file: nil, wholeFile: false, manual: false,
                                      present: context.codexPresent, derived: .missing, state: .skipped(.nothingNeeded)))
        }
        for setting in context.folders where setting.provider == .codex {
            targets.append(agentTarget(.codex, setting: setting, context: context))
        }
        for provider in ClaudeSessionProvider.hookAgents {
            let present = AgentHookInstaller.isPresent(provider, home: context.home, path: context.path)
            if let file = AgentHookInstaller.hookFile(provider, home: context.home, bundle: context.bundle) {
                let id = HookRecords.agentID(provider)
                let base = HookRecords.Base(AgentHookInstaller.state(provider, home: context.home, socket: context.socket,
                                                                     bundle: context.bundle))
                let derived = HookRecords.derive(base, record: context.records[id],
                                                 current: currentPrint(file.url, marker: marker,
                                                                       ownName: ownName(provider, context.bundle),
                                                                       wholeFile: file.wholeFile))
                let folder = AgentHookInstaller.agentFolder(provider, home: context.home, environment: context.environment)
                    ?? file.url.deletingLastPathComponent()
                let state = shownState(derived: derived, present: present, folderExists: ClaudeAccounts.isFolder(folder.path),
                                       enabled: context.shown.contains(provider), optedOut: context.optOut.contains(id))
                targets.append(HookTarget(id: id, provider: provider, place: .agent(provider, folder: nil),
                                          name: provider.displayName, folder: folder, file: file.url,
                                          wholeFile: file.wholeFile, manual: false, present: present,
                                          derived: derived, state: state))
            }
            for setting in context.folders where setting.provider == provider {
                targets.append(agentTarget(provider, setting: setting, context: context, present: present))
            }
        }
        for tool in context.tools {
            let id = tool.targetID
            let url = tool.file
            let base = HookRecords.Base(AgentHookInstaller.customState(url, tool: tool.id, socket: context.socket,
                                                                      bundle: context.bundle))
            let derived = HookRecords.derive(base, record: context.records[id], current: currentPrint(url, marker: marker))
            let state = shownState(derived: derived, present: true,
                                   folderExists: ClaudeAccounts.isFolder(url.deletingLastPathComponent().path),
                                   enabled: true, optedOut: context.optOut.contains(id))
            targets.append(HookTarget(id: id, provider: .custom, place: .custom(tool), name: tool.name,
                                      folder: url.deletingLastPathComponent(), file: url, wholeFile: false, manual: true,
                                      present: true, derived: derived, state: state))
        }
        return targets
    }

    private static func agentTarget(_ provider: ClaudeSessionProvider, setting: AgentFolderSetting, context: Context,
                                    present: Bool? = nil) -> HookTarget {
        let folder = setting.url
        let id = setting.targetID
        let marker = ClaudeHookInstaller.marker(bundle: context.bundle)
        let file = AgentHookInstaller.file(provider, folder: folder, bundle: context.bundle)
        let whole = [.copilot, .pi].contains(provider)
        let base = HookRecords.Base(AgentHookInstaller.state(provider, home: context.home, socket: context.socket,
                                                             bundle: context.bundle, folder: folder))
        let derived = HookRecords.derive(base, record: context.records[id],
                                         current: file.flatMap { currentPrint($0, marker: marker, wholeFile: whole) })
        let present = present ?? (provider == .codex ? context.codexPresent : true)
        let approval = provider == .codex && derived == .installed
            && CodexHookTrust.trusted(folder: folder, bundle: context.bundle) == false
        let state = shownState(derived: derived, present: present, folderExists: ClaudeAccounts.isFolder(folder.path),
                               enabled: provider == .codex || context.shown.contains(provider),
                               optedOut: context.optOut.contains(id), needsApproval: approval)
        return HookTarget(id: id, provider: provider, place: .agent(provider, folder: folder),
                          name: setting.name ?? folder.lastPathComponent, folder: folder, file: file,
                          wholeFile: whole, manual: true, present: present, derived: derived, state: state)
    }

    /// A Claude Code account's settings: all the hooks, some, or none.
    static func claudeState(_ url: URL, command: String, bundle: String) -> HookRecords.Base {
        guard let settings = try? ClaudeHookInstaller.read(url) else { return .unreadable }
        if ClaudeHookInstaller.isInstalled(in: settings, command: command) { return .installed }
        return ClaudeHookInstaller.hasAny(in: settings, bundle: bundle) ? .outdated : .missing
    }

    static func ownName(_ provider: ClaudeSessionProvider, _ bundle: String) -> String? {
        provider == .antigravity ? AgentHookInstaller.ownName(bundle) : nil
    }

    static func currentPrint(_ file: URL, marker: String, ownName: String? = nil, wholeFile: Bool = false) -> String? {
        HookRecords.fingerprint(of: try? ClaudeHookInstaller.contents(file), marker: marker, ownName: ownName,
                                wholeFile: wholeFile)
    }

    /// A label for each session heard through a second Codex home's hooks or
    /// a tool's, by session id: the folder's or the tool's name.
    static func labels(_ sessions: [ClaudeSession], defaults: UserDefaults = .standard) -> [String: String] {
        let folders = AgentFolders.all(defaults).filter { $0.provider == .codex }
        let tools = CustomHookTools.all(defaults)
        var labels: [String: String] = [:]
        for session in sessions {
            guard let via = session.via else { continue }
            if session.provider == .custom {
                labels[session.id] = tools.first { $0.id == via }?.name ?? session.provider.displayName
            } else if let folder = folders.first(where: { AgentHookInstaller.folderTag($0.url) == via }) {
                labels[session.id] = folder.name ?? folder.url.lastPathComponent
            }
        }
        return labels
    }

    /// One line for Settings › Sessions: how many files work, how many want a look.
    static func summary(_ targets: [HookTarget]) -> (working: Int, attention: Int) {
        (targets.filter { $0.state == .installed }.count, targets.filter { $0.state.needsAttention }.count)
    }
}

// MARK: Config folders added by hand

/// A config folder the person added for an agent: a second Codex home, a
/// second Gemini folder (#236). Kept as JSON under `notchSessionsAgentFolders`.
struct AgentFolderSetting: Codable, Equatable, Hashable {
    var agent: String
    var path: String
    var name: String?

    static let maximumPerAgent = 8

    var provider: ClaudeSessionProvider? { ClaudeSessionProvider(rawValue: agent) }
    var url: URL { URL(fileURLWithPath: path, isDirectory: true) }
    var targetID: String { "agent:\(agent):\(ClaudeAccounts.canonical(path))" }
}

enum AgentFolders {
    static func all(_ defaults: UserDefaults = .standard) -> [AgentFolderSetting] {
        guard let text = defaults.string(forKey: DefaultsKey.notchSessionsAgentFolders), text.utf8.count <= 64 * 1024,
              let decoded = try? JSONDecoder().decode([AgentFolderSetting].self, from: Data(text.utf8)) else { return [] }
        var seen = Set<String>()
        return decoded.filter { setting in
            guard let provider = setting.provider, AgentHookInstaller.foldersByHand.contains(provider),
                  setting.path.hasPrefix("/") else { return false }
            return seen.insert(setting.targetID).inserted
        }
    }

    static func save(_ folders: [AgentFolderSetting], _ defaults: UserDefaults = .standard) {
        guard !folders.isEmpty else { return defaults.removeObject(forKey: DefaultsKey.notchSessionsAgentFolders) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        if let data = try? encoder.encode(folders) {
            defaults.set(String(decoding: data, as: UTF8.self), forKey: DefaultsKey.notchSessionsAgentFolders)
        }
    }

    /// The folder the agent's hook file lives in, from the one picked: a
    /// folder holding `.gemini` stands for that `.gemini`, as Gemini's
    /// `GEMINI_CLI_HOME` does.
    static func configFolder(_ provider: ClaudeSessionProvider, picked: URL) -> URL {
        let picked = picked.standardizedFileURL
        if provider == .gemini, picked.lastPathComponent != ".gemini",
           ClaudeAccounts.isFolder(picked.appending(path: ".gemini").path) {
            return picked.appending(path: ".gemini", directoryHint: .isDirectory)
        }
        return picked
    }

    enum AddFailure: Error, Equatable {
        case notFolder, alreadyListed, tooMany
    }

    /// Adds a folder; `defaultFolder` is the one the agent reads already,
    /// which needs no second row.
    static func add(_ provider: ClaudeSessionProvider, picked: URL, defaultFolder: URL?,
                    defaults: UserDefaults = .standard) -> Result<AgentFolderSetting, AddFailure> {
        let folder = configFolder(provider, picked: picked)
        guard folder.path.hasPrefix("/"), ClaudeAccounts.isFolder(folder.path) else { return .failure(.notFolder) }
        let id = ClaudeAccounts.canonical(folder.path)
        if let defaultFolder, ClaudeAccounts.canonical(defaultFolder.path) == id { return .failure(.alreadyListed) }
        var folders = all(defaults)
        let setting = AgentFolderSetting(agent: provider.rawValue, path: folder.path)
        guard !folders.contains(where: { $0.targetID == setting.targetID }) else { return .failure(.alreadyListed) }
        guard folders.filter({ $0.agent == provider.rawValue }).count < AgentFolderSetting.maximumPerAgent else {
            return .failure(.tooMany)
        }
        folders.append(setting)
        save(folders, defaults)
        return .success(setting)
    }

    static func remove(_ setting: AgentFolderSetting, defaults: UserDefaults = .standard) {
        save(all(defaults).filter { $0.targetID != setting.targetID }, defaults)
    }

    /// The folder a Codex hook's tag names, for a session's label.
    static func folder(tag: String, _ defaults: UserDefaults = .standard) -> AgentFolderSetting? {
        all(defaults).first { $0.provider == .codex && AgentHookInstaller.folderTag($0.url) == tag }
    }
}

// MARK: Tools with Claude Code style hooks

/// A command-line tool that runs hooks the way Claude Code does, added by
/// name and config file (#236). Kept as JSON under `notchSessionsCustomHookTools`.
struct CustomHookTool: Codable, Equatable, Hashable, Identifiable {
    /// A UUID, which the tool's hooks carry.
    var id: String
    var name: String
    /// The settings file its hooks go into.
    var path: String

    static let maximum = 8
    static let longestName = 32

    var file: URL { URL(fileURLWithPath: path) }
    var targetID: String { "custom:\(id)" }
}

enum CustomHookTools {
    static func all(_ defaults: UserDefaults = .standard) -> [CustomHookTool] {
        guard let text = defaults.string(forKey: DefaultsKey.notchSessionsCustomHookTools), text.utf8.count <= 64 * 1024,
              let decoded = try? JSONDecoder().decode([CustomHookTool].self, from: Data(text.utf8)) else { return [] }
        var seen = Set<String>()
        return Array(decoded.filter { tool in
            AgentHookInstaller.isTag(tool.id) && tool.path.hasPrefix("/") && !tool.name.isEmpty && seen.insert(tool.id).inserted
        }.prefix(CustomHookTool.maximum))
    }

    static func save(_ tools: [CustomHookTool], _ defaults: UserDefaults = .standard) {
        guard !tools.isEmpty else { return defaults.removeObject(forKey: DefaultsKey.notchSessionsCustomHookTools) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        if let data = try? encoder.encode(tools) {
            defaults.set(String(decoding: data, as: UTF8.self), forKey: DefaultsKey.notchSessionsCustomHookTools)
        }
    }

    enum AddFailure: Error, Equatable {
        case noName, notInFolder, alreadyListed, tooMany, claudeAccount
    }

    /// Adds a tool by its name and the settings file its hooks go into. The
    /// file may not exist yet; its folder must.
    static func add(name: String, file: URL, accounts: [ClaudeAccount],
                    defaults: UserDefaults = .standard) -> Result<CustomHookTool, AddFailure> {
        let name = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(CustomHookTool.longestName))
        guard !name.isEmpty else { return .failure(.noName) }
        let file = file.standardizedFileURL
        guard file.path.hasPrefix("/"), ClaudeAccounts.isFolder(file.deletingLastPathComponent().path),
              !ClaudeAccounts.isFolder(file.path) else { return .failure(.notInFolder) }
        let resolved = file.resolvingSymlinksInPath().path
        // A Claude Code account's settings get Claude Code's own hooks.
        if accounts.contains(where: { $0.settingsURL.resolvingSymlinksInPath().path == resolved }) {
            return .failure(.claudeAccount)
        }
        var tools = all(defaults)
        guard !tools.contains(where: { $0.file.resolvingSymlinksInPath().path == resolved }) else { return .failure(.alreadyListed) }
        guard tools.count < CustomHookTool.maximum else { return .failure(.tooMany) }
        let tool = CustomHookTool(id: UUID().uuidString.lowercased(), name: name, path: file.path)
        tools.append(tool)
        save(tools, defaults)
        return .success(tool)
    }

    static func remove(_ tool: CustomHookTool, defaults: UserDefaults = .standard) {
        save(all(defaults).filter { $0.id != tool.id }, defaults)
    }

    static func tool(_ id: String, _ defaults: UserDefaults = .standard) -> CustomHookTool? {
        all(defaults).first { $0.id == id }
    }
}

// MARK: Folders the island made

/// Folders the island made to hold a hook file (`~/.copilot/hooks`,
/// `~/.config/opencode/plugins`), so Remove everything can take them away
/// again once empty.
enum HookFolders {
    private static let lock = NSLock()
    /// Where the list is kept; a test points it at its own.
    nonisolated(unsafe) static var store: UserDefaults = .standard

    static func made(_ defaults: UserDefaults = store) -> [String] {
        defaults.stringArray(forKey: DefaultsKey.hooksMadeFolders) ?? []
    }

    /// Notes each folder on the way to `folder` that does not exist yet.
    static func noteMade(_ folder: URL, defaults: UserDefaults = store) {
        var missing: [String] = []
        var current = folder.standardizedFileURL
        while current.path != "/", !FileManager.default.fileExists(atPath: current.path) {
            missing.append(current.path)
            current = current.deletingLastPathComponent()
        }
        guard !missing.isEmpty else { return }
        lock.lock()
        defer { lock.unlock() }
        var list = made(defaults)
        for path in missing where !list.contains(path) { list.append(path) }
        defaults.set(list, forKey: DefaultsKey.hooksMadeFolders)
    }

    /// Takes away each folder the island made that is empty now, deepest
    /// first; a folder that holds anything stays.
    static func removeEmpty(defaults: UserDefaults = store) {
        lock.lock()
        defer { lock.unlock() }
        let manager = FileManager.default
        var kept: [String] = []
        for path in made(defaults).sorted(by: { $0.count > $1.count }) {
            guard manager.fileExists(atPath: path) else { continue }
            let entries = ((try? manager.contentsOfDirectory(atPath: path)) ?? []).filter { $0 != ".DS_Store" }
            if entries.isEmpty, (try? manager.removeItem(atPath: path)) != nil { continue }
            kept.append(path)
        }
        if kept.isEmpty { defaults.removeObject(forKey: DefaultsKey.hooksMadeFolders) } else { defaults.set(kept, forKey: DefaultsKey.hooksMadeFolders) }
    }
}

// MARK: Codex's hook review

/// Whether Codex trusts the island's hooks in a Codex home. Codex runs a
/// hook from `hooks.json` only once the person trusted it in its hook
/// review, which keeps `[hooks.state."<key>"] trusted_hash` in the home's
/// `config.toml`. The hash is Codex's own: a SHA-256 of the hook's event,
/// matcher and handler as sorted, compact JSON (codex-rs/hooks discovery.rs,
/// `hook_hash`; codex-rs/config fingerprint.rs, `version_for_toml`).
enum CodexHookTrust {
    static func snake(_ event: String) -> String {
        var out = ""
        for (index, character) in event.enumerated() {
            if character.isUppercase, index > 0 { out.append("_") }
            out.append(character.lowercased())
        }
        return out
    }

    /// Codex's timeout as it runs it: SessionEnd and Interrupt at most 3
    /// seconds (1 when unset), every other event 600 when unset.
    static func timeout(_ event: String, _ value: Int?) -> Int {
        if event == "SessionEnd" || event == "Interrupt" { return min(max(value ?? 1, 1), 3) }
        return max(value ?? 600, 1)
    }

    /// JSON text as serde_json writes it.
    static func json(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case "\u{08}": out += "\\b"
            case "\u{0C}": out += "\\f"
            default:
                if scalar.value < 0x20 { out += String(format: "\\u%04x", scalar.value) } else { out.unicodeScalars.append(scalar) }
            }
        }
        return out + "\""
    }

    /// The hash Codex trusts one command hook by.
    static func hash(event: String, matcher: String?, command: String, timeout value: Int?, async: Bool = false) -> String {
        let handler = "{\"async\":\(async),\"command\":\(json(command)),\"timeout\":\(timeout(event, value)),\"type\":\"command\"}"
        var text = "{\"event_name\":\(json(snake(event))),\"hooks\":[\(handler)]"
        if let matcher { text += ",\"matcher\":\(json(matcher))" }
        text += "}"
        return "sha256:" + SHA256.hash(data: Data(text.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// The island's hooks in `hooks.json`, by Codex's key for each, with the
    /// hash each must be trusted by.
    static func expected(_ hooksFile: URL, settings: [String: Any], marker: String) -> [String: String] {
        let path = realPath(hooksFile)
        var expected: [String: String] = [:]
        let hooks = settings["hooks"] as? [String: Any] ?? [:]
        for (event, value) in hooks {
            for (groupIndex, group) in (value as? [[String: Any]] ?? []).enumerated() {
                for (handlerIndex, handler) in (group["hooks"] as? [[String: Any]] ?? []).enumerated() {
                    guard let command = handler["command"] as? String, command.contains(marker) else { continue }
                    let key = "\(path):\(snake(event)):\(groupIndex):\(handlerIndex)"
                    expected[key] = hash(event: event, matcher: group["matcher"] as? String, command: command,
                                         timeout: (handler["timeout"] as? NSNumber)?.intValue,
                                         async: handler["async"] as? Bool ?? false)
                }
            }
        }
        return expected
    }

    /// Whether Codex trusts every hook of the island's in the folder; nil
    /// when it holds none or can't be read.
    static func trusted(folder: URL, bundle: String) -> Bool? {
        let file = folder.appending(path: "hooks.json")
        guard let settings = try? ClaudeHookInstaller.read(file) else { return nil }
        let expected = expected(file, settings: settings, marker: ClaudeHookInstaller.marker(bundle: bundle))
        guard !expected.isEmpty else { return nil }
        let config = (try? String(contentsOf: folder.appending(path: "config.toml"), encoding: .utf8)) ?? ""
        let state = trustedHashes(config)
        return expected.allSatisfy { state[$0.key] == $0.value }
    }

    /// The path as Codex resolves it, links and all (`/tmp` is `/private/tmp`).
    static func realPath(_ url: URL) -> String {
        let folder = url.deletingLastPathComponent().path
        guard let resolved = realpath(folder, nil) else { return url.path }
        defer { free(resolved) }
        return String(cString: resolved) + "/" + url.lastPathComponent
    }

    /// `trusted_hash` by hook key, from a `config.toml`: tables named
    /// `[hooks.state."<key>"]`, and keys of an inline table under `[hooks.state]`.
    static func trustedHashes(_ toml: String) -> [String: String] {
        var hashes: [String: String] = [:]
        var table: [String] = []
        for raw in toml.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("[") {
                // `[table]` or `[[array]]`, a comment perhaps after it.
                let inner = line.hasPrefix("[[") ? "" : String(line.dropFirst())
                table = inner.lastIndex(of: "]").flatMap { keyPath(String(inner[..<$0])) } ?? []
                continue
            }
            guard let equals = equalsIndex(line) else { continue }
            guard let left = keyPath(String(line[..<equals])) else { continue }
            let right = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
            let full = table + left
            if full.count == 4, full[0] == "hooks", full[1] == "state", full[3] == "trusted_hash",
               let value = string(right) {
                hashes[full[2]] = value
            } else if full.count == 3, full[0] == "hooks", full[1] == "state", right.hasPrefix("{"),
                      let value = inlineHash(right) {
                hashes[full[2]] = value
            }
        }
        return hashes
    }

    /// A dotted TOML key: bare parts and quoted ones.
    static func keyPath(_ text: String) -> [String]? {
        var parts: [String] = []
        var index = text.startIndex
        func skipSpace() { while index < text.endIndex, text[index] == " " || text[index] == "\t" { index = text.index(after: index) } }
        while true {
            skipSpace()
            guard index < text.endIndex else { return parts.isEmpty ? nil : parts }
            let character = text[index]
            if character == "\"" || character == "'" {
                guard let (value, end) = quoted(text, from: index) else { return nil }
                parts.append(value)
                index = end
            } else {
                let start = index
                while index < text.endIndex, text[index].isLetter || text[index].isNumber || text[index] == "_" || text[index] == "-" {
                    index = text.index(after: index)
                }
                guard start != index else { return nil }
                parts.append(String(text[start..<index]))
            }
            skipSpace()
            guard index < text.endIndex else { return parts }
            guard text[index] == "." else { return nil }
            index = text.index(after: index)
        }
    }

    /// A quoted TOML string starting at `start`, and where it ends.
    static func quoted(_ text: String, from start: String.Index) -> (String, String.Index)? {
        let quote = text[start]
        var index = text.index(after: start)
        var value = ""
        while index < text.endIndex {
            let character = text[index]
            if character == quote { return (value, text.index(after: index)) }
            if quote == "\"", character == "\\" {
                index = text.index(after: index)
                guard index < text.endIndex else { return nil }
                switch text[index] {
                case "n": value.append("\n")
                case "t": value.append("\t")
                case "r": value.append("\r")
                case "\\": value.append("\\")
                case "\"": value.append("\"")
                default: value.append(text[index])
                }
            } else {
                value.append(character)
            }
            index = text.index(after: index)
        }
        return nil
    }

    static func string(_ text: String) -> String? {
        guard let first = text.first, first == "\"" || first == "'" else { return nil }
        return quoted(text, from: text.startIndex)?.0
    }

    /// The `=` of a key/value line, outside quotes.
    static func equalsIndex(_ line: String) -> String.Index? {
        var quote: Character?
        for index in line.indices {
            let character = line[index]
            if let open = quote { if character == open { quote = nil } ; continue }
            if character == "\"" || character == "'" { quote = character; continue }
            if character == "#" { return nil }
            if character == "=" { return index }
        }
        return nil
    }

    static func inlineHash(_ text: String) -> String? {
        guard let range = text.range(of: "trusted_hash") else { return nil }
        let rest = text[range.upperBound...].drop { $0 == " " || $0 == "=" }
        return string(String(rest))
    }
}
