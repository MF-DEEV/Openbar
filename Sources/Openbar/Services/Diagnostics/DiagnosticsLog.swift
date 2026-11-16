// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Openbar's own log on disk: one file a day under `~/Library/Logs/<bundle
/// id>`, kept for a week, so a diagnostics file (#230) can carry the last few
/// days. Lines are short notes the app writes itself (hooks checked, a session
/// event's type, an error), masked before they are written; prompts, replies,
/// commands and file contents never go in.
///
/// The Windows and Linux app writes the same files the same way
/// (`diagnostics/log.rs`), so one reader works for every report.
final class DiagnosticsLog {
    enum Level: String {
        case info = "INFO"
        case warn = "WARN"
        case error = "ERROR"
    }

    static let shared = DiagnosticsLog(folder: defaultFolder())
    /// Files older than this are deleted.
    static let keepDays = 7
    /// How many days a diagnostics file takes.
    static let exportDays = 3
    /// A day's file stops growing here, with a line that says so.
    static let dailyLimit = 2 * 1024 * 1024

    let folder: URL
    private let home: String
    private let queue = DispatchQueue(label: "com.openbar.diagnostics-log", qos: .utility)
    // Queue.
    private var handle: FileHandle?
    private var day: String?
    private var written = 0
    private var full = false

    init(folder: URL, home: String = FileManager.default.homeDirectoryForCurrentUser.path) {
        self.folder = folder
        self.home = home
    }

    static func defaultFolder(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                              bundle: String? = Bundle.main.bundleIdentifier) -> URL {
        home.appending(path: "Library/Logs").appending(path: bundle ?? "Openbar", directoryHint: .isDirectory)
    }

    static func info(_ category: String, _ message: @autoclosure () -> String) { shared.write(.info, category, message()) }
    static func warn(_ category: String, _ message: @autoclosure () -> String) { shared.write(.warn, category, message()) }
    static func error(_ category: String, _ message: @autoclosure () -> String) { shared.write(.error, category, message()) }

    func write(_ level: Level, _ category: String, _ message: String, now: Date = Date()) {
        let line = Self.line(level, category, message, at: now, home: home)
        let day = Self.day(now)
        queue.async { [self] in append(line, day: day) }
    }

    /// Waits for every line written so far to reach the disk.
    func flush() {
        queue.sync { try? handle?.synchronize() }
    }

    /// The files of the last `days` days, oldest first.
    func recentFiles(days: Int = exportDays, now: Date = Date()) -> [URL] {
        flush()
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        return Self.recent(names, days: days, now: now).map { folder.appending(path: $0) }
    }

    // MARK: Format (kept in step with log.rs)

    /// `2026-11-16T15:30:02.123Z INFO  [sessions] hooks installed for gemini`
    static func line(_ level: Level, _ category: String, _ message: String, at date: Date, home: String) -> String {
        let flat = message.replacingOccurrences(of: "\r\n", with: " ").replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
        let padded = level.rawValue.padding(toLength: 5, withPad: " ", startingAt: 0)
        return "\(timestamp(date)) \(padded) [\(category)] \(DiagnosticsRedactor.redact(flat, home: home))\n"
    }

    static func timestamp(_ date: Date) -> String {
        let millis = Int64((date.timeIntervalSince1970 * 1000).rounded(.down))
        let seconds = Date(timeIntervalSince1970: TimeInterval(millis / 1000))
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let c = calendar.dateComponents([.year, .month, .day, .hour, .minute, .second], from: seconds)
        return String(format: "%04d-%02d-%02dT%02d:%02d:%02d.%03dZ", c.year ?? 0, c.month ?? 0, c.day ?? 0,
                      c.hour ?? 0, c.minute ?? 0, c.second ?? 0, Int(((millis % 1000) + 1000) % 1000))
    }

    /// The UTC day a line belongs to, as its file names it.
    static func day(_ date: Date) -> String { String(timestamp(date).prefix(10)) }

    static func fileName(day: String) -> String { "openbar-\(day).log" }

    /// The log files among `names` from the last `days` days (today counts),
    /// oldest first.
    static func recent(_ names: [String], days: Int, now: Date) -> [String] {
        let first = day(now.addingTimeInterval(-Double(max(days, 1) - 1) * 86_400))
        return names.compactMap { name -> (String, String)? in
            guard let fileDay = dayOf(name), fileDay >= first else { return nil }
            return (fileDay, name)
        }.sorted { $0.0 < $1.0 }.map(\.1)
    }

    /// The day a log file is named after; nil for any other file.
    static func dayOf(_ name: String) -> String? {
        guard name.hasPrefix("openbar-"), name.hasSuffix(".log") else { return nil }
        let day = String(name.dropFirst("openbar-".count).dropLast(".log".count))
        guard day.count == 10, day.allSatisfy({ $0.isNumber || $0 == "-" }) else { return nil }
        return day
    }

    // MARK: Writing (queue)

    private func append(_ line: String, day: String) {
        if day != self.day { open(day) }
        guard let handle, !full else { return }
        var data = Data(line.utf8)
        if written + data.count > Self.dailyLimit {
            full = true
            data = Data(Self.line(.warn, "log", "today's log reached its size limit; later lines are dropped",
                                  at: Date(), home: home).utf8)
        }
        do {
            try handle.write(contentsOf: data)
            written += data.count
        } catch {
            self.handle = nil
        }
    }

    private func open(_ day: String) {
        try? handle?.close()
        handle = nil
        self.day = day
        full = false
        let manager = FileManager.default
        try? manager.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appending(path: Self.fileName(day: day))
        if !manager.fileExists(atPath: url.path) { manager.createFile(atPath: url.path, contents: nil) }
        guard let opened = try? FileHandle(forWritingTo: url) else { return }
        written = Int((try? opened.seekToEnd()) ?? 0)
        full = written >= Self.dailyLimit
        handle = opened
        prune(today: day)
    }

    /// Deletes the files older than the week kept.
    private func prune(today: String) {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path)) ?? []
        let kept = Set(Self.recent(names, days: Self.keepDays, now: Self.date(of: today) ?? Date()))
        for name in names where Self.dayOf(name) != nil && !kept.contains(name) {
            try? FileManager.default.removeItem(at: folder.appending(path: name))
        }
    }

    private static func date(of day: String) -> Date? {
        let parts = day.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2], hour: 12))
    }
}
