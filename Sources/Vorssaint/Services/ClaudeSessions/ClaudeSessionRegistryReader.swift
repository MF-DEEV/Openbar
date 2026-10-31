// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Reads the `sessions/<pid>.json` records Claude Code keeps while a session
/// runs. A killed session leaves its record behind and its process number
/// can come back for something else, so a record counts only while a
/// process with that number started when the record says.
enum ClaudeSessionRegistryReader {
    /// The folders Claude Code keeps its state in: the two standard ones, the
    /// same the AI section reads logs from, then any a `CLAUDE_CONFIG_DIR`
    /// moved it to.
    static func homes(_ home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        let standard = [".claude", ".config/claude"].map { home.appending(path: $0, directoryHint: .isDirectory) }
        return ClaudeConfigHomes.merge(standard, ClaudeConfigHomes.extra(home: home))
    }

    static func folders(_ home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        homes(home).map { $0.appending(path: "sessions", directoryHint: .isDirectory) }
    }

    /// Records may differ from the process start by the second they were
    /// rounded to.
    static let startTolerance: TimeInterval = 2

    static func read(_ folders: [URL], processStart: (Int32) -> Date? = processStart) -> [ClaudeSessionRecord] {
        scan(folders, processStart: processStart).records
    }

    /// The running sessions, and the running processes whose record could not
    /// be read this time, as while Claude Code rewrites it.
    static func scan(_ folders: [URL], processStart: (Int32) -> Date? = processStart)
        -> (records: [ClaudeSessionRecord], unreadable: [Int32]) {
        var records: [Int32: ClaudeSessionRecord] = [:]
        var unreadable: Set<Int32> = []
        for folder in folders {
            guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { continue }
            for name in names where name.hasSuffix(".json") {
                guard let pid = Int32(name.dropLast(5)) else { continue }
                guard let data = try? Data(contentsOf: folder.appending(path: name)), data.count < 1 << 16,
                      let record = record(data) else {
                    if processStart(pid) != nil { unreadable.insert(pid) }
                    continue
                }
                guard records[record.pid] == nil, isCurrent(record, started: processStart(record.pid)) else { continue }
                records[record.pid] = record
            }
        }
        unreadable.subtract(records.keys)
        return (records.values.sorted { $0.pid < $1.pid }, unreadable.sorted())
    }

    /// One record, or nil when it is being written or describes a process
    /// this Mac cannot see, as one in a container sharing the folder.
    static func record(_ data: Data) -> ClaudeSessionRecord? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let pid = json["pid"] as? Int, pid > 0, pid <= Int(Int32.max),
              let session = json["sessionId"] as? String, !session.isEmpty,
              let cwd = json["cwd"] as? String, !cwd.isEmpty else { return nil }
        if let domain = json["pidDomain"] as? String, domain != "darwin" { return nil }
        let status = (json["status"] as? String).flatMap(ClaudeRegistryStatus.init(rawValue:))
        var record = ClaudeSessionRecord(
            pid: Int32(pid), sessionID: session, cwd: cwd,
            name: json["name"] as? String ?? (cwd as NSString).lastPathComponent,
            status: status,
            waitingFor: status == .waiting ? json["waitingFor"] as? String : nil,
            statusUpdated: milliseconds(json["statusUpdatedAt"]) ?? milliseconds(json["updatedAt"]),
            started: (json["procStart"] as? String).flatMap(procStart) ?? milliseconds(json["startedAt"]),
            version: json["version"] as? String ?? "")
        record.nameSource = json["nameSource"] as? String
        return record
    }

    /// The record belongs to the process now running under its number.
    static func isCurrent(_ record: ClaudeSessionRecord, started: Date?) -> Bool {
        guard let started else { return false }
        guard let recorded = record.started else { return true }
        return abs(recorded.timeIntervalSince(started)) <= startTolerance
            // Older records give only the moment the session began, a little
            // after its process did.
            || (recorded > started && recorded.timeIntervalSince(started) < 60)
    }

    private static let procStartFormat: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "EEE MMM d HH:mm:ss yyyy"
        return formatter
    }()

    /// `procStart` is `ps -o lstart=` read in UTC, like "Wed Oct 21 17:50:25 2026".
    static func procStart(_ text: String) -> Date? {
        let words = text.split(separator: " ", omittingEmptySubsequences: true).joined(separator: " ")
        return procStartFormat.date(from: words)
    }

    private static func milliseconds(_ value: Any?) -> Date? {
        guard let number = value as? NSNumber, number.doubleValue > 0 else { return nil }
        return Date(timeIntervalSince1970: number.doubleValue / 1000)
    }

    /// When a running process started, or nil when there is none.
    static func processStart(_ pid: Int32) -> Date? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard pid > 0, proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(info.pbi_start_tvsec) + TimeInterval(info.pbi_start_tvusec) / 1_000_000)
    }
}

/// Folders Claude Code keeps its state in besides the standard two: the one
/// `CLAUDE_CONFIG_DIR` names, as for a second account. That variable lives in
/// the shell that starts Claude, which the app never sees, so it is looked
/// for in the app's own environment, in the environment of running Claude
/// Code processes, and beside `~/.claude` (`~/.claude-work`, `~/.claude-2`).
enum ClaudeConfigHomes {
    static let variable = "CLAUDE_CONFIG_DIR"

    /// How long a look at the running processes is reused.
    static let lifetime: TimeInterval = 5

    private static let lock = NSLock()
    private static var cached: (home: String, at: Date, homes: [URL])?

    /// The extra folders for `home`. The environments only count for the
    /// person's own home: another one is a test's or a sandbox's.
    static func extra(home: URL, now: Date = Date()) -> [URL] {
        lock.lock()
        if let cached, cached.home == home.path, now.timeIntervalSince(cached.at) < lifetime, now >= cached.at {
            defer { lock.unlock() }
            return cached.homes
        }
        lock.unlock()
        let own = home.standardizedFileURL.path == FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        let named = own ? [ProcessInfo.processInfo.environment[variable]].compactMap { $0 } + running() : []
        let found = folders(named: named, home: home) + siblings(of: home)
        lock.lock()
        cached = (home.path, now, found)
        lock.unlock()
        return found
    }

    /// The folders the variable names that exist: absolute, or under `~/`
    /// as when it was quoted in a shell profile.
    static func folders(named values: [String], home: URL) -> [URL] {
        values.compactMap { value in
            let value = value.trimmingCharacters(in: .whitespaces)
            let path: String
            if value == "~" { path = home.path }
            else if value.hasPrefix("~/") { path = home.path + "/" + value.dropFirst(2) }
            else if value.hasPrefix("/") { path = value }
            else { return nil }
            return isFolder(path) ? URL(fileURLWithPath: path, isDirectory: true) : nil
        }
    }

    /// `.claude…` folders in the home holding a session registry.
    static func siblings(of home: URL) -> [URL] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: home.path) else { return [] }
        return names.filter { $0.hasPrefix(".claude") && $0 != ".claude" }.sorted().compactMap { name in
            let folder = home.appending(path: name, directoryHint: .isDirectory)
            return isFolder(folder.appending(path: "sessions").path) ? folder : nil
        }
    }

    /// The standard folders, then the extra ones none of them already is.
    static func merge(_ standard: [URL], _ extra: [URL]) -> [URL] {
        var seen = Set<String>()
        return (standard + extra).filter { seen.insert(canonical($0)).inserted }
    }

    private static func canonical(_ url: URL) -> String {
        let path = url.standardizedFileURL.resolvingSymlinksInPath().path
        return path.count > 1 && path.hasSuffix("/") ? String(path.dropLast()) : path
    }

    private static func isFolder(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &directory) && directory.boolValue
    }

    /// The variable as running Claude Code processes of this user have it.
    /// Only processes whose program path names Claude are read.
    static func running() -> [String] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var pids = [Int32](repeating: 0, count: Int(count) + 32)
        let filled = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<Int32>.size))
        guard filled > 0 else { return [] }
        let me = getuid()
        var values: [String] = []
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        for pid in pids.prefix(Int(filled)) where pid > 0 {
            var info = proc_bsdinfo()
            let size = Int32(MemoryLayout<proc_bsdinfo>.size)
            guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size, info.pbi_uid == me,
                  proc_pidpath(pid, &path, UInt32(path.count)) > 0,
                  String(cString: path).lowercased().contains("claude"),
                  let value = ClaudeSessionHosts.environment(of: pid, keys: [variable])[variable],
                  !value.isEmpty, !values.contains(value) else { continue }
            values.append(value)
        }
        return values
    }
}
