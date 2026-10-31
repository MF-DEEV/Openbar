// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Joins what Claude Code's process registry, its transcripts and its hooks
/// say about each running session into the state the island shows. Kept free
/// of files, queues and clocks so every rule can be tested; the owner calls
/// it from one queue.
///
/// The registry is written by the session itself as its state changes, so it
/// decides whether a session waits on the person and otherwise whether it
/// works. A hook or transcript line newer than the registry's last change
/// wins, since the registry can trail it by a moment, and only those say
/// which tool runs.
final class ClaudeSessionEngine {
    /// How long an unread finished turn stays marked before it reads as idle.
    static let finishedFor: TimeInterval = 10 * 60
    /// Hook events can arrive before the registry lists their session.
    static let unlistedGrace: TimeInterval = 120
    static let timelineLength = 12

    struct Activity {
        var turnOpen = false
        var turnStarted: Date?
        /// The open turn is known only from the registry saying busy, with
        /// nothing typed, said or run since; and when the turn before began.
        var registryOnly = false
        var startedBefore: Date?
        var turnEnded: Date?
        var completed = true
        /// The newest moment a hook or transcript line told of.
        var lastFact: Date?
        var tools: [ClaudeToolUse] = []
        var subagents: [String: ClaudeSubagent] = [:]
        /// Subagents reported done before anything else was known of them;
        /// the end applies once they are.
        var endedAgents: [String: Date] = [:]
        /// When each subagent was last seen working, by its transcript.
        var agentSeen: [String: Date] = [:]
        /// Commands and subagents left running in the background, by the call
        /// that started them.
        var background: [String: Background] = [:]
        var tasks: ClaudeTaskProgress?
        var usage: [String: (tokens: AgentTokens, cost: Double)] = [:]
        var title: String?
        var lastText: String?
        var chat: [ClaudeChatMessage] = []
        /// Messages said so far, which numbers the next one.
        var said = 0
        var branch: String?
        var model: String?
        /// Permissions the hooks hold, oldest first: subagents can ask at once.
        var permissions: [ClaudePermission] = []
        var hooked = false
        /// When Claude Code last asked the person for something by a notice.
        var asked: Date?
        var acknowledged: Date?
        var firstSeen: Date

        init(firstSeen: Date) { self.firstSeen = firstSeen }

        var openTool: ClaudeToolUse? { tools.last { $0.isRunning } }
        var runningBackground: [Background] { background.values.filter { $0.ended == nil }.sorted { $0.started < $1.started } }
        var runningSubagents: [ClaudeSubagent] { subagents.values.filter(\.isRunning).sorted { $0.started < $1.started } }

        /// The session came to rest: the subagents last seen working before
        /// then ended, and the background jobs they were; one seen working
        /// later works again. Idle, with not even a command of its own
        /// running, every job begun before then ended too.
        mutating func rest(at date: Date, idle: Bool) {
            for (id, agent) in subagents where agent.isRunning && max(agent.started, agentSeen[id] ?? agent.started) <= date {
                subagents[id]?.finished = date
            }
            for (key, job) in background where job.ended == nil && job.started <= date
                && (idle || job.task.map { subagents[$0]?.isRunning == false } == true) {
                background[key]?.ended = date
            }
        }
    }

    struct Background {
        var task: String?
        let service: Bool
        var label: String?
        let started: Date
        var ended: Date?
    }

    var policy: ClaudeContentPolicy {
        didSet { if !policy.previews { purgeContent() } }
    }

    private var records: [String: ClaudeSessionRecord] = [:]
    private var activity: [String: Activity] = [:]
    /// Each session's phase, since when, and since when it has worked or
    /// waited without a break.
    private var phases: [String: (phase: ClaudeSessionPhase, since: Date, worked: Date)] = [:]

    init(policy: ClaudeContentPolicy = ClaudeContentPolicy()) {
        self.policy = policy
    }

    var sessionIDs: [String] { Array(records.keys) }

    func record(_ id: String) -> ClaudeSessionRecord? { records[id] }

    // MARK: Input

    /// Replaces the list of running sessions.
    func ingest(records list: [ClaudeSessionRecord], now: Date) {
        records = Dictionary(list.map { ($0.sessionID, $0) }, uniquingKeysWith: { first, _ in first })
        for (id, record) in records {
            // The registry left the prompt behind, so it was answered in the
            // terminal or went away with its turn.
            if record.status != .waiting, let changed = record.statusUpdated {
                activity[id]?.permissions.removeAll { $0.since < changed }
            }
            // The session went idle after the last line read, so the turn
            // ended even when its last lines were not read yet.
            if var entry = activity[id], entry.turnOpen, record.status == .idle || record.status == .shell,
               let changed = record.statusUpdated, changed > entry.lastFact ?? .distantPast {
                if entry.registryOnly {
                    // Busy a moment with nothing typed, said or run, as while
                    // Claude Code writes a recap of the session: no turn ran,
                    // and nothing finished.
                    entry.turnOpen = false
                    entry.turnStarted = entry.startedBefore
                    entry.registryOnly = false
                } else {
                    end(at: changed, completed: true, in: &entry)
                }
                activity[id] = entry
            }
            // Claude Code reads busy while any subagent works, in the turn or
            // in the background, so at rest none does, even one whose end
            // was never read.
            if record.status == .idle || record.status == .shell, let changed = record.statusUpdated {
                activity[id]?.rest(at: changed, idle: record.status == .idle)
            }
            // And it went to work before its prompt was read.
            if record.status == .busy, let changed = record.statusUpdated {
                var entry = activity[id] ?? Activity(firstSeen: now)
                if !entry.turnOpen, changed > entry.lastFact ?? .distantPast {
                    let before = entry.turnStarted
                    begin(at: changed, in: &entry)
                    entry.registryOnly = true
                    entry.startedBefore = before
                    activity[id] = entry
                }
            }
        }
        activity = activity.filter { id, entry in
            records[id] != nil || now.timeIntervalSince(entry.lastFact ?? entry.firstSeen) < Self.unlistedGrace
        }
        phases = phases.filter { records[$0.key] != nil }
    }

    func ingest(sessionID id: String, facts: [ClaudeTranscriptFact], now: Date) {
        guard !facts.isEmpty else { return }
        var entry = activity[id] ?? Activity(firstSeen: now)
        for fact in facts { apply(fact, to: &entry) }
        // Subagents first read after the registry came to rest ended by then.
        if let record = records[id], record.status == .idle || record.status == .shell, let changed = record.statusUpdated {
            entry.rest(at: changed, idle: record.status == .idle)
        }
        activity[id] = entry
    }

    func ingest(hook fact: ClaudeHookFact, now: Date) {
        var entry = activity[fact.sessionID] ?? Activity(firstSeen: now)
        entry.hooked = true
        if let agent = fact.agentID {
            applySubagent(fact.event, agent: agent, date: fact.date, to: &entry)
        } else {
            apply(fact.event, date: fact.date, to: &entry)
        }
        activity[fact.sessionID] = entry
    }

    /// The person answered, or the hook stopped waiting for an answer.
    func resolvePermission(requestID: String) {
        for id in activity.keys { activity[id]?.permissions.removeAll { $0.requestID == requestID } }
    }

    /// The permission the island shows for a session: the oldest one asked.
    func permission(of id: String) -> ClaudePermission? { activity[id]?.permissions.first }

    func holds(_ requestID: String, for id: String) -> Bool {
        activity[id]?.permissions.contains { $0.requestID == requestID } == true
    }

    /// The person saw the session's finished turn.
    func acknowledge(_ id: String, now: Date) {
        guard activity[id] != nil || records[id] != nil else { return }
        var entry = activity[id] ?? Activity(firstSeen: now)
        entry.acknowledged = now
        activity[id] = entry
    }

    /// Forgets every text, title and argument read so far.
    func purgeContent() {
        for id in activity.keys {
            activity[id]?.title = nil
            activity[id]?.lastText = nil
            activity[id]?.chat = []
            if let background = activity[id]?.background {
                activity[id]?.background = background.mapValues { var job = $0; job.label = nil; return job }
            }
            activity[id]?.tasks?.current = nil
            if let tools = activity[id]?.tools {
                activity[id]?.tools = tools.map { var tool = $0; tool.summary = nil; return tool }
            }
            if let subagents = activity[id]?.subagents {
                activity[id]?.subagents = subagents.mapValues { var agent = $0; agent.description = nil; return agent }
            }
        }
    }

    // MARK: Output

    func snapshot(now: Date) -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        var sessions: [ClaudeSession] = []
        var events: [ClaudeSessionEvent] = []
        for (id, record) in records {
            let entry = activity[id] ?? Activity(firstSeen: now)
            let phase = Self.phase(record: record, activity: entry, now: now)
            let previous = phases[id]
            let since = previous?.phase == phase ? previous?.since ?? now : now
            let busy = { (phase: ClaudeSessionPhase) in phase.isActive || phase.isWaiting }
            let worked = busy(phase) ? previous.flatMap { busy($0.phase) ? $0.worked : nil } ?? since : since
            phases[id] = (phase, since, worked)
            if let previous, previous.phase != phase {
                switch phase {
                case .waitingPermission:
                    events.append(.needsPermission(id: id, tool: entry.permissions.first?.tool ?? entry.openTool?.name ?? ""))
                case .waitingInput:
                    events.append(.needsInput(id: id))
                // Only a turn that ended after the work was seen begin is news;
                // going back to a result already shown, as after a recap, is not.
                // A turn that ended while its work went on in the background
                // is told once that work ends.
                case .finished where busy(previous.phase)
                    && (entry.turnEnded ?? .distantPast) >= (previous.phase == .background ? previous.worked : previous.since):
                    let duration = entry.turnStarted.flatMap { start in entry.turnEnded.map { $0.timeIntervalSince(start) } }
                    events.append(.finished(id: id, duration: duration))
                default: break
                }
            }
            sessions.append(session(id: id, record: record, entry: entry, phase: phase, since: since))
        }
        return (sessions, events)
    }

    /// The phase the rules above give one session.
    static func phase(record: ClaudeSessionRecord, activity entry: Activity, now: Date) -> ClaudeSessionPhase {
        // A question Claude asks waits on the person's answer, not a permission.
        if let first = entry.permissions.first { return first.asksQuestions ? .waitingInput : .waitingPermission }
        if record.status == .waiting {
            guard record.waitingFor == ClaudeSessionRecord.dialogOpen else {
                return record.waitingFor == ClaudeSessionRecord.permissionPrompt ? .waitingPermission : .waitingInput
            }
            // A dialog the person opened waits on nobody. One Claude Code
            // raises to ask something sends a notice as it opens.
            if let asked = entry.asked, asked >= (record.statusUpdated ?? .distantPast).addingTimeInterval(-askedSlack) {
                return .waitingInput
            }
        }
        let working: Bool
        if let fact = entry.lastFact, record.status == nil || fact > (record.statusUpdated ?? .distantPast) {
            working = entry.turnOpen
        } else {
            working = record.status == .busy
        }
        if working { return entry.turnOpen && entry.openTool != nil ? .runningTool : .generating }
        // Claude Code says "shell" for a session at rest with a command of its
        // own still running in the background. The work is not done while one
        // it waits for runs; a dev server or an emulator left running is not
        // work, and when nothing was seen start, it counts as work. While a
        // subagent it sent to the background works, it says "busy" though
        // the turn ended; a subagent's own subagents are sent off in its
        // transcript, so the ones listed as working count too.
        let running = entry.runningBackground
        if record.status == .shell, running.isEmpty || running.contains(where: { !$0.service }) { return .background }
        if record.status == .busy,
           running.contains(where: { !$0.service && $0.task != nil }) || !entry.runningSubagents.isEmpty {
            return .background
        }
        if let ended = entry.turnEnded, entry.completed, now.timeIntervalSince(ended) < finishedFor,
           (entry.acknowledged ?? .distantPast) < ended {
            return .finished
        }
        return .idle
    }

    private func session(id: String, record: ClaudeSessionRecord, entry: Activity,
                         phase: ClaudeSessionPhase, since: Date) -> ClaudeSession {
        let previews = policy.previews
        func gated(_ tool: ClaudeToolUse) -> ClaudeToolUse {
            var tool = tool
            if !previews { tool.summary = nil }
            return tool
        }
        var tokens = AgentTokens()
        var cost = 0.0
        for usage in entry.usage.values {
            tokens += usage.tokens
            cost += usage.cost
        }
        let subagents = Self.ordered(Array(entry.subagents.values)).map { agent -> ClaudeSubagent in
            var agent = agent
            if !previews { agent.description = nil }
            return agent
        }
        var tasks = entry.tasks
        if !previews { tasks?.current = nil }
        return ClaudeSession(
            id: id, pid: record.pid, cwd: record.cwd, project: AgentLogParser.projectName(record.cwd),
            name: record.name, nameSource: record.nameSource, branch: entry.branch, model: entry.model, host: record.host,
            phase: phase, phaseSince: since, turnStarted: entry.turnStarted,
            lastActive: [entry.turnEnded, entry.lastFact, record.statusUpdated].compactMap { $0 }.max(),
            lookedAt: entry.acknowledged,
            waitingFor: record.status == .waiting ? record.waitingFor : nil,
            currentTool: phase == .runningTool || phase == .waitingPermission ? entry.openTool.map(gated) : nil,
            permission: entry.permissions.first,
            timeline: entry.tools.suffix(Self.timelineLength).map(gated), subagents: subagents, tasks: tasks,
            tokens: tokens, cost: cost,
            title: previews ? entry.title : nil, lastText: previews ? entry.lastText : nil,
            hooked: entry.hooked, chat: previews ? entry.chat : [],
            backgroundLabel: previews
                ? entry.runningBackground.last { !$0.service }?.label ?? entry.runningSubagents.last?.description : nil,
            services: record.status == .shell || phase == .background ? entry.runningBackground.filter(\.service).count : 0,
            background: record.job.map { ClaudeBackgroundDetails(job: $0, configHome: record.configHome) })
    }

    // MARK: Rules

    private func apply(_ fact: ClaudeTranscriptFact, to entry: inout Activity) {
        switch fact {
        case .turnBegan(let date): begin(at: date, in: &entry)
        case .turnEnded(let date, let completed): end(at: date, completed: completed, in: &entry)
        case .toolStarted(let tool): start(tool, in: &entry)
        case .toolFinished(let id, let date, let failed): finish(id, at: date, failed: failed, in: &entry)
        case .text(let text, let date, let endsTurn):
            if policy.previews { entry.lastText = text }
            say(text, as: .assistant, at: date, endsTurn: endsTurn, in: &entry)
            touch(date, in: &entry)
        case .prompt(let text, let date, let images):
            say(text, as: .user, at: date, images: images, in: &entry)
        case .title(let title): if policy.previews { entry.title = title }
        case .branch(let branch): entry.branch = branch
        case .model(let model): entry.model = model
        case .usage(let key, let tokens, let cost):
            let known = entry.usage[key]
            entry.usage[key] = (known?.tokens.merged(with: tokens) ?? tokens, max(known?.cost ?? 0, cost))
        case .subagentActivity(let id, let date):
            entry.agentSeen[id] = max(entry.agentSeen[id] ?? date, date)
            if entry.subagents[id] == nil {
                entry.subagents[id] = ClaudeSubagent(id: id, type: ClaudeSubagent.untyped, started: date)
                if let ended = entry.endedAgents[id], ended >= date { entry.subagents[id]?.finished = ended }
            } else if entry.subagents[id]?.finished.map({ $0 < date }) == true,
                      (entry.endedAgents[id] ?? .distantPast) < date {
                entry.subagents[id]?.finished = nil
            }
        case .subagentEnded(let id, let date):
            endSubagent(id, at: date, in: &entry)
            for (key, job) in entry.background where job.task == id && job.ended == nil { entry.background[key]?.ended = date }
        case .backgroundStarted(let toolID, let service, let label, let date):
            if entry.background[toolID] == nil {
                entry.background[toolID] = Background(task: nil, service: service, label: policy.previews ? label : nil, started: date)
            }
            if entry.background.count > Self.backgroundKept {
                for key in entry.background.filter({ $0.value.ended != nil }).sorted(by: { $0.value.started < $1.value.started })
                        .prefix(entry.background.count - Self.backgroundKept).map(\.key) {
                    entry.background[key] = nil
                }
            }
        case .backgroundTask(let toolID, let task):
            entry.background[toolID]?.task = task
            // The end may have been read first.
            if let ended = entry.endedAgents[task] { entry.background[toolID]?.ended = ended }
        case .subagentInfo(let id, let type, let description):
            if entry.subagents[id] == nil {
                entry.subagents[id] = ClaudeSubagent(id: id, type: type, started: entry.lastFact ?? .distantPast)
            }
            entry.subagents[id]?.type = type
            if policy.previews { entry.subagents[id]?.description = description }
        case .tasks(var tasks):
            if !policy.previews { tasks.current = nil }
            entry.tasks = tasks.total > 0 ? tasks : nil
        }
    }

    /// Notices that do not ask the person anything: permissions have their
    /// own page, and an idle reminder or a sign-in is not a question.
    static let quietNotices: Set<String> = [
        "idle_prompt", "permission_prompt", "worker_permission_prompt", "auth_success", "auth_storage_failure",
        "computer_use_enter", "computer_use_exit", "elicitation_complete", "elicitation_response",
        "agent_completed", "plugin_notification", "push_notification",
    ]
    static func asks(_ type: String) -> Bool { !quietNotices.contains(type) }
    /// The notice and the registry's change are written apart.
    static let askedSlack: TimeInterval = 5

    /// Background commands and subagents kept per session, the ended ones going first.
    static let backgroundKept = 40

    /// The conversation keeps its latest messages.
    static let chatLength = 150

    private func say(_ text: String, as role: ClaudeChatMessage.Role, at date: Date, endsTurn: Bool = false,
                     images: [ClaudeChatImage] = [], in entry: inout Activity) {
        guard policy.previews else { return }
        // The same words twice are one message read twice; the same images
        // sent again later are a new prompt.
        if let last = entry.chat.last, last.role == role, last.text == text, last.images == images,
           images.isEmpty || last.at == date {
            if endsTurn { entry.chat[entry.chat.count - 1].endsTurn = true }
            return
        }
        entry.said += 1
        entry.chat.append(ClaudeChatMessage(id: entry.said, role: role, text: text, at: date, endsTurn: endsTurn,
                                            images: images))
        if entry.chat.count > Self.chatLength { entry.chat.removeFirst(entry.chat.count - Self.chatLength) }
        if !images.isEmpty { ClaudeChatImage.trim(&entry.chat, images: \.images) }
    }

    private func apply(_ event: ClaudeHookFact.Event, date: Date, to entry: inout Activity) {
        switch event {
        case .sessionStarted, .other: break
        case .sessionEnded: end(at: date, completed: false, in: &entry)
        case .promptSubmitted: begin(at: date, in: &entry)
        case .toolStarted(let tool): start(tool, in: &entry)
        case .toolFinished(let id, let failed): finish(id, at: date, failed: failed, in: &entry)
        case .permissionRequested(var permission):
            if !entry.turnOpen { begin(at: date, in: &entry) }
            // The tool that asked is the main agent's running call with the
            // same input, or the only running call by that name; its result
            // answers the request.
            if permission.toolID == nil {
                let candidates = entry.tools.filter { $0.isRunning && $0.name == permission.tool }
                permission.toolID = candidates.last { $0.inputKey != nil && $0.inputKey == permission.inputKey }?.id
                    ?? (candidates.count == 1 ? candidates.first?.id : nil)
            }
            entry.permissions.removeAll { $0.requestID == permission.requestID }
            entry.permissions.append(permission)
        case .notification(let type): if Self.asks(type) { entry.asked = date }
        case .stopped(let text):
            if policy.previews, let text { entry.lastText = text }
            end(at: date, completed: true, in: &entry)
        case .subagentStarted(let id, let type):
            // The hook adds to what the transcript already told, never resets it.
            if var known = entry.subagents[id] {
                known.started = min(known.started, date)
                if known.type == ClaudeSubagent.untyped, !type.isEmpty { known.type = type }
                entry.subagents[id] = known
            } else {
                entry.subagents[id] = ClaudeSubagent(id: id, type: type.isEmpty ? ClaudeSubagent.untyped : type, started: date)
            }
        case .subagentStopped(let id):
            endSubagent(id, at: date, in: &entry)
        }
    }

    /// A subagent's own tools tell that it works, not what the session runs.
    private func applySubagent(_ event: ClaudeHookFact.Event, agent: String, date: Date, to entry: inout Activity) {
        switch event {
        case .subagentStopped:
            apply(event, date: date, to: &entry)
        case .permissionRequested(var permission):
            // Its permission still waits on the person; its tools are not
            // followed, so only a later prompt, the end or the registry
            // settle it.
            permission.toolID = permission.toolID ?? ""
            apply(.permissionRequested(permission), date: date, to: &entry)
        default:
            // Claude Code's own forks, like the one that saves memories, run
            // tools under an agent id of their own but never start or stop
            // as subagents and keep no transcript: only a subagent the start
            // hook or a transcript told of is listed. A hook is dated when it
            // arrived, not by Claude Code's clock, so it does not say when a
            // subagent last worked.
            entry.lastFact = max(entry.lastFact ?? date, date)
        }
    }

    /// A subagent known to have run ends; one not seen yet ends as soon as it is.
    private func endSubagent(_ id: String, at date: Date, in entry: inout Activity) {
        entry.endedAgents[id] = max(entry.endedAgents[id] ?? date, date)
        guard let known = entry.subagents[id] else { return }
        entry.subagents[id]?.finished = max(known.finished ?? date, date, known.started)
    }

    /// Working subagents first, by when they began, then the ones done, the
    /// latest first.
    static func ordered(_ agents: [ClaudeSubagent]) -> [ClaudeSubagent] {
        agents.sorted { lhs, rhs in
            switch (lhs.finished, rhs.finished) {
            case (nil, nil): return lhs.started < rhs.started
            case (nil, _): return true
            case (_, nil): return false
            case let (left?, right?): return left > right
            }
        }
    }

    private func begin(at date: Date, in entry: inout Activity) {
        // A prompt read late cannot open a turn that already ended after it.
        if let ended = entry.turnEnded, date < ended { return }
        // A prompt queued while the session works joins the turn under way.
        if !entry.turnOpen {
            // Whatever still looked open stopped with the turn it served.
            closeTools(at: date, in: &entry)
            entry.turnOpen = true
            entry.turnStarted = date
        }
        // A prompt can only be typed once the dialogs before it closed.
        entry.permissions.removeAll { $0.since < date }
        touch(date, in: &entry)
    }

    private func end(at date: Date, completed: Bool, in entry: inout Activity) {
        // An older line read late cannot end a turn that began after it.
        if let started = entry.turnStarted, entry.turnOpen, date < started { return }
        entry.turnOpen = false
        entry.turnEnded = date
        entry.completed = completed
        entry.permissions.removeAll { $0.since <= date }
        closeTools(at: date, in: &entry)
        touch(date, in: &entry)
    }

    private func start(_ tool: ClaudeToolUse, in entry: inout Activity) {
        if let index = entry.tools.firstIndex(where: { $0.id == tool.id }) {
            if entry.tools[index].summary == nil, policy.previews { entry.tools[index].summary = tool.summary }
            if entry.tools[index].inputKey == nil { entry.tools[index].inputKey = tool.inputKey }
        } else {
            var tool = tool
            if !policy.previews { tool.summary = nil }
            entry.tools.append(tool)
            if entry.tools.count > Self.timelineLength * 2 { entry.tools.removeFirst(entry.tools.count - Self.timelineLength) }
        }
        // A tool after the turn ended means a turn whose prompt was not seen.
        if !entry.turnOpen, (entry.turnEnded ?? .distantPast) <= tool.started {
            entry.turnOpen = true
            entry.turnStarted = tool.started
        }
        touch(tool.started, in: &entry)
    }

    private func finish(_ id: String, at date: Date, failed: Bool, in entry: inout Activity) {
        if let index = entry.tools.firstIndex(where: { $0.id == id }), entry.tools[index].isRunning {
            entry.tools[index].finished = max(date, entry.tools[index].started)
            entry.tools[index].failed = failed
        }
        // The tool that asked ran or was refused, here or in the terminal.
        entry.permissions.removeAll { $0.toolID == id }
        // A background command refused before it ran got no id and never ends.
        if failed, let job = entry.background[id], job.task == nil, job.ended == nil { entry.background[id]?.ended = date }
        touch(date, in: &entry)
    }

    private func closeTools(at date: Date, in entry: inout Activity) {
        for index in entry.tools.indices where entry.tools[index].isRunning {
            entry.tools[index].finished = max(date, entry.tools[index].started)
        }
    }

    private func touch(_ date: Date, in entry: inout Activity) {
        entry.lastFact = max(entry.lastFact ?? date, date)
        // Something was typed, said or run: the turn is real.
        entry.registryOnly = false
    }
}
