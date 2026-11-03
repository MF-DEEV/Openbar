// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit

extension LegacyIdentity {
    // MARK: Startup

    /// Runs at launch, before any feature reads its settings. Settings and the
    /// data folder come over once; the shared leftovers are checked each
    /// launch, since the old app may be removed later.
    static func run(defaults: UserDefaults = .standard,
                    home: URL = FileManager.default.homeDirectoryForCurrentUser) {
        guard let current = Bundle.main.bundleIdentifier, let legacy = bundleID(for: current) else { return }
        let firstRun = !defaults.bool(forKey: migratedKey)
        if firstRun {
            if let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
                migrateDataFolder(support: support, current: current, legacy: legacy)
            }
            migrateDefaults(defaults, current: current, legacy: legacy)
            defaults.set(true, forKey: migratedKey)
        }
        let legacyInstalled = NSWorkspace.shared.urlForApplication(withBundleIdentifier: legacy) != nil
        let socket = ClaudeSessionService.socketPath(home: home)
        DispatchQueue.global(qos: .utility).async {
            migrateHooks(home: home, current: current, legacy: legacy, socket: socket,
                         install: firstRun, removeLegacy: !legacyInstalled)
            guard !legacyInstalled else { return }
            // The editor extension went by the old name; put the current one
            // in each editor that had it, then take the old one out.
            let editors = ClaudeBridgeInstaller.editorsWithLegacy(home: home)
            for ide in editors where !ClaudeBridgeInstaller.isInstalled(ide, home: home) {
                _ = ClaudeBridgeInstaller.install(ide, home: home)
            }
            if !editors.isEmpty {
                ClaudeBridgeInstaller.uninstall(home: home, identifier: ClaudeBridgeInstaller.legacyIdentifier)
            }
            // The folder both builds talked through: the Developer build
            // still uses it while it keeps the old ID.
            let siblings = [legacy, legacy.hasSuffix(".dev") ? String(legacy.dropLast(4)) : legacy + ".dev"]
            if siblings.allSatisfy({ NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0) == nil }) {
                try? FileManager.default.removeItem(at: home.appending(path: ".vorssaint", directoryHint: .isDirectory))
            }
        }
    }
}
