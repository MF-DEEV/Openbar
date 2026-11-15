// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

enum SessionSoundKind: String, CaseIterable, Hashable {
    case permission, question, finished, error, limit, started
    var preferenceKey: String { "notchSessionsSound" + rawValue.prefix(1).uppercased() + String(rawValue.dropFirst()) }
    var priority: Int {
        switch self { case .limit: 5; case .error: 4; case .permission: 3; case .question: 2; case .finished: 1; case .started: 0 }
    }
}

/// Read-through migration: never overwrite old choices. A newly saved per-event
/// choice (including None) wins. New events are opt-in to preserve #167's quiet
/// completions. Old disabled notifications remain disabled after upgrading.
enum SessionSoundPreferences {
    static let volumeKey = "notchSessionsSoundVolume"
    static let muteKey = "notchSessionsSoundMutedUntil"
    static func choice(_ kind: SessionSoundKind, in defaults: UserDefaults = .standard) -> String {
        if let choice = defaults.string(forKey: kind.preferenceKey) { return choice }
        guard kind == .permission || kind == .question else { return "none" }
        guard defaults.object(forKey: "notchSessionsWaitingSound") as? Bool ?? true else { return "none" }
        return defaults.string(forKey: "notchSessionsWaitingSoundName") ?? "Glass"
    }
    static func volume(in defaults: UserDefaults = .standard) -> Double {
        let value = (defaults.object(forKey: volumeKey) as? NSNumber)?.doubleValue ?? 100
        return value.isFinite ? min(100, max(0, value)) : 100
    }
    static func muted(at now: Date = Date(), in defaults: UserDefaults = .standard) -> Bool {
        defaults.double(forKey: muteKey) > now.timeIntervalSince1970
    }
    static func muteUntilTomorrow(now: Date = Date(), calendar: Calendar = .current) -> Date {
        calendar.date(byAdding: .day, value: 1, to: calendar.startOfDay(for: now)) ?? now.addingTimeInterval(86400)
    }
}
