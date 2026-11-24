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
    /// When each ask a session holds was first published, keyed by
    /// session and request: each settles on its own (#269). Main thread.
    private var shownRequests: [String: Date] = [:]
    /// What each ask looks like it does, for Allow all (#269). Queue.
    private let risks = PermissionRiskCache()
    /// Codex sessions whose requests Always allows until their turn ends.
    @Published private(set) var autoAllowed: Set<String> = []
    let events = PassthroughSubject<ClaudeSessionEvent, Never>()
    /// Whether Claude Code's settings hold this app's hooks, in every account
    /// shown; nil until checked.
    @Published private(set) var hooksInstalled: Bool?
    @Published private(set) var hookFailure: String?
    /// The Claude Code accounts on this Mac, hidden ones too (issue #216).
    @Published private(set) var accounts: [ClaudeAccount] = []
    /// Whether each account's settings hold this app's hooks, by account id.
    @Published private(set) var accountHooks: [String: ClaudeAccountHooks] = [:]
    /// The account each session runs under, by session id, only while
    /// sessions of more than one account are listed.
    @Published private(set) var accountLabels: [String: String] = [:]
    /// The editors on this Mac and whether each has the extension replies
    /// are typed through; nil until checked.
    @Published private(set) var bridges: [ClaudeBridgeStatus]?
    /// Why the last install into an editor failed, by its bundle identifier.
    @Published private(set) var bridgeFailures: [String: String] = [:]
    /// Whether the island hears Codex, for its row in Settings.
    @Published private(set) var codexStatus: CodexStatus?
    /// Whether Gemini CLI, Copilot CLI, OpenCode and Cursor hold the island's
    /// hooks or plugin, and whether each seems installed; nil until checked.
    @Published private(set) var agentHooks: [ClaudeSessionProvider: (state: AgentHookInstaller.State, present: Bool)]?
    /// Whether Hermes was told to run the island's hooks (#244); nil until
    /// they are installed.
    @Published private(set) var hermesApproval: AgentHermes.Approval?
    /// Why the last install into an agent failed.
    @Published private(set) var agentHookFailures: [ClaudeSessionProvider: String] = [:]
    /// Every config file the island writes hooks into, for the Hooks page
    /// (#236); nil until checked.
    @Published private(set) var hookTargets: [HookTarget]?
    /// Why the last Reinstall or Remove of a row failed, by row id.
    @Published private(set) var hookTargetFailures: [String: String] = [:]
    /// Rows being changed right now.
    @Published private(set) var hookTargetsBusy: Set<String> = []
    /// Watches the folders of the hook files, so hooks lost while the app
    /// runs are seen (and put back) at once.
    private var hookWatcher: AgentLogWatcher?
    private var watchedHookFiles: Set<String> = []
    private var hookCheckDue = false
    /// How often each row was put back this run; a file something keeps
    /// emptying is left alone after a few. Actions queue.
    private var repairedThisRun: [String: Int] = [:]
    /// Claude Code is installed, so the island can start sessions in a terminal.
    @Published private(set) var canStartSessions = false
    /// Its Agent View is on, so it can run them in the background too.
    @Published private(set) var backgroundAvailable = false
    /// Codex is installed, so New session can start it too.
    @Published private(set) var canStartCodex = false
    /// Gemini CLI, Copilot CLI, OpenCode and `cursor-agent` found on this Mac,
    /// which New session can start in a terminal.
    @Published private(set) var startableAgents: Set<ClaudeSessionProvider> = []
    /// How each remote host stands, by alias (docs/remote-hosts.md).
    @Published private(set) var remoteStates: [String: RemoteHostState] = [:]
    /// Hosts being set up or cleaned up right now.
    @Published private(set) var remoteBusy: Set<String> = []
    @Published private(set) var remoteFailures: [String: String] = [:]
    /// The agents shown when the settings were last read. Main thread.
    private var lastShownAgents: Set<ClaudeSessionProvider>?
    /// The agents' hook states as last logged. Main thread.
    private var agentHookSummary: String?
    /// The page that starts a new session is open.
    @Published var composingSession = false
    /// How the person answered the sample request onboarding showed, once
    /// they have (#168); nil while it waits or none was shown.
    @Published private(set) var sampleAnswer: ClaudeHookMessage.Decision?

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

    let queue = DispatchQueue(label: "com.openbar.claude-sessions", qos: .utility, autoreleaseFrequency: .workItem)
    /// AppleScript and window lookups block until the other app answers.
    private let actions = DispatchQueue(label: "com.openbar.claude-actions", qos: .userInitiated)
    private let hookQueue = DispatchQueue(label: "com.openbar.claude-hooks", qos: .userInitiated)
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
    private var inspection = PermissionInspectionGate()
    /// The sample request onboarding shows, which no agent asked, and what
    /// it has to tell the island next. Queue.
    private var sample: ClaudeSession?
    private var sampleEvents: [ClaudeSessionEvent] = []
    private var sampleRound = 0
    /// Codex sessions given Always, and the answers it has on their way.
    private var autoAllow = NotchSessionAutoAllow()
    private var autoAllowWork: [String: DispatchWorkItem] = [:]
    private var publishedAutoAllow: Set<String> = []
    /// Codex's sessions, from the app-server its terminal UIs share. They join
    /// the same list as Claude Code's (docs/codex-sessions.md).
    private var codex: CodexSessionClient?

    /// This Mac's Codex and every host's. Queue.
    private var codexClients: [CodexSessionClient] {
        (codex.map { [$0] } ?? []) + remoteCodex.keys.sorted().compactMap { remoteCodex[$0] }
    }

    /// The Codex client a session's thread belongs to. Queue.
    private func codexOwner(_ id: String) -> CodexSessionClient? {
        codexClients.first { $0.owns(id) }
    }
    /// Codex's terminal UIs by working folder, refreshed each tick, so Open
    /// and the host of a Codex session find its terminal.
    private var codexTerminals: [(pid: Int32, cwd: String)] = []
    /// Gemini CLI's, Copilot CLI's, OpenCode's and Cursor's sessions, from
    /// their hooks and plugin. Queue.
    private let hookAgents = AgentHookSessions()
    /// Cowork's sessions in the Claude app, read from its files while their
    /// switch is on (#246). Queue.
    private let cowork = ClaudeCoworkSessions()
    private var coworkRows: (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) = ([], [])
    /// The Claude app's switches: its Code tab's sessions shown, Cowork's read. Queue.
    private var desktopShown = true
    private var coworkRead = false
    private let branches = AgentGitBranch()
    /// OpenCode permission requests whose plugin waits for the island's
    /// answer, and ZCode's whose hook does.
    private var agentPending: [String: (connection: ClaudeHookServer.Connection, session: String, since: Date)] = [:]
    /// The input of each ZCode question waiting, handed back with the answers.
    private var agentInputs: [String: [String: Any]] = [:]
    /// Where each Hermes session runs: its hooks name only its gateway (#244).
    private let hermesPlaces = AgentHermesPlaces()
    /// Which of Qwen Code's events end or carry on its turns (#239). Queue.
    private var qwenTurns = AgentQwen.Turns()
    /// Each OpenCode's plugin listening for replies, by its process.
    private var listeners: [Int32: ClaudeHookServer.Connection] = [:]
    /// The Mistral Vibe sessions a New session runs through the island's
    /// app-server proxy, by the proxy's process. Those approvals and questions
    /// are answered here; Vibe's hooks only hear of them, so theirs are dropped. Queue.
    private var proxied: [String: pid_t] = [:]
    /// The agents whose sessions the list shows. Queue.
    private var shownAgents = Set(ClaudeSessionProvider.allCases)
    /// What Agent View last listed, by config folder's order. Queue.
    private var backgroundJobs: [ClaudeBackgroundJob] = []
    private var backgroundEndings: [String: Date] = [:]
    private var backgroundBranches: [String: String] = [:]
    private var listingJobs = false
    private var jobsListed: Date?
    /// `claude` runs take a moment and must not hold up the reading.
    private let commands = DispatchQueue(label: "com.openbar.claude-commands", qos: .utility)
    /// How often Agent View is asked: more often while it has sessions.
    private static let jobsEvery: TimeInterval = 10
    private static let jobsEveryQuietly: TimeInterval = 60
    /// The SSH connections to remote hosts, by alias, and what each last
    /// saw there. Queue.
    private var remoteLinks: [String: RemoteHostLink] = [:]
    private var remoteSnapshots: [String: RemoteSnapshot] = [:]
    /// Each host's Codex, through the socket its connection forwards. Queue.
    private var remoteCodex: [String: CodexSessionClient] = [:]
    /// Remote transcripts being read, by alias and path on the host.
    private var remoteTails: [String: (tail: RemoteTranscriptTail, reader: ClaudeTranscriptReader)] = [:]
    /// Permission requests whose hook waits for the island's answer.
    private var pending: [String: (connection: ClaudeHookServer.Connection, session: String, suggestions: Any?,
                                   question: [String: Any]?, approved: [String: Any]?, planModes: [ClaudePlanMode],
                                   since: Date)] = [:]
    /// Claude Code gives up on a hook after its timeout; the island lets go first.
    private static let permissionDeadline: TimeInterval = TimeInterval(ClaudeHookInstaller.permissionTimeout - 10)

    static var bundleID: String { Bundle.main.bundleIdentifier ?? "com.openbar.app" }

    /// Where the hooks write. The app's own folder, unless its path is too
    /// long for a socket.
    static func socketPath(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String {
        let preferred = PrivateFileStore.containerURL?.appending(path: "claude-hooks.sock").path
        if let preferred, ClaudeHookServer.fits(preferred) { return preferred }
        return home.appending(path: ".openbar/\(bundleID).sock").path
    }

    func syncWithPreferences() {
        SessionAutoMode.shared.settings = SessionAutoMode.Settings(defaults: .standard)
        let policy = NotchClaudeSessionsSupport.policy()
        let agents = NotchClaudeSessionsSupport.shownAgents()
        let desktop = NotchClaudeSessionsSupport.showsClaudeDesktop()
        let coworkOn = NotchClaudeSessionsSupport.readsCowork()
        guard NotchClaudeSessionsSupport.isEnabled() else { stop(); return }
        queue.async { [self] in
            if shownAgents != agents || desktopShown != desktop || coworkRead != coworkOn {
                shownAgents = agents
                desktopShown = desktop
                coworkRead = coworkOn
                if !coworkOn || !desktop { coworkRows = ([], []); _ = cowork.scan([], now: Date(), previews: false, running: false) }
                publish(now: Date())
            }
            if engine.policy != policy {
                engine.policy = policy
                // Reading again with previews on fills in what was skipped.
                if policy.previews {
                    readers = [:]; subagentMeta = []; taskStamps = [:]
                    remoteTails = [:]
                    remoteLinks.values.forEach { $0.rereadTranscripts() }
                } else {
                    ClaudeChatImage.forget()
                    hookAgents.purgeContent()
                    cowork.purgeContent()
                }
                for key in readers.keys { readers[key]?.reader.policy = policy }
                for key in remoteTails.keys { remoteTails[key]?.reader.policy = policy }
                // What was cleared leaves the island now, not at the next read.
                publish(now: Date())
            }
        }
        // A switch turned on or off puts its agent's hooks in or takes them out.
        if running, agents != lastShownAgents { syncAgentHooks() }
        let remote = RemoteHostPrefs.active()
        if running { queue.async { [self] in syncRemote(remote) } }
        lastShownAgents = agents
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
            syncRemote(remote)
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
        syncAgentHooks()
        // Hooks lost while the app was closed come back now, if the person allows it (#236).
        checkHookTargets(repair: true)
    }

    func stop() {
        guard running else { return }
        running = false
        focusedID = nil
        dressingID = nil
        permissionWatch = nil
        hookWatcher?.stop()
        hookWatcher = nil
        watchedHookFiles = []
        sessions = []
        accountLabels = [:]
        remoteStates = [:]
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
            SessionAutoMode.log.clear()
            readers = [:]
            transcripts = [:]
            subagentMeta = []
            taskStamps = [:]
            hosts = [:]
            published = []
            // Hooks waiting for an answer go back to the terminal's prompt.
            pending.values.forEach { $0.connection.finish() }
            pending = [:]
            agentPending.values.forEach { $0.connection.finish() }
            agentPending = [:]
            agentInputs = [:]
            listeners.values.forEach { $0.finish() }
            listeners = [:]
            hookAgents.prune(now: Date()) { _ in false }
            syncRemote([])
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
        guard ClaudeImageAttachments.holdsImages(pasteboard), !isRemote(id) else { return false }
        attach(ClaudeImageAttachments.images(from: pasteboard), to: id)
        return true
    }

    func attach(_ images: [URL], to id: String) {
        guard !images.isEmpty, !isRemote(id) else { return }
        var list = attachments[id] ?? []
        for image in images where !list.contains(image) { list.append(image) }
        attachments[id] = Array(list.prefix(ClaudeImageAttachments.limit))
    }

    /// A host's session takes text only: it can't open this Mac's files.
    private func isRemote(_ id: String) -> Bool {
        sessions.first { $0.id == id }?.host.remoteAlias != nil
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

    /// The ask the open page shows, when the queue opened it on one that
    /// waits behind another (#269); nil shows the session's first.
    @Published private(set) var focusedRequest: String?

    /// The session whose page is open, as the page shows it.
    var focusedSession: ClaudeSession? {
        focusedID.flatMap { id in sessions.first { $0.id == id } }?.showing(focusedRequest)
    }

    func focus(_ id: String?, request: String? = nil) {
        focusedID = id
        focusedRequest = id == nil ? nil : request
        dressingID = nil
        if let id { acknowledge(id) }
        // An open Codex page keeps its thread joined, so its replies and
        // approvals reach the island while it is shown.
        queue.async { [self] in codexClients.forEach { $0.focus(id.map { [$0] } ?? []) } }
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
            hookAgents.acknowledge(id, now: Date())
            cowork.acknowledge(id, now: Date())
            codexClients.forEach { $0.acknowledge(id, now: Date()) }
            engine.acknowledge(id, now: Date())
            publish(now: Date())
        }
    }

    /// Answers the permission the session waits on, if its hook still waits.
    /// Answers the request the person saw, and only that one: a newer request
    /// for the same session, or one already settled, is left alone.
    func decide(_ id: String, request: String, _ decision: ClaudeHookMessage.Decision, revision: String? = nil, inspectionToken: String? = nil, autoAllowTurn: Bool = false) {
        // Deny waits the same moment, so a ⌘N meant for the request before cannot deny this one.
        guard settled(id, request: request) else { return }
        let revision = revision ?? sessions.first(where: { $0.id == id })?.ask(request)
            .flatMap { p in p.content.map { $0.revision(p) } }
        queue.async { [self] in
            if id == ClaudeSessionSample.id { return answerSample(request: request, decision) }
            // Codex takes its answer through its own server; the first answer wins there.
            if let codex = codexOwner(id) {
                if codex.answer(id, requestID: request, answer: CodexAnswer(decision), revision: revision, inspectionToken: inspectionToken) {
                    codex.acknowledge(id, now: Date())
                    if autoAllowTurn, case .allow = decision {
                        _ = autoAllow.set(id, on: true, session: latest.first { $0.id == id }, now: Date())
                    }
                }
                publish(now: Date())
                return
            }
            // Any ask the session holds, not only the first: the queue
            // answers them in any order (#269).
            let current = hookAgents.owns(id) ? hookAgents.entry(id)?.permission : engine.permission(request, of: id)
            guard let current, current.requestID == request else { return }
            switch decision {
            case .deny, .answer: break
            default:
                guard inspection.allows(id, current, revision: revision, token: inspectionToken) else { return }
            }
            if hookAgents.owns(id), let entry = hookAgents.entry(id), entry.provider == .hermes {
                // Hermes's classic prompt takes Return for the Allow once it
                // has picked, arrows down to Deny and a question's number;
                // anywhere else (its Ink UI, its app, a terminal that takes no
                // keys) the answer goes through Hermes's gateway (#244).
                let shown = latest.first(where: { $0.id == id })
                if AgentHookSessions.hermesTakesKeys(entry.client), let shown, ClaudeTyping.takesKeys(shown.host),
                   let keys = AgentHookKeys.hermes(decision, current) {
                    actions.async { [self] in
                        guard ClaudeTerminalReply.press(keys, to: shown) == nil else { return }
                        queue.async { [self] in
                            hookAgents.resolve(id, request: request)
                            hookAgents.acknowledge(id, now: Date())
                            publish(now: Date())
                        }
                    }
                    return
                }
                guard let answer = AgentHermesGateway.answer(decision, current) else { return }
                let home = home, session = entry.sessionID
                actions.async { [self] in
                    let shell = ClaudeCLI.shell(home: home)
                    guard let hermes = AgentCLI.locate(.hermes, path: shell.environment["PATH"], home: home),
                          AgentHermesGateway.send(answer, session: session, hermes: hermes, environment: shell.environment)
                    else {
                        DiagnosticsLog.info("sessions", "hermes: the gateway took no answer")
                        return
                    }
                    queue.async { [self] in
                        hookAgents.resolve(id, request: request)
                        hookAgents.acknowledge(id, now: Date())
                        publish(now: Date())
                    }
                }
                return
            }
            if hookAgents.owns(id), let entry = hookAgents.entry(id), entry.provider == .qwen, AgentQwen.answeredByKeys(current) {
                // Qwen Code's own dialog takes a question's or a plan's answer,
                // as keys where its terminal takes them (#239).
                guard let session = latest.first(where: { $0.id == id }), ClaudeTyping.takesKeys(session.host),
                      let keys = AgentQwen.keys(decision, current, oneBurst: AgentQwen.oneBurst(session.host)) else { return }
                actions.async { [self] in
                    guard ClaudeTerminalReply.press(keys, to: session) == nil else { return }
                    queue.async { [self] in
                        hookAgents.resolve(id, request: request)
                        hookAgents.acknowledge(id, now: Date())
                        publish(now: Date())
                    }
                }
                return
            }
            if hookAgents.owns(id), let entry = hookAgents.entry(id), entry.provider == .grok {
                // Grok's prompt takes Return for the yes it has picked and
                // Ctrl-C for no; its hooks tell when it moves on (#243).
                guard let keys = AgentHookKeys.grok(decision) else { return }
                guard let session = latest.first(where: { $0.id == id }), ClaudeTyping.takesKeys(session.host) else { return }
                actions.async { [self] in
                    guard ClaudeTerminalReply.press(keys, to: session) == nil else { return }
                    queue.async { [self] in
                        hookAgents.resolve(id, request: request)
                        hookAgents.acknowledge(id, now: Date())
                        publish(now: Date())
                    }
                }
                return
            }
            if hookAgents.owns(id) {
                // OpenCode's plugin and ZCode's and Qwen Code's hooks wait for the
                // answer; the other agents ask in their own terminal.
                let provider = hookAgents.entry(id)?.provider
                let vibe = provider == .vibe, zcode = provider == .zcode, qwen = provider == .qwen
                if vibe || zcode || qwen {
                    // A question is answered or skipped, never allowed; answers go only to a question.
                    switch decision {
                    case .allow, .allowAlways, .approvePlan: if current.asksQuestions { return }
                    case .answer: if !current.asksQuestions { return }
                    case .deny: break
                    }
                }
                guard let asked = agentPending[request], asked.session == id,
                      let reply = vibe ? AgentHookMessage.vibeReply(decision, questions: current.questions)
                        : zcode ? AgentHookMessage.zcodeReply(decision, input: agentInputs[request])
                        : qwen ? AgentQwen.reply(decision)
                        : AgentHookMessage.opencodeReply(decision) else { return }
                if case .deny = decision {} else {
                    // ZCode stops the hook once the person answered in it, Qwen Code once it was cancelled.
                    guard !asked.connection.isClosed, !((zcode || qwen) && asked.connection.peerIsGone),
                          Date().timeIntervalSince(asked.since) <= Self.permissionDeadline else { return }
                }
                agentPending[request] = nil
                agentInputs[request] = nil
                asked.connection.finish(reply)
                hookAgents.resolve(id, request: request)
                hookAgents.acknowledge(id, now: Date())
                publish(now: Date())
                return
            }
            guard engine.holds(request, for: id), let asked = pending[request] else { return }
            if case .deny = decision {} else {
                guard !asked.connection.peerIsGone, Date().timeIntervalSince(asked.since) <= Self.permissionDeadline else { return }
            }
            // A question is answered or skipped, never allowed as it stands;
            // answers go only to a question, and a mode only to a plan.
            var decision = decision
            switch decision {
            case .allow, .allowAlways:
                guard asked.question == nil else { return }
                // A plain approval of a plan, from a shortcut or the list,
                // picks the mode the card puts first.
                if asked.approved != nil, let first = asked.planModes.first { decision = .approvePlan(first) }
            case .approvePlan(let mode): guard asked.approved != nil, asked.planModes.contains(mode) else { return }
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

    /// Allows every ask of the queue that looks safe and has been on screen
    /// a moment (#269); each goes through `decide` like a click on its row,
    /// so one that changed, or came since, is left for the person.
    func allowAll(_ queue: NotchPermissionQueue) {
        for row in queue.allowAll where settled(row.session.id, request: row.ask.requestID) {
            decide(row.session.id, request: row.ask.requestID, .allow)
        }
    }

    func inspectCommand(_ id: String, request: String, revision: String, completion: @escaping (PermissionInspection?) -> Void) {
        queue.async { [self] in
            let result: PermissionInspection?
            if let codex = codexOwner(id) {
                result = codex.inspect(id, request: request, revision: revision)
            } else if let p = hookAgents.owns(id) ? hookAgents.entry(id)?.permission : engine.permission(request, of: id), p.requestID == request {
                result = inspection.open(id, p, revision: revision)
            } else { result = nil }
            DispatchQueue.main.async { completion(result) }
        }
    }

    func closeInspection(_ token: String) {
        queue.async { [self] in
            inspection.close(token)
            codexClients.forEach { $0.closeInspection(token) }
        }
    }

    /// A request is answerable once it has been on screen a moment, so a
    /// click or ⌘Y meant for the one before it cannot answer it.
    func settled(_ id: String, request: String) -> Bool {
        guard let shown = shownRequests[Self.shownKey(id, request)] else { return false }
        return Date().timeIntervalSince(shown) >= NotchClaudeSessionsSupport.settleDelay
    }

    private static func shownKey(_ session: String, _ request: String) -> String { session + "\u{1}" + request
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
        if session.id == ClaudeSessionSample.id { return completion(nil) }
        let images = attachments[session.id] ?? []
        let reply = NotchClaudeSessionsSupport.reply(text) ?? ""
        guard !reply.isEmpty || !images.isEmpty, NotchClaudeSessionsSupport.canReply(session) else {
            return completion(session.phase.isWaiting || session.waitingFor != nil ? .waiting : .unreachable)
        }
        if let alias = session.host.remoteAlias, session.provider != .codex {
            // Typed into its tmux pane on the host, through the host's connection.
            let steps = ClaudeTyping.steps(reply)
            queue.async { [self] in
                let asking = engine.permission(of: session.id) != nil || pending.values.contains { $0.session == session.id }
                guard !reply.isEmpty, !asking, let link = remoteLinks[alias] else {
                    return finishSend(asking ? .waiting : .unreachable, session: session, completion: completion)
                }
                link.perform { [self] link in
                    finishSend(link.type(steps, pid: session.pid, session: session.id), session: session, completion: completion)
                }
            }
            return
        }
        if session.provider.reportsThroughPlugin {
            // OpenCode's plugin and Pi's extension send the reply from inside the agent.
            queue.async { [self] in
                let id = hookAgents.entry(session.id)?.sessionID
                let listener = hookAgents.entry(session.id).flatMap { listeners[$0.pid] }
                var sent = false
                if let id, let listener, !listener.isClosed, let prompt = AgentHookMessage.opencodePrompt(session: id, text: reply) {
                    listeners[session.pid] = nil
                    listener.finish(prompt)
                    sent = true
                    hookAgents.acknowledge(session.id, now: Date())
                }
                finishSend(sent ? nil : .unreachable, session: session, completion: completion)
            }
            return
        }
        if session.heardThroughHooks {
            // Typed into the agent's terminal, as for Claude Code; images stay
            // behind, since only Claude Code reads them from a paste.
            let bridge = ClaudeTerminalBridge.folder(home: home)
            actions.async { [self] in
                let failure = reply.isEmpty ? .unreachable : ClaudeTerminalReply.send(reply, to: session, bridge: bridge)
                finishSend(failure, session: session, completion: completion)
            }
            return
        }
        if session.provider == .codex {
            // Codex takes replies through its server: nothing is typed into
            // the terminal, and images go as the files they are.
            queue.async { [self] in
                let owner = codexOwner(session.id)
                let sent = owner?.reply(session.id, text: reply, images: images) ?? false
                if sent { owner?.acknowledge(session.id, now: Date()) }
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
                let startable = Set(ClaudeSessionProvider.hookAgents.filter {
                    AgentCLI.startsWithPrompt($0) && AgentCLI.locate($0, path: shell.environment["PATH"], home: home) != nil
                })
                DispatchQueue.main.async {
                    if self.canStartSessions != canStart { self.canStartSessions = canStart }
                    if self.canStartCodex != canCodex { self.canStartCodex = canCodex }
                    if self.startableAgents != startable { self.startableAgents = startable }
                    // An agent installed since is picked up as it is found.
                    if self.running { self.syncAgentHooks() }
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

    /// Deletes a background session for good, and the worktree Claude Code
    /// made for it. A worktree with changes found nowhere else is kept, and
    /// the session with it, unless `anyway` says to lose them.
    func removeBackground(_ session: ClaudeSession, anyway: Bool = false,
                          completion: @escaping (WorktreeRemoval) -> Void) {
        guard let details = session.background else { return completion(.done) }
        commands.async { [self] in
            var outcome = SessionWorktrees.removeClaudeJob(details.job, configHome: details.configHome,
                                                           cwd: session.cwd, anyway: anyway)
            if outcome == .failed("") { outcome = .failed(FeatureStrings.notchClaudeSessions(L10n.shared.language).commandFailed) }
            queue.async { [self] in listBackgroundJobs(now: Date(), force: true) }
            DispatchQueue.main.async { completion(outcome) }
        }
    }

    /// Removes the worktree the island made for a session, keeping its
    /// branch. One with changes stays unless `anyway` says to lose them.
    func removeWorktree(_ session: ClaudeSession, anyway: Bool = false,
                        completion: @escaping (WorktreeRemoval) -> Void) {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        let cwd = session.cwd
        commands.async {
            let outcome: WorktreeRemoval
            switch SessionWorktrees.remove(cwd, force: anyway) {
            case .removed: outcome = .done
            case .unsaved: outcome = .unsaved
            case .failed(let said): outcome = .failed(said.isEmpty ? text.commandFailed : said)
            }
            DispatchQueue.main.async { completion(outcome) }
        }
    }

    /// The branch a session's worktree is on, for Copy branch.
    func worktreeBranch(_ session: ClaudeSession, completion: @escaping (String?) -> Void) {
        guard let folder = SessionWorktrees.worktreeFolder(session.cwd) else { return completion(nil) }
        commands.async {
            let branch = FileManager.default.fileExists(atPath: folder) ? SessionWorktrees.branch(folder) : nil
            DispatchQueue.main.async { completion(branch) }
        }
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

    /// Starts Claude Code, Codex or another agent in a folder, in the
    /// background or in the person's terminal, with a model or the agent's
    /// default. With `worktree`, a folder in a git repository gets the
    /// session its own worktree: Claude Code makes it with `--worktree`, the
    /// island makes it for the rest. Calls back on the main thread with what
    /// went wrong.
    func startSession(in folder: String, configHome: String?, prompt: String, background: Bool,
                      provider: ClaudeSessionProvider = .claude, model: String? = nil,
                      worktree: Bool = false, worktreeName: String? = nil, safeCommands: [String] = [],
                      completion: @escaping (String?) -> Void) {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        let trimmed = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        let terminal = ClaudeTerminalLauncher.preferred(from: sessions)
        // A background session has nobody to type to it.
        let inBackground = background && (provider == .claude || provider == .codex)
        if inBackground, trimmed.isEmpty { completion(text.backgroundNeedsPrompt); return }
        commands.async { [self] in
            let finish: (String?) -> Void = { problem in DispatchQueue.main.async { completion(problem) } }
            var folder = folder
            var claudeWorktree: String?
            if worktree, let root = SessionWorktrees.state(of: folder).root {
                let name = SessionWorktrees.reserveName(custom: worktreeName, prompt: trimmed, root: root)
                if provider == .claude {
                    claudeWorktree = name
                } else {
                    switch SessionWorktrees.create(root: root, name: name) {
                    case .success(let path):
                        folder = SessionWorktrees.inside(path, for: folder, root: root)
                    case .failure(let failure):
                        return finish(failure.message.isEmpty ? text.worktreeFailed : "\(text.worktreeFailed): \(failure.message)")
                    }
                }
            }
            if provider.reportsThroughHooks {
                // In a terminal, where the agent's own UI runs and its hooks report it.
                let shell = ClaudeCLI.shell()
                guard let executable = AgentCLI.locate(provider, path: shell.environment["PATH"],
                                                       home: FileManager.default.homeDirectoryForCurrentUser) else {
                    return finish(text.commandFailed)
                }
                var line = CodexCLI.line(executable: executable,
                                         arguments: AgentCLI.startArguments(provider, prompt: trimmed), folder: folder)
                // Vibe's server runs through the island's proxy, which answers its approvals and questions.
                if provider == .vibe, let proxy = Bundle.main.executablePath,
                   let install = VibeLauncher.install(vibe: executable),
                   let proxied = VibeLauncher.line(install: install, proxy: proxy, socket: Self.socketPath(),
                                                   prompt: trimmed, folder: folder) {
                    line = proxied
                }
                return finish(ClaudeTerminalLauncher.open(line, terminal: terminal) ? nil : text.commandFailed)
            }
            if provider == .codex {
                if background {
                    return startCodexThread(in: folder, prompt: trimmed, model: model, safeCommands: safeCommands,
                                            text: text, completion: completion)
                }
                guard let executable = CodexCLI.executable() else { return finish(text.codexFailed) }
                // The terminal's thread is named only once it starts, in this folder.
                if !safeCommands.isEmpty { queue.sync { codex }?.expectSafeCommands(cwd: folder, commands: safeCommands) }
                let line = CodexCLI.line(executable: executable,
                                         arguments: CodexCLI.startArguments(prompt: trimmed, model: model), folder: folder)
                return finish(ClaudeTerminalLauncher.open(line, terminal: terminal) ? nil : text.codexFailed)
            }
            let arguments = ClaudeCLI.startArguments(prompt: trimmed, background: background, model: model,
                                                     worktree: claudeWorktree, allowed: SafeCommands.claudeRules(safeCommands))
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
            finish(problem)
        }
    }

    /// Whether a folder is in a git repository, and whether its checkout has
    /// changes a new worktree won't start with, for the New session page.
    func repositoryState(of folder: String, completion: @escaping (SessionWorktrees.RepoState) -> Void) {
        commands.async {
            let state = SessionWorktrees.state(of: folder)
            DispatchQueue.main.async { completion(state) }
        }
    }

    /// A Codex thread with no terminal, run by Codex's shared app-server,
    /// which is started first when it isn't running. On `commands`.
    private func startCodexThread(in folder: String, prompt: String, model: String?, safeCommands: [String] = [],
                                  text: NotchClaudeSessionStrings,
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
        let asked = client.startThread(cwd: folder, model: model, prompt: prompt, safeCommands: safeCommands) { error in
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
            DispatchQueue.main.async {
                if self.codexStatus != status { DiagnosticsLog.info("codex", "codex \(status)") }
                self.codexStatus = status
            }
        }
    }

    func checkAgentHooks() {
        let home = home
        let socket = Self.socketPath(home: home)
        let bundle = Self.bundleID
        actions.async {
            var states: [ClaudeSessionProvider: (state: AgentHookInstaller.State, present: Bool)] = [:]
            for provider in ClaudeSessionProvider.hookAgents {
                let state = AgentHookInstaller.state(provider, home: home, socket: socket, bundle: bundle)
                let path = ClaudeCLI.shell(home: home).environment["PATH"]
                states[provider] = (state, AgentHookInstaller.isPresent(provider, home: home, path: path))
            }
            let summary = ClaudeSessionProvider.hookAgents.map { provider in
                let entry = states[provider]
                return "\(provider.rawValue)=\(entry.map { "\($0.state)" } ?? "?")\(entry?.present == true ? "" : "(absent)")"
            }.joined(separator: " ")
            // Hermes runs a new hook only once the person allowed it there.
            let hermes = states[.hermes]?.state == .installed
                ? AgentHermes.approval(home: home, socket: socket, bundle: bundle) : nil
            DispatchQueue.main.async {
                if self.agentHookSummary != summary { DiagnosticsLog.info("hooks", "agent hooks: \(summary)") }
                self.agentHookSummary = summary
                self.agentHooks = states
                if self.hermesApproval != hermes { self.hermesApproval = hermes }
            }
        }
    }

    /// Puts the island's hooks or plugin into one agent, or takes them out,
    /// as Try again asks after a change that failed.
    func installAgentHooks(_ provider: ClaudeSessionProvider, _ install: Bool) {
        let home = home
        let socket = Self.socketPath(home: home)
        let bundle = Self.bundleID
        actions.async {
            let failure = Self.changeAgentHooks(provider, install: install, home: home, socket: socket, bundle: bundle)
            DispatchQueue.main.async {
                self.agentHookFailures[provider] = failure
                self.checkAgentHooks()
            }
        }
    }

    /// Keeps every agent's hooks in step, with no button to press: an agent
    /// found on this Mac whose switch is on gets the island's hooks or
    /// plugin, with a backup first, and an update when they point elsewhere;
    /// an agent whose switch is off has them taken out. Runs when the
    /// section starts, when its settings change, and as the agents are
    /// looked for, so one installed later is picked up.
    func syncAgentHooks() {
        let home = home
        let socket = Self.socketPath(home: home)
        let bundle = Self.bundleID
        let shown = NotchClaudeSessionsSupport.shownAgents()
        actions.async {
            let path = ClaudeCLI.shell(home: home).environment["PATH"]
            let found = Set(ClaudeSessionProvider.hookAgents.filter { AgentHookInstaller.isPresent($0, home: home, path: path) })
            let records = HookRecords.all()
            var states: [ClaudeSessionProvider: HookRecords.State] = [:]
            for provider in ClaudeSessionProvider.hookAgents {
                states[provider] = AgentHookInstaller.diagnosticsState(provider, home: home, socket: socket, bundle: bundle,
                                                                       records: records)
            }
            let plan = AgentHookInstaller.plan(found: found, shown: shown, states: states,
                                               optedOut: HookMaintenance.optedOutAgents(),
                                               repair: HookMaintenance.autoRepair())
            guard !plan.install.isEmpty || !plan.remove.isEmpty else { return }
            // Hooks the island put in that someone took out: put back, and said once (#236).
            let putBack = plan.install.filter { states[$0] == .removedByHand && self.noteRepair(HookRecords.agentID($0)) }
            var failures: [ClaudeSessionProvider: String?] = [:]
            for provider in plan.install {
                failures[provider] = Self.changeAgentHooks(provider, install: true, home: home, socket: socket, bundle: bundle)
            }
            for provider in plan.remove {
                failures[provider] = Self.changeAgentHooks(provider, install: false, home: home, socket: socket, bundle: bundle)
            }
            DispatchQueue.main.async {
                for (provider, failure) in failures { self.agentHookFailures[provider] = failure }
                self.checkAgentHooks()
                self.announceRepair(putBack.filter { failures[$0] == .some(nil) }.map(\.displayName))
                self.checkHookTargets()
            }
        }
    }

    // MARK: Hooks page (#236)

    private var hookEnvironment: HookMaintenance.Environment {
        HookMaintenance.Environment(home: home, socket: Self.socketPath(home: home), bundle: Self.bundleID)
    }

    /// Everything the rows are read from; off the main thread.
    private static func hookContext(_ env: HookMaintenance.Environment) -> HookTargets.Context {
        let path = ClaudeCLI.shell(home: env.home).environment["PATH"]
        return HookTargets.Context(home: env.home, socket: env.socket, bundle: env.bundle, path: path,
                                   accounts: ClaudeAccounts.all(home: env.home),
                                   shown: NotchClaudeSessionsSupport.shownAgents(), folders: AgentFolders.all(),
                                   tools: CustomHookTools.all(), optOut: HookMaintenance.optOut(),
                                   records: HookRecords.all(), codexPresent: CodexCLI.locate(path: path, home: env.home) != nil)
    }

    /// Reads every row again; with `repair`, puts back the hooks that were
    /// lost first, as the person allows (#236).
    func checkHookTargets(repair: Bool = false) {
        let env = hookEnvironment
        actions.async { [self] in
            var context = Self.hookContext(env)
            var targets = HookTargets.list(context)
            var putBack: [String] = []
            if repair {
                let due = HookMaintenance.repairs(targets, records: context.records, autoRepair: HookMaintenance.autoRepair())
                for target in due where noteRepair(target.id) {
                    do {
                        try HookMaintenance.install(target, env: env)
                        DiagnosticsLog.info("hooks", "put back the hooks in \(target.file?.path ?? target.id)")
                        putBack.append(target.provider == .claude ? "Claude Code (\(target.name))" : target.name)
                    } catch {
                        DiagnosticsLog.error("hooks", "could not put back the hooks in \(target.file?.path ?? target.id): \(error)")
                    }
                }
                if !due.isEmpty {
                    context = Self.hookContext(env)
                    targets = HookTargets.list(context)
                }
            }
            DispatchQueue.main.async {
                if self.hookTargets != targets { self.hookTargets = targets }
                self.watchHookFiles(targets)
                self.announceRepair(putBack)
                if !putBack.isEmpty { self.checkHooks() }
            }
        }
    }

    /// Counts one putting back of a row; false once a row came back often
    /// enough this run that something must keep taking it out. Actions queue.
    private func noteRepair(_ id: String) -> Bool {
        let count = repairedThisRun[id, default: 0]
        guard count < 3 else { return false }
        repairedThisRun[id] = count + 1
        return true
    }

    /// Says once that lost hooks were put back.
    private func announceRepair(_ names: [String]) {
        guard !names.isEmpty else { return }
        DiagnosticsLog.info("hooks", "told the person the hooks came back in \(names.joined(separator: ", "))")
        Notifier.post(title: HooksPageStrings.repairedTitle, body: HooksPageStrings.repairedBody(names))
    }

    /// Watches the folders the hook files are in; a change to one of the
    /// files reads the rows again, and puts lost hooks back.
    private func watchHookFiles(_ targets: [HookTarget]) {
        guard running else { return }
        let files = Set(targets.compactMap { $0.file?.resolvingSymlinksInPath().path })
        guard files != watchedHookFiles else { return }
        watchedHookFiles = files
        let folders = Set(files.map { URL(fileURLWithPath: $0).deletingLastPathComponent().path }
            .filter { ClaudeAccounts.isFolder($0) })
        let watcher = hookWatcher ?? AgentLogWatcher(queue: .main) { [weak self] paths, rescan in
            guard let self else { return }
            let watched = self.watchedHookFiles
            guard rescan || paths.contains(where: { watched.contains(URL(fileURLWithPath: $0).resolvingSymlinksInPath().path) })
            else { return }
            self.hookFilesChanged()
        }
        hookWatcher = watcher
        watcher.start(folders.sorted())
    }

    /// A hook file changed on disk: read again once things settle.
    private func hookFilesChanged() {
        guard !hookCheckDue else { return }
        hookCheckDue = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self else { return }
            self.hookCheckDue = false
            self.checkHooks()
            self.checkAgentHooks()
            self.syncAgentHooks()
            self.checkHookTargets(repair: true)
        }
    }

    /// Reinstall or Remove on one row, a copy of the file kept first.
    func changeHookTarget(_ target: HookTarget, install: Bool) {
        let env = hookEnvironment
        hookTargetsBusy.insert(target.id)
        actions.async {
            let failure: String?
            do {
                if install { try HookMaintenance.reinstall(target, env: env) } else { try HookMaintenance.takeOut(target, env: env) }
                failure = nil
                DiagnosticsLog.info("hooks", "\(install ? "reinstalled" : "removed") the hooks in \(target.file?.path ?? target.id)")
            } catch {
                failure = Self.failureText(error)
                DiagnosticsLog.error("hooks", "could not \(install ? "reinstall" : "remove") the hooks in \(target.file?.path ?? target.id): \(failure ?? "")")
            }
            DispatchQueue.main.async {
                self.hookTargetsBusy.remove(target.id)
                self.hookTargetFailures[target.id] = failure
                self.refreshAfterHookChange()
            }
        }
    }

    /// Takes the island out of every file it wrote into (#236); calls back
    /// with how many rows failed.
    func removeAllHooks(completion: @escaping (Int) -> Void = { _ in }) {
        let env = hookEnvironment
        let targets = hookTargets ?? []
        hookTargetsBusy.formUnion(targets.map(\.id))
        actions.async {
            let current = HookTargets.list(Self.hookContext(env))
            let failures = HookMaintenance.removeEverything(current, env: env)
            DiagnosticsLog.info("hooks", "removed every hook, \(failures.count) failed")
            DispatchQueue.main.async {
                self.hookTargetsBusy = []
                self.hookTargetFailures = failures.mapValues { Self.failureText($0) }
                self.refreshAfterHookChange()
                completion(failures.count)
            }
        }
    }

    /// Adds a config folder for an agent and puts its hooks in; why it
    /// could not be added, if it could not.
    func addHookFolder(_ provider: ClaudeSessionProvider, picked: URL) -> String? {
        let own = AgentHookInstaller.agentFolder(provider, home: home)
        switch AgentFolders.add(provider, picked: picked, defaultFolder: own) {
        case .failure(let failure): return HooksPageStrings.folderNotAdded(failure)
        case .success(let setting):
            let env = hookEnvironment
            actions.async {
                let target = HookTargets.list(Self.hookContext(env)).first { $0.id == setting.targetID }
                var failure: String?
                if let target {
                    do { try HookMaintenance.reinstall(target, env: env) } catch { failure = Self.failureText(error) }
                }
                DispatchQueue.main.async {
                    self.hookTargetFailures[setting.targetID] = failure
                    self.refreshAfterHookChange()
                }
            }
            return nil
        }
    }

    /// Adds a tool with Claude Code style hooks and puts its hooks in.
    func addCustomHookTool(name: String, file: URL) -> String? {
        switch CustomHookTools.add(name: name, file: file, accounts: accounts) {
        case .failure(let failure): return HooksPageStrings.toolNotAdded(failure)
        case .success(let tool):
            let env = hookEnvironment
            actions.async {
                let target = HookTargets.list(Self.hookContext(env)).first { $0.id == tool.targetID }
                var failure: String?
                if let target {
                    do { try HookMaintenance.reinstall(target, env: env) } catch { failure = Self.failureText(error) }
                }
                DispatchQueue.main.async {
                    self.hookTargetFailures[tool.targetID] = failure
                    self.refreshAfterHookChange()
                }
            }
            return nil
        }
    }

    /// Takes a folder or tool added by hand off the page, its hooks out first.
    func forgetHookTarget(_ target: HookTarget) {
        let env = hookEnvironment
        hookTargetsBusy.insert(target.id)
        actions.async {
            var failure: String?
            do { try HookMaintenance.forget(target, env: env) } catch { failure = Self.failureText(error) }
            if failure == nil {
                HookMaintenance.setOptOut(target.id, false, defaults: env.defaults)
                switch target.place {
                case .agent(let provider, let folder?):
                    AgentFolders.remove(AgentFolderSetting(agent: provider.rawValue, path: folder.path))
                case .custom(let tool):
                    CustomHookTools.remove(tool)
                default: break
                }
            }
            DispatchQueue.main.async {
                self.hookTargetsBusy.remove(target.id)
                self.hookTargetFailures[target.id] = failure
                if case .claude = target.place, failure == nil,
                   let account = self.accounts.first(where: { $0.settingsURL == target.file }) {
                    self.removeAccount(account)
                }
                self.refreshAfterHookChange()
            }
        }
    }

    private func refreshAfterHookChange() {
        checkHooks()
        checkAgentHooks()
        checkHookTargets()
    }

    static func failureText(_ error: Error) -> String {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        guard let failure = error as? ClaudeHookInstaller.Failure else { return error.localizedDescription }
        switch failure {
        case .unreadable: return text.settingsUnreadable
        case .unwritable(let reason): return reason
        case .busy: return text.settingsBusy
        case .noBackup: return text.settingsNoBackup
        }
    }

    /// One agent's hooks put in or taken out; why it failed, if it did.
    private static func changeAgentHooks(_ provider: ClaudeSessionProvider, install: Bool, home: URL, socket: String,
                                         bundle: String) -> String? {
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        let failure: String?
        do {
            if install {
                try AgentHookInstaller.install(provider, home: home, socket: socket, bundle: bundle)
                AgentHookInstaller.noteInstalled(provider, home: home, bundle: bundle)
            } else {
                try AgentHookInstaller.remove(provider, home: home, bundle: bundle)
                HookRecords.forget(HookRecords.agentID(provider))
            }
            failure = nil
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
        let change = install ? "installed" : "removed"
        if let failure {
            DiagnosticsLog.error("hooks", "\(provider.rawValue) hooks could not be \(change): \(failure)")
        } else {
            DiagnosticsLog.info("hooks", "\(provider.rawValue) hooks \(change)")
        }
        return failure
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
            let envelope = AgentHookMessage.isEnvelope(payload)
            if envelope ? !AgentHookMessage.waits(payload) : !ClaudeHookMessage.isPermissionRequest(payload) {
                connection.finish()
            }
            self?.queue.async { self?.receive(payload, from: connection, at: delivered) }
        }
        server = created.start() ? created : nil
        if server == nil {
            DiagnosticsLog.error("sessions", "the hook socket could not be opened at \(path)")
        } else {
            DiagnosticsLog.info("sessions", "listening for hooks at \(path)")
        }
    }

    /// One hook event, on the queue.
    private func receive(_ payload: Data, from connection: ClaudeHookServer.Connection, at now: Date) {
        // A hook that arrived as the section stopped goes back to the terminal.
        guard timer != nil else { return connection.finish() }
        if AgentHookMessage.isEnvelope(payload) { return receiveAgent(payload, from: connection, at: now) }
        // Devin CLI runs the hooks in Claude Code's settings too; its sessions
        // are heard through hooks of their own, so these are let go unanswered.
        if !isRemotePeer(connection.peer), AgentDevinClaudeHooks.isDevin(payload, peer: connection.peer) {
            return connection.finish()
        }
        // Grok CLI runs them too, with its own input; its sessions have hooks of their own.
        if AgentGrokClaudeHooks.isGrok(payload) { return connection.finish() }
        let request = UUID().uuidString
        guard var message = ClaudeHookMessage.decode(payload, requestID: request, now: now, policy: engine.policy) else {
            return connection.finish()
        }
        // A remote session's transcript is on its host, read through its connection.
        let remote = isRemotePeer(connection.peer)
        // With the Claude app's switch off, its Code tab's sessions are left
        // to the app, which asks there as it would without the island (#246).
        if !remote, !desktopShown, ClaudeDesktop.isCodeTab(entrypoint: engine.record(message.fact.sessionID)?.entrypoint) {
            return connection.finish()
        }
        if !remote, let transcript = message.transcript, !transcript.isEmpty { transcripts[message.fact.sessionID] = transcript }
        let session = message.fact.sessionID
        if let permission = message.letsThrough,
           let reason = SessionAutoMode.shared.settings.passes(
               .claude, autoMode: SessionAutoMode.isAuto(claudeMode: message.fact.mode ?? engine.mode(of: session))) {
            // A session that runs without asking is never held (#234): its
            // tool goes ahead now, and its page lists it.
            connection.finish(ClaudeHookMessage.reply(.allow, suggestions: nil, approved: message.approvalInput))
            SessionAutoMode.log.add(ClaudeLetThrough(permission, at: now, reason: reason), to: session)
            message = message.settled()
        } else if message.isPermissionRequest {
            pending[request] = (connection, message.fact.sessionID, message.suggestions, message.questionInput,
                                message.approvalInput, message.planModes, now)
        } else {
            connection.finish()
        }
        // A turn's lines are written before its Stop hook runs; reading them
        // first tells whether it left work in the background, before a notice
        // calls it done.
        if !remote, case .stopped = message.fact.event, let path = transcripts[message.fact.sessionID] {
            read(path, session: message.fact.sessionID, now: now)
        }
        let before = phase(of: message.fact.sessionID)
        engine.ingest(hook: message.fact, now: now)
        publish(now: now)
        DiagnosticsJournal.shared.record(DiagnosticsJournal.make(
            agent: .claude, event: DiagnosticsJournal.label(message.fact.event), session: message.fact.sessionID,
            from: before, to: phase(of: message.fact.sessionID), at: now), at: now)
    }

    /// The state a session was last shown in; nil when it isn't listed. Queue.
    private func phase(of id: String) -> ClaudeSessionPhase? {
        published.first { $0.id == id }?.phase
    }

    /// One event of a hook agent (Gemini CLI, Copilot CLI, Cursor, OpenCode, Pi,
    /// Mistral Vibe, Devin CLI, Google Antigravity), on the queue.
    private func receiveAgent(_ payload: Data, from connection: ClaudeHookServer.Connection, at now: Date) {
        guard let decoded = AgentHookMessage.decode(payload, now: now, policy: engine.policy),
              shownAgents.contains(decoded.provider) else { return connection.finish() }
        // Qwen Code's helpers after a turn, and its idle notes inside one, aren't the session's (#239).
        if decoded.provider == .qwen, let session = decoded.events.first?.sessionID {
            if decoded.events.contains(where: { if case .ended = $0.kind { true } else { false } }) {
                qwenTurns.forget(session)
            } else if let fact = decoded.qwen, !qwenTurns.keep(session, turn: decoded.turn, fact: fact, at: now) {
                return connection.finish()
            }
        }
        // The agent is the process above the hook's shells; OpenCode's plugin
        // writes from inside it.
        let pid = decoded.fromProxy ? 0 : AgentHookProcess.agent(fromHook: connection.peer, hint: decoded.pid)
        if let attached = decoded.attached {
            proxied[AgentHookEvent.id(decoded.provider, attached)] = connection.peer
            return connection.finish()
        }
        if decoded.listens {
            listeners.removeValue(forKey: pid)?.finish()
            listeners[pid] = connection
            return
        }
        // A request the person chose Always proceed for goes ahead now (#234).
        var proceeded: String?
        if let asked = decoded.permission, let reason = SessionAutoMode.shared.settings.passes(decoded.provider, autoMode: false),
           let permission = Self.allowable(decoded.events, request: asked.request),
           let reply = decoded.provider == .vibe ? AgentHookMessage.vibeReply(.allow)
            : decoded.provider == .zcode ? AgentHookMessage.zcodeReply(.allow)
            : decoded.provider == .qwen ? AgentQwen.reply(.allow) : AgentHookMessage.opencodeReply(.allow) {
            let id = AgentHookEvent.id(decoded.provider, asked.session)
            agentPending.removeValue(forKey: asked.request)?.connection.finish()
            connection.finish(reply)
            SessionAutoMode.log.add(ClaudeLetThrough(permission, at: now, reason: reason), to: id)
            proceeded = asked.request
        } else if let asked = decoded.permission {
            let id = AgentHookEvent.id(decoded.provider, asked.session)
            inspection.invalidate(id)
            agentPending.removeValue(forKey: asked.request)?.connection.finish()
            agentPending[asked.request] = (connection, id, now)
            agentInputs[asked.request] = decoded.input
        } else {
            connection.finish()
        }
        // For the diagnostics journal: each session's state before these events.
        var heard: [AgentHookEvent] = []
        var before: [String: ClaudeSessionPhase?] = [:]
        for event in decoded.events where before[event.id] == nil { before[event.id] = .some(phase(of: event.id)) }
        for event in decoded.events {
            var agent = pid > 0 ? pid : event.pid
            var cwd = event.cwd
            // Hermes's gateway runs its hooks: the session's client and folder
            // come from Hermes's own notes, and a chat it answers isn't listed.
            if event.provider == .hermes {
                let hermes = AgentHermes.home(home: home)
                var fresh = false
                switch event.kind {
                case .prompt, .stopped, .started: fresh = true
                default: break
                }
                if case .ended = event.kind { hermesPlaces.forget(event.sessionID) }
                guard let place = hermesPlaces.place(
                    event.sessionID, hermes: hermes, now: now, fresh: fresh,
                    record: { AgentHermesStore.record($0, hermes: hermes) },
                    notes: { AgentHermesClients.notes(hermes: hermes) }, running: { AgentHermesClients.running() },
                    app: { AgentHermesClients.app() }) else { continue }
                agent = place.pid
                cwd = place.record?.cwd
                // Where the session is shown, as Hermes recorded it when it was made.
                if let client = place.record?.client, AgentHookMessage.hermesClients.contains(client),
                   hookAgents.entry(event.id)?.client != client {
                    hookAgents.ingest(AgentHookEvent(provider: .hermes, sessionID: event.sessionID, pid: agent, cwd: cwd,
                                                     date: event.date, kind: .client(client)), now: now)
                }
                // A title is made from the prompt: only read with previews on.
                if engine.policy.previews, let title = place.record?.title, hookAgents.entry(event.id)?.title != title {
                    hookAgents.ingest(AgentHookEvent(provider: .hermes, sessionID: event.sessionID, pid: agent, cwd: cwd,
                                                     date: event.date, kind: .title(title)), now: now)
                }
            }
            // Devin CLI names no folder: the agent's own is the session's.
            if cwd == nil, agent > 0, (hookAgents.entry(event.id)?.cwd ?? "").isEmpty {
                cwd = ClaudeSessionHosts.cwd(of: agent)
            }
            var kind = event.kind
            switch kind {
            case .permission(let p) where p.requestID == proceeded, .heldPermission(let p) where p.requestID == proceeded:
                // Let through: nothing waits on the person.
                continue
            case .question where event.provider == .qwen && hookAgents.entry(event.id)?.permission != nil:
                // Qwen Code's dialog for a question or plan the island shows a card for.
                continue
            default: break
            }
            // A session run through the proxy is answered there: Vibe's hooks only hear of its requests.
            if event.provider == .vibe, !decoded.fromProxy, proxied[event.id] != nil {
                switch kind {
                case .permission, .question: continue
                default: break
                }
            }
            // Vibe's hooks carry no reply: with previews on, its session's log tells the last one.
            if event.provider == .vibe, engine.policy.previews, case .stopped(nil, let failed) = kind,
               let folder = decoded.transcript {
                kind = .stopped(lastText: AgentVibeLog.lastReply(in: folder), failed: failed)
            }
            // Hermes's hooks carry no reply: with previews on, its state.db tells the last one.
            if event.provider == .hermes, engine.policy.previews, case .stopped(nil, let failed) = kind {
                kind = .stopped(lastText: AgentHermesStore.lastReply(event.sessionID, hermes: AgentHermes.home(home: home)),
                                failed: failed)
            }
            // Antigravity's hooks carry no reply: with previews on, its transcript tells the last one.
            if event.provider == .antigravity, engine.policy.previews, case .stopped(nil, let failed) = kind,
               let path = decoded.transcript {
                kind = .stopped(lastText: AgentAntigravityLog.lastReply(in: path), failed: failed)
            }
            // Grok's hooks name the model only for xAI's own: its session's summary names any.
            if event.provider == .grok, hookAgents.entry(event.id)?.model == nil, let path = decoded.transcript {
                switch kind {
                case .prompt, .stopped:
                    if let model = AgentGrokLog.model(transcript: path) {
                        hookAgents.ingest(AgentHookEvent(provider: .grok, sessionID: event.sessionID, pid: agent, cwd: cwd,
                                                         date: event.date, kind: .model(model)), now: now)
                    }
                default: break
                }
            }
            // A Qwen Code session heard first after its start: its log names the model.
            if event.provider == .qwen, hookAgents.entry(event.id)?.model == nil, let path = decoded.transcript {
                switch kind {
                case .prompt, .stopped:
                    if let model = AgentQwen.model(transcript: path) {
                        hookAgents.ingest(AgentHookEvent(provider: .qwen, sessionID: event.sessionID, pid: agent, cwd: cwd,
                                                         date: event.date, kind: .model(model)), now: now)
                    }
                default: break
                }
            }
            let placed = AgentHookEvent(provider: event.provider, sessionID: event.sessionID, pid: agent,
                                        cwd: cwd, date: event.date, kind: kind)
            if case .permissionSettled(let request) = event.kind {
                // Answered in OpenCode itself: its plugin stops waiting too.
                for (key, waiting) in agentPending where waiting.session == placed.id && (request == nil || key == request) {
                    agentPending[key] = nil
                    agentInputs[key] = nil
                    waiting.connection.finish()
                }
            }
            hookAgents.ingest(placed, now: now)
            heard.append(placed)
        }
        publish(now: now)
        for event in heard {
            DiagnosticsJournal.shared.record(DiagnosticsJournal.make(
                agent: event.provider, event: DiagnosticsJournal.label(event.kind), session: event.id,
                from: before[event.id] ?? nil, to: phase(of: event.id), at: now), at: now)
        }
    }

    /// The request among an agent's events that a plain allow answers: a
    /// tool's, never a question.
    static func allowable(_ events: [AgentHookEvent], request: String) -> ClaudePermission? {
        for event in events {
            switch event.kind {
            case .permission(let p) where p.requestID == request && !p.asksQuestions,
                 .heldPermission(let p) where p.requestID == request && !p.asksQuestions:
                return p
            default: continue
            }
        }
        return nil
    }

    /// Lets Qwen Code's held requests go unanswered, so it asks in its own
    /// dialog: the person went to answer there (#239).
    private func releaseHeld(_ id: String) {
        queue.async { [self] in
            for (request, waiting) in agentPending where waiting.session == id {
                agentPending[request] = nil
                agentInputs[request] = nil
                waiting.connection.finish()
                hookAgents.moveToDialog(id, request: request)
            }
            publish(now: Date())
        }
    }

    /// Lets go of OpenCode and ZCode requests whose session left, whose
    /// hook went away (ZCode stops it once answered there) or that waited
    /// too long, and of listeners whose OpenCode quit.
    private func releaseAgents(now: Date) {
        hookAgents.prune(now: now)
        hermesPlaces.keep(Set(hookAgents.entries.values.filter { $0.provider == .hermes }.map(\.sessionID)))
        for (request, waiting) in agentPending {
            let entry = hookAgents.entry(waiting.session)
            let current = entry?.permission?.requestID == request
            // Qwen Code stops waiting for its hook after a minute and asks in
            // its own dialog, which the island then answers with keys (#239).
            if current, entry?.provider == .qwen, !waiting.connection.peerIsGone,
               now.timeIntervalSince(waiting.since) > AgentQwen.holdLimit {
                agentPending[request] = nil
                waiting.connection.finish()
                hookAgents.moveToDialog(waiting.session, request: request)
                continue
            }
            guard !current || waiting.connection.isClosed || waiting.connection.peerIsGone
                || now.timeIntervalSince(waiting.since) > Self.permissionDeadline
            else { continue }
            agentPending[request] = nil
            agentInputs[request] = nil
            waiting.connection.finish()
            if current { hookAgents.resolve(waiting.session, request: request) }
        }
        for (id, proxy) in proxied where !AgentHookSessions.isAlive(proxy) { proxied[id] = nil }
        for (pid, listener) in listeners where listener.isClosed || !AgentHookSessions.isAlive(pid) {
            listeners[pid] = nil
            listener.finish()
        }
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

    /// Reads the accounts again and whether each holds the hooks.
    func checkHooks() {
        let home = home
        let command = ClaudeHookInstaller.command(socket: Self.socketPath(home: home), bundle: Self.bundleID)
        let bundle = Self.bundleID
        actions.async {
            let accounts = ClaudeAccounts.all(home: home)
            var states: [String: ClaudeAccountHooks] = [:]
            for account in accounts {
                states[account.id] = Self.hookState(account.settingsURL, command: command, bundle: bundle)
            }
            DispatchQueue.main.async {
                if self.accounts != accounts {
                    self.accounts = accounts
                    self.updateAccountLabels()
                }
                if self.accountHooks != states {
                    let summary = accounts.map { "\($0.name)=\(states[$0.id].map { "\($0)" } ?? "?")" }.joined(separator: " ")
                    DiagnosticsLog.info("hooks", "claude hooks: \(summary)")
                    self.accountHooks = states
                }
                let shown = accounts.filter { !$0.hidden }
                self.hooksInstalled = shown.isEmpty ? false : shown.allSatisfy { states[$0.id] == .installed }
            }
        }
    }

    static func hookState(_ url: URL, command: String, bundle: String) -> ClaudeAccountHooks {
        guard let settings = try? ClaudeHookInstaller.read(url) else { return .unreadable }
        if ClaudeHookInstaller.isInstalled(in: settings, command: command) { return .installed }
        return ClaudeHookInstaller.hasAny(in: settings, bundle: bundle) ? .outdated : .missing
    }

    /// Puts the hooks into every account shown, or takes them out of every
    /// account, hidden ones too. With `only`, into or out of that one.
    func installHooks(_ install: Bool, only: ClaudeAccount? = nil) {
        let home = home
        let bundle = Self.bundleID
        let command = ClaudeHookInstaller.command(socket: Self.socketPath(home: home), bundle: bundle)
        let text = FeatureStrings.notchClaudeSessions(L10n.shared.language)
        actions.async {
            let accounts = only.map { [$0] } ?? ClaudeAccounts.all(home: home).filter { !install || !$0.hidden }
            var failures: [String: String] = [:]
            // Two folders whose settings are one linked file change it once.
            var done = Set<String>()
            for account in accounts {
                let file = account.settingsURL.resolvingSymlinksInPath().path
                guard done.insert(file).inserted else { continue }
                // Taking out never makes a settings file that was not there.
                if !install, (try? ClaudeHookInstaller.contents(account.settingsURL)) == nil { continue }
                if let failure = Self.changeHooks(account.settingsURL, install: install, command: command,
                                                  bundle: bundle, text: text) {
                    failures[account.id] = failure
                }
            }
            DispatchQueue.main.async {
                let names = Dictionary(uniqueKeysWithValues: accounts.map { ($0.id, $0.name) })
                self.hookFailure = failures.isEmpty ? nil
                    : failures.count == 1 && accounts.count == 1 ? failures.values.first
                    : failures.sorted { $0.key < $1.key }.map { "\(names[$0.key] ?? $0.key): \($0.value)" }.joined(separator: "; ")
                self.checkHooks()
            }
        }
    }

    private static func changeHooks(_ url: URL, install: Bool, command: String, bundle: String,
                                    text: NotchClaudeSessionStrings) -> String? {
        let failure: String?
        do {
            try ClaudeHookInstaller.update(url) { settings in
                install ? ClaudeHookInstaller.installing(into: settings, command: command, bundle: bundle)
                    : ClaudeHookInstaller.removing(from: settings, bundle: bundle)
            }
            if install {
                HookRecords.noteInstalled(HookRecords.claudeID(url), file: url, marker: ClaudeHookInstaller.marker(bundle: bundle))
            } else {
                HookRecords.forget(HookRecords.claudeID(url))
            }
            failure = nil
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
        let change = install ? "installed" : "removed"
        if let failure {
            DiagnosticsLog.error("hooks", "claude hooks could not be \(change) in \(url.path): \(failure)")
        } else {
            DiagnosticsLog.info("hooks", "claude hooks \(change) in \(url.path)")
        }
        return failure
    }

    // MARK: Accounts (#216)

    private func updateAccountLabels() {
        var labels = ClaudeAccounts.labels(sessions, accounts: accounts, home: home)
        // A second Codex home and a tool added by hand say which they are (#236).
        labels.merge(HookTargets.labels(sessions)) { current, _ in current }
        if labels != accountLabels { accountLabels = labels }
    }

    func renameAccount(_ account: ClaudeAccount, to name: String) {
        ClaudeAccounts.rename(account, to: name)
        checkHooks()
    }

    func setAccountHidden(_ account: ClaudeAccount, _ hidden: Bool) {
        ClaudeAccounts.setHidden(account, hidden)
        checkHooks()
    }

    @discardableResult
    func addAccount(_ folder: URL) -> Bool {
        defer { checkHooks() }
        return ClaudeAccounts.add(folder, home: home) != nil
    }

    func removeAccount(_ account: ClaudeAccount) {
        ClaudeAccounts.remove(account)
        checkHooks()
    }

    /// Brings the session's terminal forward. Calls back on the main thread
    /// with whether the window was found, or that the session's tmux or
    /// zellij has no terminal attached (#235).
    func openHost(_ session: ClaudeSession, completion: @escaping (ClaudeHostOpen) -> Void = { _ in }) {
        acknowledge(session.id)
        if session.provider == .qwen { releaseHeld(session.id) }
        if session.id == ClaudeSessionSample.id { return completion(.missing) }
        if session.provider == .codex, session.host.remoteAlias != nil {
            // A host's Codex runs in a terminal there, which nothing here shows.
            return DispatchQueue.main.async { completion(.missing) }
        }
        if let alias = session.host.remoteAlias {
            // Its pane comes forward in the host's tmux; no window here shows it.
            queue.async { [self] in
                guard let link = remoteLinks[alias] else { return DispatchQueue.main.async { completion(.missing) } }
                link.perform { link in
                    let found = link.focus(pid: session.pid, session: session.id)
                    DispatchQueue.main.async { completion(found ? .opened : .missing) }
                }
            }
            return
        }
        if session.origin != nil {
            // The Claude app shows it; no terminal does (#246).
            return actions.async {
                let opened = ClaudeDesktop.open(session)
                DispatchQueue.main.async { completion(opened ? .opened : .missing) }
            }
        }
        let homes = ClaudeSessionRegistryReader.homes(home)
        let terminal = ClaudeTerminalLauncher.preferred(from: sessions)
        actions.async {
            let result = session.background != nil
                ? (ClaudeHostFocus.attach(session, terminal: terminal) ? .opened : .missing)
                : ClaudeHostFocus.open(session, homes: homes)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Attaches a detached tmux or zellij session in a new window of the
    /// person's terminal, with the line `openHost` gave.
    func attachDetached(_ line: String, completion: @escaping (Bool) -> Void = { _ in }) {
        let terminal = ClaudeTerminalLauncher.preferred(from: sessions)
        actions.async {
            let opened = ClaudeTerminalLauncher.open(line, terminal: terminal)
            DispatchQueue.main.async { completion(opened) }
        }
    }

    // MARK: Remote hosts

    /// Starts a connection for each host turned on and ends the others. On the queue.
    private func syncRemote(_ wanted: [RemoteHost]) {
        let aliases = Set(wanted.map(\.alias))
        for (alias, link) in remoteLinks where !aliases.contains(alias) {
            link.stop()
            if let client = remoteCodex.removeValue(forKey: alias) {
                // Waits a moment for its thread, so not on the queue.
                DispatchQueue.global(qos: .utility).async { client.stop() }
                publish(now: Date())
            }
            remoteLinks[alias] = nil
            remoteSnapshots[alias] = nil
            remoteTails = remoteTails.filter { !$0.key.hasPrefix(alias + "\n") }
        }
        // Connections reach the island through its socket, so they wait for it.
        guard let socket = server?.path else { return publishRemoteStates() }
        for host in wanted where remoteLinks[host.alias] == nil {
            let link = RemoteHostLink(alias: host.alias, bundle: Self.bundleID, localSocket: socket)
            let alias = host.alias
            link.onState = { [weak self, weak link] state in
                guard let self, let link else { return }
                if !state.isConnected {
                    // A host that went away takes its sessions with it, and
                    // nothing else waits on it.
                    queue.async { [self] in
                        guard self.remoteLinks[alias] === link, self.remoteSnapshots.removeValue(forKey: alias) != nil else { return }
                        self.refresh()
                    }
                }
                DispatchQueue.main.async { [self] in
                    guard self.running else { return }
                    if self.remoteStates[alias].map(Self.remoteKind) != Self.remoteKind(state) {
                        DiagnosticsLog.info("remote", "host \(alias) \(Self.remoteKind(state))")
                    }
                    self.remoteStates[alias] = state
                }
            }
            link.onSnapshot = { [weak self, weak link] snapshot in
                self?.queue.async { [weak self] in
                    guard let self, let link, remoteLinks[alias] === link else { return }
                    ingestRemote(alias, snapshot)
                }
            }
            remoteLinks[alias] = link
            // The host's Codex, joined like this Mac's once its socket is
            // forwarded and its daemon runs (issue #200).
            let client = CodexSessionClient(socket: URL(fileURLWithPath: link.codexSocket), remoteAlias: alias,
                                            ready: { [weak link] in link?.codexReady ?? false })
            remoteCodex[alias] = client
            client.start { [weak self] in self?.queue.async { self?.publish(now: Date()) } }
            link.start()
        }
        publishRemoteStates()
    }

    /// Settings forgets hosts that are no longer listed.
    /// A host's state in a word or two, for the log; why it is down, too.
    static func remoteKind(_ state: RemoteHostState) -> String {
        switch state {
        case .connecting: return "connecting"
        case .connected: return "connected"
        case .disconnected(let reason, _): return "disconnected: \(reason)"
        }
    }

    private func publishRemoteStates() {
        let aliases = Set(remoteLinks.keys)
        DispatchQueue.main.async { [self] in
            remoteStates = remoteStates.filter { aliases.contains($0.key) }
            remoteFailures = remoteFailures.filter { aliases.contains($0.key) }
        }
    }

    /// What one look at a host found: its sessions for the next reading of
    /// the registry, and the new lines of their transcripts now. On the queue.
    private func ingestRemote(_ alias: String, _ snapshot: RemoteSnapshot) {
        guard timer != nil else { return }
        let now = Date()
        let isNew = remoteSnapshots[alias]?.records.map(\.sessionID) != snapshot.records.map(\.sessionID)
        remoteSnapshots[alias] = snapshot
        let owners = Dictionary(snapshot.transcripts.map { ($0.value, $0.key) }, uniquingKeysWith: { first, _ in first })
        let prefix = alias + "\n"
        for chunk in snapshot.chunks {
            guard let session = owners[chunk.path] else { continue }
            let key = prefix + chunk.path
            var entry = remoteTails[key] ?? (RemoteTranscriptTail(), ClaudeTranscriptReader(policy: engine.policy, path: chunk.path))
            let taken = entry.tail.take(chunk)
            if taken.restarted, chunk.offset == 0 { entry.reader = ClaudeTranscriptReader(policy: engine.policy, path: chunk.path) }
            var facts: [ClaudeTranscriptFact] = []
            for line in taken.lines {
                facts.append(contentsOf: entry.reader.parse(RemoteClock.line(line, by: snapshot.clockOffset), now: now))
            }
            remoteTails[key] = entry
            if !facts.isEmpty { engine.ingest(sessionID: session, facts: facts, now: now) }
        }
        remoteTails = remoteTails.filter { !$0.key.hasPrefix(prefix) || owners[String($0.key.dropFirst(prefix.count))] != nil }
        // A session that came or went shows at once, not at the next tick.
        if isNew { refresh() }
    }

    /// Whether a hook came through a remote host's connection. On the queue.
    private func isRemotePeer(_ pid: pid_t) -> Bool {
        pid > 0 && remoteLinks.values.contains { $0.peer == pid }
    }

    /// Adds the island's hooks to the host's Claude Code settings, or takes
    /// them out again, putting back the file as it was before.
    func changeRemoteHooks(_ alias: String, install: Bool) {
        guard !remoteBusy.contains(alias) else { return }
        remoteBusy.insert(alias)
        remoteFailures[alias] = nil
        let bundle = Self.bundleID
        queue.async { [self] in
            guard let link = remoteLinks[alias] else {
                return DispatchQueue.main.async { [self] in remoteBusy.remove(alias) }
            }
            link.perform { [self] link in
                var info = link.info
                if info == nil, let probe = RemoteOneShot.run(alias, mode: "probe \(RemoteSSH.socketName(bundle: bundle)) \(bundle)").lines {
                    info = RemoteAnswer.probe(probe)
                }
                let failure: RemoteHostSetup.Failure?
                if install {
                    failure = info.map { RemoteHostSetup.install(alias, info: $0, bundle: bundle) } ?? .unreachable("")
                } else {
                    failure = RemoteHostSetup.remove(alias, bundle: bundle)
                }
                if failure == nil {
                    link.markHooks(installed: install)
                    // The hooks write to the socket the connection forwards; a
                    // fresh one is sure to have it.
                    if install { link.reconnectNow() }
                }
                let state = link.state
                DispatchQueue.main.async { [self] in
                    remoteBusy.remove(alias)
                    remoteFailures[alias] = failure.map(RemoteHostStrings.describe)
                    if let state, running { remoteStates[alias] = state }
                }
            }
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
        releaseAgents(now: now)
        let agentPids = Set(hookAgents.entries.values.map(\.pid))
        hosts = hosts.filter { alive.contains($0.key) || codexPids.contains($0.key) || agentPids.contains($0.key) }
        for index in records.indices {
            let pid = records[index].pid
            // A background session has no terminal to find.
            if let job = records[index].job {
                records[index].host = .background(job: job)
                continue
            }
            // The Claude app's Code tab runs it: the app shows it, whatever
            // its environment says of a terminal (#246).
            if ClaudeDesktop.isCodeTab(entrypoint: records[index].entrypoint) {
                records[index].host = .app(ClaudeDesktop.bundle)
                continue
            }
            if hosts[pid] == nil { hosts[pid] = ClaudeSessionHosts.host(of: pid) }
            records[index].host = hosts[pid] ?? .unknown
        }
        // Sessions on remote hosts, as their connections last saw them. Their
        // process numbers are the host's, so none of the lookups above apply.
        records += remoteSnapshots.keys.sorted().flatMap { remoteSnapshots[$0]?.records ?? [] }
        engine.ingest(records: records, now: now)
        listBackgroundJobs(now: now)
        ticks += 1
        let sideFiles = ticks % Self.sideFilesEvery == 1
        let ids = Set(records.map(\.sessionID))
        readers = readers.filter { ids.contains(sessionID(ofReader: $0.key)) }
        for record in records where record.host.remoteAlias == nil {
            guard let transcript = transcript(for: record) else { continue }
            read(transcript, session: record.sessionID, now: now)
            if sideFiles {
                readSubagents(of: transcript, session: record.sessionID, now: now)
                readTasks(record.sessionID, now: now)
            }
        }
        transcripts = transcripts.filter { ids.contains($0.key) }
        readCowork(now: now)
        releaseSettled(now: now)
        publish(now: now)
        let active = published.contains { $0.phase.isActive || $0.phase.isWaiting }
        schedule(active ? Self.activeTick : Self.quietTick)
    }

    /// Reads Cowork's session files while its switch is on: every tick while
    /// one of its sessions works or waits, otherwise every few (#246).
    private func readCowork(now: Date) {
        guard coworkRead, desktopShown else { return }
        let busy = coworkRows.sessions.contains { $0.phase.isActive || $0.phase.isWaiting }
        guard busy || ticks % Self.sideFilesEvery == 1 else { return }
        let found = cowork.scan(ClaudeDesktop.coworkFolders(home: home), now: now, previews: engine.policy.previews,
                                running: ClaudeDesktop.isRunning())
        coworkRows = (found.sessions, coworkRows.events + found.events)
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
        // The Code tab's sessions follow the Claude app's switch (#246).
        let claude = snapshot.sessions.filter { desktopShown || $0.origin != .desktop }
            .map { agentColors.decorate($0, now: now) }
        let fromCowork = coworkRows
        coworkRows.events = []
        let fromCodex = codex?.snapshot(now: now, previews: engine.policy.previews) ?? (sessions: [], events: [])
        var codexSessions = fromCodex.sessions.map { session in
            placed(session, others: fromCodex.sessions.filter { $0.id != session.id }.map(\.cwd))
        }
        var codexEvents = fromCodex.events
        // Hosts' threads keep their host: no terminal here shows them.
        for alias in remoteCodex.keys.sorted() {
            guard let remote = remoteCodex[alias]?.snapshot(now: now, previews: engine.policy.previews) else { continue }
            codexSessions += remote.sessions
            codexEvents += remote.events
        }
        let fromAgents = hookAgents.snapshot(now: now, previews: engine.policy.previews, host: { [self] pid in
            guard pid > 0 else { return .unknown }
            if hosts[pid] == nil {
                // ZCode's app runs its agent itself, so its window shows the session.
                hosts[pid] = ClaudeSessionHosts.agentApp(of: pid).map { .app($0) } ?? ClaudeSessionHosts.host(of: pid)
            }
            return hosts[pid] ?? .unknown
        }, branch: { [self] folder in branches.branch(of: folder, now: now) })
        let agentSessions = fromAgents.sessions.filter { shownAgents.contains($0.provider) }
        let sorted = SessionAutoMode.log.decorate(NotchClaudeSessionsSupport.sorted(
            withBackground(claude, now: now) + codexSessions + agentSessions + fromCowork.sessions + (sample.map { [$0] } ?? [])
        ).map { withLiveBranch($0, now: now) })
        let assessed = risks.assess(sorted, now: now)
        inspection.retain(assessed)
        SessionAutoMode.log.prune(listed: Set(assessed.map(\.id)), now: now)
        let shown = autoAllowing(assessed, now: now)
        let allowed = autoAllow.sessions
        let hidden = Set(assessed.filter { NotchClaudeSessionsSupport.hidesAutoApproval($0, autoAllowed: allowed, now: now) }
            .map(\.id))
        let hiddenDesktop = desktopShown ? [] : Set(snapshot.sessions.filter { $0.origin == .desktop }.map(\.id))
        let events = (snapshot.events.filter { !hiddenDesktop.contains(Self.eventID($0)) }
                      + codexEvents + fromAgents.events + fromCowork.events)
            .filter { NotchClaudeSessionsSupport.shows($0, hidden: hidden) } + sampleEvents
        sampleEvents = []
        let changed = shown != published || allowed != publishedAutoAllow
        if changed { journalCodex(shown, now: now) }
        published = shown
        publishedAutoAllow = allowed
        guard changed || !events.isEmpty else { return }
        DispatchQueue.main.async { [self] in
            guard running else { return }
            if changed {
                sessions = shown
                // A session in a folder not listed yet brings its account in.
                let known = Set(accounts.map(\.id))
                if ClaudeAccounts.inUse(shown).contains(where: { !known.contains($0) }) { checkHooks() }
                updateAccountLabels()
                counts = ClaudeSessionCounts(shown)
                if autoAllowed != allowed { autoAllowed = allowed }
                // An answered ask, or a session that left, forgets its
                // request: one coming back (Codex numbers requests afresh
                // after a restart) waits its settle delay again.
                let held = Set(shown.flatMap { session in session.asks.map { Self.shownKey(session.id, $0.requestID) } })
                shownRequests = shownRequests.filter { held.contains($0.key) }
                for key in held where shownRequests[key] == nil { shownRequests[key] = Date() }
                let listed = Set(shown.map(\.id))
                if let focusedID, !listed.contains(focusedID) {
                    self.focusedID = nil
                    dressingID = nil
                    // Its Codex thread is let go too, so it can unload.
                    queue.async { [self] in codexClients.forEach { $0.focus([]) } }
                }
            }
            events.forEach(self.events.send)
        }
    }

    static func eventID(_ event: ClaudeSessionEvent) -> String {
        switch event {
        case .needsPermission(let id, _), .needsInput(let id), .finished(let id, _): return id
        }
    }

    /// Codex is heard through its app-server, not hooks: what the journal
    /// keeps of it is each thread's change of state. Queue.
    private func journalCodex(_ shown: [ClaudeSession], now: Date) {
        for session in shown where session.codexServer {
            let before = published.first { $0.id == session.id }?.phase
            guard before != session.phase else { continue }
            DiagnosticsJournal.shared.record(DiagnosticsJournal.make(
                agent: .codex, event: (before == nil ? "listed" : "update", nil), session: session.id,
                from: before, to: session.phase, at: now), at: now)
        }
    }

    /// The branch as the session's checkout has it now, so a checkout inside
    /// it shows on its row (#233). A host's sessions keep what it said.
    private func withLiveBranch(_ session: ClaudeSession, now: Date) -> ClaudeSession {
        guard session.host.remoteAlias == nil, !session.cwd.isEmpty else { return session }
        var session = session
        session.branch = AgentGitBranch.live(reported: session.branch, head: branches.head(of: session.cwd, now: now))
        return session
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
        guard timer != nil, let codex = codexOwner(id), autoAllow.fire(id, request: request, in: latest, now: Date())
        else { return }
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
        // zellij's variables reach everything started in its panes too, a
        // tmux run in one among them; only a process zellij's server runs
        // under is in its pane.
        if case .zellij(var target) = host {
            if let server = ClaudeZellij.server(of: pid) {
                target.server = server
                // The terminal showing it, for its name; Open looks again.
                if let client = ClaudeZellij.client(of: target), let outer = Self.host(of: client.pid).bundle {
                    target.outer = outer
                }
                return .zellij(target)
            }
            for key in ["ZELLIJ", "ZELLIJ_SESSION_NAME", "ZELLIJ_PANE_ID"] { environment[key] = nil }
            host = classify(bundle: bundle, tty: device, port: port, environment: environment, owner: owner)
        }
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

extension ClaudeTerminalFocus {
    /// Whether the session's own tab or pane is in front (#210).
    static func state(of session: ClaudeSession) -> State {
        guard session.pid > 1 else { return .unknown }
        return state(session.host, pid: session.pid, hostOf: ClaudeSessionHosts.host(of:))
    }
}

/// Brings a session's terminal forward: its own tab, pane or window in a
/// terminal that can be asked for one, the window holding its folder in an
/// editor, or just the app elsewhere.
enum ClaudeHostFocus {
    static func focus(_ session: ClaudeSession, homes: [URL]) -> Bool {
        focus(session.host, pid: session.pid, cwd: session.cwd, homes: homes)
    }

    static func open(_ session: ClaudeSession, homes: [URL]) -> ClaudeHostOpen {
        reach(session.host, pid: session.pid, cwd: session.cwd, homes: homes)
    }

    /// Like `focus`, telling a tmux or zellij with no terminal attached,
    /// however deep in other multiplexers it sits, from one not found.
    static func reach(_ host: ClaudeSessionHost, pid: Int32, cwd: String, homes: [URL], depth: Int = 0) -> ClaudeHostOpen {
        // The terminal showing a multiplexer comes forward the way a session
        // in that terminal would, a multiplexer showing it in turn included.
        func outer(_ client: Int32) -> ClaudeHostOpen {
            guard depth < 3 else { return .opened }
            let shown = reach(ClaudeSessionHosts.host(of: client), pid: client, cwd: cwd, homes: homes, depth: depth + 1)
            if case .detached = shown { return shown }
            return .opened
        }
        switch host {
        case .tmux(let target):
            let tty = ClaudeSessionHosts.tty(of: pid)
            guard let client = ClaudeTmux.focus(target, tty: tty) else {
                return ClaudeTmux.attachLine(target, tty: tty).map { .detached(attach: $0) } ?? .missing
            }
            return outer(client.pid)
        case .zellij(let target):
            switch ClaudeZellij.focus(target, pid: pid) {
            case .missing: return .missing
            case .detached: return ClaudeZellij.attachLine(target).map { .detached(attach: $0) } ?? .missing
            // Selected in zellij, though which terminal shows it isn't known.
            case .client(nil): return .opened
            case .client(let client?): return outer(client.pid)
            }
        default:
            return focus(host, pid: pid, cwd: cwd, homes: homes, depth: depth) ? .opened : .missing
        }
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
        case .tmux, .zellij:
            // The multiplexer selects the pane; the terminal showing it comes
            // forward the way a session in that terminal would.
            return reach(host, pid: pid, cwd: cwd, homes: homes, depth: depth) == .opened
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
        case .background, .remote:
            // The service's openHost attaches these, or reaches them over SSH.
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
        let delay = ClaudeTyping.enterDelay(session.provider)
        let steps = delay.map { ClaudeTyping.steps(text, images: paths, enterAfter: $0) } ?? ClaudeTyping.steps(text, images: paths)
        switch session.host {
        case .terminal(let tty):
            let script = !paths.isEmpty ? ClaudeTerminalScripts.type(text, images: paths, tty: tty)
                : delay.map { ClaudeTerminalScripts.type(text, tty: tty, enterAfter: $0) } ?? ClaudeTerminalScripts.type(text, tty: tty)
            return run(script, in: ClaudeSessionHosts.terminal)
        case .iterm(let tty):
            return run(ClaudeTTYTerminalScripts.itermType(steps, tty: tty), in: ClaudeTerminalApps.iterm)
        case .ghostty(let tty):
            return run(ClaudeTTYTerminalScripts.ghosttyType(steps, tty: tty), in: ClaudeTerminalApps.ghostty)
        case .tmux(let target):
            return ClaudeTmux.send(steps, to: target, tty: ClaudeSessionHosts.tty(of: session.pid))
        case .zellij(let target):
            return ClaudeZellij.send(steps, to: target, pid: session.pid)
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
        case .kitty, .app, .remote, .unknown:
            return .unreachable
        }
    }

    /// Presses keys in the session's terminal, nothing pasted.
    static func press(_ steps: [ClaudeTypingStep], to session: ClaudeSession) -> Failure? {
        guard ClaudeTyping.takesKeys(session.host) else { return .unreachable }
        switch session.host {
        case .terminal(let tty):
            return run(ClaudeTerminalScripts.press(steps, tty: tty), in: ClaudeSessionHosts.terminal)
        case .iterm(let tty):
            return run(ClaudeTTYTerminalScripts.itermType(steps, tty: tty), in: ClaudeTerminalApps.iterm)
        case .tmux(let target):
            return ClaudeTmux.send(steps, to: target, tty: ClaudeSessionHosts.tty(of: session.pid))
        case .zellij(let target):
            return ClaudeZellij.send(steps, to: target, pid: session.pid)
        case .wezterm(let pane, let socket):
            return ClaudeWezTerm.send(steps, pane: pane, socket: socket, tty: ClaudeSessionHosts.tty(of: session.pid))
        case .kitty(let window, let socket?):
            return ClaudeKitty.send(steps, window: window, pid: session.pid, socket: socket)
        default:
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
    static let identifier = "openbar.openbar-terminal-bridge"

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

    /// The extension as it was named before Openbar had its own name.
    static let legacyIdentifier = "vorssaint.vorssaint-terminal-bridge"

    /// Takes the extension out of every editor that has it.
    static func uninstall(home: URL, identifier: String = identifier) {
        for ide in editors() {
            guard let folder = ide.places(home: home)?.extensions.path,
                  ((try? FileManager.default.contentsOfDirectory(atPath: folder)) ?? []).contains(where: { $0.hasPrefix(identifier + "-") }),
                  let cli = ide.places(home: home)?.cli else { continue }
            _ = try? run(cli.path, ["--uninstall-extension", identifier], in: home)
        }
    }

    /// The editors that still hold the extension under its old name.
    static func editorsWithLegacy(home: URL) -> [ClaudeIDE] {
        editors().filter { ide in
            guard let folder = ide.places(home: home)?.extensions.path else { return false }
            return ((try? FileManager.default.contentsOfDirectory(atPath: folder)) ?? []).contains { $0.hasPrefix(legacyIdentifier + "-") }
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
        let work = FileManager.default.temporaryDirectory.appending(path: "openbar-bridge-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: work) }
        let package = work.appending(path: "openbar-terminal-bridge.vsix")
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
/// the Antigravity extension and the `~/.openbar` folder they talk through.
enum ClaudeSessionCleanup {
    /// The official build and the Developer build share the extension and
    /// its folder; each takes away only its own hooks and socket, and the
    /// shared parts go with the last of them.
    static func siblings(of bundle: String) -> [String] {
        let official = "com.openbar.app"
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
        // Every account's settings, each file once (#216).
        var files: [URL] = []
        for url in [settings] + ClaudeAccounts.all(home: home).map(\.settingsURL)
        where !files.contains(where: { $0.resolvingSymlinksInPath() == url.resolvingSymlinksInPath() }) {
            files.append(url)
        }
        for file in files {
            guard let current = try? ClaudeHookInstaller.read(file), ClaudeHookInstaller.hasAny(in: current, bundle: bundle)
            else { continue }
            if (try? ClaudeHookInstaller.update(file) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }) != nil {
                hooks = true
            }
        }
        // Gemini CLI's, Copilot CLI's, Cursor's and OpenCode's go with them.
        AgentHookInstaller.removeAll(home: home, bundle: bundle)
        // And those in folders and tools added on the Hooks page (#236).
        for setting in AgentFolders.all() {
            guard let provider = setting.provider else { continue }
            try? AgentHookInstaller.remove(provider, home: home, bundle: bundle, folder: setting.url)
        }
        for tool in CustomHookTools.all() {
            try? AgentHookInstaller.removeCustom(tool.file, bundle: bundle)
        }
        let folder = home.appending(path: ".openbar", directoryHint: .isDirectory)
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

// MARK: - Sample request (#168)

extension ClaudeSessionService {
    /// Shows a made-up permission request in the island, the way a real one
    /// arrives, so onboarding can show what will happen before any agent
    /// asks. Answering it goes nowhere. False when sessions are off.
    @discardableResult
    func showSample(at now: Date = Date()) -> Bool {
        guard NotchClaudeSessionsSupport.isEnabled() else { return false }
        sampleAnswer = nil
        queue.async { [self] in
            sampleRound += 1
            let session = ClaudeSessionSample.waiting(round: sampleRound, home: home, now: now)
            sample = session
            sampleEvents.append(.needsPermission(id: session.id, tool: session.permission?.tool ?? ""))
            publish(now: now)
        }
        return true
    }

    /// Takes the sample out, as when onboarding closes before it was answered.
    func dismissSample() {
        queue.async { [self] in
            guard sample != nil else { return }
            sampleRound += 1
            sample = nil
            publish(now: Date())
        }
    }

    /// Queue. The sample plays out the answer: an allowed command runs a
    /// moment and finishes, a denied one stops, then the sample leaves.
    fileprivate func answerSample(request: String, _ decision: ClaudeHookMessage.Decision) {
        guard var session = sample, session.permission?.requestID == request else { return }
        let round = sampleRound
        let now = Date()
        let allowed: Bool
        switch decision {
        case .deny: allowed = false
        default: allowed = true
        }
        session = ClaudeSessionSample.answered(session, allowed: allowed, now: now)
        sample = session
        publish(now: now)
        DispatchQueue.main.async { self.sampleAnswer = decision }
        let after = { (delay: TimeInterval, change: @escaping () -> Void) in
            self.queue.asyncAfter(deadline: .now() + delay) { [self] in
                guard sampleRound == round, sample != nil else { return }
                change()
                publish(now: Date())
            }
        }
        if allowed {
            after(1.6) { [self] in
                guard let running = sample else { return }
                let finished = ClaudeSessionSample.finished(running, now: Date())
                sample = finished
                sampleEvents.append(.finished(id: finished.id, duration: 2))
            }
        }
        after(allowed ? 7 : 2.5) { [self] in sample = nil }
    }
}
