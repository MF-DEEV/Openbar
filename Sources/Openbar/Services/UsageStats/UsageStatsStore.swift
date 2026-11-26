// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The usage ping's state in the preferences, safe to use from any thread
/// (#333). Nothing is counted or sent while the setting is off, with
/// `DO_NOT_TRACK` set, or in a Developer build.
final class UsageStatsStore {
    /// Sends one request and returns its status code, or nil when it failed.
    typealias Transport = (URLRequest) async -> Int?

    private let defaults: UserDefaults
    private let environment: [String: String]
    private let developerBuild: Bool
    private let now: () -> Date
    private let lock = NSLock()
    private var sending = false

    init(defaults: UserDefaults, environment: [String: String], developerBuild: Bool, now: @escaping () -> Date = Date.init) {
        self.defaults = defaults
        self.environment = environment
        self.developerBuild = developerBuild
        self.now = now
    }

    var settingOn: Bool { defaults.bool(forKey: DefaultsKey.shareUsageStats) }

    var isActive: Bool {
        UsageStatsSupport.allowed(enabled: settingOn, developerBuild: developerBuild, environment: environment)
    }

    var state: UsageStatsState { lock.withLock { load() } }

    var installId: String? { defaults.string(forKey: DefaultsKey.usageStatsInstallId) }

    func note(sessions: [UsageStatsSessionNote]) {
        guard isActive, !sessions.isEmpty else { return }
        let at = now()
        lock.withLock {
            var state = load()
            let before = state
            UsageStatsSupport.record(sessions, into: &state, now: at)
            if state != before { save(state) }
        }
    }

    func noteOpened(_ module: String) {
        guard isActive else { return }
        lock.withLock {
            var state = load()
            UsageStatsSupport.recordOpen(module, into: &state)
            save(state)
        }
    }

    /// Switching stats off deletes the id and everything counted; switching
    /// them on again starts with a new id.
    func optOut() {
        lock.withLock {
            defaults.removeObject(forKey: DefaultsKey.usageStatsInstallId)
            defaults.removeObject(forKey: DefaultsKey.usageStatsState)
        }
    }

    func isDue(atLaunch: Bool) -> Bool {
        isActive && UsageStatsSupport.isDue(now: now(), state: state, atLaunch: atLaunch)
    }

    /// Sends the ping when it is due. Returns what happened, or nil when
    /// nothing was sent.
    @discardableResult
    func sendIfDue(atLaunch: Bool, context: UsageStatsContext, transport: Transport) async -> UsageStatsSupport.Outcome? {
        if !settingOn {
            // Off with leftovers (a change made outside the switch): clean up.
            if installId != nil || defaults.object(forKey: DefaultsKey.usageStatsState) != nil { optOut() }
            return nil
        }
        guard isActive, let url = UsageStatsSupport.endpoint(environment) else { return nil }
        let started = now()
        let prepared: (request: URLRequest, sent: UsageStatsState)? = lock.withLock {
            guard !sending else { return nil }
            var state = load()
            guard UsageStatsSupport.isDue(now: started, state: state, atLaunch: atLaunch) else { return nil }
            UsageStatsSupport.prune(&state, now: started)
            let id = defaults.string(forKey: DefaultsKey.usageStatsInstallId) ?? {
                let fresh = UUID().uuidString.lowercased()
                defaults.set(fresh, forKey: DefaultsKey.usageStatsInstallId)
                return fresh
            }()
            let payload = UsageStatsSupport.payload(installId: id, context: context, state: state)
            guard let body = UsageStatsSupport.encode(payload) else { return nil }
            var request = URLRequest(url: url, timeoutInterval: 20)
            request.httpMethod = "POST"
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.setValue("no-store", forHTTPHeaderField: "Cache-Control")
            request.setValue("Openbar/\(context.appVersion)", forHTTPHeaderField: "User-Agent")
            sending = true
            return (request, state)
        }
        guard let prepared else { return nil }
        let outcome = UsageStatsSupport.outcome(status: await transport(prepared.request))
        lock.withLock {
            sending = false
            // Switched off while the ping was on its way: keep nothing.
            guard isActive, defaults.string(forKey: DefaultsKey.usageStatsInstallId) != nil else { return }
            var state = load()
            UsageStatsSupport.settle(&state, sent: prepared.sent, outcome: outcome, now: now())
            save(state)
        }
        return outcome
    }

    private func load() -> UsageStatsState {
        guard let data = defaults.data(forKey: DefaultsKey.usageStatsState),
              let state = try? JSONDecoder().decode(UsageStatsState.self, from: data) else { return UsageStatsState() }
        return state
    }

    private func save(_ state: UsageStatsState) {
        guard let data = try? JSONEncoder().encode(state) else { return }
        defaults.set(data, forKey: DefaultsKey.usageStatsState)
    }
}
