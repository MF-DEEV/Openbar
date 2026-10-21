// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Adds the island's hooks to Claude Code's user settings, beside the
/// person's own, and takes them out again. The hook is the system's `nc`
/// writing the event to the island's socket and printing the answer; when
/// the app is not running it connects to nothing and Claude Code carries on
/// as if there were no hook. Only entries carrying this app's marker are
/// ever changed, and the file is copied aside before every change.
enum ClaudeHookInstaller {
    static let events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                         "PostToolUseFailure", "PermissionRequest", "Notification", "Stop", "SubagentStart", "SubagentStop"]
    /// Events matched against a tool name take a matcher.
    static let toolEvents: Set<String> = ["PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest"]
    /// A permission waits on the person; everything else is answered at once.
    static let permissionTimeout = 600
    static let eventTimeout = 5

    enum Failure: Error, Equatable {
        /// The file is not JSON Claude Code could read either; it is left alone.
        case unreadable
        case unwritable(String)
        /// Claude Code changed the file each time it was about to be written.
        case busy
        /// A copy of the old settings could not be kept, so nothing changed.
        case noBackup
    }

    static func settingsURL(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> URL {
        home.appending(path: ".claude/settings.json")
    }

    static func marker(bundle: String) -> String { "# vorssaint-hook \(bundle)" }

    static func command(socket: String, bundle: String) -> String {
        let quoted = "'" + socket.replacingOccurrences(of: "'", with: "'\\''") + "'"
        return "/usr/bin/nc -U \(quoted) 2>/dev/null; exit 0 \(marker(bundle: bundle))"
    }

    // MARK: Settings

    /// Whether every event has this app's current hook.
    static func isInstalled(in settings: [String: Any], command: String) -> Bool {
        let hooks = settings["hooks"] as? [String: Any] ?? [:]
        return events.allSatisfy { event in
            (hooks[event] as? [[String: Any]] ?? []).contains { group in
                (group["hooks"] as? [[String: Any]] ?? []).contains { $0["command"] as? String == command }
            }
        }
    }

    static func hasAny(in settings: [String: Any], bundle: String) -> Bool {
        let marker = marker(bundle: bundle)
        let hooks = settings["hooks"] as? [String: Any] ?? [:]
        return hooks.values.contains { value in
            (value as? [[String: Any]] ?? []).contains { group in
                (group["hooks"] as? [[String: Any]] ?? []).contains { ($0["command"] as? String)?.contains(marker) == true }
            }
        }
    }

    /// The settings without this app's hooks; groups left empty go too.
    static func removing(from settings: [String: Any], bundle: String) -> [String: Any] {
        let marker = marker(bundle: bundle)
        var settings = settings
        guard var hooks = settings["hooks"] as? [String: Any] else { return settings }
        for (event, value) in hooks {
            guard let groups = value as? [[String: Any]] else { continue }
            let kept = groups.compactMap { group -> [String: Any]? in
                guard let entries = group["hooks"] as? [[String: Any]] else { return group }
                let remaining = entries.filter { ($0["command"] as? String)?.contains(marker) != true }
                guard remaining.count != entries.count else { return group }
                if remaining.isEmpty { return nil }
                var group = group
                group["hooks"] = remaining
                return group
            }
            hooks[event] = kept.isEmpty ? nil : kept
        }
        settings["hooks"] = hooks.isEmpty ? nil : hooks
        return settings
    }

    static func installing(into settings: [String: Any], command: String, bundle: String) -> [String: Any] {
        var settings = removing(from: settings, bundle: bundle)
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        for event in events {
            let entry: [String: Any] = ["type": "command", "command": command,
                                        "timeout": event == "PermissionRequest" ? permissionTimeout : eventTimeout]
            var group: [String: Any] = ["hooks": [entry]]
            if toolEvents.contains(event) { group["matcher"] = "*" }
            hooks[event] = (hooks[event] as? [[String: Any]] ?? []) + [group]
        }
        settings["hooks"] = hooks
        return settings
    }

    // MARK: File

    /// The settings, or none when there is no file yet. A file that exists
    /// but cannot be read, as one owned by root, is an error: writing over it
    /// would lose everything in it.
    static func read(_ url: URL) throws -> [String: Any] {
        guard let data = try contents(url) else { return [:] }
        if data.allSatisfy({ $0 == 0x20 || $0 == 0x0A || $0 == 0x0D || $0 == 0x09 }) { return [:] }
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { throw Failure.unreadable }
        return json
    }

    /// What the file holds, nil when there is no file at all.
    static func contents(_ url: URL) throws -> Data? {
        let manager = FileManager.default
        var isFolder: ObjCBool = false
        guard manager.fileExists(atPath: url.path, isDirectory: &isFolder) else {
            // A link whose target is gone is not a missing file to create.
            if (try? manager.destinationOfSymbolicLink(atPath: url.path)) != nil { throw Failure.unreadable }
            return nil
        }
        guard !isFolder.boolValue, let data = manager.contents(atPath: url.path) else { throw Failure.unreadable }
        return data
    }

    /// How many copies of earlier settings are kept beside the file.
    static let backups = 3

    /// Reads the file, changes it and writes it back in one step, keeping a
    /// copy of what was there. Claude Code writes the same file, so a change
    /// made meanwhile starts the step over.
    static func update(_ link: URL, now: Date = Date(), _ change: ([String: Any]) -> [String: Any]) throws {
        let manager = FileManager.default
        // Settings kept elsewhere and linked here are changed where they live.
        let url = link.resolvingSymlinksInPath()
        for _ in 0..<3 {
            let before = (try? manager.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
            let original = try contents(url)
            let settings = try read(url)
            let changed = change(settings)
            // Nothing to change, and no file made just to hold nothing.
            if NSDictionary(dictionary: changed).isEqual(to: settings) { return }
            var data = try JSONSerialization.data(withJSONObject: changed, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])
            data.append(0x0A)
            let after = (try? manager.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
            guard before == after, (try? contents(url)) == original else { continue }
            let folder = url.deletingLastPathComponent()
            do {
                try manager.createDirectory(at: folder, withIntermediateDirectories: true)
                if let original {
                    // Settings can hold tokens: the copy is the owner's alone.
                    let stamp = Int(now.timeIntervalSince1970)
                    let backup = folder.appending(path: "\(url.lastPathComponent).vorssaint-\(stamp).bak")
                    guard manager.createFile(atPath: backup.path, contents: original, attributes: [.posixPermissions: 0o600]) else {
                        throw Failure.noBackup
                    }
                    prune(folder: folder, name: url.lastPathComponent)
                }
                let permissions = (try? manager.attributesOfItem(atPath: url.path))?[.posixPermissions] as? NSNumber
                let temporary = folder.appending(path: ".\(url.lastPathComponent).vorssaint-\(UUID().uuidString)")
                try data.write(to: temporary)
                if let permissions { try? manager.setAttributes([.posixPermissions: permissions], ofItemAtPath: temporary.path) }
                if rename(temporary.path, url.path) != 0 {
                    try? manager.removeItem(at: temporary)
                    throw Failure.unwritable(String(cString: strerror(errno)))
                }
            } catch let failure as Failure {
                throw failure
            } catch {
                throw Failure.unwritable(error.localizedDescription)
            }
            return
        }
        throw Failure.busy
    }

    /// Keeps the newest copies only.
    static func prune(folder: URL, name: String) {
        let prefix = "\(name).vorssaint-"
        let copies = ((try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? [])
            .filter { $0.hasPrefix(prefix) && $0.hasSuffix(".bak") }
            .sorted { (Int($0.dropFirst(prefix.count).dropLast(4)) ?? 0) > (Int($1.dropFirst(prefix.count).dropLast(4)) ?? 0) }
        for old in copies.dropFirst(backups) { try? FileManager.default.removeItem(at: folder.appending(path: old)) }
    }
}
