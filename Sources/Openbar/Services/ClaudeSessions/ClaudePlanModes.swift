// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// The permission mode a plan is approved into, as Claude Code's own plan
/// dialog offers it. The raw value is the mode Claude Code takes back.
enum ClaudePlanMode: String, Equatable, CaseIterable {
    case bypass = "bypassPermissions"
    case auto
    case acceptEdits
    /// Claude asks before each edit, as Claude Code's "manually approve edits".
    case manual = "default"
}

/// Which modes a plan is approved into, read as Claude Code decides them:
/// one that lets Claude work on its own (bypass when the session runs with
/// it, else auto mode where it can be used, else auto-accept edits), then
/// manual approval. The first one is what a plain approval picks.
enum ClaudePlanModes {
    /// What the session and its settings tell about the modes it can use.
    struct Reading: Equatable {
        /// Modes the session has been in, from its transcript.
        var seen: Set<String> = []
        /// A settings file starts sessions in auto mode or configures it.
        var autoConfigured = false
        /// A settings file turns auto mode off.
        var autoDisabled = false
    }

    static func offered(_ reading: Reading) -> [ClaudePlanMode] {
        // A session only runs in bypass when it was launched able to.
        if reading.seen.contains(ClaudePlanMode.bypass.rawValue) { return [.bypass, .manual] }
        let auto = !reading.autoDisabled && (reading.seen.contains(ClaudePlanMode.auto.rawValue) || reading.autoConfigured)
        return [auto ? .auto : .acceptEdits, .manual]
    }

    /// Reads the session's transcript and the settings Claude Code reads.
    /// The config folder is the one the transcript sits in, so a session run
    /// with CLAUDE_CONFIG_DIR is read from its own.
    static func read(transcript: String?, cwd: String?, managed: [String] = managedSettings) -> Reading {
        var reading = Reading()
        var settings = managed
        if let transcript, !transcript.isEmpty {
            reading.seen = modes(inTranscript: transcript)
            // <config>/projects/<project>/<session>.jsonl
            let config = URL(fileURLWithPath: transcript).deletingLastPathComponent().deletingLastPathComponent()
                .deletingLastPathComponent()
            settings.append(config.appendingPathComponent("settings.json").path)
        }
        if let cwd, !cwd.isEmpty {
            let project = URL(fileURLWithPath: cwd).appendingPathComponent(".claude")
            settings += [project.appendingPathComponent("settings.json").path,
                         project.appendingPathComponent("settings.local.json").path]
        }
        for path in settings {
            guard let data = FileManager.default.contents(atPath: path),
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { continue }
            let permissions = json["permissions"] as? [String: Any] ?? [:]
            if permissions["disableAutoMode"] as? String == "disable" || json["disableAutoMode"] as? String == "disable" {
                reading.autoDisabled = true
            }
            if permissions["defaultMode"] as? String == ClaudePlanMode.auto.rawValue || json["autoMode"] is [String: Any] {
                reading.autoConfigured = true
            }
        }
        return reading
    }

    static let managedSettings = ["/Library/Application Support/ClaudeCode/managed-settings.json"]

    /// The modes a transcript records the session in. Claude Code writes them
    /// compact, as `"permissionMode":"auto"`, on its mode lines and prompts.
    static func modes(inTranscript path: String) -> Set<String> {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .alwaysMapped) else { return [] }
        var found: Set<String> = []
        for mode in ClaudePlanMode.allCases where data.range(of: Data(#""permissionMode":"\#(mode.rawValue)""#.utf8)) != nil {
            found.insert(mode.rawValue)
        }
        return found
    }
}
