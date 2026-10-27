// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum DiscordKeyAction: Equatable {
    case toggleMute, toggleDeafen, holdMute
}

/// A key by its macOS virtual key code, or a mouse button by its number.
enum DiscordKey: Hashable {
    case key(UInt16)
    case mouse(Int)
}

struct DiscordKeybind: Equatable {
    let id: String
    let keys: [DiscordKey]
    let action: DiscordKeyAction
}

/// The mute and deafen shortcuts the person set in Discord. They are only
/// observed: nothing is registered, swallowed or forwarded, and Discord goes
/// on handling its own keys.
enum DiscordKeybinds {
    static func parse(_ json: Any) -> [DiscordKeybind] {
        guard let bindings = (json as? [String: Any])?["_state"] as? [String: Any] else { return [] }
        return bindings.compactMap { id, item -> DiscordKeybind? in
            guard let item = item as? [String: Any], item["enabled"] as? Bool != false else { return nil }
            let action: DiscordKeyAction
            switch item["action"] as? String {
            case "TOGGLE_MUTE": action = .toggleMute
            case "TOGGLE_DEAFEN": action = .toggleDeafen
            case "PUSH_TO_MUTE": action = .holdMute
            default: return nil
            }
            guard let shortcut = item["shortcut"] as? [Any] else { return nil }
            var keys: [DiscordKey] = []
            for part in shortcut {
                guard let part = part as? [Any], part.count >= 2,
                      let device = (part[0] as? NSNumber)?.intValue, let code = (part[1] as? NSNumber)?.intValue
                else { return nil }
                let key: DiscordKey
                switch device {
                // On the Mac, Discord saves a key by its USB HID usage.
                case 0, 2:
                    guard let virtual = virtualKey(usage: code) else { return nil }
                    key = .key(virtual)
                case 1:
                    guard (0...4).contains(code) else { return nil }
                    key = .mouse(code)
                default:
                    return nil
                }
                if !keys.contains(key) { keys.append(key) }
            }
            return keys.isEmpty ? nil : DiscordKeybind(id: id, keys: keys, action: action)
        }
        .sorted { $0.id < $1.id }
    }

    /// USB HID keyboard usages (page 7) to the virtual key codes NSEvent reports.
    static func virtualKey(usage: Int) -> UInt16? { usages[usage] }

    private static let usages: [Int: UInt16] = [
        4: 0x00, 5: 0x0B, 6: 0x08, 7: 0x02, 8: 0x0E, 9: 0x03, 10: 0x05, 11: 0x04, 12: 0x22, 13: 0x26,
        14: 0x28, 15: 0x25, 16: 0x2E, 17: 0x2D, 18: 0x1F, 19: 0x23, 20: 0x0C, 21: 0x0F, 22: 0x01, 23: 0x11,
        24: 0x20, 25: 0x09, 26: 0x0D, 27: 0x07, 28: 0x10, 29: 0x06,
        30: 0x12, 31: 0x13, 32: 0x14, 33: 0x15, 34: 0x17, 35: 0x16, 36: 0x1A, 37: 0x1C, 38: 0x19, 39: 0x1D,
        40: 0x24, 41: 0x35, 42: 0x33, 43: 0x30, 44: 0x31, 45: 0x1B, 46: 0x18, 47: 0x21, 48: 0x1E, 49: 0x2A,
        50: 0x2A, 51: 0x29, 52: 0x27, 53: 0x32, 54: 0x2B, 55: 0x2F, 56: 0x2C, 57: 0x39,
        58: 0x7A, 59: 0x78, 60: 0x63, 61: 0x76, 62: 0x60, 63: 0x61, 64: 0x62, 65: 0x64, 66: 0x65, 67: 0x6D,
        68: 0x67, 69: 0x6F,
        73: 0x72, 74: 0x73, 75: 0x74, 76: 0x75, 77: 0x77, 78: 0x79, 79: 0x7C, 80: 0x7B, 81: 0x7D, 82: 0x7E,
        83: 0x47, 84: 0x4B, 85: 0x43, 86: 0x4E, 87: 0x45, 88: 0x4C, 89: 0x53, 90: 0x54, 91: 0x55, 92: 0x56,
        93: 0x57, 94: 0x58, 95: 0x59, 96: 0x5B, 97: 0x5C, 98: 0x52, 99: 0x41, 100: 0x0A, 103: 0x51,
        104: 0x69, 105: 0x6B, 106: 0x71, 107: 0x6A, 108: 0x40, 109: 0x4F, 110: 0x50, 111: 0x5A,
        224: 0x3B, 225: 0x38, 226: 0x3A, 227: 0x37, 228: 0x3E, 229: 0x3C, 230: 0x3D, 231: 0x36,
    ]

    /// Left and right keys of each modifier.
    static let modifierFamilies: [[UInt16]] = [[0x38, 0x3C], [0x3B, 0x3E], [0x3A, 0x3D], [0x37, 0x36]]

    /// The modifier keys held, read from an event's flags on every event so
    /// a release seen by no monitor cannot leave one held. The low bits say
    /// which side; a keyboard that leaves them out counts as the left key.
    static func modifierKeys(rawFlags: UInt) -> Set<UInt16> {
        // Shift, Control, Option, Command: the flag, then each side's device bit and key.
        let modifiers: [(flag: UInt, left: UInt, leftKey: UInt16, right: UInt, rightKey: UInt16)] = [
            (1 << 17, 0x02, 0x38, 0x04, 0x3C), (1 << 18, 0x01, 0x3B, 0x2000, 0x3E),
            (1 << 19, 0x20, 0x3A, 0x40, 0x3D), (1 << 20, 0x08, 0x37, 0x10, 0x36),
        ]
        var keys = Set<UInt16>()
        for modifier in modifiers where rawFlags & modifier.flag != 0 {
            let left = rawFlags & modifier.left != 0
            let right = rawFlags & modifier.right != 0
            if left || !right { keys.insert(modifier.leftKey) }
            if right { keys.insert(modifier.rightKey) }
        }
        return keys
    }
}

/// Follows the voice state through the person's shortcuts while Discord's
/// controls cannot be read, and takes the controls' state whenever they can.
struct DiscordShortcutTracker {
    var active = false
    private(set) var base: DiscordVoiceState?
    private var mutedBeforeDeafen = false
    private(set) var bindings: [DiscordKeybind] = []
    /// Unknown until the first sample, so a key already held when the
    /// shortcuts arrive is not taken for a press.
    private var down: [Bool?] = []
    private var holds = Set<String>()
    private var pendingUntil: Date?

    /// How long Discord gets to answer the same key before its controls are trusted again.
    static let settle: TimeInterval = 0.35

    var state: DiscordVoiceState? {
        base.map { DiscordVoiceState(muted: $0.muted || $0.deafened || !holds.isEmpty, deafened: $0.deafened) }
    }

    mutating func setBindings(_ next: [DiscordKeybind]) {
        guard next != bindings else { return }
        down = Array(repeating: nil, count: next.count)
        bindings = next
        holds.removeAll()
    }

    mutating func seed(_ state: DiscordVoiceState) {
        if base == nil { base = state }
    }

    /// False while a held push-to-mute or a press Discord has not yet shown
    /// keeps the controls' reading from replacing the tracked state.
    @discardableResult
    mutating func live(_ state: DiscordVoiceState, now: Date) -> Bool {
        // A held push-to-mute must never replace the underlying toggle state.
        guard holds.isEmpty else { return false }
        if let pendingUntil, now < pendingUntil, self.state != state { return false }
        pendingUntil = nil
        if state.deafened, let base, !base.deafened { mutedBeforeDeafen = base.muted }
        base = state
        return true
    }

    mutating func sample(_ pressed: (DiscordKey) -> Bool, now: Date) {
        guard active else { return }
        var changes: [(String, DiscordKeyAction, Bool)] = []
        for (index, binding) in bindings.enumerated() {
            let chord = binding.keys.allSatisfy(pressed)
            // An extra modifier must not turn an unrelated shortcut into a mute.
            let modifiersMatch = DiscordKeybinds.modifierFamilies.allSatisfy { family in
                !family.contains { pressed(.key($0)) } || binding.keys.contains { key in
                    if case .key(let code) = key { return family.contains(code) }
                    return false
                }
            }
            let isDown = chord && modifiersMatch
            if let previous = down[index], previous != isDown { changes.append((binding.id, binding.action, isDown)) }
            down[index] = isDown
        }
        for (id, action, isDown) in changes {
            guard var next = base else { continue }
            switch action {
            case .toggleMute where isDown:
                if next.deafened { next = DiscordVoiceState(muted: false, deafened: false) } else { next.muted.toggle() }
            case .toggleDeafen where isDown:
                if next.deafened {
                    next = DiscordVoiceState(muted: mutedBeforeDeafen, deafened: false)
                } else {
                    mutedBeforeDeafen = next.muted
                    next = DiscordVoiceState(muted: true, deafened: true)
                }
            case .holdMute:
                if isDown { holds.insert(id) } else { holds.remove(id) }
            default:
                continue
            }
            base = next
            pendingUntil = now.addingTimeInterval(Self.settle)
        }
    }
}
