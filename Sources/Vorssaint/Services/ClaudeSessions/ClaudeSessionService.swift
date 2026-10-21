// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Combine
import Foundation

/// Follows every Claude Code session running on this Mac while the Sessions
/// section is on. It reads Claude Code's process registry and the new lines
/// of each running session's transcript, takes in the hook events the
/// sessions send, and publishes what the island shows. Nothing is written or
/// sent; what sessions say stays unread unless previews are on, and then
/// only in memory.
///
/// Reading happens on a private queue; the main thread and that queue hand
/// work to each other asynchronously.
final class ClaudeSessionService: ObservableObject {
    static let shared = ClaudeSessionService()

    @Published private(set) var sessions: [ClaudeSession] = []
    @Published private(set) var counts = ClaudeSessionCounts()
    /// The session whose page is open.
    @Published var focusedID: String?
    /// What the person is writing to each session, kept while the island closes.
    @Published var drafts: [String: String] = [:]
    let events = PassthroughSubject<ClaudeSessionEvent, Never>()

    /// While a session works or waits, its files are read this often;
    /// otherwise the registry alone is checked a little less often.
    private static let activeTick: TimeInterval = 1
    private static let quietTick: TimeInterval = 3
    /// Subagents and task lists are looked for less often than transcripts.
    private static let sideFilesEvery = 3

    let queue = DispatchQueue(label: "com.vorssaint.claude-sessions", qos: .utility, autoreleaseFrequency: .workItem)
    private let home = FileManager.default.homeDirectoryForCurrentUser

    // Main thread.
    private(set) var running = false

    // Queue.
    private let engine = ClaudeSessionEngine()
    private var timer: DispatchSourceTimer?
    private var interval: TimeInterval = 0
    private var ticks = 0
    private var transcripts: [String: String] = [:]
    private var readers: [String: (cursor: AgentLogCursor, reader: ClaudeTranscriptReader)] = [:]
    private var subagentMeta: Set<String> = []
    private var taskStamps: [String: Date] = [:]
    private var hosts: [Int32: ClaudeSessionHost] = [:]
    private var published: [ClaudeSession] = []

    func syncWithPreferences() {
        let policy = NotchClaudeSessionsSupport.policy()
        guard NotchClaudeSessionsSupport.isEnabled() else { stop(); return }
        queue.async { [self] in
            if engine.policy != policy {
                engine.policy = policy
                // Reading again with previews on fills in what was skipped.
                if policy.previews { readers = [:]; subagentMeta = []; taskStamps = [:] }
                for key in readers.keys { readers[key]?.reader.policy = policy }
            }
        }
        guard !running else { return }
        running = true
        queue.async { [self] in
            schedule(Self.activeTick)
            refresh()
        }
    }

    func stop() {
        guard running else { return }
        running = false
        focusedID = nil
        sessions = []
        counts = ClaudeSessionCounts()
        queue.async { [self] in
            timer?.cancel()
            timer = nil
            interval = 0
            readers = [:]
            transcripts = [:]
            subagentMeta = []
            taskStamps = [:]
            hosts = [:]
            published = []
            engine.ingest(records: [], now: Date.distantFuture)
        }
    }

    // MARK: Actions

    func focus(_ id: String?) {
        focusedID = id
        if let id { acknowledge(id) }
    }

    func acknowledge(_ id: String) {
        queue.async { [self] in
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    /// Takes one hook event in, from the hook server's queue.
    func receive(_ fact: ClaudeHookFact, transcript: String?) {
        queue.async { [self] in
            if let transcript, !transcript.isEmpty { transcripts[fact.sessionID] = transcript }
            engine.ingest(hook: fact, now: Date())
            publish(now: Date())
        }
    }

    /// The person answered a permission, or its hook stopped waiting.
    func resolvePermission(_ requestID: String) {
        queue.async { [self] in
            engine.resolvePermission(requestID: requestID)
            publish(now: Date())
        }
    }

    // MARK: Reading

    private func schedule(_ seconds: TimeInterval) {
        guard seconds != interval else { return }
        interval = seconds
        timer?.cancel()
        let source = DispatchSource.makeTimerSource(queue: queue)
        source.schedule(deadline: .now() + seconds, repeating: seconds, leeway: .milliseconds(200))
        source.setEventHandler { [weak self] in self?.refresh() }
        source.resume()
        timer = source
    }

    private func refresh() {
        guard timer != nil else { return }
        let now = Date()
        var records = ClaudeSessionRegistryReader.read(ClaudeSessionRegistryReader.folders(home))
        let alive = Set(records.map(\.pid))
        hosts = hosts.filter { alive.contains($0.key) }
        for index in records.indices {
            let pid = records[index].pid
            if hosts[pid] == nil { hosts[pid] = ClaudeSessionHosts.host(of: pid) }
            records[index].host = hosts[pid] ?? .unknown
        }
        engine.ingest(records: records, now: now)
        ticks += 1
        let sideFiles = ticks % Self.sideFilesEvery == 1
        let ids = Set(records.map(\.sessionID))
        readers = readers.filter { ids.contains(sessionID(ofReader: $0.key)) }
        for record in records {
            guard let transcript = transcript(for: record) else { continue }
            read(transcript, session: record.sessionID, now: now)
            if sideFiles {
                readSubagents(of: transcript, session: record.sessionID, now: now)
                readTasks(record.sessionID, now: now)
            }
        }
        transcripts = transcripts.filter { ids.contains($0.key) }
        publish(now: now)
        let active = published.contains { $0.phase.isWorking || $0.phase.isWaiting }
        schedule(active ? Self.activeTick : Self.quietTick)
    }

    private func sessionID(ofReader path: String) -> String {
        let main = AgentLogCursor.parent(of: path) ?? path
        return ((main as NSString).lastPathComponent as NSString).deletingPathExtension
    }

    /// The transcript a hook named, or the one named after the session in
    /// any project folder.
    private func transcript(for record: ClaudeSessionRecord) -> String? {
        if let known = transcripts[record.sessionID], FileManager.default.fileExists(atPath: known) { return known }
        for home in ClaudeSessionRegistryReader.homes(home) {
            let projects = home.appending(path: "projects", directoryHint: .isDirectory)
            guard let folders = try? FileManager.default.contentsOfDirectory(atPath: projects.path) else { continue }
            for folder in folders {
                let path = projects.appending(path: folder).appending(path: record.sessionID + ".jsonl").path
                if FileManager.default.fileExists(atPath: path) {
                    transcripts[record.sessionID] = path
                    return path
                }
            }
        }
        return nil
    }

    private func read(_ path: String, session: String, now: Date) {
        var entry = readers[path] ?? (AgentLogCursor(path: path, provider: .claude),
                                      ClaudeTranscriptReader(policy: engine.policy, path: path))
        var facts: [ClaudeTranscriptFact] = []
        AgentLogReader.readAppended(entry.cursor) { line in
            facts.append(contentsOf: entry.reader.parse(line, now: now))
        }
        readers[path] = entry
        engine.ingest(sessionID: session, facts: facts, now: now)
    }

    private func readSubagents(of transcript: String, session: String, now: Date) {
        let folder = ((transcript as NSString).deletingPathExtension as NSString).appendingPathComponent("subagents")
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder) else { return }
        for name in names where name.hasSuffix(".jsonl") {
            let path = (folder as NSString).appendingPathComponent(name)
            read(path, session: session, now: now)
            let meta = (path as NSString).deletingPathExtension + ".meta.json"
            if !subagentMeta.contains(meta), let id = ClaudeTranscriptReader.subagent(of: path),
               let data = FileManager.default.contents(atPath: meta), data.count < 1 << 16,
               let fact = ClaudeTranscriptReader.subagentInfo(data, id: id, policy: engine.policy) {
                subagentMeta.insert(meta)
                engine.ingest(sessionID: session, facts: [fact], now: now)
            }
        }
    }

    private func readTasks(_ session: String, now: Date) {
        for home in ClaudeSessionRegistryReader.homes(home) {
            let folder = home.appending(path: "tasks/\(session)", directoryHint: .isDirectory).path
            guard let names = try? FileManager.default.contentsOfDirectory(atPath: folder) else { continue }
            let files = names.filter { $0.hasSuffix(".json") }.map { (folder as NSString).appendingPathComponent($0) }
            let stamp = files.compactMap { try? FileManager.default.attributesOfItem(atPath: $0)[.modificationDate] as? Date }.max()
                ?? .distantPast
            guard taskStamps[session] != stamp else { return }
            taskStamps[session] = stamp
            let datas = files.prefix(500).compactMap { FileManager.default.contents(atPath: $0) }
            if let tasks = ClaudeTranscriptReader.tasks(datas, policy: engine.policy) {
                engine.ingest(sessionID: session, facts: [.tasks(tasks)], now: now)
            }
            return
        }
    }

    private func publish(now: Date) {
        let snapshot = engine.snapshot(now: now)
        let sorted = NotchClaudeSessionsSupport.sorted(snapshot.sessions)
        let changed = sorted != published
        published = sorted
        guard changed || !snapshot.events.isEmpty else { return }
        DispatchQueue.main.async { [self] in
            guard running else { return }
            if changed {
                sessions = sorted
                counts = ClaudeSessionCounts(sorted)
                if let focusedID, !sorted.contains(where: { $0.id == focusedID }) { self.focusedID = nil }
            }
            snapshot.events.forEach(events.send)
        }
    }
}

extension ClaudeSessionHosts {
    static func host(of pid: Int32) -> ClaudeSessionHost {
        let environment = self.environment(of: pid)
        let bundle = environment["__CFBundleIdentifier"]
            ?? ResponsibleProcess.regularAppOwner(of: pid)?.bundleIdentifier
        return classify(bundle: bundle, tty: tty(of: pid), port: environment["CLAUDE_CODE_SSE_PORT"].flatMap(Int.init))
    }
}
