// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV and the Openbar contributors

import Foundation

/// One ask the queue's Allow all answered (#281). Windows and Linux keep the
/// same fields (batch_log.rs).
struct BatchApproval: Codable, Equatable {
    /// The click it came with; the asks of one click share it.
    let batch: String
    /// Seconds since 1970.
    let at: TimeInterval
    let sessionId: String
    /// The session's name as the island showed it.
    var session: String = ""
    var project: String = ""
    /// The agent, as `ClaudeSessionProvider`'s raw value.
    var agent: String = ""
    var tool: String = ""
    /// The whole command, path or arguments.
    var detail: String = ""
    /// The files a change touches, when it names them.
    var paths: [String]? = nil

    /// Characters of a command kept: as much as a row shows before the
    /// command has to be read whole, which Allow all never takes.
    static let detailLimit = PermissionContent.previewLimit
    static let pathLimit = 50

    init(batch: String, session: ClaudeSession, ask: ClaudePermission, at: Date) {
        self.batch = batch
        self.at = at.timeIntervalSince1970
        sessionId = session.id
        self.session = session.name
        project = session.project
        agent = session.provider.rawValue
        tool = ask.tool
        // The whole text the agent sent, not the row's preview of it.
        let text = [ask.content?.full, ask.detail, ask.summary].compactMap { $0 }
            .first { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? ""
        detail = Self.clip(text, Self.detailLimit)
        var paths: [String] = []
        // A move names where the file goes too.
        for path in ask.diffs.flatMap({ [$0.path] + [$0.movedTo].compactMap { $0 } })
        where !path.isEmpty && !paths.contains(path) && paths.count < Self.pathLimit {
            paths.append(path)
        }
        // A path the detail already is says nothing more.
        if paths == [detail.trimmingCharacters(in: .whitespacesAndNewlines)] { paths = [] }
        self.paths = paths.isEmpty ? nil : paths
    }

    static func clip(_ text: String, _ limit: Int) -> String {
        guard text.unicodeScalars.count > limit else { return text }
        return String(String.UnicodeScalarView(text.unicodeScalars.prefix(limit))) + "…"
    }

    var date: Date { Date(timeIntervalSince1970: at) }
    var provider: ClaudeSessionProvider? { ClaudeSessionProvider(rawValue: agent) }
}

/// What Allow all let through, newest first, kept in the app's container so
/// it can be looked at afterwards in Settings › Sessions (#281). Only asks
/// the agent took are kept, and only from Allow all: an ask allowed on its
/// own row was read on its row.
final class BatchApprovalLog: ObservableObject {
    static let shared = BatchApprovalLog(
        url: PrivateFileStore.containerURL?.appendingPathComponent("sessions", isDirectory: true)
            .appendingPathComponent("batch-approvals.json"))

    /// Entries kept; the oldest go first.
    static let cap = 500
    /// The file, read whole, is ignored past this size.
    static let fileLimit = 16 * 1024 * 1024

    @Published private(set) var entries: [BatchApproval] = []

    private let url: URL?
    private let container: URL?
    /// Reads and writes happen here, in order, off the main thread.
    private let io = DispatchQueue(label: "openbar.batch-approvals")
    /// The list as the file holds it, touched only on `io`.
    private var stored: [BatchApproval]?

    init(url: URL?, container: URL? = PrivateFileStore.containerURL) {
        self.url = url
        self.container = container
        io.async { [self] in
            let loaded = loaded()
            DispatchQueue.main.async { self.entries = loaded }
        }
    }

    /// Keeps the ask, newest first, and writes the list. Any thread.
    func record(_ entry: BatchApproval) {
        io.async { [self] in
            var list = loaded()
            list.insert(entry, at: 0)
            if list.count > Self.cap { list.removeLast(list.count - Self.cap) }
            stored = list
            save(list)
            DispatchQueue.main.async { self.entries = list }
        }
    }

    func clear() {
        io.async { [self] in
            stored = []
            if let url { try? FileManager.default.removeItem(at: url) }
            DispatchQueue.main.async { self.entries = [] }
        }
    }

    /// Waits for the reads and writes asked for so far; for tests.
    func flush() { io.sync {} }

    private func loaded() -> [BatchApproval] {
        if let stored { return stored }
        let list = url.map(Self.load) ?? []
        stored = list
        return list
    }

    /// A missing, unreadable or broken file reads as an empty list.
    static func load(_ url: URL) -> [BatchApproval] {
        guard let size = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.size] as? Int,
              size <= fileLimit, let data = try? Data(contentsOf: url),
              let list = try? JSONDecoder().decode([BatchApproval].self, from: data)
        else { return [] }
        return Array(list.sorted { $0.at > $1.at }.prefix(cap))
    }

    private func save(_ list: [BatchApproval]) {
        guard let url, let data = try? JSONEncoder().encode(list),
              PrivateFileStore.createDirectory(at: url.deletingLastPathComponent(), container: container)
        else { return }
        PrivateFileStore.write(data, to: url)
    }
}
