// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// One Claude Code account: a config folder, `~/.claude` or one that
/// `CLAUDE_CONFIG_DIR` names, which keeps its own sign-in, settings and
/// sessions (issue #216).
struct ClaudeAccount: Equatable, Identifiable, Hashable {
    /// The folder as found, standardized.
    let path: String
    /// The name the person gave it, or one made from the folder.
    var name: String
    /// Whether the person named it.
    var named: Bool
    /// Added by hand rather than found.
    var manual: Bool
    /// The person asked Openbar to leave it alone: no hooks, limits or place
    /// in the New session picker.
    var hidden: Bool

    var id: String { ClaudeAccounts.canonical(path) }
    var settingsURL: URL { URL(fileURLWithPath: path, isDirectory: true).appending(path: "settings.json") }
}

/// What the person changed about an account, kept as JSON under
/// `DefaultsKey.notchSessionsClaudeAccounts`.
struct ClaudeAccountSetting: Codable, Equatable {
    var path: String
    var name: String?
    var hidden: Bool?
    var manual: Bool?

    static let maximum = 16
    static let longestName = 32

    static func decode(_ value: String) -> [Self] {
        guard value.utf8.count <= 64 * 1024,
              let decoded = try? JSONDecoder().decode([Self].self, from: Data(value.utf8)) else { return [] }
        var seen = Set<String>()
        return decoded.prefix(maximum).compactMap { setting in
            guard setting.path.hasPrefix("/"), seen.insert(ClaudeAccounts.canonical(setting.path)).inserted else { return nil }
            var setting = setting
            setting.name = setting.name.map { String($0.trimmingCharacters(in: .whitespacesAndNewlines).prefix(longestName)) }
                .flatMap { $0.isEmpty ? nil : $0 }
            return setting
        }
    }

    static func encode(_ settings: [Self]) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return (try? encoder.encode(settings)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
    }

    /// Nothing left to remember about the account.
    var isEmpty: Bool { name == nil && hidden != true && manual != true }
}

enum ClaudeAccounts {
    static let defaultName = "Default"

    /// A folder's identity: through links, without a trailing slash.
    static func canonical(_ path: String) -> String {
        let resolved = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.resolvingSymlinksInPath().path
        return resolved.count > 1 && resolved.hasSuffix("/") ? String(resolved.dropLast()) : resolved
    }

    static func settings(_ defaults: UserDefaults = .standard) -> [ClaudeAccountSetting] {
        ClaudeAccountSetting.decode(defaults.string(forKey: DefaultsKey.notchSessionsClaudeAccounts) ?? "")
    }

    static func save(_ settings: [ClaudeAccountSetting], _ defaults: UserDefaults = .standard) {
        let kept = settings.filter { !$0.isEmpty }
        if kept.isEmpty { defaults.removeObject(forKey: DefaultsKey.notchSessionsClaudeAccounts) }
        else { defaults.set(ClaudeAccountSetting.encode(kept), forKey: DefaultsKey.notchSessionsClaudeAccounts) }
    }

    /// The folders the person added by hand that still exist, which the
    /// session list reads beside the ones it finds.
    static func manualFolders(_ defaults: UserDefaults = .standard) -> [URL] {
        settings(defaults).filter { $0.manual == true && isFolder($0.path) }
            .map { URL(fileURLWithPath: $0.path, isDirectory: true) }
    }

    /// `~/.claude` is the default; `~/.claude-work` is Work; any other folder
    /// goes by its own name.
    static func name(for path: String, home: URL) -> String {
        let folder = URL(fileURLWithPath: path, isDirectory: true)
        if canonical(folder.path) == canonical(home.appending(path: ".claude").path) { return defaultName }
        let last = folder.lastPathComponent
        if folder.deletingLastPathComponent().lastPathComponent == ".config", last == "claude" { return "Config" }
        var base = last
        if base.hasPrefix(".claude") { base = String(base.dropFirst(".claude".count)) }
        base = String(base.drop { $0 == "-" || $0 == "_" || $0 == "." })
        if base.isEmpty { base = String(last.drop { $0 == "." }) }
        guard !base.isEmpty else { return defaultName }
        let capitalized = base.prefix(1).uppercased() + base.dropFirst()
        return String(capitalized.prefix(ClaudeAccountSetting.longestName))
    }

    /// Every account on this Mac, hidden ones too: each config folder the
    /// session list reads that exists, then those added by hand.
    static func all(found: [URL], settings: [ClaudeAccountSetting], home: URL) -> [ClaudeAccount] {
        var seen = Set<String>()
        var accounts: [ClaudeAccount] = []
        let manual = settings.filter { $0.manual == true }.map { URL(fileURLWithPath: $0.path, isDirectory: true) }
        for folder in found + manual {
            let path = folder.standardizedFileURL.path
            let id = canonical(path)
            guard isFolder(path), seen.insert(id).inserted else { continue }
            let setting = settings.first { canonical($0.path) == id }
            accounts.append(ClaudeAccount(path: path, name: setting?.name ?? name(for: path, home: home),
                                          named: setting?.name != nil, manual: setting?.manual == true,
                                          hidden: setting?.hidden == true))
        }
        return accounts
    }

    static func all(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                    defaults: UserDefaults = .standard) -> [ClaudeAccount] {
        // Another home is a test's or a sandbox's: the person's own choices stay out of it.
        let own = home.standardizedFileURL.path == FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        return all(found: ClaudeSessionRegistryReader.homes(home), settings: own ? settings(defaults) : [], home: home)
    }

    /// The account a session's config folder belongs to.
    static func account(for configHome: String?, in accounts: [ClaudeAccount]) -> ClaudeAccount? {
        guard let configHome else { return nil }
        let id = canonical(configHome)
        return accounts.first { $0.id == id }
    }

    /// The accounts the listed sessions run under, by id: only sessions on
    /// this Mac, since a host's folders are its own.
    static func inUse(_ sessions: [ClaudeSession]) -> [String] {
        var seen = Set<String>()
        return sessions.compactMap { session in
            guard session.provider == .claude, session.host.remoteAlias == nil, let account = session.account else { return nil }
            let id = canonical(account)
            return seen.insert(id).inserted ? id : nil
        }
    }

    /// A label for each session, by session id, only while sessions of more
    /// than one account are listed: with one, nothing changes.
    static func labels(_ sessions: [ClaudeSession], accounts: [ClaudeAccount], home: URL) -> [String: String] {
        guard inUse(sessions).count > 1 else { return [:] }
        var labels: [String: String] = [:]
        for session in sessions where session.provider == .claude && session.host.remoteAlias == nil {
            guard let path = session.account else { continue }
            labels[session.id] = account(for: path, in: accounts)?.name ?? name(for: path, home: home)
        }
        return labels
    }

    // MARK: Changes

    static func rename(_ account: ClaudeAccount, to name: String, defaults: UserDefaults = .standard) {
        change(account, defaults: defaults) { setting in
            let trimmed = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(ClaudeAccountSetting.longestName))
            setting.name = trimmed.isEmpty ? nil : trimmed
        }
    }

    static func setHidden(_ account: ClaudeAccount, _ hidden: Bool, defaults: UserDefaults = .standard) {
        change(account, defaults: defaults) { $0.hidden = hidden ? true : nil }
    }

    /// Adds a folder by hand. Nil when it is not a folder or already listed.
    @discardableResult
    static func add(_ folder: URL, home: URL = FileManager.default.homeDirectoryForCurrentUser,
                    defaults: UserDefaults = .standard) -> ClaudeAccount? {
        let path = folder.standardizedFileURL.path
        guard path.hasPrefix("/"), isFolder(path) else { return nil }
        let id = canonical(path)
        var settings = settings(defaults)
        if let index = settings.firstIndex(where: { canonical($0.path) == id }) {
            // A hidden one comes back instead.
            guard settings[index].hidden == true else { return nil }
            settings[index].hidden = nil
        } else {
            guard !all(home: home, defaults: defaults).contains(where: { $0.id == id }) else { return nil }
            guard settings.count < ClaudeAccountSetting.maximum else { return nil }
            settings.append(ClaudeAccountSetting(path: path, manual: true))
        }
        save(settings, defaults)
        return all(found: [], settings: settings, home: home).first { $0.id == id }
    }

    /// Forgets a folder added by hand.
    static func remove(_ account: ClaudeAccount, defaults: UserDefaults = .standard) {
        save(settings(defaults).filter { canonical($0.path) != account.id }, defaults)
    }

    private static func change(_ account: ClaudeAccount, defaults: UserDefaults,
                               _ body: (inout ClaudeAccountSetting) -> Void) {
        var settings = settings(defaults)
        if let index = settings.firstIndex(where: { canonical($0.path) == account.id }) {
            body(&settings[index])
        } else {
            var setting = ClaudeAccountSetting(path: account.path, manual: account.manual ? true : nil)
            body(&setting)
            settings.append(setting)
        }
        save(settings, defaults)
    }

    // MARK: New session

    /// The account a new session starts with: the last one used while it is
    /// still listed, else the folder's own, else the first.
    static func preferred(_ accounts: [ClaudeAccount], last: String?, folder: String?) -> ClaudeAccount? {
        let shown = accounts.filter { !$0.hidden }
        if let last, let account = account(for: last, in: shown) { return account }
        if let folder, let account = account(for: folder, in: shown) { return account }
        return shown.first
    }

    static func isFolder(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &directory) && directory.boolValue
    }
}

/// Whether an account's settings hold the island's hooks.
enum ClaudeAccountHooks: Equatable {
    case installed
    /// An older hook of this app, as one pointing at an old socket.
    case outdated
    case missing
    /// The settings are not JSON Claude Code could read either; left alone.
    case unreadable
}

/// The plan limits of a Claude account in use beside the one the AI
/// section's reading follows (#216).
struct ClaudeAccountLimits: Equatable {
    var limits: AgentLimits?
    var plan: AgentPlan?
    /// Waiting its turn or being checked.
    var checking = false
    var failure: AgentClaudeServer.Failure?
    /// When the last check began; nil until one has.
    var attempted: Date?
    /// Checks in a row that Claude Code did not answer.
    var unreachable = 0

    /// The reading without windows that renewed since.
    func current(at now: Date) -> AgentLimits? { AgentClaudeServer.current(limits, now: now) }
}
