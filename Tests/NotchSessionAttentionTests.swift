// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Carbon.HIToolbox
import CoreGraphics
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
        suite.run("session waiting sound") { waitingSound(suite) }
        suite.run("session waiting sound reads the terminal") { terminalFocus(suite) }
    }

    /// #210: quiet only when the session's own tab or pane is in front.
    static func terminalFocus(_ suite: TestSuite) {
        suite.expect(!NotchSessionWaitingSound.shouldPlay([.shown]), "the session's own tab in front stays quiet")
        suite.expect(NotchSessionWaitingSound.shouldPlay([.hidden]), "another tab or app plays")
        suite.expect(NotchSessionWaitingSound.shouldPlay([.unknown]), "a terminal that can't tell plays")
        suite.expect(NotchSessionWaitingSound.shouldPlay([.shown, .hidden]), "one unseen request among several plays")

        suite.expect(ClaudeTerminalFocus.sameTTY("/dev/ttys004\n", "ttys004")
                     && ClaudeTerminalFocus.sameTTY("ttys004", "/dev/ttys004"), "a device matches written either way")
        suite.expect(!ClaudeTerminalFocus.sameTTY("/dev/ttys004", "/dev/ttys040")
                     && !ClaudeTerminalFocus.sameTTY("", "/dev/"), "another device, or none, never matches")

        suite.expect(ClaudeTerminalFocus.tmuxActive("11\n"), "the active pane of the active window")
        suite.expect(!ClaudeTerminalFocus.tmuxActive("10") && !ClaudeTerminalFocus.tmuxActive("01")
                     && !ClaudeTerminalFocus.tmuxActive(""), "another pane or window is not in front")

        let clients = Data("""
            [{"pid":10,"idle_time":{"secs":0,"nanos":1000},"focused_pane_id":null},
             {"pid":11,"idle_time":{"secs":40,"nanos":0},"focused_pane_id":7},
             {"pid":12,"idle_time":{"secs":2,"nanos":0},"focused_pane_id":3}]
            """.utf8)
        suite.expect(ClaudeTerminalFocus.wezTermFocusedPane(clients) == 3,
                     "WezTerm's pane is the one of the client used last, the command line aside")
        suite.expect(ClaudeTerminalFocus.wezTermFocusedPane(Data("[]".utf8)) == nil
                     && ClaudeTerminalFocus.wezTermFocusedPane(Data("nope".utf8)) == nil, "no client, no pane")

        let kitty = Data("""
            [{"is_focused":true,"tabs":[
                {"is_focused":true,"windows":[{"id":1,"is_focused":true},{"id":2,"is_focused":false}]},
                {"is_focused":false,"windows":[{"id":3,"is_focused":false}]}]},
             {"is_focused":false,"tabs":[{"is_focused":true,"windows":[{"id":4,"is_focused":true}]}]}]
            """.utf8)
        suite.expect(ClaudeTerminalFocus.kittyFocused(kitty, window: 1) == true, "kitty's focused window shows")
        suite.expect(ClaudeTerminalFocus.kittyFocused(kitty, window: 2) == false
                     && ClaudeTerminalFocus.kittyFocused(kitty, window: 3) == false,
                     "another window or tab of the focused OS window doesn't")
        suite.expect(ClaudeTerminalFocus.kittyFocused(kitty, window: 4) == false,
                     "the active window of an OS window in the back doesn't")
        suite.expect(ClaudeTerminalFocus.kittyFocused(kitty, window: 9) == nil, "a window kitty doesn't list is unknown")

        let windows: [[String: Any]] = [
            [kCGWindowLayer as String: 25, kCGWindowOwnerPID as String: 1,
             kCGWindowBounds as String: ["Width": 200.0, "Height": 30.0]],
            [kCGWindowLayer as String: 0, kCGWindowOwnerPID as String: 2,
             kCGWindowBounds as String: ["Width": 1.0, "Height": 1.0]],
            [kCGWindowLayer as String: 0, kCGWindowOwnerPID as String: 3,
             kCGWindowBounds as String: ["Width": 800.0, "Height": 600.0]],
        ]
        suite.expect(ClaudeTerminalFocus.frontWindowOwner(windows) == 3,
                     "the front window is the first ordinary one with a size, not a panel or a sliver")

        for host in [ClaudeSessionHost.terminal(tty: "/dev/ttys001"), .iterm(tty: "/dev/ttys001"), .ghostty(tty: "/dev/ttys001")] {
            let script = ClaudeTerminalFocus.frontTTYScript(host) ?? ""
            suite.expect(script.contains("with timeout of 1 seconds") && script.contains("tty of"),
                         "\(host) is asked for its front tab's device, briefly")
        }
        suite.expect(ClaudeTerminalFocus.frontTTYScript(.app("dev.warp.Warp-Stable")) == nil,
                     "an app with no way to tell has no script")
        suite.expect(ClaudeTerminalFocus.state(.app("dev.warp.Warp-Stable"), pid: 1, hostOf: { _ in .unknown }) == .unknown
                     && ClaudeTerminalFocus.state(.ide(bundle: "com.microsoft.VSCode", port: nil), pid: 1, hostOf: { _ in .unknown }) == .unknown
                     && ClaudeTerminalFocus.state(.remote(alias: "box", tmux: true), pid: 1, hostOf: { _ in .unknown }) == .unknown,
                     "editors, other apps and remote hosts can't tell")
    }

    static func waitingSound(_ suite: TestSuite) {
        var sound = NotchSessionWaitingSound()
        let first = NotchSessionAttention.Request(wait: "waitingPermission:1", id: nil)
        suite.expect(sound.arrived(["a": first]).isEmpty, "a request already waiting at launch stays quiet")
        suite.expect(sound.arrived(["a": first]).isEmpty, "the same reading again plays nothing")
        let named = NotchSessionAttention.Request(wait: "waitingPermission:1", id: "r1")
        suite.expect(sound.arrived(["a": named]).isEmpty, "the hook naming the prompt the record told of is not a new one")
        let second = NotchSessionAttention.Request(wait: "waitingPermission:1", id: "r2")
        suite.expect(sound.arrived(["a": second]) == ["a"], "the next prompt in the same wait plays")
        suite.expect(sound.arrived(["a": NotchSessionAttention.Request(wait: "waitingPermission:1", id: nil)]).isEmpty,
                     "a reading that lost the name is still the same prompt")
        suite.expect(sound.arrived([:]).isEmpty, "a turn that ends plays nothing")
        let question = NotchSessionAttention.Request(wait: "waitingInput:9", id: nil)
        suite.expect(sound.arrived(["a": question, "b": first]) == ["a", "b"], "each new request plays, once")
        suite.expect(sound.arrived(["a": question, "b": first]).isEmpty, "and not again while it waits")

        suite.expect(NotchSessionWaitingSound.inView(expanded: true, selected: .sessions, focused: nil, session: "a"),
                     "the open list shows every session")
        suite.expect(NotchSessionWaitingSound.inView(expanded: true, selected: .sessions, focused: "a", session: "a")
                     && !NotchSessionWaitingSound.inView(expanded: true, selected: .sessions, focused: "b", session: "a"),
                     "another session's page does not show this one")
        suite.expect(!NotchSessionWaitingSound.inView(expanded: false, selected: .sessions, focused: nil, session: "a")
                     && !NotchSessionWaitingSound.inView(expanded: true, selected: .music, focused: nil, session: "a"),
                     "a closed island or another section shows nothing")

        let defaults = UserDefaults(suiteName: "com.openbar.tests.waitingSound")!
        defaults.removePersistentDomain(forName: "com.openbar.tests.waitingSound")
        defer { defaults.removePersistentDomain(forName: "com.openbar.tests.waitingSound") }
        suite.expect(NotchSessionWaitingSound.isEnabled(in: defaults), "the sound is on by default")
        defaults.set("Nope", forKey: DefaultsKey.notchSessionsWaitingSoundName)
        suite.expect(NotchSessionWaitingSound.soundName(in: defaults, available: ["Glass", "Ping"]) == "Glass",
                     "a sound this Mac lacks falls back to the chime")
        defaults.set("Ping", forKey: DefaultsKey.notchSessionsWaitingSoundName)
        suite.expect(NotchSessionWaitingSound.soundName(in: defaults, available: ["Glass", "Ping"]) == "Ping",
                     "the chosen sound plays")
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
        let own = "com.openbar.app.dev"
        suite.expect(NotchSessionAttention.allowsPresenting(frontmost: "com.apple.finder", own: own),
                     "the desktop allows presenting")
        suite.expect(NotchSessionAttention.allowsPresenting(frontmost: own, own: own), "Openbar itself does")
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
