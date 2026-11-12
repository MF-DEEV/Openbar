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
    /// What each session waited on at the last reading; nil before the first.
    private var known: [String: NotchSessionAttention.Request]?

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
