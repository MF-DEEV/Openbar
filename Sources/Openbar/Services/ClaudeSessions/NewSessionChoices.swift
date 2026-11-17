// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// What New session last started with in one project (#270): its agent,
/// account, model, worktree, safe commands and where it ran. Every field is
/// optional, so a later field reads as unset in an older entry.
struct NewSessionChoices: Codable, Equatable {
    var agent: String?
    /// The Claude account's config folder.
    var account: String?
    /// The model key per agent (its raw value).
    var models: [String: String]?
    var worktree: Bool?
    var background: Bool?
    var safe: Bool?
    /// Suggested commands turned off.
    var safeOff: [String]?
    /// Commands the person added, kept on until removed.
    var safeCustom: [String]?
    var used: Date?
}

/// The choices of each project, kept by its path in one defaults entry.
enum NewSessionChoiceStore {
    /// How many projects are kept; the least recently used go first.
    static let limit = 50

    static func all(_ defaults: UserDefaults = .standard) -> [String: NewSessionChoices] {
        guard let data = defaults.data(forKey: DefaultsKey.notchNewSessionByProject),
              let decoded = try? JSONDecoder().decode([String: NewSessionChoices].self, from: data) else { return [:] }
        return decoded
    }

    static func load(_ path: String, _ defaults: UserDefaults = .standard) -> NewSessionChoices? {
        all(defaults)[path]
    }

    static func save(_ choices: NewSessionChoices, for path: String, now: Date = Date(), _ defaults: UserDefaults = .standard) {
        var entries = all(defaults)
        var choices = choices
        choices.used = now
        entries[path] = choices
        if entries.count > limit {
            let oldest = entries.sorted { ($0.value.used ?? .distantPast) < ($1.value.used ?? .distantPast) }
            for (key, _) in oldest.prefix(entries.count - limit) { entries[key] = nil }
        }
        if let data = try? JSONEncoder().encode(entries) {
            defaults.set(data, forKey: DefaultsKey.notchNewSessionByProject)
        }
    }
}
