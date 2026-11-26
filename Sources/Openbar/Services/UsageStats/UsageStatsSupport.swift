// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// A session the ping may count: its agent and, once known, the terminal it
/// runs in, as one of the fixed ids below. The id never leaves the Mac.
struct UsageStatsSessionNote: Equatable {
    let id: String
    let provider: String
    let terminal: String?
}

/// What the usage ping keeps between sends (#333): counters since the last
/// successful one, the sessions already counted, and when it last tried.
struct UsageStatsState: Codable, Equatable {
    struct Seen: Codable, Equatable {
        var at: Date
        var terminal: Bool
    }

    var sessions: [String: Int] = [:]
    var terminals: [String: Int] = [:]
    var opened: [String: Int] = [:]
    var seen: [String: Seen] = [:]
    var lastSuccess: Date?
    var lastFailure: Date?
}

/// Read on the main thread right before a send.
struct UsageStatsContext: Equatable {
    var osVersion: String
    var arch: String
    var rosetta: Bool
    var appVersion: String
    var build: String?
    /// Whether each agent holds Openbar's hooks, by provider id.
    var hooks: [String: Bool]
    /// The island sections that are on, in their order.
    var enabled: [String]
}

/// Schema v1 of the ping. Every field is listed in docs/PRIVACY.md.
struct UsageStatsPayload: Encodable, Equatable {
    struct Agent: Encodable, Equatable {
        let sessions: Int
        let hooks: Bool
    }

    struct Features: Encodable, Equatable {
        let enabled: [String]
        var opened: [String: Int]
    }

    var v = 1
    let installId: String
    var platform = "mac"
    let osVersion: String
    let arch: String
    let rosetta: Bool?
    let appVersion: String
    let build: String?
    let agents: [String: Agent]
    var terminals: [String: Int]
    var features: Features
}

enum UsageStatsSupport {
    static let productionEndpoint = URL(string: "https://openbar-stats.vercel.app/api/v1/ping")!
    /// Every field the ping sends, in the privacy notes.
    static let detailsURL = URL(string: "https://github.com/MF-DEEV/Openbar/blob/main/docs/PRIVACY.md#network-connections")!
    static let launchDelay: TimeInterval = 60
    static let launchThreshold: TimeInterval = 20 * 3_600
    static let runningThreshold: TimeInterval = 24 * 3_600
    static let retryInterval: TimeInterval = 6 * 3_600
    static let checkInterval: TimeInterval = 3_600
    static let seenLifetime: TimeInterval = 3 * 86_400
    static let seenLimit = 2_000
    static let maxBytes = 4_096
    static let maxCount = 100_000
    static let sampleSessionID = "openbar-sample-session"
    /// The only plain-http hosts an override may name: this Mac, and the Mac
    /// as a Windows (QEMU) or Linux (Lima) test machine sees it.
    static let localHosts: Set<String> = ["localhost", "127.0.0.1", "::1", "10.0.2.2", "host.lima.internal"]

    /// `DO_NOT_TRACK=1` (or true/yes) turns the ping off whatever the setting says.
    static func doNotTrack(_ environment: [String: String]) -> Bool {
        guard let value = environment["DO_NOT_TRACK"]?.trimmingCharacters(in: .whitespaces).lowercased() else { return false }
        return ["1", "true", "yes"].contains(value)
    }

    /// Where pings go: `OPENBAR_STATS_URL` for tests, the dashboard otherwise.
    /// An override that isn't https, or plain http to a local address, sends
    /// nothing at all rather than falling back to the real dashboard.
    static func endpoint(_ environment: [String: String]) -> URL? {
        guard let raw = environment["OPENBAR_STATS_URL"]?.trimmingCharacters(in: .whitespaces), !raw.isEmpty else {
            return productionEndpoint
        }
        guard let url = URL(string: raw), let scheme = url.scheme?.lowercased(), let host = url.host?.lowercased(),
              !host.isEmpty, url.user == nil, url.password == nil else { return nil }
        if scheme == "https" { return url }
        if scheme == "http", localHosts.contains(host) { return url }
        return nil
    }

    /// Whether anything may be counted or sent. Developer builds never send,
    /// not even to an override.
    static func allowed(enabled: Bool, developerBuild: Bool, environment: [String: String]) -> Bool {
        enabled && !developerBuild && !doNotTrack(environment)
    }

    /// A minute after launch the ping goes when the last one is 20 hours old,
    /// later every 24 hours; a failure waits 6 hours. A date in the future (the
    /// clock moved back) counts as never.
    static func isDue(now: Date, state: UsageStatsState, atLaunch: Bool) -> Bool {
        let threshold = atLaunch ? launchThreshold : runningThreshold
        if let success = state.lastSuccess, success <= now, now.timeIntervalSince(success) < threshold { return false }
        if let failure = state.lastFailure, failure <= now, now.timeIntervalSince(failure) < retryInterval { return false }
        return true
    }

    /// The terminal a session runs in, as a fixed id. App names and bundle
    /// identifiers never leave the Mac: anything unlisted is "other". nil
    /// while the terminal isn't known yet.
    static func terminalID(_ host: ClaudeSessionHost) -> String? {
        switch host {
        case .terminal: return "terminal"
        case .iterm: return "iterm2"
        case .ghostty: return "ghostty"
        case .tmux: return "tmux"
        case .zellij: return "zellij"
        case .wezterm: return "wezterm"
        case .kitty: return "kitty"
        case .ide(let bundle, _), .app(let bundle): return appIDs[bundle] ?? "other"
        case .background: return "background"
        case .remote: return "remote"
        case .unknown: return nil
        }
    }

    static let appIDs: [String: String] = [
        "com.microsoft.VSCode": "vscode",
        "com.microsoft.VSCodeInsiders": "vscode",
        "com.vscodium": "vscodium",
        "com.todesktop.230313mzl4w4u92": "cursor",
        "com.exafunction.windsurf": "windsurf",
        "com.google.antigravity-ide": "antigravity",
        "dev.zcode.app": "zcode",
        "dev.warp.Warp-Stable": "warp",
        "org.alacritty": "alacritty",
        "com.anthropic.claudefordesktop": "claude-desktop",
        "dev.zed.Zed": "zed",
        "com.apple.Terminal": "terminal",
        "com.googlecode.iterm2": "iterm2",
        "com.mitchellh.ghostty": "ghostty",
        "com.github.wez.wezterm": "wezterm",
        "net.kovidgoyal.kitty": "kitty",
    ]

    /// Counts each session once, and its terminal once when first known.
    /// Sessions already counted in the last three days are not counted again,
    /// so a restart doesn't count the same sessions twice.
    static func record(_ notes: [UsageStatsSessionNote], into state: inout UsageStatsState, now: Date) {
        prune(&state, now: now)
        for note in notes where note.id != sampleSessionID && !note.id.isEmpty && isKey(note.provider) {
            if state.seen[note.id] == nil {
                state.sessions[note.provider] = bumped(state.sessions[note.provider])
                state.seen[note.id] = .init(at: now, terminal: false)
            }
            if state.seen[note.id]?.terminal == false, let terminal = note.terminal, isKey(terminal) {
                state.terminals[terminal] = bumped(state.terminals[terminal])
                state.seen[note.id]?.terminal = true
            }
        }
        if state.seen.count > seenLimit {
            for (id, _) in state.seen.sorted(by: { $0.value.at < $1.value.at }).prefix(state.seen.count - seenLimit) {
                state.seen.removeValue(forKey: id)
            }
        }
    }

    static func recordOpen(_ module: String, into state: inout UsageStatsState) {
        guard isKey(module) else { return }
        state.opened[module] = bumped(state.opened[module])
    }

    static func prune(_ state: inout UsageStatsState, now: Date) {
        state.seen = state.seen.filter { $0.value.at <= now && now.timeIntervalSince($0.value.at) < seenLifetime }
    }

    static func payload(installId: String, context: UsageStatsContext, state: UsageStatsState) -> UsageStatsPayload {
        var agents: [String: UsageStatsPayload.Agent] = [:]
        for provider in Set(state.sessions.keys).union(context.hooks.keys) where isKey(provider) {
            let sessions = state.sessions[provider] ?? 0
            let hooks = context.hooks[provider] ?? false
            if sessions > 0 || hooks { agents[provider] = .init(sessions: sessions, hooks: hooks) }
        }
        return UsageStatsPayload(
            installId: installId, osVersion: context.osVersion, arch: context.arch, rosetta: context.rosetta,
            appVersion: context.appVersion, build: context.build, agents: agents,
            terminals: state.terminals.filter { $0.value > 0 },
            features: .init(enabled: context.enabled.filter(isKey), opened: state.opened.filter { $0.value > 0 }))
    }

    /// The ping as sent, at most 4 KB: the smallest terminal counts go first,
    /// then the smallest section counts.
    static func encode(_ payload: UsageStatsPayload) -> Data? {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        var payload = payload
        while let data = try? encoder.encode(payload) {
            if data.count <= maxBytes { return data }
            if let smallest = payload.terminals.min(by: { ($0.value, $0.key) < ($1.value, $1.key) }) {
                payload.terminals.removeValue(forKey: smallest.key)
            } else if let smallest = payload.features.opened.min(by: { ($0.value, $0.key) < ($1.value, $1.key) }) {
                payload.features.opened.removeValue(forKey: smallest.key)
            } else {
                return nil
            }
        }
        return nil
    }

    enum Outcome: Equatable {
        /// The dashboard has the day's counts, or will never take them (a
        /// malformed or over-limit ping): start a new day either way.
        case delivered
        /// Network trouble or a server error: keep the counts, try in 6 hours.
        case failed
    }

    static func outcome(status: Int?) -> Outcome {
        guard let status else { return .failed }
        switch status {
        case 200..<300, 400, 413, 422, 429: return .delivered
        default: return .failed
        }
    }

    /// After a delivery, takes away what was sent, so counts that came in
    /// while the ping was on its way are kept for the next one.
    static func settle(_ state: inout UsageStatsState, sent: UsageStatsState, outcome: Outcome, now: Date) {
        switch outcome {
        case .delivered:
            subtract(&state.sessions, sent.sessions)
            subtract(&state.terminals, sent.terminals)
            subtract(&state.opened, sent.opened)
            state.lastSuccess = now
            state.lastFailure = nil
        case .failed:
            state.lastFailure = now
        }
    }

    private static func subtract(_ counts: inout [String: Int], _ sent: [String: Int]) {
        for (key, value) in sent {
            let left = (counts[key] ?? 0) - value
            counts[key] = left > 0 ? left : nil
        }
    }

    private static func bumped(_ value: Int?) -> Int { min((value ?? 0) + 1, maxCount) }

    static func isKey(_ value: String) -> Bool {
        value.range(of: "^[a-z][a-z0-9-]{0,31}$", options: .regularExpression) != nil
    }
}
