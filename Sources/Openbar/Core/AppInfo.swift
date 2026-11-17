// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Static identity of the app, shared by UI, notifications and tooling.
enum AppInfo {
    static let name = "Openbar"
    /// Openbar is built on Vorssaint under the GPL; its notice stays beside ours.
    static let copyright = "© 2026 MF DEV, Adrian and the Openbar contributors. Based on Vorssaint, © 2026 Vorssaint."
    static let websiteURL = URL(string: "https://github.com/MF-DEEV/Openbar")!
    static let repositoryURL = URL(string: "https://github.com/MF-DEEV/Openbar")!
    /// Openbar has no donation page or Discord server of its own yet; the
    /// buttons that would open them stay hidden while these are nil.
    static let coffeeURL: URL? = nil
    static let discordURL: URL? = nil
    static let socialURL = URL(string: "https://x.com/_Openbar_")!

    /// The bundle version. The fallback only applies to the bare binary
    /// (e.g. `--selftest`), never the shipped app, which reads its Info.plist.
    static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }

    /// True for the local "Openbar (Developer)" build (bundle id ends in `.dev`).
    /// It is never published and never auto-updates; all work is tested here first.
    static var isDeveloperBuild: Bool {
        (Bundle.main.bundleIdentifier ?? "").hasSuffix(".dev")
    }

    /// True when the current version is a pre-release (e.g. 3.3.4-beta.1 or 3.3.4-rc.1).
    static var isBeta: Bool {
        if isDeveloperBuild && UserDefaults.standard.bool(forKey: DefaultsKey.simulateBetaUI) {
            return true
        }
        let v = version.lowercased()
        return v.contains("-beta") || v.contains("-rc") || v.contains("-alpha")
    }

    /// The git commit a Developer build was compiled from, e.g. "ed2ebba · 2026-06-15 21:30"
    /// (or with a "-dirty" suffix on the SHA for uncommitted changes). build.sh stamps
    /// this into the Developer bundle only, so you can confirm at a glance that the
    /// running dev app matches the source you are about to change. nil in the official app.
    static var buildCommit: String? {
        Bundle.main.object(forInfoDictionaryKey: "OpenbarBuildCommit") as? String
    }
}
