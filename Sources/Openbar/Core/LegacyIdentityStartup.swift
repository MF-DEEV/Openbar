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
            let carried = migrateDefaults(defaults, current: current, legacy: legacy)
            defaults.set(true, forKey: migratedKey)
            // macOS ties permissions to the bundle ID, so the features that
            // came over are off until they are granted again. Their requests
            // normally wait for a feature's own switch; ask for the two most
            // of them need once the app is up, so Openbar is in the list.
            if carried {
                DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                    // Neither prompt shows for a permission already granted.
                    Permissions.shared.requestAccessibility()
                    Permissions.shared.requestScreenRecording()
                }
            }
        }
        let legacyInstalled = isInstalled(legacy)
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
            if !siblings.contains(where: isInstalled) {
                try? FileManager.default.removeItem(at: home.appending(path: ".vorssaint", directoryHint: .isDirectory))
            }
        }
    }

    /// Whether an app with this ID is installed. LaunchServices also knows
    /// copies in build folders and the Trash, which would keep the old hooks
    /// forever, so only the Applications folders count.
    static func isInstalled(_ bundle: String) -> Bool {
        installed(among: NSWorkspace.shared.urlsForApplications(withBundleIdentifier: bundle))
    }
}
