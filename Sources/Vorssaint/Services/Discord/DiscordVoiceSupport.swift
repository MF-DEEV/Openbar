// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit

/// What the closed island knows about the microphone: whether an input is
/// recording, whether every recording input is muted on the Mac, and
/// Discord's switches when Discord runs. No audio is ever opened.
struct VoiceChatSnapshot: Equatable {
    var recording = false
    var systemMuted = false
    var discord: DiscordVoiceState?
    /// Whether Discord has been read long enough for a change to be the
    /// person's own, not the island catching up with how Discord already was.
    var settled = false

    static let empty = VoiceChatSnapshot()
}

/// What the island shows: deafened always mutes the microphone too.
struct VoiceChatReading: Hashable {
    let muted: Bool
    let deafened: Bool

    /// Only a muted microphone keeps a live activity; an open one has none.
    var active: Bool { muted }
}

enum DiscordVoiceSupport {
    /// The editions Discord ships, by bundle identifier, with their profile folders.
    static let editions: [String: String] = [
        "com.hnc.Discord": "discord",
        "com.hnc.DiscordCanary": "discordcanary",
        "com.hnc.DiscordPTB": "discordptb",
    ]

    static let mutedSymbol = "mic.slash.fill"
    static let openSymbol = "mic.fill"

    /// The crossed-out headphones, or a crossed-out speaker on a system
    /// whose symbols do not have them.
    static let deafenedSymbol = "headphones.slash"
    static let deafenedFallback = "speaker.slash.fill"

    /// The one this system draws: the headphones arrived after macOS 14.
    static let deafenedMark = NSImage(systemSymbolName: deafenedSymbol, accessibilityDescription: nil) != nil
        ? deafenedSymbol : deafenedFallback

    /// The red dot right of the camera that says the microphone stays muted.
    static let dotSide: CGFloat = 5

    /// The marks sit at an agent mark's size, not the timer's full-height
    /// icon: two symbols side by side at that size read heavier than the
    /// strip around them.
    static func stripMarkSize(height: CGFloat, deafened: Bool) -> CGFloat {
        min(deafened ? 13 : 14, max(8, height - NotchLayout.compactEdgeGap * 2 - 4))
    }

    static let stripMarkSpacing: CGFloat = 5

    /// The wider of the two sides, the marks or the dot, drawn as the strip
    /// draws them, with the timer's clearance from the curve and air beside
    /// the camera.
    static func stripWing(deafened: Bool, in geometry: NotchGeometry) -> CGFloat {
        let provisional = geometry.compactTimerGeometry(showsDownloads: false,
                                                        wing: NotchTimerSupport.stripWingRange.lowerBound)
        let iconSize = stripMarkSize(height: provisional.compactActivityContentHeight, deafened: deafened)
        let marks = NotchKeepAwakeSupport.symbolWidth(mutedSymbol, size: iconSize)
            + (deafened ? NotchKeepAwakeSupport.symbolWidth(deafenedMark, size: iconSize) + stripMarkSpacing : 0)
        let left = marks + provisional.compactActivityEdgeInset(boxHeight: iconSize, radius: iconSize / 2)
        let right = dotSide + provisional.compactActivityEdgeInset(boxHeight: dotSide, radius: dotSide / 2)
        return max(left, right) + NotchTimerSupport.stripCameraGap
    }

    static func isEnabled(in defaults: UserDefaults = .standard) -> Bool {
        NotchSupport.routes(.voiceChat, in: defaults)
    }

    static func reading(_ snapshot: VoiceChatSnapshot) -> VoiceChatReading {
        let deafened = snapshot.discord?.deafened ?? false
        return VoiceChatReading(muted: deafened || snapshot.discord?.muted == true || snapshot.systemMuted,
                                deafened: deafened)
    }

    /// A Discord control's label names what it does when pressed, and its
    /// checked or pressed state, when it has one, says how it is now. A
    /// label it does not know is never taken for an open microphone.
    /// Returns whether the control is the headphones' and whether it is muted.
    static func control(_ name: String, checked: Bool?) -> (headphones: Bool, muted: Bool)? {
        let lowered = name.lowercased()
        guard let label = lowered.split(omittingEmptySubsequences: false, whereSeparator: { $0 == "(" || $0 == "\n" }).first
            .map({ $0.trimmingCharacters(in: .whitespaces) }) else { return nil }
        let headphones: Bool
        let muted: Bool
        switch label {
        case "mute", "wycisz", "wycisz mikrofon": (headphones, muted) = (false, false)
        case "unmute", "wyłącz wyciszenie", "wyłącz wyciszenie mikrofonu": (headphones, muted) = (false, true)
        case "deafen", "wyłącz dźwięk", "wycisz dźwięk", "wycisz słuchawki": (headphones, muted) = (true, false)
        case "undeafen", "włącz dźwięk", "wyłącz wyciszenie dźwięku", "wyłącz wyciszenie słuchawek":
            (headphones, muted) = (true, true)
        default: return nil
        }
        return (headphones, checked ?? muted)
    }

    /// The reading a change announces, or nil when there is nothing to say:
    /// the first reading, Discord as found when it is first read, Discord
    /// going away, which is unknown rather than an unmute, a change that is
    /// the Mac's own mute alone, which its own notice reports, and anything
    /// while the status is off.
    static func announcement(from previous: VoiceChatSnapshot, to next: VoiceChatSnapshot,
                             initial: Bool, enabled: Bool) -> VoiceChatReading? {
        guard enabled, !initial, next.settled, next.discord != nil, previous.discord != next.discord else { return nil }
        let before = reading(previous)
        let after = reading(next)
        return before == after ? nil : after
    }
}
