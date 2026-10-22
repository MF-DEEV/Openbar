// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Reads the `sessions/<pid>.json` records Claude Code keeps while a session
/// runs. A killed session leaves its record behind and its process number
/// can come back for something else, so a record counts only while a
/// process with that number started when the record says.
enum ClaudeSessionRegistryReader {
    /// The folders Claude Code keeps its state in, the same two the AI
    /// section reads logs from.
    static func homes(_ home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [URL] {
        [".claude", ".config/claude"].map { home.appending(path: $0, directoryHint: .isDirectory) }
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
