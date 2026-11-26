// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The anonymous daily usage ping (#333): a minute after launch when the
/// last one is 20 hours old, then about every 24 hours while the app runs.
/// docs/PRIVACY.md lists every field it sends.
final class UsageStatsService {
    static let shared = UsageStatsService()

    let store: UsageStatsStore
    private var timer: Timer?
    private let session: URLSession
    private let redirects = RefuseRedirects()

    private init() {
        store = UsageStatsStore(defaults: .standard, environment: ProcessInfo.processInfo.environment,
                                developerBuild: AppInfo.isDeveloperBuild)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.httpCookieAcceptPolicy = .never
        configuration.httpShouldSetCookies = false
        configuration.timeoutIntervalForRequest = 20
        configuration.timeoutIntervalForResource = 30
        configuration.waitsForConnectivity = false
        session = URLSession(configuration: configuration, delegate: redirects, delegateQueue: nil)
    }

    /// Main thread, at launch. A Developer build never schedules anything.
    func start() {
        guard !AppInfo.isDeveloperBuild, timer == nil else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + UsageStatsSupport.launchDelay) { [weak self] in
            self?.check(atLaunch: true)
        }
        let timer = Timer(timeInterval: UsageStatsSupport.checkInterval, repeats: true) { [weak self] _ in
            self?.check(atLaunch: false)
        }
        timer.tolerance = 600
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    /// The setting's switch. Off deletes the id and the counters at once.
    func settingChanged(_ on: Bool) {
        if !on { store.optOut() }
    }

    /// From the sessions queue, whenever the list changes.
    func note(sessions: [ClaudeSession]) {
        guard store.isActive else { return }
        store.note(sessions: sessions.map {
            UsageStatsSessionNote(id: $0.id, provider: $0.provider.rawValue, terminal: UsageStatsSupport.terminalID($0.host))
        })
    }

    /// The person opened a section of the island.
    func noteOpened(_ module: NotchModule) {
        store.noteOpened(module.rawValue)
    }

    private func check(atLaunch: Bool) {
        if !store.settingOn {
            Task { await store.sendIfDue(atLaunch: atLaunch, context: context(), transport: send) }
            return
        }
        guard store.isDue(atLaunch: atLaunch) else { return }
        let context = context()
        Task { [store] in await store.sendIfDue(atLaunch: atLaunch, context: context, transport: send) }
    }

    private func send(_ request: URLRequest) async -> Int? {
        guard let (_, response) = try? await session.data(for: request) else { return nil }
        return (response as? HTTPURLResponse)?.statusCode
    }

    /// Main thread: what the app knows right now.
    private func context() -> UsageStatsContext {
        let service = ClaudeSessionService.shared
        var hooks: [String: Bool] = [:]
        if let installed = service.hooksInstalled { hooks[ClaudeSessionProvider.claude.rawValue] = installed }
        for (provider, value) in service.agentHooks ?? [:] where provider != .claude {
            hooks[provider.rawValue] = value.state == .installed
        }
        let enabled = NotchSupport.isEnabled() ? NotchSupport.modules().map(\.rawValue) : []
        return UsageStatsContext(osVersion: UsageStatsSystem.osVersion, arch: UsageStatsSystem.arch,
                                 rosetta: UsageStatsSystem.rosetta, appVersion: AppInfo.version,
                                 build: AppInfo.build, hooks: hooks, enabled: enabled)
    }
}

/// The system facts the ping and the diagnostics file share.
enum UsageStatsSystem {
    static var osVersion: String {
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    static var arch: String {
        #if arch(arm64)
        return "arm64"
        #else
        return "x86_64"
        #endif
    }

    /// True when an Intel build runs translated on Apple silicon.
    static var rosetta: Bool {
        var translated: Int32 = 0
        var size = MemoryLayout<Int32>.size
        return sysctlbyname("sysctl.proc_translated", &translated, &size, nil, 0) == 0 && translated == 1
    }
}

/// The ping goes to one address only; a redirect is never followed.
private final class RefuseRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}
