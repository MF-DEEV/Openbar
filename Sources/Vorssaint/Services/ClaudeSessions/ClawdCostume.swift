// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

// Every Claude Code session in the island has a Clawd of its own, the little
// pixel crab Claude Code greets with, dressed in a costume drawn by lot from
// the session's id. The same session always draws the same costume, so it is
// known by it from one launch to the next; the person may dress it otherwise.

/// The color of a Clawd's shell.
enum ClawdShell: String, CaseIterable, Identifiable {
    case clay, coral, amber, moss, mint, teal, sky, iris, orchid, rose

    var id: String { rawValue }

    /// sRGB components out of 255, so the model needs no graphics framework.
    var rgb: (red: Int, green: Int, blue: Int) {
        switch self {
        // Claude Code's own `clawd_body`.
        case .clay: return (215, 119, 87)
        case .coral: return (226, 86, 72)
        case .amber: return (230, 166, 58)
        case .moss: return (112, 168, 70)
        case .mint: return (72, 184, 146)
        case .teal: return (40, 160, 170)
        case .sky: return (68, 146, 224)
        case .iris: return (116, 112, 226)
        case .orchid: return (178, 98, 214)
        case .rose: return (230, 112, 156)
        }
    }

    /// The shell Claude Code's color for a subagent stands for, as an agent's
    /// definition names it in its `color:` line.
    init?(agentColor: String) {
        switch agentColor.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "red": self = .coral
        case "orange": self = .clay
        case "yellow": self = .amber
        case "green": self = .moss
        case "cyan": self = .teal
        case "blue": self = .sky
        case "purple": self = .iris
        case "pink": self = .rose
        default: return nil
        }
    }
}

enum ClawdHat: String, CaseIterable, Identifiable {
    case none, beanie, cap, beret, topHat, crown, party, propeller, chef

    var id: String { rawValue }
}

enum ClawdEyewear: String, CaseIterable, Identifiable {
    case none, glasses, shades, round, monocle, anaglyph

    var id: String { rawValue }
}

enum ClawdExtra: String, CaseIterable, Identifiable {
    case none, bow, scarf, bowTie, flower, headphones, coffee

    var id: String { rawValue }
}

struct ClawdCostume: Hashable {
    var shell: ClawdShell
    var hat: ClawdHat
    var eyewear: ClawdEyewear
    var extra: ClawdExtra

    /// Clawd as Claude Code draws it.
    static let classic = ClawdCostume(shell: .clay, hat: .none, eyewear: .none, extra: .none)

    /// How often a lot draws something rather than nothing.
    static let hatChance = 0.7
    static let eyewearChance = 0.4
    static let extraChance = 0.45
    /// Claude's own color comes up this many times as often as any other.
    static let clayWeight = 3

    /// A number for `key` that stays the same from one launch to the next:
    /// FNV-1a over its bytes. Swift's `Hasher` is seeded anew each launch.
    static func seed(_ key: String) -> UInt64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in key.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return hash
    }

    /// The costume `key` draws, the same each time.
    static func seeded(_ key: String) -> ClawdCostume {
        var generator = ClawdRandom(seed: seed(key))
        return drawn(using: &generator)
    }

    /// A subagent's costume: drawn from its own key, in the shell of the
    /// color its definition gives it, if it gives one.
    static func seeded(_ key: String, agentColor: String?) -> ClawdCostume {
        var costume = seeded(key)
        if let shell = agentColor.flatMap(ClawdShell.init(agentColor:)) { costume.shell = shell }
        return costume
    }

    /// A costume by lot, other than `current`, for "Surprise me".
    static func random(excluding current: ClawdCostume?) -> ClawdCostume {
        var generator = SystemRandomNumberGenerator()
        var costume = drawn(using: &generator)
        // Ten draws alike in a row are as good as impossible; give up then.
        for _ in 0..<10 where costume == current { costume = drawn(using: &generator) }
        return costume
    }

    /// One part after another from the same lot, so each is its own draw.
    static func drawn<Generator: RandomNumberGenerator>(using generator: inout Generator) -> ClawdCostume {
        let shells = ClawdShell.allCases
        let weights = shells.map { $0 == .clay ? clayWeight : 1 }
        var ticket = Int(generator.next() % UInt64(weights.reduce(0, +)))
        var shell = ClawdShell.clay
        for (candidate, weight) in zip(shells, weights) {
            if ticket < weight { shell = candidate; break }
            ticket -= weight
        }
        return ClawdCostume(shell: shell,
                            hat: pick(ClawdHat.allCases, chance: hatChance, using: &generator),
                            eyewear: pick(ClawdEyewear.allCases, chance: eyewearChance, using: &generator),
                            extra: pick(ClawdExtra.allCases, chance: extraChance, using: &generator))
    }

    /// Nothing, the first case, with the chance left over; otherwise one of
    /// the rest, each as likely.
    private static func pick<Part, Generator: RandomNumberGenerator>(_ parts: [Part], chance: Double,
                                                                     using generator: inout Generator) -> Part {
        let roll = Double(generator.next() >> 11) / Double(1 << 53)
        let index = Int(generator.next() % UInt64(max(1, parts.count - 1)))
        return roll < chance && parts.count > 1 ? parts[1 + index] : parts[0]
    }
}

// The memberwise initializer stays because the one that reads a kept costume
// lives out here.
extension ClawdCostume {
    private static let version = "v1:"

    /// The costume as kept in the preferences.
    var code: String {
        Self.version + [shell.rawValue, hat.rawValue, eyewear.rawValue, extra.rawValue].joined(separator: ".")
    }

    /// Reads a kept costume. A part this version does not know comes from
    /// `fallback`; anything else unreadable is nil.
    init?(code: String, fallback: ClawdCostume) {
        guard code.hasPrefix(Self.version), code.count <= 80 else { return nil }
        let parts = code.dropFirst(Self.version.count).split(separator: ".", omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 4 else { return nil }
        self.init(shell: ClawdShell(rawValue: parts[0]) ?? fallback.shell,
                  hat: ClawdHat(rawValue: parts[1]) ?? fallback.hat,
                  eyewear: ClawdEyewear(rawValue: parts[2]) ?? fallback.eyewear,
                  extra: ClawdExtra(rawValue: parts[3]) ?? fallback.extra)
    }

    /// What a session wears: what the person chose for it, or its own lot.
    static func resolved(key: String, kept: String?, agentColor: String? = nil) -> ClawdCostume {
        let own = seeded(key, agentColor: agentColor)
        return kept.flatMap { ClawdCostume(code: $0, fallback: own) } ?? own
    }

    /// What to keep for a session dressed in `costume`: nothing when it is
    /// the session's own lot, so the choice follows the lot if it changes.
    static func kept(_ costume: ClawdCostume?, key: String) -> String? {
        guard let costume, costume != seeded(key) else { return nil }
        return costume.code
    }
}

/// SplitMix64: a small generator that gives the same numbers for the same
/// seed on every Mac and every launch.
struct ClawdRandom: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) { state = seed }

    mutating func next() -> UInt64 {
        state = state &+ 0x9e37_79b9_7f4a_7c15
        var value = state
        value = (value ^ (value >> 30)) &* 0xbf58_476d_1ce4_e5b9
        value = (value ^ (value >> 27)) &* 0x94d0_49bb_1331_11eb
        return value ^ (value >> 31)
    }
}
