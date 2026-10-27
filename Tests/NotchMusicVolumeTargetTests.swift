// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum NotchMusicVolumeTargetTests {
    static func run(_ suite: TestSuite) {
        typealias Source = NotchMusicVolumeTarget.Source
        let player = Source(id: "player", pid: 42, bundle: "org.example.player")
        let browser = Source(id: "browser", pid: 99, bundle: "org.example.browser")
        func resolve(_ pid: Int32?, _ bundle: String?, _ sources: [Source] = []) -> String? {
            NotchMusicVolumeTarget.resolve(pid: pid, bundle: bundle, sources: sources)
        }
        suite.expect(
            resolve(42, nil, [player, browser]) == "player",
            "the selected player uses its exact mixer process")
        suite.expect(
            resolve(7, "org.example.browser", [player, browser]) == "browser",
            "a browser's media helper resolves to its owning browser audio row")
        suite.expect(
            resolve(nil, "org.example.player", [player]) == "player",
            "bundle identity works when the player omits its pid")
        suite.expect(
            resolve(123, "org.example.unknown", [player, browser]) == nil,
            "an unsupported player never adjusts another app or system volume")
        suite.expect(
            resolve(nil, nil, [player]) == nil && resolve(0, "", [player]) == nil
                && resolve(42, "org.example.player") == nil,
            "missing identities and missing mixer rows remain unavailable")
        suite.expect(
            resolve(42, "org.example.player", [Source(id: "other", pid: 42, bundle: "org.example.other")])
                == nil,
            "PID reuse cannot redirect a player's slider to an unrelated app")
        suite.expect(
            resolve(
                42, "org.example.player",
                [
                    Source(id: "other", pid: 42, bundle: "org.example.other"),
                    Source(id: "right", pid: 77, bundle: "org.example.player"),
                ]) == "right",
            "a reused PID still permits the correct unambiguous bundle owner")
        let bypassed = Source(id: "bypassed", pid: 42, bundle: "org.example.player", bypassed: true)
        suite.expect(
            resolve(42, "org.example.player", [bypassed]) == nil,
            "bypassed conferencing and pro-audio apps remain untouched")
        let duplicate = Source(id: "another", pid: 43, bundle: "org.example.player")
        suite.expect(
            resolve(nil, "org.example.player", [player, duplicate]) == nil,
            "ambiguous bundle matches remain unavailable rather than selecting an arbitrary process")
        suite.expect(
            resolve(42, "org.example.player", [player, duplicate]) == "player",
            "an exact process resolves otherwise ambiguous bundle matches")
        suite.expect(
            resolve(
                42, "org.example.player", [player, Source(id: "duplicate-pid", pid: 42, bundle: nil)])
                == nil,
            "duplicate process ownership does not choose an arbitrary gain engine")
    }
}
