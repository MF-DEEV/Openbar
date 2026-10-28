// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Carbon.HIToolbox
import Foundation

/// Decisions for the island's Sessions section, kept out of its views so they
/// can be tested.
enum NotchClaudeSessionsSupport {
    /// The section rides on the AI feature, which installs everything about
    /// coding agents, and has its own switch; it can also be hidden.
    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        NotchSupport.isEnabled(in: defaults) && NotchSupport.modules(in: defaults).contains(.sessions)
    }

    static func showsActivity(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsLiveActivity) as? Bool ?? true
    }

    static func autoExpands(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsAutoExpand) as? Bool ?? true
    }

    static func policy(in defaults: UserDefaults = .standard) -> ClaudeContentPolicy {
        ClaudeContentPolicy(previews: defaults.bool(forKey: DefaultsKey.notchSessionsContentPreview))
    }

    static func showsBuddies(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsBuddies) as? Bool ?? true
    }

    // MARK: Order

    /// Sessions that wait on the person come first, then the ones at work,
    /// then work in the background and unread results; quiet sessions follow
    /// from the one that worked last, then by folder.
    static func sorted(_ sessions: [ClaudeSession]) -> [ClaudeSession] {
        sessions.sorted { lhs, rhs in
            let left = rank(lhs.phase), right = rank(rhs.phase)
            if left != right { return left < right }
            switch lhs.phase {
            case .waitingPermission, .waitingInput:
                if lhs.phaseSince != rhs.phaseSince { return lhs.phaseSince < rhs.phaseSince }
            case .generating, .runningTool:
                let a = lhs.turnStarted ?? lhs.phaseSince, b = rhs.turnStarted ?? rhs.phaseSince
                if a != b { return a < b }
            case .background, .finished:
                if lhs.phaseSince != rhs.phaseSince { return lhs.phaseSince > rhs.phaseSince }
            case .idle:
                // The one that last worked comes first, so a session just
                // looked at does not sink among the long quiet ones.
                let a = lhs.lastActive ?? .distantPast, b = rhs.lastActive ?? .distantPast
                if a != b { return a > b }
            }
            let byName = lhs.project.localizedStandardCompare(rhs.project)
            if byName != .orderedSame { return byName == .orderedAscending }
            return lhs.pid < rhs.pid
        }
    }

    static func rank(_ phase: ClaudeSessionPhase) -> Int {
        switch phase {
        case .waitingPermission: return 0
        case .waitingInput: return 1
        case .runningTool, .generating: return 2
        case .background: return 3
        case .finished: return 4
        case .idle: return 5
        }
    }

    // MARK: Island

    /// Whether a session's change should open the island by itself: only a
    /// closed one, so a page or list the person has open never changes
    /// under the pointer, and never while the island is held back, as during
    /// a recording or in full screen.
    static func shouldAutoExpand(_ event: ClaudeSessionEvent, enabled: Bool, expanded: Bool, suspended: Bool) -> Bool {
        guard enabled, !suspended, !expanded else { return false }
        switch event {
        case .needsPermission, .needsInput: return true
        case .finished: return false
        }
    }

    /// How long the buttons wait after the request on screen changes.
    static let settleDelay: TimeInterval = 0.8

    /// How long the desktop or Vorssaint stays in front before a waiting
    /// request takes the island, so passing through on the way to another
    /// app does not open it.
    static let presentingDelay: TimeInterval = 0.75

    /// The keys that answer a request on screen, as the dialogs that ask
    /// the same do, and the names they are claimed under while borrowed.
    /// Each is the key that types its letter on the layout in use, so ⌘Y
    /// is where the Y is on QWERTZ too.
    static var allowShortcut: GlobalShortcut { commandShortcut(typing: "y", fallback: kVK_ANSI_Y) }
    static var denyShortcut: GlobalShortcut { commandShortcut(typing: "n", fallback: kVK_ANSI_N) }
    static var alwaysShortcut: GlobalShortcut { commandShortcut(typing: "t", fallback: kVK_ANSI_T) }
    static let allowShortcutKey = "notchSessionsAllowShortcut"
    static let denyShortcutKey = "notchSessionsDenyShortcut"
    static let alwaysShortcutKey = "notchSessionsAlwaysShortcut"

    static func commandShortcut(typing letter: String, fallback: Int,
                                label: (Int64) -> String? = { GlobalShortcut.layoutKeyLabel(for: $0, usesCommand: true) })
        -> GlobalShortcut {
        let code = (0..<128).first { label(Int64($0))?.lowercased() == letter }.map(Int64.init) ?? Int64(fallback)
        return GlobalShortcut(keyCode: code, modifiers: .command)
    }

    /// One count beside the camera: how many sessions are in a phase, and
    /// for the ones at work, whose they are.
    struct StripItem: Equatable, Identifiable {
        let phase: ClaudeSessionPhase
        let count: Int
        /// The agent at work, which colors its count; nil for the phases
        /// every agent shares.
        var provider: ClaudeSessionProvider? = nil

        var id: String { phase.rawValue + "." + (provider?.rawValue ?? "all") }
    }

    /// The counts beside the camera, leaving out the empty ones: waiting,
    /// then each agent at work in its own color, then finished. The agent's
    /// mark shows once, on the other side of the camera; each count keeps
    /// the phase's symbol.
    static func stripReading(_ counts: ClaudeSessionCounts) -> [StripItem] {
        let codex = min(max(0, counts.workingCodex), max(0, counts.working))
        return [StripItem(phase: .waitingPermission, count: counts.waiting),
                StripItem(phase: .generating, count: max(0, counts.working) - codex, provider: .claude),
                StripItem(phase: .generating, count: codex, provider: .codex),
                StripItem(phase: .finished, count: counts.finished)]
            .filter { $0.count > 0 }
    }

    /// Whose mark the strip shows while sessions work: Claude's while any
    /// Claude session works, Codex's when only Codex does, nil when none do.
    static func stripAgent(_ counts: ClaudeSessionCounts) -> ClaudeSessionProvider? {
        guard counts.working > 0 else { return nil }
        return counts.working > counts.workingCodex ? .claude : .codex
    }

    /// The counts as text, for measuring the strip.
    static func stripText(_ counts: ClaudeSessionCounts) -> String {
        stripReading(counts).map { "• \($0.count)" }.joined(separator: "  ")
    }

    /// How wide the strip draws the counts: a mark, its number, and room
    /// between them.
    static func stripReadingWidth(_ counts: ClaudeSessionCounts, textSize: CGFloat) -> CGFloat {
        let items = stripReading(counts)
        guard !items.isEmpty else { return 0 }
        let digits = items.map { String($0.count).count }.reduce(0, +)
        return CGFloat(items.count) * (textSize * 0.9 + 2) + CGFloat(digits) * textSize * 0.62
            + CGFloat(items.count - 1) * 6
    }

    static func symbol(_ phase: ClaudeSessionPhase) -> String {
        switch phase {
        case .waitingPermission: return "hand.raised.fill"
        case .waitingInput: return "questionmark.bubble.fill"
        case .runningTool: return "hammer.fill"
        case .generating: return "sparkle"
        case .background: return "hourglass"
        case .finished: return "checkmark.circle.fill"
        case .idle: return "moon.zzz.fill"
        }
    }

    // MARK: Replies

    /// A reply can be typed only where the session listens for one: never
    /// while a dialog waits, where the keys would answer it instead.
    static func canReply(_ session: ClaudeSession) -> Bool {
        // Keys typed while any dialog is open would go to the dialog. Codex
        // takes replies through its server, wherever its terminal is.
        (session.provider == .codex || session.host.acceptsReplies) && !session.phase.isWaiting && session.waitingFor == nil
    }

    static let replyLimit = 4000

    /// How many colors projects are told apart by.
    static let projectColors = 8

    /// A project's color, the same each time for the same folder name so a
    /// project is known by it at a glance.
    static func projectColor(_ project: String) -> Int {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in project.lowercased().utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return Int(hash % UInt64(projectColors))
    }

    /// What a session is called: the person's own name for it, then one
    /// given with /rename, the title of its conversation, then Claude Code's
    /// own name, which follows the folder.
    static func name(_ session: ClaudeSession, custom: String?) -> String {
        if let custom = custom?.trimmingCharacters(in: .whitespacesAndNewlines), !custom.isEmpty { return custom }
        if session.nameSource == "user", !session.name.isEmpty { return session.name }
        if let title = session.title, !title.isEmpty { return title }
        return session.name.isEmpty ? session.project : session.name
    }

    /// The longest name the island keeps, and how many it keeps at most.
    static let nameLimit = 60
    static let namesKept = 100

    /// The names with one changed. Names of sessions no longer running go
    /// first once there are too many.
    static func renamed(_ names: [String: String], id: String, to name: String, live: Set<String>) -> [String: String] {
        let trimmed = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(nameLimit))
        return kept(names, id: id, value: trimmed.isEmpty ? nil : trimmed, live: live)
    }

    /// What the island keeps by session, with one value changed or, with
    /// nil, removed. Values of sessions no longer running go first once
    /// there are more than `limit`.
    static func kept(_ values: [String: String], id: String, value: String?, live: Set<String>,
                     limit: Int = namesKept) -> [String: String] {
        var result = values
        result[id] = value
        if result.count > limit {
            for key in result.keys.sorted() where !live.contains(key) && key != id && result.count > limit {
                result[key] = nil
            }
        }
        return result
    }

    // MARK: Clawds

    /// A quiet session dozes off once it has done nothing this long; before
    /// that it stays awake, as after the person reads what it did.
    static let dozeAfter: TimeInterval = 5 * 60

    /// What a session's Clawd wears: what the person chose for it, or the
    /// lot its own figure draws, Clawd's or Codex's robot's.
    static func costume(for session: ClaudeSession, kept: [String: String]) -> ClawdCostume {
        ClawdCostume.resolved(key: session.id, kept: kept[session.id], character: session.provider.character)
    }

    /// What a session's Clawd does in its phase.
    static func motion(_ session: ClaudeSession, now: Date) -> NotchClawdMotion {
        switch session.phase {
        case .generating: return .think
        case .runningTool: return .work
        case .background: return .slow
        case .waitingPermission: return .alert
        case .waitingInput: return .ask
        case .finished: return .content
        case .idle: return dozes(session, now: now) ? .doze : .rest
        }
    }

    /// A session quiet for long: its Clawd sleeps and its row dims. Opening
    /// its page counts as something happening, so a result read long after
    /// it came is not read beside a sleeping Clawd.
    static func dozes(_ session: ClaudeSession, now: Date) -> Bool {
        guard session.phase == .idle else { return false }
        let last = max(session.lastActive ?? session.phaseSince, session.lookedAt ?? .distantPast)
        return now.timeIntervalSince(last) >= dozeAfter
    }

    /// How far a sleeping session's row fades: a little, so the ones at work
    /// stand out and the quiet ones stay readable.
    static let dozingOpacity = 0.65

    /// When the session's work ended, which its Clawd hops for. Counted from
    /// the phase's own start, so every Clawd of the session on screen hops
    /// as one, and one drawn once the hop is over does not hop again.
    static func cheer(_ session: ClaudeSession) -> Date? {
        session.phase == .finished ? session.phaseSince : nil
    }

    /// What a session's Clawd does in a notice: the notice's own symbol says
    /// what happened, so the Clawd matches its words.
    static func noticeMotion(symbol: String) -> NotchClawdMotion {
        switch symbol {
        case self.symbol(.finished): return .content
        case self.symbol(.waitingInput): return .ask
        default: return .alert
        }
    }

    /// A subagent works while it runs and rests, done, after.
    static func motion(_ agent: ClaudeSubagent) -> NotchClawdMotion {
        agent.isRunning ? .work : .done
    }

    /// The key a subagent's costume is drawn from.
    static func costumeKey(_ agent: ClaudeSubagent, in session: ClaudeSession) -> String {
        session.id + "/" + agent.id
    }

    /// What Escape closes on the Sessions page: the wardrobe, then a
    /// session's page, and with neither open, the island itself.
    enum PageLayer: Equatable { case wardrobe, session }

    static func pageLayer(focused: String?, dressing: String?) -> PageLayer? {
        if dressing != nil, dressing == focused { return .wardrobe }
        return focused == nil ? nil : .session
    }

    /// What the conversation says Claude is doing while a turn runs, picked
    /// once per turn as Claude Code's own spinner does.
    static let thinkingWords = [
        "Thinking", "Pondering", "Brewing", "Musing", "Noodling", "Percolating", "Simmering", "Tinkering",
        "Mulling", "Crafting", "Conjuring", "Pontificating", "Ruminating", "Churning", "Whirring", "Cogitating",
    ]
    static func thinkingWord(turn: Date?) -> String {
        let seed = Int((turn?.timeIntervalSinceReferenceDate ?? 0).rounded(.down))
        return thinkingWords[((seed % thinkingWords.count) + thinkingWords.count) % thinkingWords.count]
    }

    /// The spinner's glyphs, played forward and back.
    static let thinkingGlyphs = ["·", "✢", "✳", "✶", "✻", "✽"]
    static func thinkingGlyph(step: Int) -> String {
        let cycle = thinkingGlyphs.count * 2 - 2
        let place = ((step % cycle) + cycle) % cycle
        return thinkingGlyphs[place < thinkingGlyphs.count ? place : cycle - place]
    }

    /// What the conversation sets apart: each reply a turn ended on, which
    /// sums the turn up, and the replies written on the way to one. A turn
    /// needs no prompt of its own, as one a slash command, a finished task or
    /// a timer began; replies of a turn still running are neither.
    static func answers(in chat: [ClaudeChatMessage]) -> (answers: Set<Int>, onTheWay: Set<Int>) {
        var answers = Set<Int>(), onTheWay = Set<Int>()
        var answered = false
        for message in chat.reversed() {
            if message.role == .user {
                answered = false
            } else if message.endsTurn {
                answers.insert(message.id)
                answered = true
            } else if answered {
                onTheWay.insert(message.id)
            }
        }
        return (answers, onTheWay)
    }

    static let imageLabel = try? NSRegularExpression(pattern: #"\[Image\s*#\d+\]"#, options: [.caseInsensitive])

    /// A prompt's text without the `[Image #1]` labels standing for the
    /// images shown beside it. While any of those images is missing, every
    /// label stays, so nothing sent disappears without a trace.
    static func withoutImageLabels(_ text: String, images: Int) -> String {
        guard images > 0, let label = imageLabel else { return text }
        let range = NSRange(text.startIndex..., in: text)
        guard label.numberOfMatches(in: text, range: range) <= images else { return text }
        return label.stringByReplacingMatches(in: text, range: range, withTemplate: "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Text sizes a session's enlarged page offers.
    static let largeTextSizes: ClosedRange<Double> = 12...24
    static func largeTextSize(_ size: Double) -> Double {
        guard size.isFinite else { return largeTextSizes.lowerBound + 3 }
        return min(max(size.rounded(), largeTextSizes.lowerBound), largeTextSizes.upperBound)
    }

    /// Claude writes Markdown; bold, italics, code and links read as such,
    /// and line breaks stay. A fenced block keeps its lines as written, in
    /// code type; anything else that does not parse shows as written.
    static func markdown(_ text: String) -> AttributedString {
        var result = AttributedString()
        for (index, part) in text.components(separatedBy: "```").enumerated() {
            guard !index.isMultiple(of: 2) else {
                result += inlineMarkdown(part)
                continue
            }
            var code = Substring(part)
            // The fence's own line names the language, which is not shown.
            if let line = code.firstIndex(of: "\n"), !code[..<line].contains(" ") { code = code[code.index(after: line)...] }
            if code.hasSuffix("\n") { code = code.dropLast() }
            var block = AttributedString(code)
            block.inlinePresentationIntent = .code
            result += block
        }
        return linked(result)
    }

    /// Addresses written out, which Markdown leaves as text, become links;
    /// code keeps them as written.
    static func linked(_ text: AttributedString) -> AttributedString {
        var result = text
        let string = String(result.characters)
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return result }
        for match in detector.matches(in: string, range: NSRange(string.startIndex..., in: string)) {
            guard let url = match.url, ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
                  let range = Range(match.range, in: string) else { continue }
            let start = result.index(result.startIndex, offsetByCharacters: string.distance(from: string.startIndex, to: range.lowerBound))
            let end = result.index(start, offsetByCharacters: string.distance(from: range.lowerBound, to: range.upperBound))
            let piece = result[start..<end]
            let taken = piece.runs.contains { $0.link != nil || $0.inlinePresentationIntent?.contains(.code) == true }
            if !taken { result[start..<end].link = url }
        }
        return result
    }

    private static func inlineMarkdown(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        return (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
    }

    /// ⌘Y allows and ⌘N denies, as in the dialogs that ask the same; ⌘T
    /// picks Always.
    static func decision(key: String?, commandOnly: Bool) -> ClaudeHookMessage.Decision? {
        guard commandOnly else { return nil }
        switch key?.lowercased() {
        case "y": return .allow
        case "n": return .deny
        case "t": return .allowAlways
        default: return nil
        }
    }

    // MARK: Always, for one Codex turn

    /// Always on a Codex request allows every request of the turn under
    /// way, this one included. It is given for one turn at a time, so only
    /// to a session at work.
    static func canAutoAllow(_ session: ClaudeSession) -> Bool {
        session.provider == .codex && session.phase != .finished && session.phase != .idle
    }

    /// Whether Always, given at `given` while the turn begun at `turn` ran,
    /// still holds: until that turn ends, the session goes, or another turn
    /// starts, even one whose end the island missed. A turn whose start the
    /// island never saw ends with the first one that starts after it.
    static func keepsAutoAllow(_ session: ClaudeSession?, turn: Date?, given: Date) -> Bool {
        guard let session, canAutoAllow(session) else { return false }
        guard let started = session.turnStarted else { return true }
        return turn.map { $0 == started } ?? (started <= given)
    }

    /// How long Codex may say it waits with no request the island holds
    /// before that shows as a wait again: briefly around a request it
    /// answers, never for one only the terminal can see.
    static let statusOnlyWait: TimeInterval = 3

    /// A wait Always answers, shown as the work it is: the request, and the
    /// moments around it when Codex says it waits with no request listed.
    /// Questions, and requests only the terminal can answer, still wait.
    static func hidesAutoApproval(_ session: ClaudeSession, autoAllowed: Set<String>, now: Date) -> Bool {
        autoAllowed.contains(session.id) && session.provider == .codex && session.phase == .waitingPermission
            && session.waitingFor != CodexSessionState.answerInTerminal && session.permission?.asksQuestions != true
            && (session.permission != nil || now.timeIntervalSince(session.phaseSince) < statusOnlyWait)
    }

    /// The request Always answers for the person, once it settled.
    static func autoAllowRequest(_ session: ClaudeSession, autoAllowed: Set<String>, now: Date) -> String? {
        guard hidesAutoApproval(session, autoAllowed: autoAllowed, now: now) else { return nil }
        return session.permission?.requestID
    }

    /// The sessions as the island shows them: a wait Always answers is
    /// work under way, so no count, notice or card shows a request for it.
    /// What the service answers stays as it came.
    static func presented(_ sessions: [ClaudeSession], autoAllowed: Set<String>, now: Date) -> [ClaudeSession] {
        let hides = { hidesAutoApproval($0, autoAllowed: autoAllowed, now: now) }
        guard sessions.contains(where: hides) else { return sessions }
        return sorted(sessions.map { session in
            guard hides(session) else { return session }
            var working = session
            working.phase = session.currentTool != nil ? .runningTool : .generating
            working.phaseSince = session.turnStarted ?? session.phaseSince
            working.permission = nil
            working.waitingFor = nil
            return working
        })
    }

    /// A notice of a wait Always answers is not given.
    static func shows(_ event: ClaudeSessionEvent, hidden: Set<String>) -> Bool {
        switch event {
        case .needsPermission(let id, _), .needsInput(let id): return !hidden.contains(id)
        case .finished: return true
        }
    }

    /// What goes back for Claude's questions: for each, the options picked,
    /// in the order Claude wrote them, and the person's own words, which
    /// take the place of a single choice. Nil until every question has an
    /// answer.
    static func answers(_ questions: [ClaudeQuestion], picked: [Int: Set<String>], typed: [Int: String]) -> [String: [String]]? {
        guard !questions.isEmpty else { return nil }
        var answers: [String: [String]] = [:]
        for (index, question) in questions.enumerated() {
            var chosen = question.options.map(\.label).filter { picked[index]?.contains($0) == true }
            let own = clipped((typed[index] ?? "").trimmingCharacters(in: .whitespacesAndNewlines), utf16: ClaudeQuestion.ownAnswerLimit)
            if !own.isEmpty { chosen = question.multiSelect ? chosen + [own] : [own] }
            if !question.multiSelect { chosen = Array(chosen.prefix(1)) }
            guard !chosen.isEmpty else { return nil }
            answers[question.question] = chosen
        }
        return answers
    }

    /// The text cut to whole characters within a count of UTF-16 units, the
    /// length Claude Code measures.
    static func clipped(_ text: String, utf16 limit: Int) -> String {
        guard text.utf16.count > limit else { return text }
        var used = 0
        var end = text.startIndex
        for index in text.indices {
            let size = text[index].utf16.count
            guard used + size <= limit else { break }
            used += size
            end = text.index(after: index)
        }
        return String(text[..<end])
    }

    /// The reply as it goes to the terminal, or nil when nothing is left.
    static func reply(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(replyLimit))
    }
}

/// Always for Codex turns: which sessions have it, and which requests it
/// answers and when. The service keeps one on its queue and runs what it says.
struct NotchSessionAutoAllow {
    struct Given: Equatable {
        /// The turn it was given in; nil until that turn's start is known.
        var turn: Date?
        let at: Date
    }

    /// Sessions given Always, and when.
    private(set) var turns: [String: Given] = [:]
    /// When each session's request first came, as long as a click or a key
    /// waits on it, so Always waits as long.
    private var seen: [String: (request: String, at: Date)] = [:]
    /// The request each session has an answer on its way for.
    private(set) var scheduled: [String: String] = [:]
    /// Requests already answered, so none is sent twice.
    private var tried: Set<String> = []

    var sessions: Set<String> { Set(turns.keys) }

    /// What to change in the answers on their way.
    struct Plan: Equatable {
        struct Answer: Equatable {
            let id: String
            let request: String
            let after: TimeInterval
        }

        var cancel: [String] = []
        var schedule: [Answer] = []
    }

    /// Turns Always on for a session at work, or off; with nil it turns
    /// over. False when nothing changed.
    mutating func set(_ id: String, on: Bool?, session: ClaudeSession?, now: Date) -> Bool {
        if on ?? (turns[id] == nil) {
            guard turns[id] == nil, let session, session.id == id, NotchClaudeSessionsSupport.canAutoAllow(session)
            else { return false }
            turns[id] = Given(turn: session.turnStarted, at: now)
            // A request Codex refused before is tried again, or it would stay hidden unanswered.
            tried = tried.filter { !$0.hasPrefix(id + "\n") }
            return true
        }
        return turns.removeValue(forKey: id) != nil
    }

    /// Takes in the sessions as they came: a turn that ended, even unseen
    /// between two readings, takes Always with it, and each request it
    /// covers is answered once it settled.
    mutating func update(_ sessions: [ClaudeSession], now: Date) -> Plan {
        for session in sessions {
            let request = session.permission?.requestID
            if seen[session.id]?.request != request { seen[session.id] = request.map { ($0, now) } }
        }
        let listed = Set(sessions.map(\.id))
        seen = seen.filter { listed.contains($0.key) }
        turns = turns.filter { id, given in
            NotchClaudeSessionsSupport.keepsAutoAllow(sessions.first { $0.id == id }, turn: given.turn, given: given.at)
        }
        for (id, given) in turns where given.turn == nil {
            turns[id]?.turn = sessions.first { $0.id == id }?.turnStarted
        }
        let allowed = self.sessions
        var due: [String: String] = [:]
        for session in sessions {
            if let request = NotchClaudeSessionsSupport.autoAllowRequest(session, autoAllowed: allowed, now: now) {
                due[session.id] = request
            }
        }
        var plan = Plan()
        for (id, request) in scheduled.sorted(by: { $0.key < $1.key }) where due[id] != request {
            scheduled[id] = nil
            plan.cancel.append(id)
        }
        let live = Set(sessions.compactMap { session in session.permission.map { Self.key(session.id, $0.requestID) } })
        tried.formIntersection(live)
        for (id, request) in due.sorted(by: { $0.key < $1.key })
        where scheduled[id] == nil && !tried.contains(Self.key(id, request)) {
            let waited = now.timeIntervalSince(seen[id]?.at ?? now)
            scheduled[id] = request
            plan.schedule.append(.init(id: id, request: request,
                                       after: max(0, NotchClaudeSessionsSupport.settleDelay - waited)))
        }
        return plan
    }

    /// When an answer's time comes: whether the request still waits and
    /// Always still holds. One that does is not answered again.
    mutating func fire(_ id: String, request: String, in sessions: [ClaudeSession], now: Date) -> Bool {
        guard scheduled[id] == request else { return false }
        scheduled[id] = nil
        guard let session = sessions.first(where: { $0.id == id }),
              NotchClaudeSessionsSupport.autoAllowRequest(session, autoAllowed: self.sessions, now: now) == request
        else { return false }
        tried.insert(Self.key(id, request))
        return true
    }

    /// An answer Codex did not take ends Always: the person decides from here.
    mutating func failed(_ id: String) {
        turns[id] = nil
    }

    private static func key(_ id: String, _ request: String) -> String { id + "\n" + request }
}
