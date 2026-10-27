// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Carbon.HIToolbox
import Foundation

/// A request waits while another app is in front and takes the island once
/// the person is back; the sessions on hand decide what still waits.
enum NotchSessionAttentionTests {
    static func run(_ suite: TestSuite) {
        suite.run("session attention requests") { requests(suite) }
        suite.run("session attention defers and resumes") { defers(suite) }
        suite.run("session attention reconciles") { reconciles(suite) }
        suite.run("session attention context") { context(suite) }
        suite.run("session answer keys follow the layout") { keys(suite) }
    }

    static func session(_ id: String, _ phase: ClaudeSessionPhase, request: String? = nil,
                        since: TimeInterval = 0) -> ClaudeSession {
        var session = NotchSessionAgentTests.session(id, phase, provider: .claude)
        session.phaseSince = Date(timeIntervalSinceReferenceDate: since)
        session.permission = request.map {
            ClaudePermission(requestID: $0, tool: "Bash", summary: nil, offersAlways: false, since: session.phaseSince)
        }
        return session
    }

    static func requests(_ suite: TestSuite) {
        suite.expect(NotchSessionAttention.request(of: session("a", .waitingPermission, request: "r1")) == "r1",
                     "a permission is told apart by its request")
        let input = NotchSessionAttention.request(of: session("a", .waitingInput, since: 10))
        suite.expect(input != nil && input != NotchSessionAttention.request(of: session("a", .waitingInput, since: 20)),
                     "a prompt without a request is told apart by when it began")
        suite.expect(NotchSessionAttention.request(of: session("a", .generating)) == nil
                     && NotchSessionAttention.request(of: session("a", .finished, request: "r1")) == nil,
                     "a session not waiting asks for nothing")
    }

    static func defers(_ suite: TestSuite) {
        var attention = NotchSessionAttention()
        suite.expect(!attention.handle("a", request: "r1", allowed: false), "another app in front holds the request")
        suite.expect(attention.deferred == ["a": "r1"], "and keeps it for later")
        suite.expect(attention.sync(["a": "r1"], allowed: true) == ["a"], "back at the desktop, it is presented")
        suite.expect(attention.deferred.isEmpty, "and no longer held")
        suite.expect(!attention.handle("a", request: "r1", allowed: true)
                     && attention.sync(["a": "r1"], allowed: true).isEmpty,
                     "the same request is not presented twice while the person stays")
        suite.expect(attention.handle("a", request: "r2", allowed: true), "the session's next request is")
        // Leaving and coming back brings a request that still waits back.
        suite.expect(attention.sync(["a": "r2"], allowed: false).isEmpty, "leaving presents nothing")
        suite.expect(attention.sync(["a": "r2"], allowed: true) == ["a"], "returning presents what still waits")
    }

    static func reconciles(_ suite: TestSuite) {
        var attention = NotchSessionAttention()
        _ = attention.handle("a", request: "r1", allowed: false)
        _ = attention.handle("b", request: "r2", allowed: false)
        // "a" was answered in its terminal meanwhile; "b" moved on to a new request.
        suite.expect(attention.sync(["b": "r3"], allowed: true) == ["b"],
                     "only what still waits is presented, as it is now: \(attention.deferred)")
        suite.expect(attention.deferred.isEmpty, "an answered request is forgotten")
        suite.expect(attention.sync(["c": "r4", "d": "r5"], allowed: true) == ["c", "d"],
                     "a request from before launch, whose event never came, is presented")
        suite.expect(attention.sync([:], allowed: true).isEmpty
                     && attention.sync(["c": "r4"], allowed: true) == ["c"],
                     "a session gone and back with the same request starts afresh")
    }

    static func context(_ suite: TestSuite) {
        let own = "com.vorssaint.utils.dev"
        suite.expect(NotchSessionAttention.allowsPresenting(frontmost: "com.apple.finder", own: own),
                     "the desktop allows presenting")
        suite.expect(NotchSessionAttention.allowsPresenting(frontmost: own, own: own), "Vorssaint itself does")
        suite.expect(!NotchSessionAttention.allowsPresenting(frontmost: "com.apple.Terminal", own: own)
                     && !NotchSessionAttention.allowsPresenting(frontmost: nil, own: own),
                     "another app, or none known, does not")
    }

    static func keys(_ suite: TestSuite) {
        // QWERTZ swaps Y and Z: the key that types Y is the US Z.
        let qwertz: (Int64) -> String? = { code in
            switch Int(code) {
            case kVK_ANSI_Z: return "Y"
            case kVK_ANSI_Y: return "Z"
            case kVK_ANSI_N: return "N"
            default: return nil
            }
        }
        suite.expect(NotchClaudeSessionsSupport.commandShortcut(typing: "y", fallback: kVK_ANSI_Y, label: qwertz)
                     == GlobalShortcut(keyCode: Int64(kVK_ANSI_Z), modifiers: .command),
                     "⌘Y is the key that types Y")
        suite.expect(NotchClaudeSessionsSupport.commandShortcut(typing: "n", fallback: kVK_ANSI_N, label: qwertz)
                     == GlobalShortcut(keyCode: Int64(kVK_ANSI_N), modifiers: .command),
                     "⌘N stays where N is")
        suite.expect(NotchClaudeSessionsSupport.commandShortcut(typing: "y", fallback: kVK_ANSI_Y, label: { _ in nil })
                     == GlobalShortcut(keyCode: Int64(kVK_ANSI_Y), modifiers: .command),
                     "without a layout to read, the US position is used")
    }
}
