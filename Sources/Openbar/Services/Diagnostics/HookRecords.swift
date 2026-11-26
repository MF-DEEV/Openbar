// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CryptoKit
import Foundation

/// What Openbar last wrote into each agent's hook settings, kept so a
/// diagnostics file (#230) can say more than "outdated": hooks an older
/// Openbar wrote, hooks someone changed by hand, and hooks someone took out
/// by hand. Only a fingerprint and a date are kept, never the file.
enum HookRecords {
    struct Record: Codable, Equatable {
        let installedAt: Date
        let fingerprint: String
    }

    /// A hook's state as a diagnostics file shows it.
    enum State: String, Codable {
        case installed
        /// Openbar's own hooks from an older version, or pointing elsewhere.
        case outdated
        /// Openbar's hooks are there, but not as Openbar wrote them.
        case changedByHand
        /// Openbar put hooks in and never took them out, and they are gone.
        case removedByHand
        case missing
        case unreadable
    }

    /// The state the settings pages read, before the records say more.
    enum Base {
        case installed, outdated, missing, unreadable
    }

    static func agentID(_ provider: ClaudeSessionProvider) -> String { "agent:\(provider.rawValue)" }

    /// The folder is resolved, not the file: a settings file (or the link to
    /// one) deleted by hand must still name the record Openbar made for it.
    static func claudeID(_ settings: URL) -> String {
        let folder = settings.deletingLastPathComponent().resolvingSymlinksInPath()
        return "claude:\(folder.appending(path: settings.lastPathComponent).path)"
    }

    // MARK: Store

    static func all(in defaults: UserDefaults = .standard) -> [String: Record] {
        guard let data = defaults.data(forKey: DefaultsKey.diagnosticsHookRecords),
              let records = try? JSONDecoder().decode([String: Record].self, from: data) else { return [:] }
        return records
    }

    /// Notes what Openbar just wrote into `file`.
    static func noteInstalled(_ id: String, file: URL, marker: String, ownName: String? = nil, wholeFile: Bool = false,
                              at date: Date = Date(), in defaults: UserDefaults = .standard) {
        guard let print = fingerprint(of: try? ClaudeHookInstaller.contents(file), marker: marker,
                                      ownName: ownName, wholeFile: wholeFile) else { return forget(id, in: defaults) }
        update(in: defaults) { $0[id] = Record(installedAt: date, fingerprint: print) }
    }

    /// Openbar took its hooks out itself: nothing to miss any more.
    static func forget(_ id: String, in defaults: UserDefaults = .standard) {
        update(in: defaults) { $0[id] = nil }
    }

    private static let lock = NSLock()

    private static func update(in defaults: UserDefaults, _ change: (inout [String: Record]) -> Void) {
        lock.lock()
        defer { lock.unlock() }
        var records = all(in: defaults)
        change(&records)
        if records.isEmpty {
            defaults.removeObject(forKey: DefaultsKey.diagnosticsHookRecords)
        } else if let data = try? JSONEncoder().encode(records) {
            defaults.set(data, forKey: DefaultsKey.diagnosticsHookRecords)
        }
    }

    // MARK: Reading

    /// What the state the settings show becomes once the record is weighed.
    static func derive(_ base: Base, record: Record?, current: String?) -> State {
        switch base {
        case .installed: return .installed
        case .unreadable: return .unreadable
        case .missing: return record == nil ? .missing : .removedByHand
        case .outdated:
            // Hooks from before records were kept can't be told apart.
            guard let record else { return .outdated }
            return current == record.fingerprint ? .outdated : .changedByHand
        }
    }

    /// A fingerprint of Openbar's part of a settings file: the whole file
    /// when the file is Openbar's own (a Copilot hook file, an OpenCode or Pi
    /// plugin), otherwise every string that carries Openbar's marker, plus
    /// the entry under Openbar's own name (Antigravity) and the lines of a
    /// marked block (Vibe's hooks.toml). Nil when there is no file.
    static func fingerprint(of data: Data?, marker: String, ownName: String? = nil, wholeFile: Bool = false) -> String? {
        guard let data else { return nil }
        if wholeFile { return hex(SHA256.hash(data: data)) }
        var parts: [String] = []
        if let json = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) {
            collect(json, marker: marker, into: &parts)
            if let ownName, let dict = json as? [String: Any], let own = dict[ownName],
               let canonical = try? JSONSerialization.data(withJSONObject: own, options: [.sortedKeys, .fragmentsAllowed]) {
                parts.append("\(ownName)=\(String(decoding: canonical, as: UTF8.self))")
            }
        } else if let text = String(data: data, encoding: .utf8) {
            var inBlock = false
            for line in text.components(separatedBy: "\n") {
                let marked = line.contains(marker)
                if marked && line.contains("\(marker) start") { inBlock = true }
                if inBlock || marked { parts.append(line) }
                if marked && line.hasSuffix("\(marker) end") { inBlock = false }
            }
        }
        guard !parts.isEmpty else { return nil }
        return hex(SHA256.hash(data: Data(parts.sorted().joined(separator: "\n").utf8)))
    }

    private static func collect(_ value: Any, marker: String, into parts: inout [String]) {
        switch value {
        case let text as String where text.contains(marker): parts.append(text)
        case let list as [Any]: list.forEach { collect($0, marker: marker, into: &parts) }
        case let dict as [String: Any]: dict.values.forEach { collect($0, marker: marker, into: &parts) }
        default: break
        }
    }

    private static func hex(_ digest: SHA256.Digest) -> String {
        digest.map { String(format: "%02x", $0) }.joined()
    }
}

extension HookRecords.Base {
    init(_ state: AgentHookInstaller.State) {
        switch state {
        case .installed: self = .installed
        case .outdated: self = .outdated
        case .missing: self = .missing
        case .unreadable: self = .unreadable
        }
    }

    init(_ state: ClaudeAccountHooks) {
        switch state {
        case .installed: self = .installed
        case .outdated: self = .outdated
        case .missing: self = .missing
        case .unreadable: self = .unreadable
        }
    }
}

extension AgentHookInstaller {
    /// The file that holds an agent's hooks, and whether the whole file is
    /// Openbar's own.
    static func hookFile(_ provider: ClaudeSessionProvider, home: URL, bundle: String) -> (url: URL, wholeFile: Bool)? {
        switch provider {
        case .gemini: return (geminiSettings(home: home), false)
        case .cursor: return (cursorHooks(home: home), false)
        case .copilot: return (copilotHooks(home: home, bundle: bundle), true)
        case .opencode: return (opencodePlugin(home: home, bundle: bundle), true)
        case .pi: return (piExtension(home: home, bundle: bundle), true)
        case .vibe: return (vibeHooks(home: home), false)
        case .devin: return (devinConfig(home: home), false)
        case .antigravity: return (antigravityHooks(home: home), false)
        case .grok: return (grokHooks(home: home, bundle: bundle), true)
        case .zcode: return (zcodeConfig(home: home), false)
        case .hermes: return (AgentHermes.config(home: home), false)
        case .qwen: return (AgentQwen.settings(home: home), false)
        case .droid: return (AgentDroid.settings(home: home), false)
        case .kimi: return (AgentKimi.config(home: home), false)
        case .kiro: return (AgentKiro.hooks(home: home, bundle: bundle), true)
        // Cline gets no hooks: the island reads its own session store (#241).
        case .claude, .codex, .cline, .custom: return nil
        }
    }

    /// The record id, file and fingerprint parameters for one agent's hooks.
    static func noteInstalled(_ provider: ClaudeSessionProvider, home: URL, bundle: String) {
        guard let file = hookFile(provider, home: home, bundle: bundle) else { return }
        HookRecords.noteInstalled(HookRecords.agentID(provider), file: file.url, marker: marker(bundle: bundle),
                                  ownName: provider == .antigravity ? ownName(bundle) : nil, wholeFile: file.wholeFile)
    }

    /// The agent's hooks as a diagnostics file shows them.
    static func diagnosticsState(_ provider: ClaudeSessionProvider, home: URL, socket: String, bundle: String,
                                 records: [String: HookRecords.Record]) -> HookRecords.State {
        let base = HookRecords.Base(state(provider, home: home, socket: socket, bundle: bundle))
        let file = hookFile(provider, home: home, bundle: bundle)
        let current = file.flatMap { file in
            HookRecords.fingerprint(of: try? ClaudeHookInstaller.contents(file.url), marker: marker(bundle: bundle),
                                    ownName: provider == .antigravity ? ownName(bundle) : nil, wholeFile: file.wholeFile)
        }
        return HookRecords.derive(base, record: records[HookRecords.agentID(provider)], current: current)
    }
}
