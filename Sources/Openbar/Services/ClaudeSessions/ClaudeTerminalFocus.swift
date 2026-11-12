// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import CoreGraphics
import Foundation

/// Whether the person is looking at a session in its own terminal: its app in
/// front, that app's window on screen, and in it the very tab or pane the
/// session runs in (#210). The waiting sound keeps quiet only then. A terminal
/// that can't say which tab is in front (editors, Warp, any other app) never
/// counts as showing it, so the sound plays as it did before.
enum ClaudeTerminalFocus {
    enum State: Equatable {
        /// The session's own tab or pane is in front and on screen.
        case shown
        /// Something else is: another app, another tab, a hidden window.
        case hidden
        /// The terminal can't tell, or couldn't be asked.
        case unknown
    }

    /// How long one question to a terminal may take before it counts as unknown.
    static let askTimeout: TimeInterval = 1

    /// `pid` is the process on the session's terminal: the session's own, or
    /// for a tmux in front of it, the tmux client's, whose terminal `hostOf`
    /// finds. Off the main thread: it asks the terminal, which takes a moment.
    static func state(_ host: ClaudeSessionHost, pid: Int32, depth: Int = 0,
                      hostOf: (Int32) -> ClaudeSessionHost) -> State {
        switch host {
        case .terminal, .iterm, .ghostty, .wezterm, .kitty:
            guard let bundle = host.bundle, let tty = ClaudeSessionHosts.tty(of: pid) else { return .unknown }
            guard inFront(bundle) else { return .hidden }
            return tab(host, bundle: bundle, pid: pid, tty: tty)
        case .tmux(let target):
            guard depth < 3, ClaudeTmux.holds(target, tty: ClaudeSessionHosts.tty(of: pid)) else { return .unknown }
            guard let flags = ClaudeTmux.run(target, ["display-message", "-p", "-t", target.pane,
                                                      "#{pane_active}#{window_active}"]) else { return .unknown }
            guard tmuxActive(flags) else { return .hidden }
            // The pane is the one its tmux shows; the terminal showing that tmux decides the rest.
            guard let client = ClaudeTmux.client(of: target) else { return .hidden }
            guard !client.elsewhere else { return .hidden }
            return state(hostOf(client.pid), pid: client.pid, depth: depth + 1, hostOf: hostOf)
        case .ide, .app, .background, .remote, .unknown:
            return .unknown
        }
    }

    /// The session's tab, once its app is in front.
    private static func tab(_ host: ClaudeSessionHost, bundle: String, pid: Int32, tty: String) -> State {
        switch host {
        case .terminal, .iterm, .ghostty:
            guard let script = frontTTYScript(host), AppleScriptRunner.consentToAutomate(bundleID: bundle, ask: false) else {
                return .unknown
            }
            let result = AppleScriptRunner.run(script)
            guard result.ok else { return .unknown }
            return sameTTY(result.output, tty) ? .shown : .hidden
        case .wezterm(let pane, let socket):
            guard ClaudeWezTerm.pane(pane, socket: socket, tty: tty) != nil,
                  let listed = ClaudeWezTerm.run(["list-clients", "--format", "json"], socket: socket),
                  let focused = wezTermFocusedPane(Data(listed.utf8)) else { return .unknown }
            return focused == pane ? .shown : .hidden
        case .kitty(let window, let socket):
            // Without remote control kitty can't be asked which window has focus.
            guard let socket, let listed = ClaudeKitty.run(["ls"], socket: socket),
                  let focused = kittyFocused(Data(listed.utf8), window: window) else { return .unknown }
            guard ClaudeKitty.pids(in: Data(listed.utf8), window: window).contains(pid) else { return .unknown }
            return focused ? .shown : .hidden
        default:
            return .unknown
        }
    }

    // MARK: The app in front

    /// The app is in front, the screen unlocked, and a window of that app is
    /// the frontmost window on screen, so not minimised or on another Space.
    static func inFront(_ bundle: String) -> Bool {
        guard !screenLocked(),
              let front = NSWorkspace.shared.frontmostApplication, front.bundleIdentifier == bundle else { return false }
        return frontWindowOwner() == front.processIdentifier
    }

    private static func screenLocked() -> Bool {
        guard let session = CGSessionCopyCurrentDictionary() as? [String: Any] else { return false }
        return session["CGSSessionScreenIsLocked"] as? Bool ?? false
    }

    /// The owner of the frontmost ordinary window on screen.
    private static func frontWindowOwner() -> pid_t? {
        guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                                       kCGNullWindowID) as? [[String: Any]] else { return nil }
        return frontWindowOwner(windows)
    }

    /// The first window of layer 0 with some size; panels, the menu bar and
    /// the island sit on other layers.
    static func frontWindowOwner(_ windows: [[String: Any]]) -> pid_t? {
        for window in windows {
            guard window[kCGWindowLayer as String] as? Int == 0,
                  let bounds = window[kCGWindowBounds as String] as? [String: Any],
                  (bounds["Width"] as? Double ?? 0) > 1, (bounds["Height"] as? Double ?? 0) > 1 else { continue }
            return (window[kCGWindowOwnerPID as String] as? Int).map { pid_t($0) }
        }
        return nil
    }

    // MARK: What each terminal says

    /// A script that answers with the device of the tab in front of the front window.
    static func frontTTYScript(_ host: ClaudeSessionHost) -> String? {
        let ask: String
        switch host {
        case .terminal:
            ask = "tell application id \"\(ClaudeSessionHosts.terminal)\" to return tty of selected tab of front window"
        case .iterm:
            ask = "tell application id \"\(ClaudeTerminalApps.iterm)\" to return tty of current session of current window"
        case .ghostty:
            ask = "tell application id \"\(ClaudeTerminalApps.ghostty)\" to return tty of focused terminal of selected tab of front window"
        default:
            return nil
        }
        return "with timeout of \(Int(askTimeout)) seconds\n\(ask)\nend timeout"
    }

    /// One device, written either way: "/dev/ttys004" or "ttys004".
    static func sameTTY(_ a: String, _ b: String) -> Bool {
        func short(_ tty: String) -> String {
            let trimmed = tty.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.hasPrefix("/dev/") ? String(trimmed.dropFirst(5)) : trimmed
        }
        let first = short(a)
        return !first.isEmpty && first == short(b)
    }

    /// tmux's `#{pane_active}#{window_active}`: the pane is the active one of
    /// its window, and the window the active one of its session.
    static func tmuxActive(_ flags: String) -> Bool {
        flags.trimmingCharacters(in: .whitespacesAndNewlines) == "11"
    }

    /// The pane WezTerm's window has focus on, from `cli list-clients`. The
    /// command line itself is listed as a client with no pane; the client
    /// used last wins among the rest.
    static func wezTermFocusedPane(_ json: Data) -> Int? {
        guard let clients = (try? JSONSerialization.jsonObject(with: json)) as? [[String: Any]] else { return nil }
        return clients
            .compactMap { client -> (pane: Int, idle: Double)? in
                guard let pane = client["focused_pane_id"] as? Int else { return nil }
                return (pane, idleSeconds(client["idle_time"]))
            }
            .min { $0.idle < $1.idle }?.pane
    }

    /// WezTerm writes a duration as `{"secs": 1, "nanos": 5}`.
    private static func idleSeconds(_ value: Any?) -> Double {
        guard let value = value as? [String: Any] else { return .infinity }
        return (value["secs"] as? Double ?? 0) + (value["nanos"] as? Double ?? 0) / 1e9
    }

    /// Whether kitty says the window has focus: its OS window, its tab and
    /// the window itself. Nil when kitty doesn't list it.
    static func kittyFocused(_ json: Data, window: Int) -> Bool? {
        guard let instances = (try? JSONSerialization.jsonObject(with: json)) as? [[String: Any]] else { return nil }
        for instance in instances {
            for tab in instance["tabs"] as? [[String: Any]] ?? [] {
                for entry in tab["windows"] as? [[String: Any]] ?? [] where entry["id"] as? Int == window {
                    return instance["is_focused"] as? Bool == true
                        && tab["is_focused"] as? Bool == true
                        && entry["is_focused"] as? Bool == true
                }
            }
        }
        return nil
    }
}
