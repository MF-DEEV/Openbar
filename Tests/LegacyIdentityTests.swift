// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

enum LegacyIdentityTests {
    static func run(_ suite: TestSuite) {
        suite.run("legacy identity settings") { settings(suite) }
        suite.run("legacy identity data folder") { dataFolder(suite) }
        suite.run("legacy identity hooks") { hooks(suite) }
    }

    static func settings(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/me")
        suite.expect(!LegacyIdentity.installed(among: [
            URL(fileURLWithPath: "/Users/me/Desktop/Projekty/x/build/stage.noindex/Vorssaint (Developer).app"),
            URL(fileURLWithPath: "/Users/me/.Trash/Vorssaint (Developer).app"),
        ], home: home), "a copy in a build folder or the Trash is not an installed app")
        suite.expect(LegacyIdentity.installed(among: [URL(fileURLWithPath: "/Applications/Vorssaint.app")], home: home)
                     && LegacyIdentity.installed(among: [URL(fileURLWithPath: "/Users/me/Applications/Vorssaint.app")], home: home),
                     "an app in either Applications folder is")
        suite.expect(LegacyIdentity.bundleID(for: "com.openbar.app") == "com.vorssaint.utils"
                     && LegacyIdentity.bundleID(for: "com.openbar.app.dev") == "com.vorssaint.utils.dev"
                     && LegacyIdentity.bundleID(for: "com.other") == nil,
                     "the release and the Developer build each come from their own old ID")
        let old = "com.openbar.tests.legacy-old"
        let new = "com.openbar.tests.legacy-new"
        let defaults = UserDefaults(suiteName: new)!
        defer {
            defaults.removePersistentDomain(forName: old)
            defaults.removePersistentDomain(forName: new)
        }
        defaults.removePersistentDomain(forName: new)
        suite.expect(!LegacyIdentity.migrateDefaults(defaults, current: new, legacy: old),
                     "a Mac that never had the old app has nothing to bring over")
        defaults.setPersistentDomain([
            "screenshotHideVorssaintWindows": true,
            DefaultsKey.keepAwakeActiveIcon: "vorssaint",
            DefaultsKey.launchAtLoginWanted: true,
            "folder": "/Users/me/Library/Application Support/\(old)/Shelf",
            "clip": "a note about vorssaint stays as it was",
            "shared": "old",
        ], forName: old)
        defaults.setPersistentDomain(["shared": "mine"], forName: new)
        suite.expect(LegacyIdentity.migrateDefaults(defaults, current: new, legacy: old), "the old settings come over")
        let result = defaults.persistentDomain(forName: new) ?? [:]
        suite.expect(result[DefaultsKey.screenshotHideOpenbarWindows] as? Bool == true && result["screenshotHideVorssaintWindows"] == nil,
                     "a key that named Vorssaint takes Openbar's name")
        suite.expect(result[DefaultsKey.keepAwakeActiveIcon] as? String == KeepAwakeActiveIcon.openbar.rawValue,
                     "the keep-awake icon chosen by its old name stays chosen")
        suite.expect(result[DefaultsKey.launchAtLoginWanted] as? Bool == true, "launch at login is still wanted, so it is set up again")
        suite.expect(result["folder"] as? String == "/Users/me/Library/Application Support/\(new)/Shelf",
                     "a path into the old data folder points at the new one")
        suite.expect(result["clip"] as? String == "a note about vorssaint stays as it was", "other text is copied as it was")
        suite.expect(result["shared"] as? String == "mine", "a setting the new app already has wins")
    }

    static func dataFolder(_ suite: TestSuite) {
        let manager = FileManager.default
        let support = manager.temporaryDirectory.appending(path: "openbar-legacy-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? manager.removeItem(at: support) }
        let old = support.appending(path: "com.vorssaint.utils/ShelfFiles", directoryHint: .isDirectory)
        try? manager.createDirectory(at: old, withIntermediateDirectories: true)
        manager.createFile(atPath: old.appending(path: "a.png").path, contents: Data([1, 2, 3]))
        try? manager.createSymbolicLink(atPath: old.appending(path: "link").path, withDestinationPath: "/etc/hosts")
        suite.expect(LegacyIdentity.migrateDataFolder(support: support, current: "com.openbar.app", legacy: "com.vorssaint.utils"),
                     "the old data folder is copied")
        let copied = support.appending(path: "com.openbar.app/ShelfFiles")
        suite.expect(manager.contents(atPath: copied.appending(path: "a.png").path) == Data([1, 2, 3]),
                     "with the files it held")
        suite.expect((try? manager.destinationOfSymbolicLink(atPath: copied.appending(path: "link").path)) == nil,
                     "but not a link out of it")
        suite.expect(manager.fileExists(atPath: old.appending(path: "a.png").path), "and the old folder is left for the old app")
        manager.createFile(atPath: old.appending(path: "b.png").path, contents: Data([4]))
        suite.expect(!LegacyIdentity.migrateDataFolder(support: support, current: "com.openbar.app", legacy: "com.vorssaint.utils")
                     && !manager.fileExists(atPath: copied.appending(path: "b.png").path),
                     "a data folder the new app already has is never written over")
        let leftovers = (try? manager.contentsOfDirectory(atPath: support.path))?.filter { $0.contains("migrating") } ?? []
        suite.expect(leftovers.isEmpty, "no half-copied folder is left behind")
    }

    static func hooks(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "openbar-legacy-home-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? manager.removeItem(at: home) }
        try? manager.createDirectory(at: home.appending(path: ".claude"), withIntermediateDirectories: true)
        let legacy = "com.vorssaint.utils", current = "com.openbar.app"
        suite.expect(ClaudeHookInstaller.marker(bundle: legacy) == "# vorssaint-hook com.vorssaint.utils"
                     && ClaudeHookInstaller.marker(bundle: current) == "# openbar-hook com.openbar.app",
                     "old hooks are found by the marker they were written with")
        let settingsURL = ClaudeHookInstaller.settingsURL(home: home)
        let old = ClaudeHookInstaller.installing(into: ["model": "opus"],
                                                 command: ClaudeHookInstaller.command(socket: "/tmp/old.sock", bundle: legacy),
                                                 bundle: legacy)
        try? JSONSerialization.data(withJSONObject: old).write(to: settingsURL)
        let socket = "/tmp/new.sock"
        let kept = LegacyIdentity.migrateHooks(home: home, current: current, legacy: legacy, socket: socket,
                                               install: true, removeLegacy: false)
        var read = (try? ClaudeHookInstaller.read(settingsURL)) ?? [:]
        suite.expect(kept.claude && ClaudeHookInstaller.isInstalled(in: read, command: ClaudeHookInstaller.command(socket: socket, bundle: current))
                     && ClaudeHookInstaller.hasAny(in: read, bundle: legacy),
                     "the new hooks go in, and the old app's stay while it is installed")
        _ = try? ClaudeHookInstaller.update(settingsURL) { ClaudeHookInstaller.removing(from: $0, bundle: current) }
        LegacyIdentity.migrateHooks(home: home, current: current, legacy: legacy, socket: socket, install: false, removeLegacy: true)
        read = (try? ClaudeHookInstaller.read(settingsURL)) ?? [:]
        suite.expect(!ClaudeHookInstaller.hasAny(in: read, bundle: legacy) && !ClaudeHookInstaller.hasAny(in: read, bundle: current)
                     && read["model"] as? String == "opus",
                     "once the old app is gone its hooks go, and hooks removed by hand are not put back")

        let opencode = AgentHookInstaller.opencodePlugin(home: home, bundle: legacy)
        suite.expect(opencode.lastPathComponent == "vorssaint-com.vorssaint.utils.js", "the old agent files keep their old names")
        try? manager.createDirectory(at: opencode.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? AgentHookInstaller.opencodeFile(socket: "/tmp/old.sock", bundle: legacy).write(to: opencode)
        let moved = LegacyIdentity.migrateHooks(home: home, current: current, legacy: legacy, socket: socket,
                                                install: true, removeLegacy: true)
        let fresh = AgentHookInstaller.opencodePlugin(home: home, bundle: current)
        suite.expect(moved.agents == [.opencode] && !manager.fileExists(atPath: opencode.path)
                     && manager.fileExists(atPath: fresh.path) && fresh.lastPathComponent == "openbar-com.openbar.app.js",
                     "an agent's old hook file is replaced by Openbar's own")
    }
}
