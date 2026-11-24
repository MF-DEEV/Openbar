// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// One event Openbar heard from a session, as a diagnostics file shows it:
/// what kind of event, from which agent, when, and how the session's state
/// changed. Never what was said or run: a tool's name at most.
struct DiagnosticsEvent: Codable, Equatable {
    let time: String
    let agent: String
    let event: String
    var detail: String?
    let session: String
    var from: String?
    var to: String?
}

/// The last events heard from sessions, in memory, for a diagnostics file
/// (#230). Each also goes into the day's log as a line.
final class DiagnosticsJournal {
    static let shared = DiagnosticsJournal()
    static let capacity = 200

    private let lock = NSLock()
    private var items: [DiagnosticsEvent] = []
    private let log: DiagnosticsLog?

    init(log: DiagnosticsLog? = .shared) {
        self.log = log
    }

    func record(_ event: DiagnosticsEvent, at date: Date = Date()) {
        lock.lock()
        items.append(event)
        if items.count > Self.capacity { items.removeFirst(items.count - Self.capacity) }
        lock.unlock()
        var line = "\(event.agent) \(event.event)"
        if let detail = event.detail { line += " (\(detail))" }
        line += " session \(event.session)"
        if event.from != event.to { line += " \(event.from ?? "none") -> \(event.to ?? "none")" }
        log?.write(.info, "events", line, now: date)
    }

    /// The events kept, oldest first.
    func recent() -> [DiagnosticsEvent] {
        lock.lock()
        defer { lock.unlock() }
        return items
    }

    // MARK: Reading events

    static func make(agent: ClaudeSessionProvider, event: (name: String, detail: String?), session: String,
                     from: ClaudeSessionPhase?, to: ClaudeSessionPhase?, at date: Date) -> DiagnosticsEvent {
        DiagnosticsEvent(time: DiagnosticsLog.timestamp(date), agent: agent.rawValue, event: event.name,
                         detail: event.detail, session: shortSession(session),
                         from: from?.rawValue, to: to?.rawValue)
    }

    /// A Claude Code hook's kind, and the tool or notice it names.
    static func label(_ event: ClaudeHookFact.Event) -> (name: String, detail: String?) {
        switch event {
        case .sessionStarted: return ("sessionStarted", nil)
        case .sessionEnded: return ("sessionEnded", nil)
        case .promptSubmitted: return ("promptSubmitted", nil)
        case .toolStarted(let tool): return ("toolStarted", safe(tool.name))
        case .toolFinished(_, let failed): return ("toolFinished", failed ? "failed" : nil)
        case .permissionRequested(let permission): return ("permissionRequested", safe(permission.tool))
        case .notification(let type): return ("notification", safe(type))
        case .stopped: return ("stopped", nil)
        case .subagentStarted(_, let type): return ("subagentStarted", safe(type))
        case .subagentStopped: return ("subagentStopped", nil)
        case .other(let name): return ("other", safe(name))
        }
    }

    /// Another agent's event kind, and the tool it names.
    static func label(_ kind: AgentHookEvent.Kind) -> (name: String, detail: String?) {
        switch kind {
        case .started: return ("sessionStarted", nil)
        case .ended: return ("sessionEnded", nil)
        case .prompt: return ("promptSubmitted", nil)
        case .working: return ("working", nil)
        case .toolStarted(let tool): return ("toolStarted", safe(tool.name))
        case .toolFinished(_, let name, _, let failed, _): return ("toolFinished", safe(failed ? "\(name):failed" : name))
        case .permission(let permission): return ("permissionRequested", safe(permission.tool))
        case .heldPermission(let permission): return ("permissionRequested", safe(permission.tool))
        case .permissionSettled: return ("permissionSettled", nil)
        case .question: return ("question", nil)
        case .reply: return ("reply", nil)
        case .stopped(_, let failed): return ("stopped", failed ? "failed" : nil)
        case .model: return ("model", nil)
        case .mayAsk: return ("mayAsk", nil)
        case .title: return ("title", nil)
        case .usage: return ("usage", nil)
        case .client: return ("client", nil)
        }
    }

    /// A name made only of what tool and event names are made of; anything
    /// else (a sentence, a path) is left out.
    static func safe(_ text: String?) -> String? {
        guard let text, !text.isEmpty, text.count <= 64 else { return nil }
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_.:-"))
        guard text.unicodeScalars.allSatisfy({ allowed.contains($0) && $0.isASCII }) else { return nil }
        return text
    }

    /// The first characters of the agent's own id for a session: enough to
    /// tell sessions apart in one file.
    static func shortSession(_ id: String) -> String {
        let own = id.split(separator: ":", maxSplits: 1).count == 2
            && ClaudeSessionProvider(rawValue: String(id.split(separator: ":")[0])) != nil
            ? String(id.split(separator: ":", maxSplits: 1)[1]) : id
        let kept = own.unicodeScalars.filter { CharacterSet.alphanumerics.contains($0) && $0.isASCII }
        return String(String.UnicodeScalarView(kept.prefix(8)))
    }
}
