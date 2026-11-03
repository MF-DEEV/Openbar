// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Carries an install over from the time Openbar still shipped under
/// Vorssaint's bundle IDs (`com.vorssaint.utils`, and `.dev` for the Developer
/// build). Openbar now has its own, so macOS sees it as a new app: settings,
/// the app's data folder and the hooks it wrote for coding agents are brought
/// over here, once. Permissions are the system's and have to be granted again.
///
/// What the old ID left in shared places (hooks in Claude Code's settings, the
/// editor extension, `~/.vorssaint`) is only taken away once no app with that
/// ID is installed: it may still be an older Openbar, or Vorssaint itself.
enum LegacyIdentity {
    static let migratedKey = "legacyIdentityMigrated"

    /// The bundle ID the same build had before Openbar's own name.
    static func bundleID(for current: String) -> String? {
        switch current {
        case "com.openbar.app": return "com.vorssaint.utils"
        case "com.openbar.app.dev": return "com.vorssaint.utils.dev"
        default: return nil
        }
    }

    static func isLegacy(_ bundle: String) -> Bool { bundle.hasPrefix("com.vorssaint.") }

    // MARK: Settings

    /// The old settings as the new app reads them: keys and values that named
    /// Vorssaint take Openbar's name, and paths into the old data folder point
    /// at the new one. Everything else is copied as it was.
    static func migratedDefaults(_ legacy: [String: Any], from old: String, to new: String) -> [String: Any] {
        var result: [String: Any] = [:]
        for (key, value) in legacy {
            result[key.replacingOccurrences(of: "Vorssaint", with: "Openbar")] = migratedValue(value, from: old, to: new)
        }
        // The keep-awake icon's raw value was the old name.
        if result[DefaultsKey.keepAwakeActiveIcon] as? String == "vorssaint" {
            result[DefaultsKey.keepAwakeActiveIcon] = KeepAwakeActiveIcon.openbar.rawValue
        }
        return result
    }

    private static func migratedValue(_ value: Any, from old: String, to new: String) -> Any {
        switch value {
        case let text as String:
            return text.replacingOccurrences(of: "Application Support/\(old)/", with: "Application Support/\(new)/")
        case let list as [Any]:
            return list.map { migratedValue($0, from: old, to: new) }
        case let map as [String: Any]:
            return map.mapValues { migratedValue($0, from: old, to: new) }
        default:
            return value
        }
    }

    /// Copies the old settings into an app that has none of its own yet. A
    /// key already set in the new app wins. Returns whether anything came over.
    @discardableResult
    static func migrateDefaults(_ defaults: UserDefaults = .standard, current: String, legacy: String) -> Bool {
        guard let old = defaults.persistentDomain(forName: legacy), !old.isEmpty else { return false }
        let mine = defaults.persistentDomain(forName: current) ?? [:]
        var merged = migratedDefaults(old, from: legacy, to: current)
        for (key, value) in mine { merged[key] = value }
        defaults.setPersistentDomain(merged, forName: current)
        return true
    }

    // MARK: Data folder

    /// Copies `Application Support/<old id>` to `Application Support/<new id>`
    /// when the new one does not exist yet. Only folders and regular files are
    /// copied; a socket or a link from the old app has no use here.
    @discardableResult
    static func migrateDataFolder(support: URL, current: String, legacy: String) -> Bool {
        let manager = FileManager.default
        let source = support.appending(path: legacy, directoryHint: .isDirectory)
        let target = support.appending(path: current, directoryHint: .isDirectory)
        var isFolder: ObjCBool = false
        guard manager.fileExists(atPath: source.path, isDirectory: &isFolder), isFolder.boolValue,
              !manager.fileExists(atPath: target.path) else { return false }
        // Built beside the target and moved in whole, so a copy cut short
        // never looks like a finished folder.
        let staging = support.appending(path: ".\(current)-migrating-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? manager.removeItem(at: staging) }
        guard copyTree(from: source, to: staging) else { return false }
        return (try? manager.moveItem(at: staging, to: target)) != nil
    }

    private static func copyTree(from source: URL, to target: URL) -> Bool {
        let manager = FileManager.default
        do {
            try manager.createDirectory(at: target, withIntermediateDirectories: true,
                                        attributes: [.posixPermissions: 0o700])
            let keys: [URLResourceKey] = [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey]
            for item in try manager.contentsOfDirectory(at: source, includingPropertiesForKeys: keys) {
                let values = try item.resourceValues(forKeys: Set(keys))
                let destination = target.appending(path: item.lastPathComponent)
                if values.isSymbolicLink == true { continue }
                if values.isDirectory == true {
                    guard copyTree(from: item, to: destination) else { return false }
                } else if values.isRegularFile == true {
                    try manager.copyItem(at: item, to: destination)
                }
            }
            return true
        } catch {
            return false
        }
    }

    // MARK: Hooks

    /// With `install`, puts this app's hooks wherever the old ID had them, so
    /// Sessions keeps working without a trip to Settings; that happens once,
    /// so hooks the person later removes stay removed. With `removeLegacy`,
    /// takes the old ones out. Returns what changed, for the tests.
    @discardableResult
    static func migrateHooks(home: URL, current: String, legacy: String, socket: String,
                             install: Bool, removeLegacy: Bool) -> (claude: Bool, agents: [ClaudeSessionProvider]) {
        var claude = false
        let settings = ClaudeHookInstaller.settingsURL(home: home)
        if let read = try? ClaudeHookInstaller.read(settings), ClaudeHookInstaller.hasAny(in: read, bundle: legacy) {
            let command = ClaudeHookInstaller.command(socket: socket, bundle: current)
            claude = (try? ClaudeHookInstaller.update(settings) { settings in
                var settings = settings
                if removeLegacy { settings = ClaudeHookInstaller.removing(from: settings, bundle: legacy) }
                if install, !ClaudeHookInstaller.isInstalled(in: settings, command: command) {
                    settings = ClaudeHookInstaller.installing(into: settings, command: command, bundle: current)
                }
                return settings
            }) != nil
        }
        var agents: [ClaudeSessionProvider] = []
        for provider in ClaudeSessionProvider.hookAgents {
            let old = AgentHookInstaller.state(provider, home: home, socket: socket, bundle: legacy)
            guard old == .installed || old == .outdated else { continue }
            if install, AgentHookInstaller.state(provider, home: home, socket: socket, bundle: current) != .installed {
                guard (try? AgentHookInstaller.install(provider, home: home, socket: socket, bundle: current)) != nil else { continue }
            }
            if removeLegacy { try? AgentHookInstaller.remove(provider, home: home, bundle: legacy) }
            agents.append(provider)
        }
        return (claude, agents)
    }
}
