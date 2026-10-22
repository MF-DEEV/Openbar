// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClawdCostumeTests {
    static func run(_ suite: TestSuite) {
        suite.run("clawd costume lot") { lot(suite) }
        suite.run("clawd costume keeping") { keeping(suite) }
        suite.run("clawd motions and pages") { motions(suite) }
        suite.run("clawd strings") { strings(suite) }
    }

    static func lot(_ suite: TestSuite) {
        // FNV-1a, not Swift's Hasher, which is seeded anew every launch.
        suite.expect(ClawdCostume.seed("") == 0xcbf2_9ce4_8422_2325 && ClawdCostume.seed("a") == 0xaf63_dc4c_8601_ec8c,
                     "a session's number is FNV-1a over its id, the same on every launch")
        let id = "99c35831-5f0b-4c3e-9d0a-0f3c2a7b8e11"
        suite.expect(ClawdCostume.seeded(id) == ClawdCostume.seeded(id), "a session always draws the same costume")
        var generator = ClawdRandom(seed: 42)
        let first = (0..<4).map { _ in generator.next() }
        generator = ClawdRandom(seed: 42)
        suite.expect(first == (0..<4).map { _ in generator.next() } && Set(first).count == 4,
                     "the lot gives the same numbers for the same seed, and not the same number twice")
        generator = ClawdRandom(seed: 0)
        suite.expect(generator.next() == 0xe220_a839_7b1d_cdaf && generator.next() == 0x6e78_9e6a_a1b9_65f4,
                     "the lot is SplitMix64, giving its published numbers")
        // A session nobody dressed wears its lot. Drawing in another order, or
        // a part added in the middle of a list, would change every such
        // Clawd at the next update.
        suite.expect(ClawdCostume.seeded(id).code == "v1:teal.none.monocle.none"
                     && ClawdCostume.seeded("0f9a6c1e-2b7d-4e55-8a31-c4d2e6f7a890").code == "v1:orchid.beret.anaglyph.coffee"
                     && ClawdCostume.seeded("s/a1").code == "v1:clay.propeller.none.scarf",
                     "a session draws the same costume from one version of the app to the next: \(ClawdCostume.seeded(id).code)")

        let costumes = (0..<600).map { ClawdCostume.seeded("session-\($0)") }
        suite.expect(Set(costumes.map(\.shell)) == Set(ClawdShell.allCases), "every shell comes up")
        suite.expect(Set(costumes.map(\.hat)) == Set(ClawdHat.allCases)
                     && Set(costumes.map(\.eyewear)) == Set(ClawdEyewear.allCases)
                     && Set(costumes.map(\.extra)) == Set(ClawdExtra.allCases),
                     "every hat, pair of eyewear and extra comes up, and so does nothing")
        func share(_ count: Int) -> Double { Double(count) / Double(costumes.count) }
        let bare = (hat: share(costumes.filter { $0.hat == .none }.count),
                    eyewear: share(costumes.filter { $0.eyewear == .none }.count),
                    extra: share(costumes.filter { $0.extra == .none }.count))
        suite.expect(abs(bare.hat - (1 - ClawdCostume.hatChance)) < 0.08
                     && abs(bare.eyewear - (1 - ClawdCostume.eyewearChance)) < 0.08
                     && abs(bare.extra - (1 - ClawdCostume.extraChance)) < 0.08,
                     "each part is left off about as often as meant: \(bare)")
        let clay = share(costumes.filter { $0.shell == .clay }.count)
        let other = share(costumes.filter { $0.shell == .sky }.count)
        suite.expect(clay > other * 1.8, "Claude's own color comes up more often than any other: \(clay) to \(other)")
        suite.expect(Set(costumes).count > 400, "sessions are told apart by their costumes: \(Set(costumes).count) of 600")

        let current = ClawdCostume.seeded(id)
        suite.expect((0..<30).allSatisfy { _ in ClawdCostume.random(excluding: current) != current },
                     "Surprise me always changes something")

        let colors = ["red", "orange", "yellow", "green", "cyan", "blue", "purple", "pink"]
        let shells = colors.compactMap(ClawdShell.init(agentColor:))
        suite.expect(shells == [.coral, .clay, .amber, .moss, .teal, .sky, .iris, .rose],
                     "Claude Code's eight agent colors each have their shell: \(shells)")
        suite.expect(ClawdShell(agentColor: " Blue\n") == .sky && ClawdShell(agentColor: "teal") == nil
                     && ClawdShell(agentColor: "") == nil,
                     "a color is read whatever its case and spacing, and one Claude Code does not offer is not")
        let agent = ClawdCostume.seeded("s/a1", agentColor: "green")
        var drawn = ClawdCostume.seeded("s/a1")
        drawn.shell = .moss
        suite.expect(agent == drawn, "a subagent keeps its own lot and only takes its color's shell")
        suite.expect(ClawdCostume.seeded("s/a1", agentColor: nil) == ClawdCostume.seeded("s/a1")
                     && ClawdCostume.seeded("s/a1", agentColor: "mauve") == ClawdCostume.seeded("s/a1"),
                     "without a color it Claude Code knows, the lot decides the shell too")
    }

    static func keeping(_ suite: TestSuite) {
        let costume = ClawdCostume(shell: .sky, hat: .topHat, eyewear: .shades, extra: .bowTie)
        suite.expect(costume.code == "v1:sky.topHat.shades.bowTie", "a costume keeps as a short versioned line: \(costume.code)")
        suite.expect(ClawdCostume(code: costume.code, fallback: .classic) == costume, "and reads back the same")
        for costume in [ClawdCostume.classic] + (0..<50).map({ ClawdCostume.seeded("k\($0)") }) {
            suite.expect(ClawdCostume(code: costume.code, fallback: .classic) == costume, "\(costume.code) round trips")
        }
        let fallback = ClawdCostume(shell: .rose, hat: .crown, eyewear: .monocle, extra: .coffee)
        suite.expect(ClawdCostume(code: "v1:sky.sombrero.shades.cape", fallback: fallback)
                     == ClawdCostume(shell: .sky, hat: .crown, eyewear: .shades, extra: .coffee),
                     "a part a later version added comes from the session's own lot, the rest stays")
        for garbage in ["", "sky.cap.none.none", "v2:sky.cap.none.none", "v1:sky.cap.none", "v1:sky.cap.none.none.bow",
                        "v1:" + String(repeating: "x", count: 200)] {
            suite.expect(ClawdCostume(code: garbage, fallback: fallback) == nil, "unreadable \(garbage.prefix(20)) is ignored")
        }
        let id = "session-1"
        suite.expect(ClawdCostume.resolved(key: id, kept: nil) == ClawdCostume.seeded(id)
                     && ClawdCostume.resolved(key: id, kept: "nonsense") == ClawdCostume.seeded(id)
                     && ClawdCostume.resolved(key: id, kept: costume.code) == costume,
                     "a session wears what the person chose for it, or else its own lot")
        suite.expect(ClawdCostume.kept(ClawdCostume.seeded(id), key: id) == nil && ClawdCostume.kept(nil, key: id) == nil,
                     "dressing a session in its own lot keeps nothing")
        let other = ClawdCostume.random(excluding: ClawdCostume.seeded(id))
        suite.expect(ClawdCostume.kept(other, key: id) == other.code, "anything else is kept")

        var kept = NotchClaudeSessionsSupport.kept([:], id: "a", value: "v1:sky.cap.none.none", live: ["a"])
        suite.expect(kept == ["a": "v1:sky.cap.none.none"], "a choice is kept by session")
        kept = NotchClaudeSessionsSupport.kept(kept, id: "a", value: nil, live: ["a"])
        suite.expect(kept.isEmpty, "and going back to its own lot removes it")
        let many = Dictionary(uniqueKeysWithValues: (0..<NotchClaudeSessionsSupport.namesKept).map { ("old\($0)", "x") })
        kept = NotchClaudeSessionsSupport.kept(many.merging(["live": "keep"]) { a, _ in a }, id: "new", value: "fresh",
                                               live: ["live", "new"])
        suite.expect(kept.count == NotchClaudeSessionsSupport.namesKept && kept["live"] == "keep" && kept["new"] == "fresh",
                     "past the limit, choices for sessions no longer running go first")

        let exported = SettingsBackupSupport.exportKeys()
        suite.expect(exported.contains(DefaultsKey.notchSessionsBuddies), "the switch for Clawds travels with a backup")
        suite.expect(!exported.contains(DefaultsKey.notchSessionsCostumes) && !exported.contains(DefaultsKey.notchSessionsNames),
                     "costumes and names, kept by this Mac's session ids, do not")
        suite.expect(Defaults.registeredDefaults[DefaultsKey.notchSessionsBuddies] as? Bool == true,
                     "Clawds are on until the person turns them off")
        let defaults = UserDefaults(suiteName: "com.vorssaint.tests.clawd")!
        defaults.removePersistentDomain(forName: "com.vorssaint.tests.clawd")
        defer { defaults.removePersistentDomain(forName: "com.vorssaint.tests.clawd") }
        suite.expect(NotchClaudeSessionsSupport.showsBuddies(in: defaults), "with nothing stored, Clawds show")
        defaults.set(false, forKey: DefaultsKey.notchSessionsBuddies)
        suite.expect(!NotchClaudeSessionsSupport.showsBuddies(in: defaults), "and the switch hides them")
    }

    static func motions(_ suite: TestSuite) {
        let now = ClaudeSessionEngineTests.at(1000)
        func session(_ phase: ClaudeSessionPhase, since: TimeInterval = 990, active: TimeInterval? = nil) -> ClaudeSession {
            var session = ClaudeSession(id: "s", pid: 1, cwd: "/x/p", project: "p", name: "p", host: .unknown, phase: phase,
                                        phaseSince: ClaudeSessionEngineTests.at(since), timeline: [], subagents: [],
                                        tokens: AgentTokens(), cost: 0, hooked: false)
            session.lastActive = active.map(ClaudeSessionEngineTests.at)
            return session
        }
        let pairs: [(ClaudeSessionPhase, NotchClawdMotion)] = [
            (.generating, .think), (.runningTool, .work), (.background, .slow), (.waitingPermission, .alert),
            (.waitingInput, .ask), (.finished, .content),
        ]
        for (phase, motion) in pairs {
            suite.expect(NotchClaudeSessionsSupport.motion(session(phase), now: now) == motion, "\(phase) is \(motion)")
        }
        suite.expect(NotchClaudeSessionsSupport.motion(session(.idle, since: 990, active: 900), now: now) == .rest,
                     "a session quiet for a few minutes, as one just read, stays awake")
        suite.expect(NotchClaudeSessionsSupport.motion(session(.idle, since: 0, active: 1000 - NotchClaudeSessionsSupport.dozeAfter - 1),
                                                       now: now) == .doze,
                     "a session quiet for long dozes off")
        suite.expect(NotchClaudeSessionsSupport.motion(session(.idle, since: 0), now: now) == .doze,
                     "without a time it last worked, how long it has been idle counts")
        suite.expect(NotchClaudeSessionsSupport.cheer(session(.finished, since: 995)) == ClaudeSessionEngineTests.at(995)
                     && NotchClaudeSessionsSupport.cheer(session(.idle)) == nil,
                     "a session's Clawd hops from when its work ended, and only then")
        let running = ClaudeSubagent(id: "a1", type: "Explore", started: now)
        var done = running
        done.finished = now
        suite.expect(NotchClaudeSessionsSupport.motion(running) == .work && NotchClaudeSessionsSupport.motion(done) == .done,
                     "a subagent works while it runs and rests once done")
        suite.expect(NotchClaudeSessionsSupport.costumeKey(running, in: session(.idle)) == "s/a1",
                     "a subagent draws its costume from its session and its own id")
        suite.expect(NotchClaudeSessionsSupport.noticeMotion(symbol: NotchClaudeSessionsSupport.symbol(.finished)) == .content
                     && NotchClaudeSessionsSupport.noticeMotion(symbol: NotchClaudeSessionsSupport.symbol(.waitingInput)) == .ask
                     && NotchClaudeSessionsSupport.noticeMotion(symbol: NotchClaudeSessionsSupport.symbol(.waitingPermission)) == .alert,
                     "a notice's Clawd matches what the notice says")

        typealias Layer = NotchClaudeSessionsSupport.PageLayer
        suite.expect(NotchClaudeSessionsSupport.pageLayer(focused: nil, dressing: nil) == nil
                     && NotchClaudeSessionsSupport.pageLayer(focused: "a", dressing: nil) == Layer.session
                     && NotchClaudeSessionsSupport.pageLayer(focused: "a", dressing: "a") == Layer.wardrobe
                     && NotchClaudeSessionsSupport.pageLayer(focused: "b", dressing: "a") == Layer.session
                     && NotchClaudeSessionsSupport.pageLayer(focused: nil, dressing: "a") == nil,
                     "Escape closes the wardrobe, then the page, then the island; a wardrobe left over from another page does not count")
    }

    static func strings(_ suite: TestSuite) {
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        let shells = ClawdShell.allCases.map(text.name)
        let hats = ClawdHat.allCases.map(text.name)
        let eyewear = ClawdEyewear.allCases.map(text.name)
        let extras = ClawdExtra.allCases.map(text.name)
        for names in [shells, hats, eyewear, extras] {
            suite.expect(names.allSatisfy { !$0.isEmpty } && Set(names).count == names.count,
                         "every part has a name of its own: \(names)")
        }
        suite.expect(Set(shells).isDisjoint(with: ClawdShell.allCases.map(\.rawValue))
                     && hats.dropFirst().allSatisfy { name in !ClawdHat.allCases.map(\.rawValue).contains(name) },
                     "names are words for people, not the keys they are stored by")
        suite.expect(hats.first == text.nothing && eyewear.first == text.nothing && extras.first == text.nothing,
                     "taking a part off is called the same in every row")
        suite.expect(text.dressUp("vorsaint") == "Change vorsaint’s outfit", "the Clawd's button names its session")
    }
}
