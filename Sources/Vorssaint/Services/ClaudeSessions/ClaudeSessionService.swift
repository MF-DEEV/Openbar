// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Combine
import Foundation

/// Follows every Claude Code session running on this Mac while the Sessions
/// section is on. It reads Claude Code's process registry and the new lines
/// of each running session's transcript, takes in the hook events the
/// sessions send, and publishes what the island shows. Nothing is written or
/// sent until the person acts on a session; what sessions say stays unread
/// unless previews are on, and then only in memory. It also asks Claude Code's
/// Agent View now and then which sessions run in the background.
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
    /// Codex sessions whose requests Always allows until their turn ends.
    @Published private(set) var autoAllowed: Set<String> = []
    let events = PassthroughSubject<ClaudeSessionEvent, Never>()
    /// Whether Claude Code's settings hold this app's hooks; nil until checked.
    @Published private(set) var hooksInstalled: Bool?
    @Published private(set) var hookFailure: String?
    /// The editors on this Mac and whether each has the extension replies
    /// are typed through; nil until checked.
    @Published private(set) var bridges: [ClaudeBridgeStatus]?
    /// Why the last install into an editor failed, by its bundle identifier.
    @Published private(set) var bridgeFailures: [String: String] = [:]
    /// Whether the island hears Codex, for its row in Settings.
    @Published private(set) var codexStatus: CodexStatus?
    /// Claude Code is installed, so the island can start sessions in a terminal.
    @Published private(set) var canStartSessions = false
    /// Its Agent View is on, so it can run them in the background too.
    @Published private(set) var backgroundAvailable = false
    /// Codex is installed, so New session can start it too.
    @Published private(set) var canStartCodex = false
    /// The page that starts a new session is open.
    @Published var composingSession = false

    enum CodexStatus: Equatable {
        case connected
        /// Codex's folder is there but no app-server runs.
        case notRunning
        case notFound
    }

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
    /// The sessions as they came, before Always hid the waits it answers.
    private var latest: [ClaudeSession] = []
    /// Codex sessions given Always, and the answers it has on their way.
    private var autoAllow = NotchSessionAutoAllow()
    private var autoAllowWork: [String: DispatchWorkItem] = [:]
    private var publishedAutoAllow: Set<String> = []
    /// Codex's sessions, from the app-server its terminal UIs share. They join
    /// the same list as Claude Code's (docs/codex-sessions.md).
    private var codex: CodexSessionClient?
    /// Codex's terminal UIs by working folder, refreshed each tick, so Open
    /// and the host of a Codex session find its terminal.
    private var codexTerminals: [(pid: Int32, cwd: String)] = []
    /// What Agent View last listed, by config folder's order. Queue.
    private var backgroundJobs: [ClaudeBackgroundJob] = []
    private var backgroundEndings: [String: Date] = [:]
    private var backgroundBranches: [String: String] = [:]
    private var listingJobs = false
    private var jobsListed: Date?
    /// `claude` runs take a moment and must not hold up the reading.
    private let commands = DispatchQueue(label: "com.vorssaint.claude-commands", qos: .utility)
    /// How often Agent View is asked: more often while it has sessions.
    private static let jobsEvery: TimeInterval = 10
    private static let jobsEveryQuietly: TimeInterval = 60
    /// Permission requests whose hook waits for the island's answer.
    private var pending: [String: (connection: ClaudeHookServer.Connection, session: String, suggestions: Any?,
                                   question: [String: Any]?, approved: [String: Any]?, since: Date)] = [:]
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
                if policy.previews { readers = [:]; subagentMeta = []; taskStamps = [:] } else { ClaudeChatImage.forget() }
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
            let client = CodexSessionClient()
            codex = client
            client.start { [weak self] in self?.queue.async { self?.publish(now: Date()) } }
            schedule(Self.activeTick)
            refresh()
        }
        let home = home
        actions.async {
            ClaudeImageAttachments.sweep()
            // An extension the person once added is kept current by the app,
            // so a newer one never waits on a click in Settings.
            for ide in ClaudeBridgeInstaller.editors()
            where ClaudeBridgeInstaller.isInstalled(ide, home: home) && ClaudeBridgeInstaller.isOutdated(ide, home: home) {
                _ = ClaudeBridgeInstaller.install(ide, home: home)
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
        autoAllowed = []
        composingSession = false
        queue.async { [self] in
            backgroundJobs = []
            backgroundEndings = [:]
            backgroundBranches = [:]
            jobsListed = nil
            timer?.cancel()
            timer = nil
            interval = 0
            // Always holds for one run of the section, like the turn it covers.
            latest = []
            autoAllow = NotchSessionAutoAllow()
            autoAllowWork.values.forEach { $0.cancel() }
            autoAllowWork = [:]
            publishedAutoAllow = []
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
            codex?.stop()
            codex = nil
            codexTerminals = []
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
        // An open Codex page keeps its thread joined, so its replies and
        // approvals reach the island while it is shown.
        queue.async { [self] in codex?.focus(id.map { [$0] } ?? []) }
    }

    /// What a subagent's Clawd wears: its own lot, in the color its
    /// definition gives it.
    func costume(for agent: ClaudeSubagent, in session: ClaudeSession) -> ClawdCostume {
        ClawdCostume.seeded(NotchClaudeSessionsSupport.costumeKey(agent, in: session), agentColor: agent.color)
    }

    /// Dresses a session's Clawd, or with nil gives it its own costume back.
    func setCostume(_ costume: ClawdCostume?, for id: String) {
        // Compared with the lot of whoever wears it: Clawd's or Codex's robot's.
        let character = sessions.first { $0.id == id }?.provider.character ?? .clawd
        costumes = NotchClaudeSessionsSupport.kept(costumes, id: id,
                                                   value: ClawdCostume.kept(costume, key: id, character: character),
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
            codex?.acknowledge(id, now: Date())
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    /// Answers the permission the session waits on, if its hook still waits.
    /// Answers the request the person saw, and only that one: a newer request
    /// for the same session, or one already settled, is left alone.
    func decide(_ id: String, request: String, _ decision: ClaudeHookMessage.Decision) {
        guard settled(id, request: request) else { return }
        queue.async { [self] in
            // Codex takes its answer through its own server; the first answer wins there.
            if let codex, codex.owns(id) {
                if codex.answer(id, requestID: request, answer: CodexAnswer(decision)) {
                    codex.acknowledge(id, now: Date())
                }
                publish(now: Date())
                return
            }
            guard engine.holds(request, for: id), let asked = pending[request] else { return }
            // A question is answered or skipped, never allowed as it stands;
            // answers go only to a question.
            switch decision {
            case .allow, .allowAlways: guard asked.question == nil else { return }
            case .answer: guard asked.question != nil else { return }
            case .deny: break
            }
            pending[request] = nil
            asked.connection.finish(ClaudeHookMessage.reply(decision, suggestions: asked.suggestions, input: asked.question,
                                                                   approved: asked.approved))
            engine.resolvePermission(requestID: request)
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    /// A request is answerable once it has been on screen a moment, so a
    /// click or ⌘Y meant for the one before it cannot answer it.
    func settled(_ id: String, request: String) -> Bool {
        guard let shown = shownRequests[id], shown.request == request else { return false }
        return Date().timeIntervalSince(shown.at) >= NotchClaudeSessionsSupport.settleDelay
    }

    /// Always on a Codex session: every request of the turn under way is
    /// allowed as it comes, the one waiting now too, until the turn ends.
    /// With nil it turns over.
    func setAutoAllow(_ id: String, _ on: Bool? = nil) {
        queue.async { [self] in
            guard autoAllow.set(id, on: on, session: latest.first { $0.id == id }, now: Date()) else { return }
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
        if session.provider == .codex {
            // Codex takes replies through its server: nothing is typed into
            // the terminal, and images go as the files they are.
            queue.async { [self] in
                let sent = codex?.reply(session.id, text: reply, images: images) ?? false
                if sent { codex?.acknowledge(session.id, now: Date()) }
                // Refused while it waits on a prompt, one Always answers too;
                // otherwise Codex couldn't be reached.
                let waiting = latest.first { $0.id == session.id }.map { $0.phase.isWaiting || $0.waitingFor != nil } ?? false
                finishSend(sent ? nil : (waiting ? .waiting : .unreachable), session: session, completion: completion)
            }
            return
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

    // MARK: Background sessions

    /// Asks Agent View for its sessions, when it is time to or `force`d after
    /// an action. On the queue.
    private func listBackgroundJobs(now: Date, force: Bool = false) {
        guard !listingJobs else { return }
        if !force, let listed = jobsListed,
           now.timeIntervalSince(listed) < (backgroundJobs.isEmpty ? Self.jobsEveryQuietly : Self.jobsEvery) { return }
        listingJobs = true
        jobsListed = now
        let homes = ClaudeSessionRegistryReader.homes(home)
        let home = home
        commands.async { [weak self] in
            let shell = ClaudeCLI.shell()
            var jobs: [ClaudeBackgroundJob] = []
            var listed = false
            let turnedOff = ClaudeBackgroundJobs.isOn(shell.environment[ClaudeCLI.disableVariable])
                || homes.contains { folder in
                    FileManager.default.contents(atPath: folder.appending(path: "settings.json").path)
                        .map(ClaudeBackgroundJobs.settingsDisable) ?? false
                }
            if !turnedOff {
                for folder in homes where FileManager.default.fileExists(atPath: folder.appending(path: "sessions").path) {
                    let path = folder.standardizedFileURL.path
                    guard let output = ClaudeCLI.claude(["agents", "--json", "--all"], configHome: path, in: home.path),
                          output.status == 0, let found = ClaudeBackgroundJobs.decode(output.out, configHome: path) else { continue }
                    listed = true
                    jobs += found
                }
            }
            self?.queue.async { [weak self] in
                guard let self else { return }
                self.listingJobs = false
                guard self.timer != nil else { return }
                self.apply(jobs, available: listed)
                let canStart = shell.executable != nil
                let canCodex = CodexCLI.locate(path: shell.environment["PATH"], home: home) != nil
                DispatchQueue.main.async {
                    if self.canStartSessions != canStart { self.canStartSessions = canStart }
                    if self.canStartCodex != canCodex { self.canStartCodex = canCodex }
                    if self.backgroundAvailable != listed { self.backgroundAvailable = listed }
                }
            }
        }
    }

    private func apply(_ jobs: [ClaudeBackgroundJob], available: Bool) {
        let now = Date()
        let alive = Set(backgroundJobs.filter { !$0.ended }.map(\.id))
        backgroundEndings = ClaudeBackgroundJobs.endings(jobs, known: backgroundEndings, alive: alive, now: now)
        backgroundJobs = jobs
        publish(now: now)
    }

    /// The sessions with what Agent View adds: each one's state, and rows for
    /// those whose process has ended.
    private func withBackground(_ sessions: [ClaudeSession], now: Date) -> [ClaudeSession] {
        guard !backgroundJobs.isEmpty || sessions.contains(where: { $0.background != nil }) else { return sessions }
        var listed = sessions
        for index in listed.indices where listed[index].background != nil {
            let id = listed[index].id
            guard let job = backgroundJobs.first(where: { $0.sessionID == id }) else { continue }
            listed[index].background?.state = job.state
            if let branch = listed[index].branch { backgroundBranches[id] = branch }
        }
        let live = Set(listed.map(\.id))
        let ended = ClaudeBackgroundJobs.kept(backgroundJobs.filter { !live.contains($0.sessionID) },
                                              endings: backgroundEndings, now: now)
        return listed + ended.map {
            ClaudeBackgroundJobs.session($0, ended: backgroundEndings[$0.id] ?? now, branch: backgroundBranches[$0.sessionID])
        }
    }

    /// What a `claude` command answered, as the failure to show; nil when it worked.
    private func failure(of output: ClaudeCLI.Output?) -> String? {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        guard let output else { return text.commandFailed }
        if output.status == 0 { return nil }
        return output.error.isEmpty ? text.commandFailed : output.error
    }

    /// Runs one of `claude stop`, `claude rm`. Calls back on the main thread
    /// with what went wrong.
    private func backgroundCommand(_ verb: String, _ session: ClaudeSession, completion: @escaping (String?) -> Void) {
        guard let details = session.background else { return completion(nil) }
        commands.async { [self] in
            let problem = failure(of: ClaudeCLI.claude([verb, details.job], configHome: details.configHome))
            queue.async { [self] in listBackgroundJobs(now: Date(), force: true) }
            DispatchQueue.main.async { completion(problem) }
        }
    }

    /// Stops a background session. Its conversation is kept.
    func stopBackground(_ session: ClaudeSession, completion: @escaping (String?) -> Void) {
        backgroundCommand("stop", session, completion: completion)
    }

    /// Deletes a background session for good.
    func removeBackground(_ session: ClaudeSession, completion: @escaping (String?) -> Void) {
        backgroundCommand("rm", session, completion: completion)
    }

    /// Its recent output, read off the screen `claude logs` prints. With a
    /// nil text, there is none to read: a stopped session keeps no screen.
    func backgroundLogs(_ session: ClaudeSession, completion: @escaping (String?) -> Void) {
        guard let details = session.background else { return completion(nil) }
        commands.async {
            let output = ClaudeCLI.claude(["logs", details.job], configHome: details.configHome)
            var screen = output.flatMap { $0.status == 0 ? ClaudeTerminalScreen.text($0.out) : nil }
            // Its last output may be nothing but a spinner; the screen itself still reads.
            if !details.ended, !ClaudeTerminalScreen.isReadable(screen ?? "") {
                screen = ClaudeBackgroundReply.screen(job: details.job, configHome: details.configHome) ?? screen
            }
            DispatchQueue.main.async { completion(screen) }
        }
    }

    /// Folders to start a session in, the latest worked in first.
    func recentFolders(completion: @escaping ([ClaudeRecentFolder]) -> Void) {
        let homes = ClaudeSessionRegistryReader.homes(home)
        commands.async {
            let folders = ClaudeRecentFolders.read(homes: homes)
            DispatchQueue.main.async { completion(folders) }
        }
    }

    /// The models each agent can start with, for the New session page.
    func newSessionModels(configHome: String?, completion: @escaping (_ claude: [NewSessionModel], _ codex: [NewSessionModel]) -> Void) {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        commands.async {
            let claude = NewSessionModels.readClaude(configHome: configHome, defaultName: text.defaultModel)
            let codex = NewSessionModels.readCodex(home: CodexSessionClient.home(), defaultName: text.defaultModel)
            DispatchQueue.main.async { completion(claude, codex) }
        }
    }

    /// Starts Claude Code or Codex in a folder, in the background or in the
    /// person's terminal, with a model or the agent's default. Calls back on
    /// the main thread with what went wrong.
    func startSession(in folder: String, configHome: String?, prompt: String, background: Bool,
                      provider: ClaudeSessionProvider = .claude, model: String? = nil,
                      completion: @escaping (String?) -> Void) {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        let trimmed = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let terminal = ClaudeTerminalLauncher.preferred(from: sessions)
        if provider == .codex {
            commands.async { [self] in
                if background {
                    startCodexThread(in: folder, prompt: trimmed, model: model, text: text, completion: completion)
                    return
                }
                var problem: String?
                if let executable = CodexCLI.executable() {
                    let line = CodexCLI.line(executable: executable,
                                             arguments: CodexCLI.startArguments(prompt: trimmed, model: model), folder: folder)
                    if !ClaudeTerminalLauncher.open(line, terminal: terminal) { problem = text.codexFailed }
                } else {
                    problem = text.codexFailed
                }
                DispatchQueue.main.async { completion(problem) }
            }
            return
        }
        // A background session has nobody to type to it.
        if background, trimmed.isEmpty { completion(text.backgroundNeedsPrompt); return }
        let arguments = ClaudeCLI.startArguments(prompt: trimmed, background: background, model: model)
        commands.async { [self] in
            var problem: String?
            if background {
                let output = ClaudeCLI.claude(arguments, configHome: configHome, in: folder, timeout: 30)
                problem = failure(of: output)
                // Claude Code asks only in a terminal whether a folder can be trusted.
                if let error = output?.error, output?.status != 0, error.contains("not trusted") { problem = text.folderNotTrusted }
            } else if let executable = ClaudeCLI.shell().executable {
                let line = ClaudeCLI.line(executable: executable, arguments: arguments, configHome: configHome, folder: folder)
                if !ClaudeTerminalLauncher.open(line, terminal: terminal) { problem = text.commandFailed }
            } else {
                problem = text.commandFailed
            }
            queue.async { [self] in listBackgroundJobs(now: Date(), force: true) }
            DispatchQueue.main.async { completion(problem) }
        }
    }

    /// A Codex thread with no terminal, run by Codex's shared app-server,
    /// which is started first when it isn't running. On `commands`.
    private func startCodexThread(in folder: String, prompt: String, model: String?, text: NotchClaudeSessionStrings,
                                  completion: @escaping (String?) -> Void) {
        let finish: (String?) -> Void = { problem in DispatchQueue.main.async { completion(problem) } }
        guard !prompt.isEmpty else { finish(text.backgroundNeedsPrompt); return }
        guard let client = queue.sync(execute: { codex }) else { finish(text.codexFailed); return }
        if !client.connected {
            guard let output = CodexCLI.run(CodexCLI.daemonStart), output.status == 0 else { finish(text.codexNotStarted); return }
            // The client looks for the daemon's socket every few seconds.
            let deadline = Date().addingTimeInterval(CodexSessionClient.retry * 3)
            while !client.connected, Date() < deadline { Thread.sleep(forTimeInterval: 0.2) }
        }
        let asked = client.startThread(cwd: folder, model: model, prompt: prompt) { error in
            finish(error.map { $0.isEmpty ? text.codexFailed : $0 })
        }
        if !asked { finish(text.codexNotStarted) }
    }

    func checkCodex() {
        queue.async { [self] in
            let status: CodexStatus
            if codex?.connected == true { status = .connected }
            else if let home = CodexSessionClient.home(), FileManager.default.fileExists(atPath: home.path) { status = .notRunning }
            else { status = .notFound }
            DispatchQueue.main.async { self.codexStatus = status }
        }
    }

    func checkBridge() {
        let home = home
        actions.async {
            let bridges = ClaudeBridgeInstaller.editors().map { ide in
                let installed = ClaudeBridgeInstaller.isInstalled(ide, home: home)
                return ClaudeBridgeStatus(ide: ide, installed: installed,
                                          outdated: installed && ClaudeBridgeInstaller.isOutdated(ide, home: home))
            }
            DispatchQueue.main.async { self.bridges = bridges }
        }
    }

    func installBridge(_ ide: ClaudeIDE) {
        let home = home
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        actions.async {
            let failure: String? = switch ClaudeBridgeInstaller.install(ide, home: home) {
            case nil: nil
            case .missing?: text.bridgeMissing
            case .noEditor?: text.editorMissing(ide.name)
            case .packaging?: text.bridgePackageFailed
            case .refused?: text.bridgeRefused(ide.name)
            case .other(let reason)?: reason
            }
            DispatchQueue.main.async {
                self.bridgeFailures[ide.bundle] = failure
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
        let request = UUID().uuidString
        guard let message = ClaudeHookMessage.decode(payload, requestID: request, now: now, policy: engine.policy) else {
            return connection.finish()
        }
        if let transcript = message.transcript, !transcript.isEmpty { transcripts[message.fact.sessionID] = transcript }
        if message.isPermissionRequest {
            pending[request] = (connection, message.fact.sessionID, message.suggestions, message.questionInput,
                                message.approvalInput, now)
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
        let terminal = ClaudeTerminalLauncher.preferred(from: sessions)
        actions.async {
            let found = session.background != nil
                ? ClaudeHostFocus.attach(session, terminal: terminal)
                : ClaudeHostFocus.focus(session, homes: homes)
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
        // Codex's terminal UIs are looked for every few ticks: it lists every process.
        if codex?.connected != true {
            codexTerminals = []
        } else if ticks % Self.sideFilesEvery == 0 {
            codexTerminals = ClaudeSessionHosts.codexTerminals()
        }
        let codexPids = Set(codexTerminals.map(\.pid))
        hosts = hosts.filter { alive.contains($0.key) || codexPids.contains($0.key) }
        for index in records.indices {
            let pid = records[index].pid
            // A background session has no terminal to find.
            if let job = records[index].job {
                records[index].host = .background(job: job)
                continue
            }
            if hosts[pid] == nil { hosts[pid] = ClaudeSessionHosts.host(of: pid) }
            records[index].host = hosts[pid] ?? .unknown
        }
        engine.ingest(records: records, now: now)
        listBackgroundJobs(now: now)
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
        let claude = snapshot.sessions.map { agentColors.decorate($0, now: now) }
        let fromCodex = codex?.snapshot(now: now, previews: engine.policy.previews) ?? (sessions: [], events: [])
        let codexSessions = fromCodex.sessions.map { session in
            placed(session, others: fromCodex.sessions.filter { $0.id != session.id }.map(\.cwd))
        }
        let sorted = NotchClaudeSessionsSupport.sorted(withBackground(claude, now: now) + codexSessions)
        let shown = autoAllowing(sorted, now: now)
        let allowed = autoAllow.sessions
        let hidden = Set(sorted.filter { NotchClaudeSessionsSupport.hidesAutoApproval($0, autoAllowed: allowed, now: now) }
            .map(\.id))
        let events = (snapshot.events + fromCodex.events).filter { NotchClaudeSessionsSupport.shows($0, hidden: hidden) }
        let changed = shown != published || allowed != publishedAutoAllow
        published = shown
        publishedAutoAllow = allowed
        guard changed || !events.isEmpty else { return }
        DispatchQueue.main.async { [self] in
            guard running else { return }
            if changed {
                sessions = shown
                counts = ClaudeSessionCounts(shown)
                if autoAllowed != allowed { autoAllowed = allowed }
                for session in shown {
                    let request = session.permission?.requestID
                    if shownRequests[session.id]?.request != request {
                        shownRequests[session.id] = request.map { (request: $0, at: Date()) }
                    }
                }
                // A session that left forgets its request: one coming back
                // (Codex numbers requests afresh after a restart) waits its
                // settle delay again.
                let listed = Set(shown.map(\.id))
                shownRequests = shownRequests.filter { listed.contains($0.key) }
                if let focusedID, !listed.contains(focusedID) {
                    self.focusedID = nil
                    dressingID = nil
                    // Its Codex thread is let go too, so it can unload.
                    queue.async { [self] in codex?.focus([]) }
                }
            }
            events.forEach(self.events.send)
        }
    }

    /// Keeps Always to the turn it was given in, sends each answer it
    /// plans, and gives the sessions as the island shows them. On the queue.
    private func autoAllowing(_ sessions: [ClaudeSession], now: Date) -> [ClaudeSession] {
        latest = sessions
        let plan = autoAllow.update(sessions, now: now)
        for id in plan.cancel { autoAllowWork.removeValue(forKey: id)?.cancel() }
        for answer in plan.schedule {
            let work = DispatchWorkItem { [weak self] in self?.autoAllowNow(answer.id, request: answer.request) }
            autoAllowWork[answer.id] = work
            queue.asyncAfter(deadline: .now() + answer.after, execute: work)
        }
        return NotchClaudeSessionsSupport.presented(sessions, autoAllowed: autoAllow.sessions, now: now)
    }

    /// Allows a request Always covers, if it still waits and Always still holds.
    private func autoAllowNow(_ id: String, request: String) {
        autoAllowWork[id] = nil
        guard timer != nil, let codex, autoAllow.fire(id, request: request, in: latest, now: Date()) else { return }
        // One answered in the terminal meanwhile is no failure; one Codex refused ends Always.
        if !codex.answer(id, requestID: request, answer: .allow), codex.waits(id, requestID: request) {
            autoAllow.failed(id)
        }
        publish(now: Date())
    }

    /// A Codex session with the terminal UI that shows it: the one codex
    /// running in its folder gives it a pid and a host, so Open brings its tab
    /// forward like a Claude Code session's.
    private func placed(_ session: ClaudeSession, others: [String]) -> ClaudeSession {
        guard let pid = ClaudeSessionHosts.codexTerminal(cwd: session.cwd, among: codexTerminals, others: others) else {
            return session
        }
        if hosts[pid] == nil { hosts[pid] = ClaudeSessionHosts.host(of: pid) }
        var placed = session
        placed.pid = pid
        placed.host = hosts[pid] ?? session.host
        return placed
    }
}

extension ClaudeSessionHosts {
    static func host(of pid: Int32) -> ClaudeSessionHost {
        var environment = self.environment(of: pid)
        let device = tty(of: pid)
        let bundle = environment["__CFBundleIdentifier"]
        // Only asked for when the environment leaves the app open.
        let owner = bundle == nil || environment["TERM_PROGRAM"] == "vscode"
            ? ResponsibleProcess.regularAppOwner(of: pid)?.bundleIdentifier : nil
        let port = environment["CLAUDE_CODE_SSE_PORT"].flatMap(Int.init)
        var host = classify(bundle: bundle, tty: device, port: port, environment: environment, owner: owner)
        // tmux names its pane in the environment of everything started in
        // it, an editor opened from it too: the pane has to be the session's.
        if case .tmux(var target) = host {
            guard ClaudeTmux.holds(target, tty: device) else {
                environment["TMUX"] = nil
                environment["TMUX_PANE"] = nil
                return classify(bundle: bundle, tty: device, port: port, environment: environment, owner: owner)
            }
            // The terminal showing it, for its name; Open looks again.
            if let client = ClaudeTmux.client(of: target), let outer = Self.host(of: client.pid).bundle {
                target.outer = outer
                host = .tmux(target)
            }
        }
        return host
    }
}

/// Brings a session's terminal forward: its own tab, pane or window in a
/// terminal that can be asked for one, the window holding its folder in an
/// editor, or just the app elsewhere.
enum ClaudeHostFocus {
    static func focus(_ session: ClaudeSession, homes: [URL]) -> Bool {
        focus(session.host, pid: session.pid, cwd: session.cwd, homes: homes)
    }

    /// `depth` stops a tmux shown in a tmux shown in … from going on forever.
    static func focus(_ host: ClaudeSessionHost, pid: Int32, cwd: String, homes: [URL], depth: Int = 0) -> Bool {
        switch host {
        case .terminal(let tty):
            return script(ClaudeSessionHosts.terminal, ClaudeTerminalScripts.focus(tty: tty))
        case .iterm(let tty):
            return script(ClaudeTerminalApps.iterm, ClaudeTTYTerminalScripts.itermFocus(tty: tty))
        case .ghostty(let tty):
            return script(ClaudeTerminalApps.ghostty, ClaudeTTYTerminalScripts.ghosttyFocus(tty: tty))
        case .tmux(let target):
            // tmux selects the pane; the terminal showing it comes forward
            // the way a session in that terminal would.
            guard let client = ClaudeTmux.focus(target, tty: ClaudeSessionHosts.tty(of: pid)) else { return false }
            guard depth < 3 else { return true }
            _ = focus(ClaudeSessionHosts.host(of: client.pid), pid: client.pid, cwd: cwd, homes: homes, depth: depth + 1)
            return true
        case .wezterm(let pane, let socket):
            guard let found = ClaudeWezTerm.focus(pane: pane, socket: socket, tty: ClaudeSessionHosts.tty(of: pid)) else {
                return activate(ClaudeTerminalApps.wezterm, window: nil)
            }
            return activate(ClaudeTerminalApps.wezterm, window: found.windowTitle)
        case .kitty(let window, let socket):
            // Without remote control kitty can only come forward as a whole.
            if let socket { _ = ClaudeKitty.focus(window: window, pid: pid, socket: socket) }
            return activate(ClaudeTerminalApps.kitty, window: nil)
        case .ide(let bundle, let port):
            let ide = ClaudeIDE.find(bundle) ?? ClaudeIDE(bundle: bundle, name: bundle)
            let folders = port.map { ClaudeIDELock.folders(port: $0, homes: homes) } ?? []
            // The editor brings forward the window that already has the
            // session's folder open when asked to open that folder, with no
            // permission needed. Only a folder that is open is asked for, so
            // no new window ever opens.
            let folder = ClaudeIDELock.openFolder(cwd: cwd, folders: folders)
            // The app comes forward first: asked from its command line while
            // another app is in front, the editor only reorders its own
            // windows and stays behind. A window is looked up by its title
            // only when the command line has no folder to ask for.
            let activated = activate(bundle, window: folder == nil ? ClaudeIDELock.windowHint(cwd: cwd, folders: folders) : nil)
            let raised = folder.map { open($0, in: ide) } ?? false
            // The extension then shows the session's own terminal in it.
            if let shell = ClaudeTerminalBridge.shell(of: pid) {
                _ = ClaudeTerminalBridge.focus(shell: shell, folder: ClaudeTerminalBridge.folder())
            }
            return activated || raised
        case .app(let bundle):
            return activate(bundle, window: nil)
        case .background:
            // The service's openHost attaches these; Open has no window to raise.
            return false
        case .unknown:
            return false
        }
    }

    /// Runs a script that finds the session's own tab; without Automation,
    /// or when the tab is gone, the app still comes forward.
    private static func script(_ bundle: String, _ source: String) -> Bool {
        guard AppleScriptRunner.consentToAutomate(bundleID: bundle) else { return activate(bundle, window: nil) }
        let result = AppleScriptRunner.run(source)
        return result.ok && result.output == "found" ? true : activate(bundle, window: nil)
    }

    /// Opens the person's terminal with `claude attach` running in it, which
    /// also brings back a session that has stopped.
    static func attach(_ session: ClaudeSession, terminal: String?) -> Bool {
        guard let details = session.background, let executable = ClaudeCLI.shell().executable else { return false }
        let line = ClaudeCLI.line(executable: executable, arguments: ["attach", details.job],
                                  configHome: details.configHome, folder: nil)
        return ClaudeTerminalLauncher.open(line, terminal: terminal)
    }

    private static func open(_ folder: String, in ide: ClaudeIDE) -> Bool {
        guard let cli = ide.places(home: FileManager.default.homeDirectoryForCurrentUser)?.cli else { return false }
        let process = Process()
        process.executableURL = cli
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
        let steps = ClaudeTyping.steps(text, images: paths)
        switch session.host {
        case .terminal(let tty):
            let script = paths.isEmpty ? ClaudeTerminalScripts.type(text, tty: tty)
                : ClaudeTerminalScripts.type(text, images: paths, tty: tty)
            return run(script, in: ClaudeSessionHosts.terminal)
        case .iterm(let tty):
            return run(ClaudeTTYTerminalScripts.itermType(steps, tty: tty), in: ClaudeTerminalApps.iterm)
        case .ghostty(let tty):
            return run(ClaudeTTYTerminalScripts.ghosttyType(steps, tty: tty), in: ClaudeTerminalApps.ghostty)
        case .tmux(let target):
            return ClaudeTmux.send(steps, to: target, tty: ClaudeSessionHosts.tty(of: session.pid))
        case .wezterm(let pane, let socket):
            return ClaudeWezTerm.send(steps, pane: pane, socket: socket, tty: ClaudeSessionHosts.tty(of: session.pid))
        case .kitty(let window, let socket?):
            return ClaudeKitty.send(steps, window: window, pid: session.pid, socket: socket)
        case .ide:
            guard let shell = ClaudeTerminalBridge.shell(of: session.pid) else { return .unreachable }
            if paths.isEmpty { return ClaudeTerminalBridge.send(text, shell: shell, folder: bridge) ? nil : .unreachable }
            return ClaudeTerminalBridge.send(text, images: paths, named: named.isEmpty ? paths : named.map(\.path),
                                             shell: shell, folder: bridge)
        case .background(let job):
            return ClaudeBackgroundReply.send(text, images: paths, job: job, configHome: session.background?.configHome)
        case .kitty, .app, .unknown:
            return .unreachable
        }
    }

    /// Runs a script that types into the session's own tab, with the app's
    /// Automation consent; it answers "sent" only when it found the tab.
    private static func run(_ script: String, in bundle: String) -> Failure? {
        guard AppleScriptRunner.consentToAutomate(bundleID: bundle) else { return .unreachable }
        let result = AppleScriptRunner.run(script)
        return result.ok && result.output == "sent" ? nil : .unreachable
    }
}

/// An editor on this Mac and the state of the extension in it.
struct ClaudeBridgeStatus: Equatable, Identifiable {
    let ide: ClaudeIDE
    let installed: Bool
    let outdated: Bool
    var id: String { ide.bundle }
}

/// Puts the bridge extension that ships inside the app into an editor built
/// on VS Code (Antigravity, VS Code, Cursor, Windsurf), through the editor's
/// own command line. The same extension serves them all.
enum ClaudeBridgeInstaller {
    static let identifier = "vorssaint.vorssaint-terminal-bridge"

    static func installedVersions(_ ide: ClaudeIDE, home: URL) -> [String] {
        guard let folder = ide.places(home: home)?.extensions.path else { return [] }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: folder)) ?? []
        return names.filter { $0.hasPrefix(identifier + "-") }.map { String($0.dropFirst(identifier.count + 1)) }
    }

    static func isInstalled(_ ide: ClaudeIDE, home: URL) -> Bool { !installedVersions(ide, home: home).isEmpty }

    /// The editors on this Mac, the ones the island can reach.
    static func editors() -> [ClaudeIDE] { ClaudeIDE.all.filter { $0.app != nil } }

    /// The version the app carries, from the extension's own manifest.
    static var bundledVersion: String? {
        guard let manifest = Bundle.main.url(forResource: "AntigravityBridge", withExtension: nil)?
                .appending(path: "extension/package.json"),
              let data = FileManager.default.contents(atPath: manifest.path),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        return json["version"] as? String
    }

    static func isOutdated(_ ide: ClaudeIDE, home: URL) -> Bool {
        guard let bundled = bundledVersion else { return false }
        return !installedVersions(ide, home: home).contains { $0 == bundled || $0.hasPrefix(bundled + "-") }
    }

    /// Takes the extension out of every editor that has it.
    static func uninstall(home: URL) {
        for ide in editors() where isInstalled(ide, home: home) {
            guard let cli = ide.places(home: home)?.cli else { continue }
            _ = try? run(cli.path, ["--uninstall-extension", identifier], in: home)
        }
    }

    enum Failure: Equatable {
        case missing, noEditor, packaging, refused
        case other(String)
    }

    /// Installs the extension, or the newer one the app carries. Nil when it worked.
    static func install(_ ide: ClaudeIDE, home: URL) -> Failure? {
        guard let source = Bundle.main.url(forResource: "AntigravityBridge", withExtension: nil) else { return .missing }
        guard let cli = ide.places(home: home)?.cli else { return .noEditor }
        let work = FileManager.default.temporaryDirectory.appending(path: "vorssaint-bridge-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: work) }
        let package = work.appending(path: "vorssaint-terminal-bridge.vsix")
        do {
            try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
            let zip = try run("/usr/bin/zip", ["-q", "-r", "-X", package.path, "."], in: source)
            guard zip == 0 else { return .packaging }
            let installed = try run(cli.path, ["--install-extension", package.path, "--force"], in: work)
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
