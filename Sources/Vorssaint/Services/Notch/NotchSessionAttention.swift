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
    /// Requests held back while another app was in front, by session.
    private(set) var deferred: [String: String] = [:]
    /// The request each session last had presented, so a repeated reading
    /// does not bring back what the person already saw or dismissed.
    private var presented: [String: String] = [:]

    /// What the session waits on, told apart from the request before it:
    /// a permission by its id, any other prompt by when it began.
    static func request(of session: ClaudeSession) -> String? {
        switch session.phase {
        case .waitingPermission, .waitingInput:
            return session.permission?.requestID
                ?? "\(session.phase.rawValue):\(session.phaseSince.timeIntervalSinceReferenceDate)"
        default:
            return nil
        }
    }

    /// Whether the request is to be presented now. One that may not be is
    /// held until presenting is allowed again.
    mutating func handle(_ id: String, request: String, allowed: Bool) -> Bool {
        guard allowed else {
            deferred[id] = request
            // Back at the desktop, a request still waiting is shown again.
            presented[id] = nil
            return false
        }
        deferred[id] = nil
        guard presented[id] != request else { return false }
        presented[id] = request
        return true
    }

    /// Reconciles with the requests the sessions wait on now, by session,
    /// and returns the sessions whose request is to be presented now.
    mutating func sync(_ waiting: [String: String], allowed: Bool) -> [String] {
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
