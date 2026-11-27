// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import CryptoKit
import Darwin
import Foundation

/// A JetBrains IDE on this Mac (#329): any app built on the IntelliJ
/// Platform, told by the `product-info.json` every one of them carries,
/// IntelliJ IDEA, PyCharm, WebStorm, GoLand, PhpStorm, RubyMine, CLion,
/// Rider, RustRover and Android Studio alike, from Toolbox or not.
struct ClaudeJetBrainsIDE: Equatable, Identifiable {
    let app: URL
    let bundle: String
    /// "PyCharm", as the IDE names itself.
    let name: String
    let version: String
    /// The folder its settings and plugins live in, like "PyCharm2026.3".
    let dataDirectory: String
    /// "JetBrains", or "Google" for Android Studio: the folder above that one.
    let vendor: String
    var id: String { "\(vendor)/\(dataDirectory)" }

    /// Where the IDE loads plugins from at start.
    func plugins(home: URL) -> URL {
        home.appending(path: "Library/Application Support/\(vendor)/\(dataDirectory)/plugins", directoryHint: .isDirectory)
    }

    /// "PyCharm 2026.3": the name and the release its settings folder belongs to.
    var plugin: String {
        let release = version.split(separator: ".").prefix(2).joined(separator: ".")
        return release.isEmpty ? name : "\(name) \(release)"
    }
}

/// The JetBrains bridge plugin (Tools/JetBrainsBridge) and the IDEs it goes
/// into. Without it Open still finds the project's window; with it Open
/// selects the session's terminal tab and Reply types into it.
enum ClaudeJetBrains {
    /// What the IDE's terminal tells a shell it runs in.
    static let terminalEmulator = "JetBrains-JediTerm"
    static let pluginFile = "openbar-terminal-bridge.jar"
    /// Products on the platform that aren't IDEs with a terminal: Gateway,
    /// the remote development client, Fleet.
    static let notIDEs: Set<String> = ["GW", "JBC", "FL", "CWMGuest"]

    /// The identifiers LaunchServices is asked for; any other app with a
    /// product-info.json is found by looking in the usual folders.
    static let knownBundles = [
        "com.jetbrains.intellij", "com.jetbrains.intellij.ce", "com.jetbrains.pycharm", "com.jetbrains.pycharm.ce",
        "com.jetbrains.WebStorm", "com.jetbrains.goland", "com.jetbrains.PhpStorm", "com.jetbrains.rubymine",
        "com.jetbrains.CLion", "com.jetbrains.rider", "com.jetbrains.rustrover", "com.jetbrains.datagrip",
        "com.jetbrains.dataspell", "com.jetbrains.aqua", "com.google.android.studio",
    ]

    // MARK: Telling an IDE

    /// The IDE an app bundle is, from its product-info.json; nil for any other app.
    static func ide(app: URL) -> ClaudeJetBrainsIDE? {
        let info = app.appending(path: "Contents/Resources/product-info.json")
        guard let data = FileManager.default.contents(atPath: info.path), data.count < 1 << 20,
              let product = try? JSONDecoder().decode(ProductInfo.self, from: data),
              !notIDEs.contains(product.productCode ?? ""), plain(product.dataDirectoryName),
              let bundle = Bundle(url: app)?.bundleIdentifier else { return nil }
        let vendor = product.productVendor.flatMap { plain($0) ? $0 : nil } ?? "JetBrains"
        return ClaudeJetBrainsIDE(app: app.standardizedFileURL, bundle: bundle, name: product.name,
                                  version: product.version ?? "", dataDirectory: product.dataDirectoryName, vendor: vendor)
    }

    private struct ProductInfo: Decodable {
        let name: String
        let version: String?
        let productCode: String?
        let dataDirectoryName: String
        let productVendor: String?
    }

    /// A name with no way out of the folder it is joined to.
    static func plain(_ name: String) -> Bool {
        !name.isEmpty && !name.contains("/") && name != "." && name != ".."
    }

    private static let cache = NSLock()
    private static var known: Set<String> = []

    /// Whether an app identifier is a JetBrains IDE's. A running copy is
    /// asked first, since a session runs in one, then LaunchServices. Only a
    /// yes is kept: an IDE installed later is found then.
    static func isIDE(_ bundle: String?) -> Bool {
        guard let bundle, !bundle.isEmpty else { return false }
        if knownBundles.contains(bundle) { return true }
        guard bundle.hasPrefix("com.jetbrains.") || bundle.hasPrefix("com.google.android.studio") else { return false }
        cache.lock()
        if known.contains(bundle) {
            cache.unlock()
            return true
        }
        cache.unlock()
        let apps = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).compactMap(\.bundleURL)
            + [NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle)].compactMap { $0 }
        guard apps.contains(where: { ide(app: $0) != nil }) else { return false }
        cache.lock()
        known.insert(bundle)
        cache.unlock()
        return true
    }

    private static var names: [String: String] = [:]

    /// The IDE's name as it calls itself ("PyCharm"), from the running app
    /// or the one LaunchServices knows; the app's own name otherwise.
    static func name(bundle: String, ide pid: Int32?) -> String {
        cache.lock()
        if let name = names[bundle] {
            cache.unlock()
            return name
        }
        cache.unlock()
        let app = pid.flatMap { NSRunningApplication(processIdentifier: $0)?.bundleURL }
            ?? NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first?.bundleURL
            ?? NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle)
        guard let found = app.flatMap(ide(app:))?.name else { return ClaudeTerminalApps.name(of: bundle) }
        cache.lock()
        names[bundle] = found
        cache.unlock()
        return found
    }

    /// The IDE a running process belongs to: its app's bundle.
    static func ide(pid: Int32) -> ClaudeJetBrainsIDE? {
        NSRunningApplication(processIdentifier: pid)?.bundleURL.flatMap(ide(app:))
    }

    /// The IDE process above a session: the nearest ancestor that is the app
    /// `bundle` names. The IDE starts the shell of each terminal tab itself.
    static func process(above pid: Int32, bundle: String, parent: (Int32) -> Int32? = AgentHookProcess.parent(of:),
                        bundleOf: (Int32) -> String? = { NSRunningApplication(processIdentifier: $0)?.bundleIdentifier }) -> Int32? {
        var current = pid
        for _ in 0..<8 {
            guard let next = parent(current), next > 1 else { return nil }
            if bundleOf(next) == bundle { return next }
            current = next
        }
        return nil
    }

    // MARK: Finding the IDEs

    /// Every JetBrains IDE on this Mac, one per settings folder: two copies
    /// of the same version share their settings and plugins.
    static func installed(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [ClaudeJetBrainsIDE] {
        var apps: [URL] = []
        for bundle in knownBundles {
            apps += NSWorkspace.shared.urlsForApplications(withBundleIdentifier: bundle)
        }
        apps += candidates(in: [URL(fileURLWithPath: "/Applications"), home.appending(path: "Applications")])
        // Toolbox before 2.0 kept its IDEs in its own folder.
        let toolbox = home.appending(path: "Library/Application Support/JetBrains/Toolbox/apps")
        for product in folders(in: toolbox) {
            for channel in folders(in: product) where channel.lastPathComponent.hasPrefix("ch-") {
                for build in folders(in: channel) { apps += candidates(in: [build]) }
            }
        }
        var seen = Set<String>()
        return apps.compactMap(ide(app:))
            .filter { !$0.app.path.contains("/.Trash/") && seen.insert($0.id).inserted }
            .sorted { ($0.name, $0.version) < ($1.name, $1.version) }
    }

    /// The apps in these folders and one level below them ("~/Applications/JetBrains Toolbox").
    private static func candidates(in roots: [URL]) -> [URL] {
        var found: [URL] = []
        for root in roots {
            for item in folders(in: root) {
                if item.pathExtension == "app" { found.append(item) }
                else { found += folders(in: item).filter { $0.pathExtension == "app" } }
            }
        }
        return found
    }

    private static func folders(in folder: URL) -> [URL] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        return names.filter { !$0.hasPrefix(".") }.map { folder.appending(path: $0, directoryHint: .isDirectory) }
    }

    // MARK: The plugin

    /// The plugin the app carries.
    static var bundledPlugin: URL? {
        Bundle.main.url(forResource: "JetBrainsBridge", withExtension: nil)?.appending(path: pluginFile)
    }

    enum State: Equatable {
        case missing, current, outdated
    }

    static func state(_ ide: ClaudeJetBrainsIDE, home: URL, bundled: URL? = bundledPlugin) -> State {
        let installed = ide.plugins(home: home).appending(path: pluginFile)
        guard FileManager.default.fileExists(atPath: installed.path) else { return .missing }
        // The jar is built the same each time from the same sources, so a
        // different one is another version.
        guard let bundled, let ours = digest(bundled) else { return .current }
        return digest(installed) == ours ? .current : .outdated
    }

    private static func digest(_ file: URL) -> SHA256.Digest? {
        guard let data = try? Data(contentsOf: file), data.count < 16 << 20 else { return nil }
        return SHA256.hash(data: data)
    }

    enum Failure: Equatable {
        case missing
        case other(String)
    }

    /// Puts the plugin the app carries into the IDE's plugins folder, which
    /// it loads at its next start. Nil when it worked.
    static func install(_ ide: ClaudeJetBrainsIDE, home: URL, bundled: URL? = bundledPlugin) -> Failure? {
        guard let bundled, FileManager.default.fileExists(atPath: bundled.path) else { return .missing }
        let folder = ide.plugins(home: home)
        let target = folder.appending(path: pluginFile)
        let partial = folder.appending(path: ".\(pluginFile).\(UUID().uuidString)")
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: bundled, to: partial)
            // Never half a jar where the IDE looks: the new one takes the old one's place at once.
            if FileManager.default.fileExists(atPath: target.path) {
                _ = try FileManager.default.replaceItemAt(target, withItemAt: partial)
            } else {
                try FileManager.default.moveItem(at: partial, to: target)
            }
            return nil
        } catch {
            try? FileManager.default.removeItem(at: partial)
            return .other(error.localizedDescription)
        }
    }

    /// Takes the plugin out of the IDE; a running IDE keeps it until it restarts.
    static func remove(_ ide: ClaudeJetBrainsIDE, home: URL) -> Failure? {
        let target = ide.plugins(home: home).appending(path: pluginFile)
        guard FileManager.default.fileExists(atPath: target.path) else { return nil }
        do {
            try FileManager.default.removeItem(at: target)
            return nil
        } catch {
            return .other(error.localizedDescription)
        }
    }

    /// Takes the plugin out of every IDE that has it, when the app goes.
    static func removeAll(home: URL) {
        for ide in installed(home: home) where state(ide, home: home, bundled: nil) != .missing {
            _ = remove(ide, home: home)
        }
    }

    /// Whether the IDE runs now, so a change waits for its restart.
    static func isRunning(_ ide: ClaudeJetBrainsIDE) -> Bool {
        NSRunningApplication.runningApplications(withBundleIdentifier: ide.bundle)
            .contains { $0.bundleURL?.standardizedFileURL == ide.app }
    }

    /// The plugin is loaded and listening in the IDE process `ide`: its
    /// socket, named after that process, is there.
    static func listening(ide: Int32, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        let socket = ClaudeTerminalBridge.folder(home: home).appending(path: "\(ide).sock").path
        var info = stat()
        return lstat(socket, &info) == 0 && (info.st_mode & S_IFMT) == S_IFSOCK
    }

    /// The plugin is in the IDE's plugins folder, loaded or waiting for a restart.
    static func installed(bundle: String, home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Bool {
        let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first?.bundleURL
            ?? NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle)
        guard let ide = app.flatMap(ide(app:)) else { return false }
        return FileManager.default.fileExists(atPath: ide.plugins(home: home).appending(path: pluginFile).path)
    }

    // MARK: The project window

    /// The project open in the IDE process `ide` that holds the session's
    /// folder, from the lock file Claude Code's JetBrains plugin keeps for
    /// that IDE; nil when it keeps none.
    static func openProject(cwd: String, ide: Int32, homes: [URL]) -> String? {
        let folders = ClaudeIDELock.all(homes: homes).filter { $0.pid == Int(ide) }.flatMap { $0.workspaceFolders ?? [] }
        return ClaudeIDELock.openFolder(cwd: cwd, folders: folders)
    }

    /// The project a JetBrains window title names: "alpha – README.md",
    /// "alpha [~/work/alpha] – README.md", or just "alpha".
    static func project(inTitle title: String) -> String {
        var name = title
        for separator in [" – ", " — ", " - "] {
            if let range = name.range(of: separator) { name = String(name[..<range.lowerBound]) }
        }
        if let bracket = name.range(of: " [") { name = String(name[..<bracket.lowerBound]) }
        return name.trimmingCharacters(in: .whitespaces)
    }

    /// The window to raise among the IDE's: the one whose project is the
    /// deepest folder holding the session's folder, when exactly one is.
    static func window(for cwd: String, titles: [String]) -> Int? {
        let projects = titles.map { project(inTitle: $0).lowercased() }
        var folder = URL(fileURLWithPath: cwd).standardizedFileURL
        for _ in 0..<12 {
            let name = folder.lastPathComponent.lowercased()
            guard !name.isEmpty, name != "/" else { return nil }
            let matches = projects.indices.filter { projects[$0] == name }
            if matches.count == 1 { return matches[0] }
            if matches.count > 1 { return nil }
            folder.deleteLastPathComponent()
        }
        return nil
    }
}
