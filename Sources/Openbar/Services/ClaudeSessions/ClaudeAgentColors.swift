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
    /// between its first two `---` lines, read as Claude Code reads it: after
    /// a byte order mark, with any line ending, and without a comment.
    static func frontmatter(_ data: Data) -> (name: String?, color: String?) {
        var head = String(decoding: data.prefix(headLimit), as: UTF8.self)
        if head.hasPrefix("\u{FEFF}") { head.removeFirst() }
        // "\r\n" is one character, which a split on "\n" alone would miss.
        var lines = head.split(omittingEmptySubsequences: false, whereSeparator: \.isNewline).makeIterator()
        guard lines.next()?.trimmingCharacters(in: .whitespaces) == "---" else { return (nil, nil) }
        var name: String?, color: String?
        while let line = lines.next() {
            let text = line.trimmingCharacters(in: .whitespaces)
            if text == "---" { return (name, color) }
            guard let colon = text.firstIndex(of: ":") else { continue }
            let key = text[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            let value = scalar(text[text.index(after: colon)...])
            guard !value.isEmpty else { continue }
            if key == "name" { name = value } else if key == "color" { color = value }
        }
        // No closing line: what was read was not frontmatter.
        return (nil, nil)
    }

    /// A one-line value: what its quotes hold, or the words before a comment.
    /// Anything else after the quotes is not YAML, and Claude Code then takes
    /// the whole line as written, which names no color.
    static func scalar(_ raw: Substring) -> String {
        let text = raw.trimmingCharacters(in: .whitespaces)
        if let quote = text.first, quote == "\"" || quote == "'" {
            let inside = text.dropFirst()
            guard let end = inside.firstIndex(of: quote) else { return text }
            let rest = inside[inside.index(after: end)...]
            let after = rest.trimmingCharacters(in: .whitespaces)
            guard after.isEmpty || (after.hasPrefix("#") && rest.first?.isWhitespace == true) else { return text }
            return String(inside[..<end])
        }
        if text.hasPrefix("#") { return "" }
        let comment = text.range(of: " #")?.lowerBound ?? text.range(of: "\t#")?.lowerBound ?? text.endIndex
        return text[..<comment].trimmingCharacters(in: .whitespaces)
    }

    /// Where an agent called `type` may be defined, in the order Claude Code
    /// prefers: a plugin's agent by its `plugin:agent` name; otherwise the
    /// project's own agents, from its folder up, then the person's.
    static func folders(type: String, cwd: String, home: URL, plugins: Data?, readsProtected: Bool) -> [URL] {
        if let colon = type.firstIndex(of: ":") {
            let plugin = String(type[..<colon])
            return pluginFolders(plugins, plugin: plugin).filter { readsProtected || !isProtected($0.path, home: home) }
        }
        var folders: [URL] = []
        var folder = URL(fileURLWithPath: plain(cwd), isDirectory: true).standardizedFileURL
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
        let path = plain(path)
        let homePath = home.standardizedFileURL.path
        let guarded = ["Desktop", "Documents", "Downloads", "Library/Mobile Documents", "Library/CloudStorage",
                       "Pictures", "Movies", "Music"]
            .map { homePath + "/" + $0 }
        return guarded.contains { path == $0 || path.hasPrefix($0 + "/") } || path == "/Volumes" || path.hasPrefix("/Volumes/")
    }

    /// The same path without the system's own name for the data volume, by
    /// which a home folder can also be reached.
    static func plain(_ path: String) -> String {
        let data = "/System/Volumes/Data"
        return path.hasPrefix(data + "/") ? String(path.dropFirst(data.count)) : path
    }

    /// The agent folders of a plugin synced from the person's account, which
    /// Claude Code keeps under `plugins/synced`, a folder per account. Each
    /// account's `manifest.json` lists its plugins by folder, with "~g" and
    /// a generation after the name when it gives one; older copies stay
    /// beside them for a while and are not read. Without a manifest to go
    /// by, every folder is looked at, the highest generation first.
    static func syncedPluginFolders(_ root: URL, plugin: String) -> [URL] {
        let files = FileManager.default
        guard let accounts = try? files.contentsOfDirectory(atPath: root.path) else { return [] }
        var folders: [URL] = []
        for account in accounts.sorted().prefix(20) {
            let base = root.appending(path: account, directoryHint: .isDirectory)
            let names = json(base.appending(path: "manifest.json")).map(liveFolders)
                ?? ((try? files.contentsOfDirectory(atPath: base.path)) ?? []).sorted(by: newer).prefix(filesLimit).map { $0 }
            for name in names {
                let folder = base.appending(path: name, directoryHint: .isDirectory)
                let named = ["plugin.json", ".claude-plugin/plugin.json"].contains {
                    json(folder.appending(path: $0))?["name"] as? String == plugin
                }
                if named { folders.append(folder.appending(path: "agents", directoryHint: .isDirectory)) }
            }
        }
        return folders
    }

    /// The folders an account's synced plugins live in, as its manifest
    /// names them: `{"plugins": [{"name": "canva"}, {"name": "…", "generation": 5}]}`.
    static func liveFolders(_ manifest: [String: Any]) -> [String] {
        guard let plugins = manifest["plugins"] as? [[String: Any]] else { return [] }
        return plugins.prefix(filesLimit).compactMap { entry in
            guard let name = entry["name"] as? String, !name.isEmpty, !name.contains("/"), name != ".", name != ".." else { return nil }
            guard let generation = entry["generation"] as? Int, generation > 1 else { return name }
            return "\(name)~g\(generation)"
        }
    }

    /// Whether one plugin folder's copy is newer than another's: `name~g10`
    /// after `name~g9`, and both after the bare `name`.
    static func newer(_ lhs: String, _ rhs: String) -> Bool {
        func split(_ folder: String) -> (String, Int) {
            guard let mark = folder.range(of: "~g", options: .backwards),
                  let generation = Int(folder[mark.upperBound...]) else { return (folder, 1) }
            return (String(folder[..<mark.lowerBound]), generation)
        }
        let left = split(lhs), right = split(rhs)
        return left.0 == right.0 ? left.1 > right.1 : left.0 > right.0
    }

    /// A small JSON object from a plain file, or nil.
    private static func json(_ file: URL) -> [String: Any]? {
        guard (try? file.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]))
                .map({ $0.isRegularFile == true && ($0.fileSize ?? .max) < 1 << 20 }) == true,
              let data = FileManager.default.contents(atPath: file.path) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
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
        var folders = ClaudeAgentDefinitions.folders(type: type, cwd: cwd, home: home, plugins: plugins,
                                                     readsProtected: readsProtected)
        if let colon = type.firstIndex(of: ":") {
            let plugin = String(type[..<colon])
            folders += ClaudeSessionRegistryReader.homes(home).flatMap {
                ClaudeAgentDefinitions.syncedPluginFolders($0.appending(path: "plugins/synced", directoryHint: .isDirectory),
                                                           plugin: plugin)
            }
        }
        for folder in folders {
            if let found = definition(named: name, in: folder) { return found.color }
        }
        return nil
    }

    /// The definition called `name` in `folder`: by its own `name:` line, or
    /// failing that by its file name.
    private func definition(named name: String, in folder: URL) -> (name: String, color: String?)? {
        // A folder linked into a guarded one is guarded too: listing it is
        // what macOS asks about.
        let folder = folder.resolvingSymlinksInPath()
        guard readsProtected || !ClaudeAgentDefinitions.isProtected(folder.path, home: home),
              let files = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return nil }
        var byFile: (name: String, color: String?)?
        for file in files.filter({ $0.hasSuffix(".md") }).sorted().prefix(ClaudeAgentDefinitions.filesLimit) {
            // Only a plain file: opening a pipe waits for a writer, and would
            // hold up the queue the sessions and their hooks share. A link is
            // followed only where the app may read without a prompt.
            let resolved = folder.appending(path: file).resolvingSymlinksInPath()
            guard readsProtected || !ClaudeAgentDefinitions.isProtected(resolved.path, home: home),
                  (try? resolved.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true,
                  let handle = FileHandle(forReadingAtPath: resolved.path) else { continue }
            let data = (try? handle.read(upToCount: ClaudeAgentDefinitions.headLimit)) ?? Data()
            try? handle.close()
            let head = ClaudeAgentDefinitions.frontmatter(data)
            if head.name == name { return (name, head.color) }
            if head.name == nil, (file as NSString).deletingPathExtension == name { byFile = (name, head.color) }
        }
        return byFile
    }
}
