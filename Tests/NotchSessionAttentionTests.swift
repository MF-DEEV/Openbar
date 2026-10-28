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
        suite.run("session attention names one prompt once") { naming(suite) }
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

    static func r(_ id: String, wait: String = "w") -> NotchSessionAttention.Request {
        NotchSessionAttention.Request(wait: wait, id: id)
    }

    static func requests(_ suite: TestSuite) {
        suite.expect(NotchSessionAttention.request(of: session("a", .waitingPermission, request: "r1"))?.id == "r1",
                     "a permission is named by its request")
        let input = NotchSessionAttention.request(of: session("a", .waitingInput, since: 10))
        suite.expect(input != nil && input != NotchSessionAttention.request(of: session("a", .waitingInput, since: 20)),
                     "a prompt without a request is told apart by when it began")
        suite.expect(NotchSessionAttention.request(of: session("a", .generating)) == nil
                     && NotchSessionAttention.request(of: session("a", .finished, request: "r1")) == nil,
                     "a session not waiting asks for nothing")
    }

    static func defers(_ suite: TestSuite) {
        var attention = NotchSessionAttention()
        suite.expect(!attention.handle("a", request: r("r1"), allowed: false), "another app in front holds the request")
        suite.expect(attention.deferred == ["a": r("r1")], "and keeps it for later")
        suite.expect(attention.sync(["a": r("r1")], allowed: true) == ["a"], "back at the desktop, it is presented")
        suite.expect(attention.deferred.isEmpty, "and no longer held")
        suite.expect(!attention.handle("a", request: r("r1"), allowed: true)
                     && attention.sync(["a": r("r1")], allowed: true).isEmpty,
                     "the same request is not presented twice while the person stays")
        suite.expect(attention.handle("a", request: r("r2"), allowed: true), "the session's next request is")
        // Leaving and coming back brings a request that still waits back.
        suite.expect(attention.sync(["a": r("r2")], allowed: false).isEmpty, "leaving presents nothing")
        suite.expect(attention.sync(["a": r("r2")], allowed: true) == ["a"], "returning presents what still waits")
    }

    static func reconciles(_ suite: TestSuite) {
        var attention = NotchSessionAttention()
        _ = attention.handle("a", request: r("r1"), allowed: false)
        _ = attention.handle("b", request: r("r2"), allowed: false)
        // "a" was answered in its terminal meanwhile; "b" moved on to a new request.
        suite.expect(attention.sync(["b": r("r3")], allowed: true) == ["b"],
                     "only what still waits is presented, as it is now: \(attention.deferred)")
        suite.expect(attention.deferred.isEmpty, "an answered request is forgotten")
        suite.expect(attention.sync(["c": r("r4"), "d": r("r5")], allowed: true) == ["c", "d"],
                     "a request from before launch, whose event never came, is presented")
        suite.expect(attention.sync([:], allowed: true).isEmpty
                     && attention.sync(["c": r("r4")], allowed: true) == ["c"],
                     "a session gone and back with the same request starts afresh")
    }

    static func naming(_ suite: TestSuite) {
        let unnamed = NotchSessionAttention.Request(wait: "waitingPermission:5", id: nil)
        let named = NotchSessionAttention.Request(wait: "waitingPermission:5", id: "r1")
        var attention = NotchSessionAttention()
        suite.expect(attention.sync(["a": unnamed], allowed: true) == ["a"],
                     "the terminal's record of a prompt presents it before the hook asks")
        suite.expect(attention.sync(["a": named], allowed: true).isEmpty,
                     "the hook naming the same prompt does not present it again")
        suite.expect(attention.sync(["a": unnamed], allowed: true).isEmpty,
                     "nor does the record outliving the hook's answer")
        suite.expect(attention.sync(["a": NotchSessionAttention.Request(wait: "waitingPermission:5", id: "r2")],
                                    allowed: true) == ["a"],
                     "a second named prompt in the same wait is a new one")
        suite.expect(attention.sync(["a": NotchSessionAttention.Request(wait: "waitingPermission:9", id: nil)],
                                    allowed: true) == ["a"],
                     "a later wait is a new prompt")
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
