// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The first run about agent sessions (#168): its setup and the sample
/// request Try it puts in the island.
enum OnboardingAgentTests {
    static func run(_ suite: TestSuite) {
        suite.run("onboarding agents setup") { setup(suite) }
        suite.run("onboarding sample request") { sample(suite) }
        suite.run("onboarding strings") { strings(suite) }
    }

    static func setup(_ suite: TestSuite) {
        suite.expect(FeaturePreset.allCases.first == .agents && FeaturePreset.recommended == .agents,
                     "the AI agents setup leads and is the one picked for a new install")
        suite.expect(FeaturePreset.agents.features == [.notch, .notchAgents],
                     "the AI agents setup installs the island and its agents feature, nothing else")
        suite.expect(Set(FeaturePreset.agents.enableKeys)
                        == [DefaultsKey.notchEnabled, DefaultsKey.notchSessionsEnabled, DefaultsKey.notchAgentsEnabled],
                     "the AI agents setup switches the island and its Sessions page on")
        let name = "com.openbar.tests.onboarding-agents.\(UUID().uuidString)"
        guard let defaults = UserDefaults(suiteName: name) else { return suite.expect(false, "defaults suite") }
        defer { defaults.removePersistentDomain(forName: name) }
        for feature in AppFeature.allCases { defaults.set(FeaturePreset.agents.features.contains(feature),
                                                          forKey: feature.availabilityKey) }
        for key in FeaturePreset.agents.enableKeys { defaults.set(true, forKey: key) }
        suite.expect(NotchClaudeSessionsSupport.isEnabled(in: defaults),
                     "after the AI agents setup, sessions run with no visit to Settings")
    }

    static func sample(_ suite: TestSuite) {
        let now = Date(timeIntervalSince1970: 1_792_600_000)
        let home = URL(fileURLWithPath: "/Users/someone")
        let waiting = ClaudeSessionSample.waiting(round: 3, home: home, now: now)
        suite.expect(waiting.id == ClaudeSessionSample.id && waiting.phase == .waitingPermission,
                     "the sample waits for permission, under its own id")
        suite.expect(waiting.permission?.requestID == "openbar-sample-3" && waiting.permission?.detail == "npm test",
                     "each showing gets its own request, with the command in full")
        suite.expect(waiting.hooked && waiting.pid == 0 && waiting.host == .unknown,
                     "the sample is answerable in the island but has no process or terminal")
        suite.expect(waiting.cwd == "/Users/someone/Projects/sample-app", "the sample lives in a made-up project")

        let allowed = ClaudeSessionSample.answered(waiting, allowed: true, now: now.addingTimeInterval(2))
        suite.expect(allowed.permission == nil && allowed.phase == .runningTool
                        && allowed.currentTool?.summary == "npm test",
                     "an allowed sample runs its command")
        let denied = ClaudeSessionSample.answered(waiting, allowed: false, now: now.addingTimeInterval(2))
        suite.expect(denied.permission == nil && denied.phase == .finished && denied.currentTool == nil,
                     "a denied sample stops")
        let finished = ClaudeSessionSample.finished(allowed, now: now.addingTimeInterval(4))
        suite.expect(finished.phase == .finished && finished.currentTool == nil
                        && finished.timeline.count == 1 && finished.timeline.first?.finished != nil,
                     "the command finishes into the timeline")
    }

    static func strings(_ suite: TestSuite) {
        for language in AppLanguage.allCases {
            let text = FeatureStrings.onboarding(language)
            let all = [text.welcomeTitle, text.welcomeBody, text.welcomeToolkit, text.setupTitle, text.setupAgentsName,
                       text.setupAgentsDesc, text.agentsTitle, text.agentsBody, text.permissionsTitle, text.tryTitle,
                       text.tryBody, text.tryButton, text.doneTitle, text.doneBody, text.skip]
            suite.expect(all.allSatisfy { !$0.trimmingCharacters(in: .whitespaces).isEmpty },
                         "onboarding speaks \(language.rawValue)")
            suite.expect(!all.contains { $0.localizedCaseInsensitiveContains("black hole") },
                         "no onboarding text names the old icon (\(language.rawValue))")
            if language != .enUS {
                suite.expect(text.welcomeTitle != OnboardingStrings.enUS.welcomeTitle,
                             "the welcome is translated (\(language.rawValue))")
            }
        }
    }
}
