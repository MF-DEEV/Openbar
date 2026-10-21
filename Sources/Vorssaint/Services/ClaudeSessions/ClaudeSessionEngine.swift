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
        var turnEnded: Date?
        var completed = true
        /// The newest moment a hook or transcript line told of.
        var lastFact: Date?
        var tools: [ClaudeToolUse] = []
        var subagents: [String: ClaudeSubagent] = [:]
        var tasks: ClaudeTaskProgress?
        var usage: [String: (tokens: AgentTokens, cost: Double)] = [:]
        var title: String?
        var lastText: String?
        var branch: String?
        var model: String?
        /// Permissions the hooks hold, oldest first: subagents can ask at once.
        var permissions: [ClaudePermission] = []
        var hooked = false
        var acknowledged: Date?
        var firstSeen: Date

        init(firstSeen: Date) { self.firstSeen = firstSeen }

        var openTool: ClaudeToolUse? { tools.last { $0.isRunning } }
    }

    var policy: ClaudeContentPolicy {
        didSet { if !policy.previews { purgeContent() } }
    }

    private var records: [String: ClaudeSessionRecord] = [:]
    private var activity: [String: Activity] = [:]
    private var phases: [String: (phase: ClaudeSessionPhase, since: Date)] = [:]

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
                end(at: changed, completed: true, in: &entry)
                activity[id] = entry
            }
            // And it went to work before its prompt was read.
            if record.status == .busy, let changed = record.statusUpdated {
                var entry = activity[id] ?? Activity(firstSeen: now)
                if !entry.turnOpen, changed > entry.lastFact ?? .distantPast {
                    begin(at: changed, in: &entry)
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
            phases[id] = (phase, since)
            if let previous, previous.phase != phase {
                switch phase {
                case .waitingPermission:
                    events.append(.needsPermission(id: id, tool: entry.permissions.first?.tool ?? entry.openTool?.name ?? ""))
                case .waitingInput:
                    events.append(.needsInput(id: id))
                case .finished where previous.phase.isWorking || previous.phase.isWaiting:
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
        if !entry.permissions.isEmpty { return .waitingPermission }
        if record.status == .waiting {
            return record.waitingFor == ClaudeSessionRecord.permissionPrompt ? .waitingPermission : .waitingInput
        }
        let working: Bool
        if let fact = entry.lastFact, record.status == nil || fact > (record.statusUpdated ?? .distantPast) {
            working = entry.turnOpen
        } else {
            working = record.status == .busy
        }
        if working { return entry.turnOpen && entry.openTool != nil ? .runningTool : .generating }
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
        let subagents = entry.subagents.values.sorted { $0.started < $1.started }.map { agent -> ClaudeSubagent in
            var agent = agent
            if !previews { agent.description = nil }
            return agent
        }
        var tasks = entry.tasks
        if !previews { tasks?.current = nil }
        return ClaudeSession(
            id: id, pid: record.pid, cwd: record.cwd, project: AgentLogParser.projectName(record.cwd),
            name: record.name, branch: entry.branch, model: entry.model, host: record.host,
            phase: phase, phaseSince: since, turnStarted: entry.turnStarted,
            waitingFor: record.status == .waiting ? record.waitingFor : nil,
            currentTool: phase == .runningTool || phase == .waitingPermission ? entry.openTool.map(gated) : nil,
            permission: entry.permissions.first,
            timeline: entry.tools.suffix(Self.timelineLength).map(gated), subagents: subagents, tasks: tasks,
            tokens: tokens, cost: cost,
            title: previews ? entry.title : nil, lastText: previews ? entry.lastText : nil,
            hooked: entry.hooked)
    }

    // MARK: Rules

    private func apply(_ fact: ClaudeTranscriptFact, to entry: inout Activity) {
        switch fact {
        case .turnBegan(let date): begin(at: date, in: &entry)
        case .turnEnded(let date, let completed): end(at: date, completed: completed, in: &entry)
        case .toolStarted(let tool): start(tool, in: &entry)
        case .toolFinished(let id, let date, let failed): finish(id, at: date, failed: failed, in: &entry)
        case .text(let text, let date):
            if policy.previews { entry.lastText = text }
            touch(date, in: &entry)
        case .title(let title): if policy.previews { entry.title = title }
        case .branch(let branch): entry.branch = branch
        case .model(let model): entry.model = model
        case .usage(let key, let tokens, let cost):
            let known = entry.usage[key]
            entry.usage[key] = (known?.tokens.merged(with: tokens) ?? tokens, max(known?.cost ?? 0, cost))
        case .subagentActivity(let id, let date):
            if entry.subagents[id] == nil {
                entry.subagents[id] = ClaudeSubagent(id: id, type: "Agent", started: date)
            } else if entry.subagents[id]?.finished.map({ $0 < date }) == true {
                entry.subagents[id]?.finished = nil
            }
        case .subagentEnded(let id, let date):
            if entry.subagents[id] == nil { entry.subagents[id] = ClaudeSubagent(id: id, type: "Agent", started: date) }
            entry.subagents[id]?.finished = date
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

    private func apply(_ event: ClaudeHookFact.Event, date: Date, to entry: inout Activity) {
        switch event {
        case .sessionStarted, .other: break
        case .sessionEnded: end(at: date, completed: false, in: &entry)
        case .promptSubmitted: begin(at: date, in: &entry)
        case .toolStarted(let tool): start(tool, in: &entry)
        case .toolFinished(let id, let failed): finish(id, at: date, failed: failed, in: &entry)
        case .permissionRequested(var permission):
            if !entry.turnOpen { begin(at: date, in: &entry) }
            // The tool that asked is the main agent's newest one by that name
            // still waiting to run; its result answers the request.
            if permission.toolID == nil {
                permission.toolID = entry.tools.last { $0.isRunning && $0.name == permission.tool }?.id
            }
            entry.permissions.removeAll { $0.requestID == permission.requestID }
            entry.permissions.append(permission)
        case .notification: break
        case .stopped(let text):
            if policy.previews, let text { entry.lastText = text }
            end(at: date, completed: true, in: &entry)
        case .subagentStarted(let id, let type):
            entry.subagents[id] = ClaudeSubagent(id: id, type: type,
                                                 description: entry.subagents[id]?.description, started: date)
        case .subagentStopped(let id):
            if entry.subagents[id] == nil { entry.subagents[id] = ClaudeSubagent(id: id, type: "Agent", started: date) }
            entry.subagents[id]?.finished = date
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
            if entry.subagents[agent] == nil { entry.subagents[agent] = ClaudeSubagent(id: agent, type: "Agent", started: date) }
            entry.lastFact = max(entry.lastFact ?? date, date)
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
        touch(date, in: &entry)
    }

    private func closeTools(at date: Date, in entry: inout Activity) {
        for index in entry.tools.indices where entry.tools[index].isRunning {
            entry.tools[index].finished = max(date, entry.tools[index].started)
        }
    }

    private func touch(_ date: Date, in entry: inout Activity) {
        entry.lastFact = max(entry.lastFact ?? date, date)
    }
}
