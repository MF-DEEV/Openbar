// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit

/// A sound once when a session starts waiting for the person (#167): for
/// each new permission, question or plan, never for a turn that just ended.
/// Requests already waiting when Openbar starts stay quiet, and so does one
/// whose page, or the list holding it, is already open, or whose own tab is
/// in front in its terminal (#210). The same as Windows'
/// `sessions/waitingSound.ts`.
struct NotchSessionWaitingSound {
    private var eventsInitialized = false
    private var seenEvents: Set<String> = []
    private var eventOrder: [String] = []
    private let since: Date
    init(since: Date = Date()) { self.since = since }
    /// What each session waited on at the last reading; nil before the first.
    private var known: [String: NotchSessionAttention.Request]?

    struct Event: Equatable {
        let session: String
        let kind: SessionSoundKind
        let id: String
        let at: Date
    }

    /// The same six events as sessions/waitingSound.ts. The service has already
    /// folded subagent requests into the parent's row; children never start an
    /// independent notification. Restored sessions are baseline, not new starts.
    mutating func events(_ sessions: [ClaudeSession]) -> [Event] {
        let requests = Dictionary(sessions.compactMap { session in
            NotchSessionAttention.request(of: session).map { (session.id, $0) }
        }, uniquingKeysWith: { first, _ in first })
        let fresh = eventsInitialized ? Set(arrived(requests)) : Set(requests.keys)
        if !eventsInitialized { _ = arrived(requests) }
        var candidates: [Event] = []
        for session in sessions {
            if fresh.contains(session.id), let request = requests[session.id] {
                let kind: SessionSoundKind = session.phase == .waitingInput || session.permission?.asksQuestions == true ? .question : .permission
                candidates.append(Event(session: session.id, kind: kind, id: request.id ?? request.wait, at: session.permission?.since ?? session.phaseSince))
            }
            if let at = session.startedAt {
                candidates.append(Event(session: session.id, kind: .started, id: String(at.timeIntervalSince1970), at: at))
            }
            if let terminal = Self.terminal(session) { candidates.append(terminal) }
        }
        var result: [Event] = []
        for event in candidates {
            let key = "\(event.session.utf8.count):\(event.session):\(event.kind.rawValue):\(event.id)"
            guard seenEvents.insert(key).inserted else { continue }
            eventOrder.append(key)
            if eventOrder.count > 4096 { seenEvents.remove(eventOrder.removeFirst()) }
            if eventsInitialized && event.at >= since { result.append(event) }
        }
        eventsInitialized = true
        return result
    }

    static func terminal(_ session: ClaudeSession) -> Event? {
        guard !session.phase.isWorking, !session.phase.isWaiting else { return nil }
        let began = session.turnStarted ?? .distantPast
        if session.phase == .limited, let stop = session.limitStop, stop.at >= began {
            return Event(session: session.id, kind: .limit, id: String(stop.at.timeIntervalSince1970), at: stop.at)
        }
        if let failed = session.failedAt, failed >= began, let ended = session.turnEndedAt, failed <= ended {
            return Event(session: session.id, kind: .error, id: String(failed.timeIntervalSince1970), at: failed)
        }
        if session.turnCompleted, let ended = session.turnEndedAt, ended >= began {
            return Event(session: session.id, kind: .finished, id: String(ended.timeIntervalSince1970), at: ended)
        }
        return nil
    }

    /// The sessions whose request is new since the last reading. The hook
    /// and the terminal's record can tell of one prompt a moment apart, so a
    /// prompt that only gains its name is not new (`Request.isSame`).
    mutating func arrived(_ waiting: [String: NotchSessionAttention.Request]) -> [String] {
        guard let before = known else {
            known = waiting
            return []
        }
        var next: [String: NotchSessionAttention.Request] = [:]
        var new: [String] = []
        for (id, request) in waiting {
            if let old = before[id], old.isSame(as: request) {
                // Keep the name once known, so the next prompt of the same wait is told apart.
                next[id] = old.id == nil ? request : old
            } else {
                next[id] = request
                new.append(id)
            }
        }
        known = next
        return new.sorted()
    }

    /// Whether the person already looks at the session's page or at the list.
    static func inView(expanded: Bool, selected: NotchModule, focused: String?, session: String) -> Bool {
        expanded && selected == .sessions && (focused == nil || focused == session)
    }

    /// Whether to play for the new requests the island doesn't show, by what
    /// their terminals say (#210): quiet only when every one of them is in
    /// front in its own tab or pane. A terminal that can't tell plays.
    static func shouldPlay(_ terminals: [ClaudeTerminalFocus.State]) -> Bool {
        terminals.contains { $0 != .shown }
    }

    /// The longest the terminals are asked before the sound plays anyway.
    static let terminalCheckLimit: TimeInterval = 1.5

    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        defaults.object(forKey: DefaultsKey.notchSessionsWaitingSound) as? Bool ?? true
    }

    /// The timer's chime, so Openbar sounds the same for both.
    static let defaultSound = "Glass"

    /// The system sound to play: the one chosen if this Mac has it, else the chime.
    static func soundName(in defaults: UserDefaults = .standard,
                          available: [String] = TextSnippetSupport.alertSoundNames) -> String? {
        TextSnippetSupport.resolvedSoundName(stored: defaults.string(forKey: DefaultsKey.notchSessionsWaitingSoundName),
                                             available: available, fallback: defaultSound)
    }

    static func play(_ name: String?) {
        guard let name else { return }
        let path = TextSnippetSupport.systemSoundsPath + "/" + name + ".aiff"
        NSSound(contentsOfFile: path, byReference: true)?.play()
    }
}
