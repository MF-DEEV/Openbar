// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// When a session's request may take the island. While the person works in
/// another app, a request only adds to the closed island's waiting count;
/// it is presented once they are back at the desktop or in Vorssaint. The
/// sessions on hand are the authority, so a request whose event was missed,
/// or came before launch, is presented on return all the same, and one
/// answered meanwhile never is.
struct NotchSessionAttention {
    /// One prompt a session waits on. The terminal's own record and the
    /// hook can tell of the same prompt a moment apart, the hook naming it
    /// and the record not, so one wait with no name on either side is the
    /// same prompt; two names differ only when both are known.
    struct Request: Equatable {
        /// The wait the prompt belongs to: its phase and when it began.
        let wait: String
        /// The hook's request, once it has asked.
        let id: String?

        func isSame(as other: Request) -> Bool {
            guard wait == other.wait else { return id != nil && id == other.id }
            guard let id, let other = other.id else { return true }
            return id == other
        }
    }

    /// Requests held back while another app was in front, by session.
    private(set) var deferred: [String: Request] = [:]
    /// The request each session last had presented, so a repeated reading
    /// does not bring back what the person already saw or dismissed.
    private var presented: [String: Request] = [:]

    static func request(of session: ClaudeSession) -> Request? {
        switch session.phase {
        case .waitingPermission, .waitingInput:
            return Request(wait: "\(session.phase.rawValue):\(session.phaseSince.timeIntervalSinceReferenceDate)",
                           id: session.permission?.requestID)
        default:
            return nil
        }
    }

    /// Whether the request is to be presented now. One that may not be is
    /// held until presenting is allowed again.
    mutating func handle(_ id: String, request: Request, allowed: Bool) -> Bool {
        guard allowed else {
            deferred[id] = request
            // Back at the desktop, a request still waiting is shown again.
            presented[id] = nil
            return false
        }
        deferred[id] = nil
        if let shown = presented[id], shown.isSame(as: request) {
            // The hook's name for a prompt already shown is kept, so the
            // next prompt in the same wait is told apart from it.
            if shown.id == nil { presented[id] = request }
            return false
        }
        presented[id] = request
        return true
    }

    /// Reconciles with the requests the sessions wait on now, by session,
    /// and returns the sessions whose request is to be presented now.
    mutating func sync(_ waiting: [String: Request], allowed: Bool) -> [String] {
        deferred = deferred.filter { waiting[$0.key] != nil }
        presented = presented.filter { waiting[$0.key] != nil }
        return waiting.keys.sorted().filter { handle($0, request: waiting[$0]!, allowed: allowed) }
    }

    /// The person is at the desktop, whose owner Finder is, or in Vorssaint
    /// itself; any other app in front is work not to be interrupted.
    static func allowsPresenting(frontmost bundleID: String?, own: String?) -> Bool {
        guard let bundleID else { return false }
        return bundleID == own || bundleID == Defaults.finderBundleIdentifier
    }
}
