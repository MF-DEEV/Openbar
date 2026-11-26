// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The anonymous daily usage ping (#333): what it sends, when, and that it
/// sends nothing while switched off, under DO_NOT_TRACK or in a Developer build.
enum UsageStatsTests {
    static func run(_ suite: TestSuite) {
        suite.run("usage stats: payload fields") { payload(suite) }
        suite.run("usage stats: schedule") { schedule(suite) }
        suite.run("usage stats: endpoint and gates") { gates(suite) }
        suite.run("usage stats: counting") { counting(suite) }
        suite.run("usage stats: terminals") { terminals(suite) }
        suite.run("usage stats: size cap") { sizeCap(suite) }
        suite.run("usage stats: store sends and settles") { store(suite) }
        suite.run("usage stats: opt-out and quiet modes") { quiet(suite) }
        suite.run("usage stats: settings export") { backup(suite) }
    }

    static let context = UsageStatsContext(osVersion: "15.3.0", arch: "arm64", rosetta: false, appVersion: "3.6.0",
                                           build: "110", hooks: ["claude": true, "codex": false, "gemini": false],
                                           enabled: ["sessions", "mixer", "downloads"])

    static func payload(_ suite: TestSuite) {
        var state = UsageStatsState()
        state.sessions = ["claude": 12, "codex": 3]
        state.terminals = ["ghostty": 9, "vscode": 6]
        state.opened = ["sessions": 41, "mixer": 2]
        let id = "3f2b8c4e-9a1d-4e7b-8c2f-1a2b3c4d5e6f"
        guard let data = UsageStatsSupport.encode(UsageStatsSupport.payload(installId: id, context: context, state: state)),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return suite.expect(false, "the payload encodes")
        }
        suite.expect(Set(json.keys) == ["v", "installId", "platform", "osVersion", "arch", "rosetta", "appVersion", "build",
                                        "agents", "terminals", "features"], "exactly the schema's fields: \(json.keys.sorted())")
        suite.expect(json["v"] as? Int == 1 && json["platform"] as? String == "mac" && json["installId"] as? String == id,
                     "version, platform and id")
        let agents = json["agents"] as? [String: [String: Any]] ?? [:]
        suite.expect(Set(agents.keys) == ["claude", "codex"], "agents with sessions or hooks only: \(agents.keys.sorted())")
        suite.expect(agents["claude"]?["sessions"] as? Int == 12 && agents["claude"]?["hooks"] as? Bool == true, "claude")
        suite.expect(agents["codex"]?["sessions"] as? Int == 3 && agents["codex"]?["hooks"] as? Bool == false, "codex")
        suite.expect(Set((agents["claude"] ?? [:]).keys) == ["sessions", "hooks"], "nothing else per agent")
        let features = json["features"] as? [String: Any] ?? [:]
        suite.expect(features["enabled"] as? [String] == ["sessions", "mixer", "downloads"], "enabled sections in order")
        suite.expect((features["opened"] as? [String: Int]) == ["sessions": 41, "mixer": 2], "section opens")
        suite.expect((json["terminals"] as? [String: Int]) == ["ghostty": 9, "vscode": 6], "terminals")
        let text = String(decoding: data, as: UTF8.self)
        suite.expect(!text.contains("/Users") && !text.contains("com."), "no paths or bundle ids")

        var noBuild = context
        noBuild.build = nil
        let bare = UsageStatsSupport.encode(UsageStatsSupport.payload(installId: id, context: noBuild, state: UsageStatsState()))
        let bareJSON = bare.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? [:]
        suite.expect(bareJSON["build"] == nil, "a missing build is left out, not null")
    }

    static func schedule(_ suite: TestSuite) {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        var state = UsageStatsState()
        suite.expect(UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "never sent: due at launch")
        state.lastSuccess = now.addingTimeInterval(-19 * 3_600)
        suite.expect(!UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "19 h after a send: not at launch")
        state.lastSuccess = now.addingTimeInterval(-21 * 3_600)
        suite.expect(UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "21 h after: due at launch")
        suite.expect(!UsageStatsSupport.isDue(now: now, state: state, atLaunch: false), "21 h after: not while running")
        state.lastSuccess = now.addingTimeInterval(-24 * 3_600)
        suite.expect(UsageStatsSupport.isDue(now: now, state: state, atLaunch: false), "24 h after: due while running")
        state.lastFailure = now.addingTimeInterval(-5 * 3_600)
        suite.expect(!UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "a failure 5 h ago waits")
        state.lastFailure = now.addingTimeInterval(-6 * 3_600)
        suite.expect(UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "6 h after a failure it tries again")
        state = UsageStatsState(lastSuccess: now.addingTimeInterval(86_400 * 30))
        suite.expect(UsageStatsSupport.isDue(now: now, state: state, atLaunch: true), "a send dated in the future counts as never")
    }

    static func gates(_ suite: TestSuite) {
        suite.expect(UsageStatsSupport.endpoint([:]) == UsageStatsSupport.productionEndpoint, "the dashboard by default")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "http://127.0.0.1:3000/api/v1/ping"])?.absoluteString
                     == "http://127.0.0.1:3000/api/v1/ping", "a local http override")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "http://10.0.2.2:3000/x"]) != nil, "the QEMU host")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "https://preview.example.dev/api"]) != nil, "https anywhere")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "http://example.com/api"]) == nil, "plain http elsewhere sends nothing")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "not a url"]) == nil, "a broken override sends nothing")
        suite.expect(UsageStatsSupport.endpoint(["OPENBAR_STATS_URL": "https://u:p@example.com/"]) == nil, "no credentials")
        suite.expect(UsageStatsSupport.doNotTrack(["DO_NOT_TRACK": "1"]) && UsageStatsSupport.doNotTrack(["DO_NOT_TRACK": "true"]),
                     "DO_NOT_TRACK=1")
        suite.expect(!UsageStatsSupport.doNotTrack(["DO_NOT_TRACK": "0"]) && !UsageStatsSupport.doNotTrack([:]), "DO_NOT_TRACK=0")
        suite.expect(UsageStatsSupport.allowed(enabled: true, developerBuild: false, environment: [:]), "on")
        suite.expect(!UsageStatsSupport.allowed(enabled: false, developerBuild: false, environment: [:]), "switched off")
        suite.expect(!UsageStatsSupport.allowed(enabled: true, developerBuild: true,
                                                environment: ["OPENBAR_STATS_URL": "http://127.0.0.1:1/"]),
                     "a Developer build never sends, even with an override")
        suite.expect(!UsageStatsSupport.allowed(enabled: true, developerBuild: false, environment: ["DO_NOT_TRACK": "1"]), "DNT")
        suite.expect(UsageStatsSupport.outcome(status: 204) == .delivered && UsageStatsSupport.outcome(status: 429) == .delivered
                     && UsageStatsSupport.outcome(status: 400) == .delivered, "stored, over the limit or refused end the day")
        suite.expect(UsageStatsSupport.outcome(status: 503) == .failed && UsageStatsSupport.outcome(status: nil) == .failed
                     && UsageStatsSupport.outcome(status: 404) == .failed, "server or network trouble retries")
    }

    static func counting(_ suite: TestSuite) {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        var state = UsageStatsState()
        UsageStatsSupport.record([
            .init(id: "a", provider: "claude", terminal: nil),
            .init(id: "b", provider: "codex", terminal: "ghostty"),
            .init(id: UsageStatsSupport.sampleSessionID, provider: "claude", terminal: "terminal"),
        ], into: &state, now: now)
        suite.expect(state.sessions == ["claude": 1, "codex": 1], "each session once, never the sample: \(state.sessions)")
        suite.expect(state.terminals == ["ghostty": 1], "a terminal once known")
        UsageStatsSupport.record([
            .init(id: "a", provider: "claude", terminal: "vscode"),
            .init(id: "b", provider: "codex", terminal: "ghostty"),
        ], into: &state, now: now.addingTimeInterval(60))
        suite.expect(state.sessions == ["claude": 1, "codex": 1], "a session listed again isn't counted again")
        suite.expect(state.terminals == ["ghostty": 1, "vscode": 1], "a terminal found later is counted once")
        UsageStatsSupport.record([.init(id: "a", provider: "claude", terminal: "vscode")], into: &state,
                                 now: now.addingTimeInterval(2 * 86_400))
        suite.expect(state.sessions["claude"] == 1, "a restart within three days doesn't count it twice")
        UsageStatsSupport.record([.init(id: "a", provider: "claude", terminal: nil)], into: &state,
                                 now: now.addingTimeInterval(4 * 86_400))
        suite.expect(state.sessions["claude"] == 2, "after three days a session is forgotten")
        UsageStatsSupport.record([.init(id: "x", provider: "Not An Id", terminal: "/Applications/X.app")], into: &state, now: now)
        suite.expect(state.sessions["Not An Id"] == nil && state.terminals["/Applications/X.app"] == nil, "only plain ids")

        UsageStatsSupport.recordOpen("sessions", into: &state)
        UsageStatsSupport.recordOpen("sessions", into: &state)
        suite.expect(state.opened == ["sessions": 2], "section opens")

        var many = UsageStatsState()
        let notes = (0..<(UsageStatsSupport.seenLimit + 50)).map { UsageStatsSessionNote(id: "s\($0)", provider: "claude", terminal: nil) }
        UsageStatsSupport.record(notes, into: &many, now: now)
        suite.expect(many.seen.count == UsageStatsSupport.seenLimit, "the counted list stays bounded")

        var sent = state
        UsageStatsSupport.recordOpen("sessions", into: &state)
        UsageStatsSupport.settle(&state, sent: sent, outcome: .delivered, now: now)
        suite.expect(state.opened == ["sessions": 1] && state.sessions.isEmpty && state.terminals.isEmpty,
                     "a delivery keeps only what came in meanwhile: \(state.opened)")
        suite.expect(state.lastSuccess == now && state.lastFailure == nil, "the send is remembered")
        sent = state
        UsageStatsSupport.settle(&state, sent: sent, outcome: .failed, now: now)
        suite.expect(state.opened == ["sessions": 1] && state.lastFailure == now, "a failure keeps the counts")
    }

    static func terminals(_ suite: TestSuite) {
        suite.expect(UsageStatsSupport.terminalID(.terminal(tty: "/dev/ttys001")) == "terminal", "Terminal")
        suite.expect(UsageStatsSupport.terminalID(.ghostty(tty: "/dev/ttys002")) == "ghostty", "Ghostty")
        suite.expect(UsageStatsSupport.terminalID(.ide(bundle: "com.todesktop.230313mzl4w4u92", port: nil)) == "cursor", "Cursor")
        suite.expect(UsageStatsSupport.terminalID(.ide(bundle: "com.microsoft.VSCode", port: 1)) == "vscode", "VS Code")
        suite.expect(UsageStatsSupport.terminalID(.app("com.secret.private-app")) == "other", "unknown apps stay unnamed")
        suite.expect(UsageStatsSupport.terminalID(.remote(alias: "my-server", tmux: true)) == "remote", "no host names")
        suite.expect(UsageStatsSupport.terminalID(.background(job: "j")) == "background", "background")
        suite.expect(UsageStatsSupport.terminalID(.unknown) == nil, "unknown waits")
    }

    static func sizeCap(_ suite: TestSuite) {
        var state = UsageStatsState()
        for i in 0..<400 { state.terminals["t\(i)"] = i + 1 }
        for i in 0..<40 { state.opened["m\(i)"] = 1 }
        let payload = UsageStatsSupport.payload(installId: UUID().uuidString.lowercased(), context: context, state: state)
        guard let data = UsageStatsSupport.encode(payload) else { return suite.expect(false, "trimmed to fit") }
        suite.expect(data.count <= UsageStatsSupport.maxBytes, "at most 4 KB: \(data.count)")
        let json = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
        let kept = json["terminals"] as? [String: Int] ?? [:]
        suite.expect(kept["t399"] == 400 && kept["t0"] == nil, "the largest terminal counts stay")
    }

    static func withDefaults(_ name: String, _ body: (UserDefaults) -> Void) {
        let suiteName = "com.openbar.tests.usage-stats-\(name)"
        guard let defaults = UserDefaults(suiteName: suiteName) else { return }
        defaults.removePersistentDomain(forName: suiteName)
        defaults.register(defaults: [DefaultsKey.shareUsageStats: true])
        defer { defaults.removePersistentDomain(forName: suiteName) }
        body(defaults)
    }

    final class Recorder {
        let lock = NSLock()
        var requests: [URLRequest] = []
        var status: Int?
        init(status: Int?) { self.status = status }
        func transport(_ request: URLRequest) async -> Int? {
            lock.withLock { requests.append(request); return status }
        }
    }

    static func wait<T>(_ work: @escaping () async -> T) -> T {
        let done = DispatchSemaphore(value: 0)
        var result: T?
        Task {
            result = await work()
            done.signal()
        }
        done.wait()
        return result!
    }

    static func store(_ suite: TestSuite) {
        withDefaults("send") { defaults in
            var clock = Date(timeIntervalSince1970: 1_800_000_000)
            let store = UsageStatsStore(defaults: defaults, environment: ["OPENBAR_STATS_URL": "http://127.0.0.1:9/ping"],
                                        developerBuild: false, now: { clock })
            store.note(sessions: [.init(id: "a", provider: "claude", terminal: "ghostty")])
            store.noteOpened("sessions")
            let recorder = Recorder(status: 503)
            let failed = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
            suite.expect(failed == .failed && recorder.requests.count == 1, "a server error is a failure")
            suite.expect(store.state.sessions == ["claude": 1] && store.state.lastFailure == clock, "and keeps the counts")
            let firstID = store.installId
            suite.expect(firstID?.count == 36, "an id is made for the first send")

            clock = clock.addingTimeInterval(3_600)
            recorder.status = 204
            let early = wait { await store.sendIfDue(atLaunch: false, context: context, transport: recorder.transport) }
            suite.expect(early == nil && recorder.requests.count == 1, "no retry before 6 hours")

            clock = clock.addingTimeInterval(6 * 3_600)
            let sent = wait { await store.sendIfDue(atLaunch: false, context: context, transport: recorder.transport) }
            suite.expect(sent == .delivered && recorder.requests.count == 2, "the retry goes after 6 hours")
            suite.expect(store.state.sessions.isEmpty && store.state.opened.isEmpty && store.state.lastSuccess == clock,
                         "a delivery starts the counts again")
            suite.expect(store.installId == firstID, "the id stays")
            let request = recorder.requests.last!
            suite.expect(request.httpMethod == "POST" && request.url?.absoluteString == "http://127.0.0.1:9/ping",
                         "POST to the override")
            suite.expect(request.value(forHTTPHeaderField: "Content-Type") == "application/json"
                         && request.value(forHTTPHeaderField: "User-Agent") == "Openbar/3.6.0", "headers")
            let body = request.httpBody.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] } ?? [:]
            suite.expect(body["installId"] as? String == firstID, "the body carries the id")

            clock = clock.addingTimeInterval(23 * 3_600)
            let tooSoon = wait { await store.sendIfDue(atLaunch: false, context: context, transport: recorder.transport) }
            suite.expect(tooSoon == nil, "not again within 24 hours")
            clock = clock.addingTimeInterval(3_600)
            let next = wait { await store.sendIfDue(atLaunch: false, context: context, transport: recorder.transport) }
            suite.expect(next == .delivered && recorder.requests.count == 3, "the next day it goes again")
        }
    }

    static func quiet(_ suite: TestSuite) {
        withDefaults("optout") { defaults in
            let store = UsageStatsStore(defaults: defaults, environment: [:], developerBuild: false)
            store.note(sessions: [.init(id: "a", provider: "claude", terminal: "kitty")])
            let recorder = Recorder(status: 204)
            _ = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
            suite.expect(store.installId != nil && defaults.data(forKey: DefaultsKey.usageStatsState) != nil, "sent once")
            let oldID = store.installId

            defaults.set(false, forKey: DefaultsKey.shareUsageStats)
            store.optOut()
            suite.expect(store.installId == nil && defaults.object(forKey: DefaultsKey.usageStatsState) == nil,
                         "switching off deletes the id and the counts")
            store.note(sessions: [.init(id: "b", provider: "claude", terminal: nil)])
            store.noteOpened("mixer")
            suite.expect(defaults.object(forKey: DefaultsKey.usageStatsState) == nil, "nothing is counted while off")
            let off = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
            suite.expect(off == nil && recorder.requests.count == 1, "nothing is sent while off")

            defaults.set(true, forKey: DefaultsKey.shareUsageStats)
            _ = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
            suite.expect(store.installId != nil && store.installId != oldID, "switching on again makes a new id")
        }
        withDefaults("leftovers") { defaults in
            defaults.set("old-id", forKey: DefaultsKey.usageStatsInstallId)
            defaults.set(false, forKey: DefaultsKey.shareUsageStats)
            let store = UsageStatsStore(defaults: defaults, environment: [:], developerBuild: false)
            let recorder = Recorder(status: 204)
            _ = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
            suite.expect(store.installId == nil && recorder.requests.isEmpty, "an id left while off is cleaned up, unsent")
        }
        for (name, environment, developer) in [("dnt", ["DO_NOT_TRACK": "1"], false),
                                               ("dev", ["OPENBAR_STATS_URL": "http://127.0.0.1:9/"], true),
                                               ("badurl", ["OPENBAR_STATS_URL": "http://example.com/"], false)] {
            withDefaults(name) { defaults in
                let store = UsageStatsStore(defaults: defaults, environment: environment, developerBuild: developer)
                store.note(sessions: [.init(id: "a", provider: "claude", terminal: nil)])
                let recorder = Recorder(status: 204)
                let result = wait { await store.sendIfDue(atLaunch: true, context: context, transport: recorder.transport) }
                suite.expect(result == nil && recorder.requests.isEmpty, "\(name): no request")
                suite.expect(store.installId == nil, "\(name): no id")
                if name != "badurl" {
                    suite.expect(defaults.object(forKey: DefaultsKey.usageStatsState) == nil, "\(name): nothing counted")
                }
            }
        }
    }

    static func backup(_ suite: TestSuite) {
        let keys = SettingsBackupSupport.exportKeys()
        suite.expect(!keys.contains(DefaultsKey.usageStatsInstallId), "a settings export leaves the install id out")
        suite.expect(!keys.contains(DefaultsKey.usageStatsState), "and the counters")
        suite.expect(SettingsBackupSupport.machineStateKeys.contains(DefaultsKey.usageStatsInstallId), "listed as machine state")
        suite.expect(keys.contains(DefaultsKey.shareUsageStats), "the switch itself travels")
        suite.expect(Defaults.registeredDefaults[DefaultsKey.shareUsageStats] as? Bool == true, "on by default")
    }
}
