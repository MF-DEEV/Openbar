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
    /// Images added to each draft, as files Claude Code reads.
    @Published private(set) var attachments: [String: [URL]] = [:]
    /// Names the person gave sessions in the island, by session. Kept in the
    /// app's own preferences, never in Claude Code's files.
    @Published private(set) var names: [String: String] =
        UserDefaults.standard.dictionary(forKey: DefaultsKey.notchSessionsNames) as? [String: String] ?? [:]
    /// Costumes the person dressed sessions' Clawds in, by session, as
    /// `ClawdCostume.code`; a session missing here wears its own lot.
    @Published private(set) var costumes: [String: String] =
        (UserDefaults.standard.dictionary(forKey: DefaultsKey.notchSessionsCostumes) ?? [:]).compactMapValues { $0 as? String }
    /// The session whose Clawd is being dressed. Its page stays focused
    /// under the wardrobe, so an enlarged island keeps its size.
    @Published private(set) var dressingID: String?
    /// The permission each session shows and since when. Main thread.
    private var shownRequests: [String: (request: String, at: Date)] = [:]
    let events = PassthroughSubject<ClaudeSessionEvent, Never>()
    /// Whether Claude Code's settings hold this app's hooks; nil until checked.
    @Published private(set) var hooksInstalled: Bool?
    @Published private(set) var hookFailure: String?
    /// Whether Antigravity has the extension replies are typed through.
    @Published private(set) var bridgeInstalled: Bool?
    @Published private(set) var bridgeFailure: String?
    /// The app carries a newer extension than the one installed.
    @Published private(set) var bridgeOutdated = false

    /// While a session works or waits, its files are read this often;
    /// otherwise the registry alone is checked a little less often.
    private static let activeTick: TimeInterval = 1
    private static let quietTick: TimeInterval = 3
    /// Subagents and task lists are looked for less often than transcripts.
    private static let sideFilesEvery = 3

    let queue = DispatchQueue(label: "com.vorssaint.claude-sessions", qos: .utility, autoreleaseFrequency: .workItem)
    /// AppleScript and window lookups block until the other app answers.
    private let actions = DispatchQueue(label: "com.vorssaint.claude-actions", qos: .userInitiated)
    private let hookQueue = DispatchQueue(label: "com.vorssaint.claude-hooks", qos: .userInitiated)
    private let home = FileManager.default.homeDirectoryForCurrentUser

    // Main thread.
    private(set) var running = false
    private var permissionWatch: AnyCancellable?

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
    private var lastRecords: [Int32: ClaudeSessionRecord] = [:]
    private let agentColors = ClaudeAgentColors()
    private var server: ClaudeHookServer?
    /// Permission requests whose hook waits for the island's answer.
    private var pending: [String: (connection: ClaudeHookServer.Connection, session: String, suggestions: Any?,
                                   question: [String: Any]?, since: Date)] = [:]
    /// Claude Code gives up on a hook after its timeout; the island lets go first.
    private static let permissionDeadline: TimeInterval = TimeInterval(ClaudeHookInstaller.permissionTimeout - 10)

    static var bundleID: String { Bundle.main.bundleIdentifier ?? "com.vorssaint.utils" }

    /// Where the hooks write. The app's own folder, unless its path is too
    /// long for a socket.
    static func socketPath(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String {
        let preferred = PrivateFileStore.containerURL?.appending(path: "claude-hooks.sock").path
        if let preferred, ClaudeHookServer.fits(preferred) { return preferred }
        return home.appending(path: ".vorssaint/\(bundleID).sock").path
    }

    func syncWithPreferences() {
        let policy = NotchClaudeSessionsSupport.policy()
        guard NotchClaudeSessionsSupport.isEnabled() else { stop(); return }
        queue.async { [self] in
            if engine.policy != policy {
                engine.policy = policy
                // Reading again with previews on fills in what was skipped.
                if policy.previews { readers = [:]; subagentMeta = []; taskStamps = [:] }
                for key in readers.keys { readers[key]?.reader.policy = policy }
                // What was cleared leaves the island now, not at the next read.
                publish(now: Date())
            }
        }
        guard !running else { return }
        running = true
        // Agents defined inside folders macOS guards are read only with Full
        // Disk Access, which spares the person a prompt out of nowhere.
        permissionWatch = Permissions.shared.$fullDiskAccess.removeDuplicates().sink { [weak self] granted in
            self?.queue.async { self?.agentColors.readsProtected = granted }
        }
        let socket = Self.socketPath(home: home)
        queue.async { [self] in
            startServer(at: socket)
            schedule(Self.activeTick)
            refresh()
        }
        let home = home
        actions.async {
            ClaudeImageAttachments.sweep()
            // An extension the person once added is kept current by the app,
            // so a newer one never waits on a click in Settings.
            if ClaudeBridgeInstaller.isInstalled(home: home), ClaudeBridgeInstaller.isOutdated(home: home) {
                _ = ClaudeBridgeInstaller.install(home: home)
            }
        }
        checkHooks()
    }

    func stop() {
        guard running else { return }
        running = false
        focusedID = nil
        dressingID = nil
        permissionWatch = nil
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
            // Hooks waiting for an answer go back to the terminal's prompt.
            pending.values.forEach { $0.connection.finish() }
            pending = [:]
            server?.stop()
            server = nil
            agentColors.reset()
            engine.ingest(records: [], now: Date.distantFuture)
        }
    }

    // MARK: Actions

    /// Adds the pasteboard's images to a session's draft; false when it holds
    /// none, so the paste goes to the text instead.
    @discardableResult
    func attachImages(from pasteboard: NSPasteboard, to id: String) -> Bool {
        guard ClaudeImageAttachments.holdsImages(pasteboard) else { return false }
        attach(ClaudeImageAttachments.images(from: pasteboard), to: id)
        return true
    }

    func attach(_ images: [URL], to id: String) {
        guard !images.isEmpty else { return }
        var list = attachments[id] ?? []
        for image in images where !list.contains(image) { list.append(image) }
        attachments[id] = Array(list.prefix(ClaudeImageAttachments.limit))
    }

    func detach(_ image: URL, from id: String) {
        attachments[id]?.removeAll { $0 == image }
        if attachments[id]?.isEmpty == true { attachments[id] = nil }
    }

    /// What the island calls a session.
    func displayName(_ session: ClaudeSession) -> String {
        NotchClaudeSessionsSupport.name(session, custom: names[session.id])
    }

    /// Names a session, or with an empty name goes back to its own.
    func rename(_ id: String, to name: String) {
        names = NotchClaudeSessionsSupport.renamed(names, id: id, to: name, live: Set(sessions.map(\.id)))
        UserDefaults.standard.set(names, forKey: DefaultsKey.notchSessionsNames)
    }

    func focus(_ id: String?) {
        focusedID = id
        dressingID = nil
        if let id { acknowledge(id) }
    }

    /// What a session's Clawd wears.
    func costume(for session: ClaudeSession) -> ClawdCostume {
        ClawdCostume.resolved(key: session.id, kept: costumes[session.id])
    }

    /// What a subagent's Clawd wears: its own lot, in the color its
    /// definition gives it.
    func costume(for agent: ClaudeSubagent, in session: ClaudeSession) -> ClawdCostume {
        ClawdCostume.seeded(NotchClaudeSessionsSupport.costumeKey(agent, in: session), agentColor: agent.color)
    }

    /// Dresses a session's Clawd, or with nil gives it its own costume back.
    func setCostume(_ costume: ClawdCostume?, for id: String) {
        costumes = NotchClaudeSessionsSupport.kept(costumes, id: id, value: ClawdCostume.kept(costume, key: id),
                                                   live: Set(sessions.map(\.id)))
        UserDefaults.standard.set(costumes, forKey: DefaultsKey.notchSessionsCostumes)
    }

    /// Opens the wardrobe of the session whose page is open, or with nil
    /// goes back to the page.
    func dress(_ id: String?) {
        guard id == nil || id == focusedID else { return }
        dressingID = id
    }

    func acknowledge(_ id: String) {
        queue.async { [self] in
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    /// Answers the permission the session waits on, if its hook still waits.
    /// Answers the request the person saw, and only that one: a newer request
    /// for the same session, or one already settled, is left alone.
    func decide(_ id: String, request: String, _ decision: ClaudeHookMessage.Decision) {
        // A request is answerable once it has been on screen a moment, so a
        // click or ⌘Y meant for the one before it cannot answer it.
        guard let shown = shownRequests[id], shown.request == request,
              Date().timeIntervalSince(shown.at) >= NotchClaudeSessionsSupport.settleDelay else { return }
        queue.async { [self] in
            guard engine.holds(request, for: id), let asked = pending[request] else { return }
            // A question is answered or skipped, never allowed as it stands;
            // answers go only to a question.
            switch decision {
            case .allow, .allowAlways: guard asked.question == nil else { return }
            case .answer: guard asked.question != nil else { return }
            case .deny: break
            }
            pending[request] = nil
            asked.connection.finish(ClaudeHookMessage.reply(decision, suggestions: asked.suggestions, input: asked.question))
            engine.resolvePermission(requestID: request)
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    typealias SendFailure = ClaudeReplyFailure

    /// Types a reply into the session's terminal, as if the person typed it
    /// there. Calls back on the main thread.
    func send(_ text: String, to session: ClaudeSession, completion: @escaping (SendFailure?) -> Void) {
        let images = attachments[session.id] ?? []
        let reply = NotchClaudeSessionsSupport.reply(text) ?? ""
        guard !reply.isEmpty || !images.isEmpty, NotchClaudeSessionsSupport.canReply(session) else {
            return completion(session.phase.isWaiting || session.waitingFor != nil ? .waiting : .unreachable)
        }
        let folders = ClaudeSessionRegistryReader.folders(home)
        let bridge = ClaudeTerminalBridge.folder(home: home)
        // A permission the hooks already hold opens a dialog the registry may
        // not show yet; the keys would land in it.
        queue.async { [self] in
            let asking = engine.permission(of: session.id) != nil || pending.values.contains { $0.session == session.id }
            let transcript = transcripts[session.id]
            actions.async { [self] in
                // The session may have opened a dialog since the island last looked.
                let now = ClaudeSessionRegistryReader.read(folders).first { $0.pid == session.pid }
                let failure: SendFailure?
                if now?.sessionID != session.id { failure = .unreachable }
                else if asking || now?.status == .waiting { failure = .waiting }
                else {
                    // Copies where Claude reads without asking, for a reply that
                    // has to name its images instead of attaching them.
                    let folder = transcript.flatMap { ClaudeImageAttachments.sessionFolder(transcript: $0, session: session.id) }
                    let named = images.isEmpty ? [] : ClaudeImageAttachments.copies(images, in: folder)
                    failure = ClaudeTerminalReply.send(reply, images: images, named: named, to: session, bridge: bridge)
                }
                finishSend(failure, session: session, completion: completion)
            }
        }
    }

    private func finishSend(_ failure: SendFailure?, session: ClaudeSession, completion: @escaping (SendFailure?) -> Void) {
        DispatchQueue.main.async {
            if failure == nil {
                self.drafts[session.id] = nil
                self.attachments[session.id] = nil
                self.acknowledge(session.id)
            }
            completion(failure)
        }
    }

    func checkBridge() {
        let home = home
        actions.async {
            let installed = ClaudeBridgeInstaller.isInstalled(home: home)
            let outdated = installed && ClaudeBridgeInstaller.isOutdated(home: home)
            DispatchQueue.main.async {
                self.bridgeInstalled = installed
                self.bridgeOutdated = outdated
            }
        }
    }

    func installBridge() {
        let home = home
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        actions.async {
            let failure: String? = switch ClaudeBridgeInstaller.install(home: home) {
            case nil: nil
            case .missing?: text.bridgeMissing
            case .noAntigravity?: text.antigravityMissing
            case .packaging?: text.bridgePackageFailed
            case .refused?: text.bridgeRefused
            case .other(let reason)?: reason
            }
            DispatchQueue.main.async {
                self.bridgeFailure = failure
                self.checkBridge()
            }
        }
    }

    // MARK: Hooks

    private func startServer(at path: String) {
        guard server == nil else { return }
        // Hooks get their own queue: a long transcript read must not keep a
        // session waiting on its hook.
        let created = ClaudeHookServer(path: path, queue: hookQueue) { [weak self] payload, connection in
            // Only a permission waits for an answer; every other hook is let
            // go here, so a long read on the session queue never holds it.
            let delivered = Date()
            if !ClaudeHookMessage.isPermissionRequest(payload) { connection.finish() }
            self?.queue.async { self?.receive(payload, from: connection, at: delivered) }
        }
        server = created.start() ? created : nil
    }

    /// One hook event, on the queue.
    private func receive(_ payload: Data, from connection: ClaudeHookServer.Connection, at now: Date) {
        // A hook that arrived as the section stopped goes back to the terminal.
        guard timer != nil else { return connection.finish() }
        // A demo recording shows only the sessions it set up; any other one
        // is answered in its terminal.
        guard ClaudeSessionRegistryReader.isShown(hook: payload, under: ClaudeSessionRegistryReader.shownRoot) else {
            return connection.finish()
        }
        let request = UUID().uuidString
        guard let message = ClaudeHookMessage.decode(payload, requestID: request, now: now, policy: engine.policy) else {
            return connection.finish()
        }
        if let transcript = message.transcript, !transcript.isEmpty { transcripts[message.fact.sessionID] = transcript }
        if message.isPermissionRequest {
            pending[request] = (connection, message.fact.sessionID, message.suggestions, message.questionInput, now)
        } else {
            connection.finish()
        }
        // A turn's lines are written before its Stop hook runs; reading them
        // first tells whether it left work in the background, before a notice
        // calls it done.
        if case .stopped = message.fact.event, let path = transcripts[message.fact.sessionID] {
            read(path, session: message.fact.sessionID, now: now)
        }
        engine.ingest(hook: message.fact, now: now)
        publish(now: now)
    }

    /// Lets go of hooks whose question was answered in the terminal, whose
    /// hook went away, or that waited as long as Claude Code allows.
    private func releaseSettled(now: Date) {
        for (request, waiting) in pending {
            let current = engine.holds(request, for: waiting.session)
            guard !current || waiting.connection.peerIsGone || now.timeIntervalSince(waiting.since) > Self.permissionDeadline
            else { continue }
            pending[request] = nil
            waiting.connection.finish()
            if current { engine.resolvePermission(requestID: request) }
        }
    }

    func checkHooks() {
        let url = ClaudeHookInstaller.settingsURL(home: home)
        let command = ClaudeHookInstaller.command(socket: Self.socketPath(home: home), bundle: Self.bundleID)
        actions.async {
            let installed = (try? ClaudeHookInstaller.read(url)).map { ClaudeHookInstaller.isInstalled(in: $0, command: command) }
            DispatchQueue.main.async { self.hooksInstalled = installed ?? false }
        }
    }

    func installHooks(_ install: Bool) {
        let url = ClaudeHookInstaller.settingsURL(home: home)
        let bundle = Self.bundleID
        let command = ClaudeHookInstaller.command(socket: Self.socketPath(home: home), bundle: bundle)
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        actions.async {
            var failure: String?
            do {
                try ClaudeHookInstaller.update(url) { settings in
                    install ? ClaudeHookInstaller.installing(into: settings, command: command, bundle: bundle)
                        : ClaudeHookInstaller.removing(from: settings, bundle: bundle)
                }
            } catch let error as ClaudeHookInstaller.Failure {
                switch error {
                case .unreadable: failure = text.settingsUnreadable
                case .unwritable(let reason): failure = reason
                case .busy: failure = text.settingsBusy
                case .noBackup: failure = text.settingsNoBackup
                }
            } catch {
                failure = error.localizedDescription
            }
            DispatchQueue.main.async {
                self.hookFailure = failure
                self.checkHooks()
            }
        }
    }

    /// Brings the session's terminal forward. Calls back on the main thread
    /// with whether the window was found.
    func openHost(_ session: ClaudeSession, completion: @escaping (Bool) -> Void = { _ in }) {
        acknowledge(session.id)
        let homes = ClaudeSessionRegistryReader.homes(home)
        actions.async {
            let found = ClaudeHostFocus.focus(session, homes: homes)
            DispatchQueue.main.async { completion(found) }
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
        let scan = ClaudeSessionRegistryReader.scan(ClaudeSessionRegistryReader.folders(home))
        // A record caught while Claude Code rewrites it keeps its last reading,
        // so its page and clock do not reset for a tick.
        var records = scan.records + scan.unreadable.compactMap { lastRecords[$0] }
        lastRecords = Dictionary(records.map { ($0.pid, $0) }, uniquingKeysWith: { first, _ in first })
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
        releaseSettled(now: now)
        publish(now: now)
        let active = published.contains { $0.phase.isActive || $0.phase.isWaiting }
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
            let tasks = ClaudeTranscriptReader.tasks(datas, policy: engine.policy) ?? ClaudeTaskProgress()
            engine.ingest(sessionID: session, facts: [.tasks(tasks)], now: now)
            return
        }
        // The list was cleared or removed.
        if taskStamps.removeValue(forKey: session) != nil {
            engine.ingest(sessionID: session, facts: [.tasks(ClaudeTaskProgress())], now: now)
        }
    }

    private func publish(now: Date) {
        let snapshot = engine.snapshot(now: now)
        let sorted = NotchClaudeSessionsSupport.sorted(snapshot.sessions.map { agentColors.decorate($0, now: now) })
        let changed = sorted != published
        published = sorted
        guard changed || !snapshot.events.isEmpty else { return }
        DispatchQueue.main.async { [self] in
            guard running else { return }
            if changed {
                sessions = sorted
                counts = ClaudeSessionCounts(sorted)
                for session in sorted {
                    let request = session.permission?.requestID
                    if shownRequests[session.id]?.request != request {
                        shownRequests[session.id] = request.map { (request: $0, at: Date()) }
                    }
                }
                if let focusedID, !sorted.contains(where: { $0.id == focusedID }) {
                    self.focusedID = nil
                    dressingID = nil
                }
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

/// Brings a session's terminal forward: its own tab in Terminal, the window
/// holding its folder in Antigravity, or just the app elsewhere.
enum ClaudeHostFocus {
    static func focus(_ session: ClaudeSession, homes: [URL]) -> Bool {
        switch session.host {
        case .terminal(let tty):
            guard AppleScriptRunner.consentToAutomate(bundleID: ClaudeSessionHosts.terminal) else {
                return activate(ClaudeSessionHosts.terminal, window: nil)
            }
            let result = AppleScriptRunner.run(ClaudeTerminalScripts.focus(tty: tty))
            return result.ok && result.output == "found" ? true : activate(ClaudeSessionHosts.terminal, window: nil)
        case .antigravity(let port):
            let folders = port.map { ClaudeIDELock.folders(port: $0, homes: homes) } ?? []
            // Antigravity brings forward the window that already has the
            // session's folder open when asked to open that folder, with no
            // permission needed. Only a folder that is open is asked for, so
            // no new window ever opens.
            let raised = ClaudeIDELock.openFolder(cwd: session.cwd, folders: folders).map(openInAntigravity) ?? false
            // The extension then shows the session's own terminal in it.
            if let shell = ClaudeTerminalBridge.shell(of: session.pid) {
                _ = ClaudeTerminalBridge.focus(shell: shell, folder: ClaudeTerminalBridge.folder())
            }
            return raised || activate(ClaudeSessionHosts.antigravity,
                                      window: ClaudeIDELock.windowHint(cwd: session.cwd, folders: folders))
        case .app(let bundle):
            return activate(bundle, window: nil)
        case .unknown:
            return false
        }
    }

    private static func openInAntigravity(_ folder: String) -> Bool {
        guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeSessionHosts.antigravity) else {
            return false
        }
        let process = Process()
        process.executableURL = app.appending(path: ClaudeBridgeInstaller.cli)
        process.arguments = [folder]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return false }
        process.waitUntilExit()
        return process.terminationStatus == 0
    }

    /// Activates the app and, when Accessibility allows, raises its window
    /// whose title names `window`. The app comes forward first: activating it
    /// afterwards would bring back the window that was in front.
    private static func activate(_ bundle: String, window hint: String?) -> Bool {
        guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: bundle).first else { return false }
        let activated = DispatchQueue.main.sync { app.activate() }
        if let hint, !hint.isEmpty, AXIsProcessTrusted() {
            let element = AXUIElementCreateApplication(app.processIdentifier)
            AXUIElementSetMessagingTimeout(element, 0.5)
            var value: CFTypeRef?
            if AXUIElementCopyAttributeValue(element, kAXWindowsAttribute as CFString, &value) == .success,
               let windows = value as? [AXUIElement] {
                let match = windows.first { window in
                    var title: CFTypeRef?
                    AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &title)
                    return (title as? String)?.localizedCaseInsensitiveContains(hint) == true
                }
                if let match {
                    AXUIElementSetAttributeValue(match, kAXMinimizedAttribute as CFString, kCFBooleanFalse)
                    AXUIElementSetAttributeValue(match, kAXMainAttribute as CFString, kCFBooleanTrue)
                    AXUIElementPerformAction(match, kAXRaiseAction as CFString)
                    AXUIElementSetAttributeValue(element, kAXFocusedWindowAttribute as CFString, match)
                }
            }
        }
        return activated
    }
}

enum ClaudeTerminalReply {
    typealias Failure = ClaudeReplyFailure

    static func send(_ text: String, images: [URL] = [], named: [URL] = [], to session: ClaudeSession, bridge: URL) -> Failure? {
        let paths = images.map(\.path)
        switch session.host {
        case .terminal(let tty):
            guard AppleScriptRunner.consentToAutomate(bundleID: ClaudeSessionHosts.terminal) else { return .unreachable }
            let script = paths.isEmpty ? ClaudeTerminalScripts.type(text, tty: tty)
                : ClaudeTerminalScripts.type(text, images: paths, tty: tty)
            let result = AppleScriptRunner.run(script)
            return result.ok && result.output == "sent" ? nil : .unreachable
        case .antigravity:
            guard let shell = ClaudeTerminalBridge.shell(of: session.pid) else { return .unreachable }
            if paths.isEmpty { return ClaudeTerminalBridge.send(text, shell: shell, folder: bridge) ? nil : .unreachable }
            return ClaudeTerminalBridge.send(text, images: paths, named: named.isEmpty ? paths : named.map(\.path),
                                             shell: shell, folder: bridge)
        case .app, .unknown:
            return .unreachable
        }
    }
}

/// Puts the bridge extension that ships inside the app into Antigravity,
/// through Antigravity's own command line.
enum ClaudeBridgeInstaller {
    static let identifier = "vorssaint.vorssaint-terminal-bridge"
    static let cli = "Contents/Resources/app/bin/antigravity-ide"

    static func installedVersions(home: URL) -> [String] {
        let folder = home.appending(path: ".antigravity-ide/extensions").path
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder)) ?? []
        return names.filter { $0.hasPrefix(identifier + "-") }.map { String($0.dropFirst(identifier.count + 1)) }
    }

    static func isInstalled(home: URL) -> Bool { !installedVersions(home: home).isEmpty }

    /// The version the app carries, from the extension's own manifest.
    static var bundledVersion: String? {
        guard let manifest = Bundle.main.url(forResource: "AntigravityBridge", withExtension: nil)?
                .appending(path: "extension/package.json"),
              let data = FileManager.default.contents(atPath: manifest.path),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        return json["version"] as? String
    }

    static func isOutdated(home: URL) -> Bool {
        guard let bundled = bundledVersion else { return false }
        return !installedVersions(home: home).contains { $0 == bundled || $0.hasPrefix(bundled + "-") }
    }

    static func uninstall(home: URL) {
        guard isInstalled(home: home),
              let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeSessionHosts.antigravity) else { return }
        _ = try? run(app.appending(path: cli).path, ["--uninstall-extension", identifier], in: home)
    }

    enum Failure: Equatable {
        case missing, noAntigravity, packaging, refused
        case other(String)
    }

    /// Installs the extension, or the newer one the app carries. Nil when it worked.
    static func install(home: URL) -> Failure? {
        guard let source = Bundle.main.url(forResource: "AntigravityBridge", withExtension: nil) else { return .missing }
        guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeSessionHosts.antigravity) else {
            return .noAntigravity
        }
        let work = FileManager.default.temporaryDirectory.appending(path: "vorssaint-bridge-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: work) }
        let package = work.appending(path: "vorssaint-terminal-bridge.vsix")
        do {
            try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
            let zip = try run("/usr/bin/zip", ["-q", "-r", "-X", package.path, "."], in: source)
            guard zip == 0 else { return .packaging }
            let installed = try run(app.appending(path: cli).path, ["--install-extension", package.path, "--force"], in: work)
            return installed == 0 ? nil : .refused
        } catch {
            return .other(error.localizedDescription)
        }
    }

    private static func run(_ tool: String, _ arguments: [String], in folder: URL) throws -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        process.currentDirectoryURL = folder
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        return process.terminationStatus
    }
}

/// What the Sessions section leaves outside the app's own folders, taken
/// away when the app is uninstalled: its hooks in Claude Code's settings,
/// the Antigravity extension and the `~/.vorssaint` folder they talk through.
enum ClaudeSessionCleanup {
    /// The official build and the Developer build share the extension and
    /// its folder; each takes away only its own hooks and socket, and the
    /// shared parts go with the last of them.
    static func siblings(of bundle: String) -> [String] {
        let official = "com.vorssaint.utils"
        return bundle == official ? [official + ".dev"] : [official]
    }

    /// What was taken away, for the uninstaller to report.
    @discardableResult
    static func removeForUninstall(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                                   bundle: String = ClaudeSessionService.bundleID,
                                   siblingInstalled: (String) -> Bool = { NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0) != nil })
        -> (hooks: Bool, shared: Bool) {
        let settings = ClaudeHookInstaller.settingsURL(home: home)
        var hooks = false
        if let current = try? ClaudeHookInstaller.read(settings), ClaudeHookInstaller.hasAny(in: current, bundle: bundle) {
            hooks = (try? ClaudeHookInstaller.update(settings) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }) != nil
        }
        let folder = home.appending(path: ".vorssaint", directoryHint: .isDirectory)
        try? FileManager.default.removeItem(at: folder.appending(path: "\(bundle).sock"))
        let others = siblings(of: bundle).filter { sibling in
            siblingInstalled(sibling)
                || ((try? ClaudeHookInstaller.read(settings)).map { ClaudeHookInstaller.hasAny(in: $0, bundle: sibling) } ?? false)
        }
        guard others.isEmpty else { return (hooks, false) }
        ClaudeBridgeInstaller.uninstall(home: home)
        try? FileManager.default.removeItem(at: folder)
        return (hooks, true)
    }
}
