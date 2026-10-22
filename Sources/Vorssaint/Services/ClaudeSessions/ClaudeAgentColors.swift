// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The color Claude Code gives a subagent, from the `color:` line its
/// definition may start with, so the subagent's Clawd wears that shell. Only
/// the head of a definition is read, and only its name and color are kept.
enum ClaudeAgentDefinitions {
    /// How much of a definition is read: its frontmatter comes first.
    static let headLimit = 4096
    /// Definitions looked at in one folder, at most.
    static let filesLimit = 200

    /// The `name:` and `color:` of a definition's frontmatter, the block
    /// between its first two `---` lines.
    static func frontmatter(_ data: Data) -> (name: String?, color: String?) {
        let head = String(decoding: data.prefix(headLimit), as: UTF8.self)
        var lines = head.split(separator: "\n", omittingEmptySubsequences: false).makeIterator()
        guard lines.next()?.trimmingCharacters(in: .whitespaces) == "---" else { return (nil, nil) }
        var name: String?, color: String?
        while let line = lines.next() {
            let text = line.trimmingCharacters(in: .whitespaces)
            if text == "---" { return (name, color) }
            guard let colon = text.firstIndex(of: ":") else { continue }
            let key = text[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            var value = text[text.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            if value.count >= 2, let first = value.first, first == value.last, first == "\"" || first == "'" {
                value = String(value.dropFirst().dropLast())
            }
            guard !value.isEmpty else { continue }
            if key == "name" { name = value } else if key == "color" { color = value }
        }
        // No closing line: what was read was not frontmatter.
        return (nil, nil)
    }

    /// Where an agent called `type` may be defined, in the order Claude Code
    /// prefers: a plugin's agent by its `plugin:agent` name; otherwise the
    /// project's own agents, from its folder up, then the person's.
    static func folders(type: String, cwd: String, home: URL, plugins: Data?, readsProtected: Bool) -> [URL] {
        if let colon = type.firstIndex(of: ":") {
            let plugin = String(type[..<colon])
            return pluginFolders(plugins, plugin: plugin)
        }
        var folders: [URL] = []
        var folder = URL(fileURLWithPath: cwd, isDirectory: true).standardizedFileURL
        let homePath = home.standardizedFileURL.path
        // Up to the home folder, whose agents are the person's own and come
        // last, or to the top of the disk.
        while !cwd.isEmpty, folder.path != "/", folder.path != homePath {
            if readsProtected || !isProtected(folder.path, home: home) {
                folders.append(folder.appending(path: ".claude/agents", directoryHint: .isDirectory))
            }
            let parent = folder.deletingLastPathComponent()
            guard parent.path != folder.path else { break }
            folder = parent
        }
        folders += ClaudeSessionRegistryReader.homes(home).map { $0.appending(path: "agents", directoryHint: .isDirectory) }
        return folders
    }

    /// Folders macOS guards with a prompt of its own: reading them unasked
    /// would show the person a dialog out of nowhere.
    static func isProtected(_ path: String, home: URL) -> Bool {
        let homePath = home.standardizedFileURL.path
        let guarded = ["Desktop", "Documents", "Downloads", "Library/Mobile Documents", "Pictures", "Movies", "Music"]
            .map { homePath + "/" + $0 }
        return guarded.contains { path == $0 || path.hasPrefix($0 + "/") } || path == "/Volumes" || path.hasPrefix("/Volumes/")
    }

    /// The agent folders of an installed plugin, from Claude Code's
    /// `installed_plugins.json`: `{"plugins": {"name@marketplace": [{"installPath": …}]}}`.
    static func pluginFolders(_ data: Data?, plugin: String) -> [URL] {
        guard let data, data.count < 1 << 20,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let plugins = json["plugins"] as? [String: Any] else { return [] }
        var folders: [URL] = []
        for (key, value) in plugins.sorted(by: { $0.key < $1.key }) {
            guard key.split(separator: "@", maxSplits: 1).first.map(String.init) == plugin,
                  let installs = value as? [[String: Any]] else { continue }
            for install in installs {
                guard let path = install["installPath"] as? String, !path.isEmpty else { continue }
                folders.append(URL(fileURLWithPath: path, isDirectory: true).appending(path: "agents", directoryHint: .isDirectory))
            }
        }
        return folders
    }

    /// The name an agent goes by in `type`: a plugin's agent drops the plugin.
    static func agentName(_ type: String) -> String {
        guard let colon = type.firstIndex(of: ":") else { return type }
        return String(type[type.index(after: colon)...])
    }
}

/// Looks agents' colors up and remembers them for a while, so a definition
/// changed in the meantime is picked up. Used only on the sessions queue.
final class ClaudeAgentColors {
    /// How long an answer is kept before the definitions are read again.
    static let keptFor: TimeInterval = 30

    private let home: URL
    private var known: [String: (color: String?, at: Date)] = [:]
    /// Whether the app may read inside folders macOS guards, which it may
    /// only with Full Disk Access.
    var readsProtected = false

    init(home: URL = FileManager.default.homeDirectoryForCurrentUser) {
        self.home = home
    }

    func reset() { known = [:] }

    /// The color an agent of `type` working in `cwd` is given, if any.
    func color(type: String, cwd: String, now: Date) -> String? {
        guard !type.isEmpty, type != ClaudeSubagent.untyped else { return nil }
        let key = type + "|" + cwd + (readsProtected ? "|all" : "")
        if let entry = known[key], now.timeIntervalSince(entry.at) < Self.keptFor, entry.at <= now { return entry.color }
        let color = lookUp(type: type, cwd: cwd)
        if known.count > 500 { known = [:] }
        known[key] = (color, now)
        return color
    }

    /// The session with each subagent's color filled in.
    func decorate(_ session: ClaudeSession, now: Date) -> ClaudeSession {
        guard !session.subagents.isEmpty else { return session }
        var session = session
        for index in session.subagents.indices {
            session.subagents[index].color = color(type: session.subagents[index].type, cwd: session.cwd, now: now)
        }
        return session
    }

    private func lookUp(type: String, cwd: String) -> String? {
        let plugins = type.contains(":")
            ? ClaudeSessionRegistryReader.homes(home).lazy.compactMap {
                FileManager.default.contents(atPath: $0.appending(path: "plugins/installed_plugins.json").path)
            }.first
            : nil
        let name = ClaudeAgentDefinitions.agentName(type)
        for folder in ClaudeAgentDefinitions.folders(type: type, cwd: cwd, home: home, plugins: plugins,
                                                     readsProtected: readsProtected) {
            if let found = definition(named: name, in: folder) { return found.color }
        }
        return nil
    }

    /// The definition called `name` in `folder`: by its own `name:` line, or
    /// failing that by its file name.
    private func definition(named name: String, in folder: URL) -> (name: String, color: String?)? {
        guard let files = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return nil }
        var byFile: (name: String, color: String?)?
        for file in files.filter({ $0.hasSuffix(".md") }).sorted().prefix(ClaudeAgentDefinitions.filesLimit) {
            let path = folder.appending(path: file).path
            guard let handle = FileHandle(forReadingAtPath: path) else { continue }
            let data = (try? handle.read(upToCount: ClaudeAgentDefinitions.headLimit)) ?? Data()
            try? handle.close()
            let head = ClaudeAgentDefinitions.frontmatter(data)
            if head.name == name { return (name, head.color) }
            if head.name == nil, (file as NSString).deletingPathExtension == name { byFile = (name, head.color) }
        }
        return byFile
    }
}
