// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// What the Hooks page does to a config file (#236): Reinstall, Remove, and
/// Remove everything, each with a copy of the file kept first, and putting
/// back hooks that were lost.
enum HookMaintenance {
    struct Environment {
        var home: URL
        var socket: String
        var bundle: String
        var defaults: UserDefaults = .standard
    }

    // MARK: Rows the person took the hooks out of

    static func optOut(_ defaults: UserDefaults = .standard) -> Set<String> {
        Set(defaults.stringArray(forKey: DefaultsKey.hooksOptOut) ?? [])
    }

    static func setOptOut(_ id: String, _ out: Bool, defaults: UserDefaults = .standard) {
        var ids = optOut(defaults)
        if out { ids.insert(id) } else { ids.remove(id) }
        if ids.isEmpty { defaults.removeObject(forKey: DefaultsKey.hooksOptOut) } else { defaults.set(ids.sorted(), forKey: DefaultsKey.hooksOptOut) }
    }

    /// The hook agents whose default row the person took the hooks out of,
    /// which keeping hooks in step leaves alone.
    static func optedOutAgents(_ defaults: UserDefaults = .standard) -> Set<ClaudeSessionProvider> {
        let ids = optOut(defaults)
        return Set(ClaudeSessionProvider.installedAgents.filter { ids.contains(HookRecords.agentID($0)) })
    }

    static func autoRepair(_ defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.hooksAutoRepair) as? Bool ?? true
    }

    // MARK: Changing one file

    static func marker(_ env: Environment) -> String { ClaudeHookInstaller.marker(bundle: env.bundle) }

    /// Puts the island's hooks into the target's file, a copy kept first.
    static func install(_ target: HookTarget, env: Environment) throws {
        switch target.place {
        case .claude(let url):
            let command = ClaudeHookInstaller.command(socket: env.socket, bundle: env.bundle)
            try ClaudeHookInstaller.update(url) { ClaudeHookInstaller.installing(into: $0, command: command, bundle: env.bundle) }
        case .agent(let provider, let folder):
            try AgentHookInstaller.install(provider, home: env.home, socket: env.socket, bundle: env.bundle, folder: folder)
        case .custom(let tool):
            try AgentHookInstaller.installCustom(tool.file, tool: tool.id, socket: env.socket, bundle: env.bundle)
        }
        guard let file = target.file else { return }
        HookRecords.noteInstalled(target.id, file: file, marker: marker(env),
                                  ownName: HookTargets.ownName(target.provider, env.bundle), wholeFile: target.wholeFile,
                                  in: env.defaults)
    }

    /// Takes the island's hooks out of the target's file, a copy kept first.
    static func remove(_ target: HookTarget, env: Environment) throws {
        switch target.place {
        case .claude(let url):
            guard try ClaudeHookInstaller.contents(url) != nil else { break }
            try ClaudeHookInstaller.update(url) { ClaudeHookInstaller.removing(from: $0, bundle: env.bundle) }
        case .agent(let provider, let folder):
            try AgentHookInstaller.remove(provider, home: env.home, bundle: env.bundle, folder: folder)
        case .custom(let tool):
            try AgentHookInstaller.removeCustom(tool.file, bundle: env.bundle)
        }
        HookRecords.forget(target.id, in: env.defaults)
    }

    /// Reinstall: the row is the person's again to keep in step.
    static func reinstall(_ target: HookTarget, env: Environment) throws {
        setOptOut(target.id, false, defaults: env.defaults)
        try install(target, env: env)
    }

    /// Remove: the hooks come out and stay out until Reinstall.
    static func takeOut(_ target: HookTarget, env: Environment) throws {
        setOptOut(target.id, true, defaults: env.defaults)
        try remove(target, env: env)
    }

    /// A folder or tool taken off the page: its hooks out, and its file as
    /// it was before the island, as Remove everything leaves it.
    static func forget(_ target: HookTarget, env: Environment) throws {
        try remove(target, env: env)
        setOptOut(target.id, false, defaults: env.defaults)
        guard let file = target.file else { return }
        try restoreOriginal(file, now: Date())
        if target.wholeFile { dropOwnCopies(of: file) }
        HookFolders.removeEmpty()
    }

    /// Takes the island out of every file it wrote into, putting back each
    /// file as it was before the island where nothing else changed in it
    /// since, and taking away the files and folders the island made. Every
    /// row is left out of keeping in step until Reinstall. Failures by row id.
    static func removeEverything(_ targets: [HookTarget], env: Environment) -> [String: ClaudeHookInstaller.Failure] {
        var failures: [String: ClaudeHookInstaller.Failure] = [:]
        var done = Set<String>()
        for target in targets {
            guard let file = target.file else { continue }
            // Rows that held hooks stay without them; an agent's own row too,
            // or its switch would put them in again. A row that never had any
            // is left as it was.
            if case .agent(_, folder: nil) = target.place { setOptOut(target.id, true, defaults: env.defaults) }
            else if target.canRemove { setOptOut(target.id, true, defaults: env.defaults) }
            // Two rows over one linked file change it once.
            guard done.insert(file.resolvingSymlinksInPath().path).inserted else { continue }
            do {
                try remove(target, env: env)
                try restoreOriginal(file, now: Date())
                if target.wholeFile { dropOwnCopies(of: file) }
            } catch let failure as ClaudeHookInstaller.Failure {
                failures[target.id] = failure
            } catch {
                failures[target.id] = .unwritable(error.localizedDescription)
            }
        }
        HookFolders.removeEmpty()
        return failures
    }

    /// The copies of a file that was the island's alone hold nothing of the
    /// person's; in a folder the island made they go with it.
    static func dropOwnCopies(of url: URL) {
        let folder = url.deletingLastPathComponent()
        guard HookFolders.made().contains(folder.standardizedFileURL.path) else { return }
        let prefix = "\(url.lastPathComponent).openbar-"
        for name in (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        where name.hasPrefix(prefix) && name.hasSuffix(".bak") {
            try? FileManager.default.removeItem(at: folder.appending(path: name))
        }
    }

    // MARK: The file before the island

    /// Once the island's hooks are out: the file as it was before the island
    /// when that is all that is left in it, or no file when the island made
    /// it. A file the person changed since stays as it is now. A copy of what
    /// is replaced or taken away is kept beside it.
    static func restoreOriginal(_ url: URL, now: Date) throws {
        let manager = FileManager.default
        let copy = ClaudeHookInstaller.originalCopy(of: url), absent = ClaudeHookInstaller.absentNote(of: url)
        let current = try ClaudeHookInstaller.contents(url)
        if manager.fileExists(atPath: absent.path) {
            if let current {
                guard isLeftover(current) else { return ClaudeHookInstaller.forgetOriginal(of: url) }
                try ClaudeHookInstaller.keepCopy(of: current, beside: url, now: now)
                do { try manager.removeItem(at: url) } catch { throw ClaudeHookInstaller.Failure.unwritable(error.localizedDescription) }
            }
            ClaudeHookInstaller.forgetOriginal(of: url)
            return
        }
        guard let original = manager.contents(atPath: copy.path) else { return }
        defer { ClaudeHookInstaller.forgetOriginal(of: url) }
        guard let current, current != original, sameContent(current, original) else { return }
        try ClaudeHookInstaller.replace(url, original: current, with: original, now: now)
    }

    /// Whether a file holds only what the island adds around its hooks: an
    /// empty object, an empty `hooks`, Cursor's and Devin's `version`.
    static func isLeftover(_ data: Data) -> Bool {
        if data.allSatisfy({ $0 == 0x20 || $0 == 0x0A || $0 == 0x0D || $0 == 0x09 }) { return true }
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return false }
        return json.allSatisfy { key, value in isEmptyContainer(value) || (key == "version" && (value as? NSNumber) == 1) }
    }

    /// Whether what is left after the hooks came out is the file from
    /// before: the same JSON once keys the island added empty are set aside
    /// (Cursor's and Devin's `version`, an emptied `hooks`), or the same text.
    static func sameContent(_ current: Data, _ original: Data) -> Bool {
        if let now = (try? JSONSerialization.jsonObject(with: current)) as? [String: Any],
           let before = (try? JSONSerialization.jsonObject(with: original)) as? [String: Any] {
            let added = now.filter { key, value in
                before[key] == nil && (isEmptyContainer(value) || (key == "version" && (value as? NSNumber) == 1))
            }
            let kept = now.filter { added[$0.key] == nil }
            return NSDictionary(dictionary: kept).isEqual(to: before)
        }
        let trim: (Data) -> String = { String(decoding: $0, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines) }
        return trim(current) == trim(original)
    }

    private static func isEmptyContainer(_ value: Any) -> Bool {
        if let dict = value as? [String: Any] { return dict.isEmpty }
        if let list = value as? [Any] { return list.isEmpty }
        return false
    }

    // MARK: Putting lost hooks back

    /// The rows whose hooks were lost and come back by themselves: hooks the
    /// island put in that are gone or from an older version, on a row that is
    /// on, found, and not taken out by the person. Hooks changed by hand are
    /// the person's. Agents' own rows are kept in step with their switches
    /// elsewhere (`AgentHookInstaller.plan`).
    static func repairs(_ targets: [HookTarget], records: [String: HookRecords.Record], autoRepair: Bool) -> [HookTarget] {
        guard autoRepair else { return [] }
        return targets.filter { target in
            if case .agent(_, folder: nil) = target.place { return false }
            guard target.file != nil, records[target.id] != nil else { return false }
            return (target.state == .missing && target.derived == .removedByHand) || target.state == .outdated
        }
    }
}
