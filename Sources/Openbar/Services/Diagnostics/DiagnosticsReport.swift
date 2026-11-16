// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// The parts of a diagnostics file (#230), in the order the screen before
/// saving lists them. That screen, the README inside the file and the files
/// written all come from this one list, so what the person is shown is what
/// the file holds. The Windows and Linux app has the same parts and file
/// names (`diagnostics/report.rs`).
enum DiagnosticsPart: String, CaseIterable, Codable {
    case system, features, agents, events, hosts, logs

    /// Parts the person can leave out; the others are always in.
    var isOptional: Bool { self == .events || self == .hosts || self == .logs }

    /// The file this part is written to; the logs are a folder of files.
    var fileName: String {
        switch self {
        case .logs: return "logs/"
        default: return "\(rawValue).json"
        }
    }

    /// What the part holds, in the README every report carries (English, for
    /// whoever reads the report).
    var readme: String {
        switch self {
        case .system: return "Openbar version and update channel, the system version, the CPU type and the displays."
        case .features: return "Which of Openbar's features are on."
        case .agents: return "Each coding agent: installed or not, its version, the state of Openbar's hooks in it and where its settings live."
        case .events: return "The last \(DiagnosticsJournal.capacity) events Openbar heard from sessions: event type, agent, time and state change."
        case .hosts: return "The terminals and editors found, and Claude, Codex and remote host names (labels only)."
        case .logs: return "Openbar's own logs from the last \(DiagnosticsLog.exportDays) days."
        }
    }
}

struct DiagnosticsSystem: Codable, Equatable {
    struct App: Codable, Equatable {
        let name: String
        let version: String
        var build: String?
        var commit: String?
        let channel: String
        let developerBuild: Bool
    }

    struct OS: Codable, Equatable {
        let name: String
        let version: String
        var build: String?
        var kernel: String?
        /// Linux only: X11 or Wayland, and the desktop.
        var session: String?
        var desktop: String?
    }

    struct Display: Codable, Equatable {
        let width: Int
        let height: Int
        let scale: Double
        let main: Bool
        var builtIn: Bool?
        var notch: Bool?
    }

    let app: App
    let os: OS
    let cpu: String
    var model: String?
    let language: String
    let displays: [Display]
}

struct DiagnosticsFeature: Codable, Equatable {
    let id: String
    let group: String
    let on: Bool
}

struct DiagnosticsAgent: Codable, Equatable {
    struct Hooks: Codable, Equatable {
        /// The Claude account the hooks are in, by its label.
        var account: String?
        let state: String
        var configPath: String?
    }

    let id: String
    let name: String
    let installed: Bool
    var version: String?
    /// How Openbar hears the agent, where that isn't hooks (Codex).
    var status: String?
    var hooks: [Hooks]
}

struct DiagnosticsHosts: Codable, Equatable {
    struct Editor: Codable, Equatable {
        let name: String
        /// Openbar's extension for typing replies: installed, outdated or missing.
        let bridge: String
    }

    struct Remote: Codable, Equatable {
        let label: String
        let state: String
    }

    var terminals: [String]
    var editors: [Editor]
    /// How many sessions run in each terminal or editor now.
    var inUse: [String: Int]
    var claudeAccounts: [String]
    var codexAccounts: [String]
    var remoteHosts: [Remote]
}

struct DiagnosticsManifest: Codable, Equatable {
    let format: Int
    let created: String
    let platform: String
    let app: String
    let parts: [String]
    let files: [String]
}

/// What a diagnostics file is made from, once collected.
struct DiagnosticsInput {
    var system: DiagnosticsSystem
    var features: [DiagnosticsFeature]
    var agents: [DiagnosticsAgent]
    var events: [DiagnosticsEvent]
    var hosts: DiagnosticsHosts
    /// Log files by name, oldest first.
    var logs: [(name: String, text: String)]
}

enum DiagnosticsReport {
    static let format = 1
    /// The folder every file sits in inside the zip.
    static let folder = "openbar-diagnostics"

    /// `openbar-diagnostics-20261116-1530.zip`, in local time.
    static func fileName(now: Date = Date(), timeZone: TimeZone = .current) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = timeZone
        let c = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: now)
        return String(format: "openbar-diagnostics-%04d%02d%02d-%02d%02d.zip", c.year ?? 0, c.month ?? 0, c.day ?? 0,
                      c.hour ?? 0, c.minute ?? 0)
    }

    /// Every file of the report, by its path inside the zip's folder, masked.
    /// The parts not chosen are left out entirely; the README and the
    /// manifest say which went in.
    static func files(parts chosen: Set<DiagnosticsPart>, input: DiagnosticsInput, platform: String, now: Date,
                      home: String) -> [(path: String, data: Data)] {
        let parts = DiagnosticsPart.allCases.filter { !$0.isOptional || chosen.contains($0) }
        var files: [(path: String, data: Data)] = []
        for part in parts {
            switch part {
            case .system: files.append((part.fileName, json(input.system, home: home)))
            case .features: files.append((part.fileName, json(["features": input.features], home: home)))
            case .agents: files.append((part.fileName, json(["agents": input.agents], home: home)))
            case .events: files.append((part.fileName, json(["events": input.events], home: home)))
            case .hosts: files.append((part.fileName, json(input.hosts, home: home)))
            case .logs:
                for log in input.logs {
                    files.append(("logs/\(log.name)", Data(redactLines(log.text, home: home).utf8)))
                }
            }
        }
        let app = "\(input.system.app.name) \(input.system.app.version)"
        let created = DiagnosticsLog.timestamp(now)
        let readme = readme(parts: parts, platform: platform, app: app, created: created,
                            logs: input.logs.map(\.name))
        files.insert(("README.txt", Data(readme.utf8)), at: 0)
        let listed = (files.map(\.path) + ["manifest.json"]).sorted()
        let manifest = DiagnosticsManifest(format: format, created: created, platform: platform, app: app,
                                           parts: parts.map(\.rawValue), files: listed)
        files.insert(("manifest.json", json(manifest, home: home)), at: 1)
        return files
    }

    static func readme(parts: [DiagnosticsPart], platform: String, app: String, created: String, logs: [String]) -> String {
        var lines = ["Openbar diagnostics", "",
                     "Made by \(app) on \(platform), \(created).", "",
                     "What is inside:"]
        for part in parts {
            if part == .logs {
                lines.append("- logs/: \(part.readme)" + (logs.isEmpty ? " (none were written yet)" : ""))
            } else {
                lines.append("- \(part.fileName): \(part.readme)")
            }
        }
        let left = DiagnosticsPart.allCases.filter { !parts.contains($0) }
        if !left.isEmpty {
            lines += ["", "Left out before saving:"]
            lines += left.map { "- \($0.fileName): \($0.readme)" }
        }
        lines += ["",
                  "Never included: prompts, replies, commands and file contents.",
                  "Masked everywhere: tokens and API keys (<secret>), email addresses (<email>) and the home folder (~).",
                  "",
                  "Openbar sent this file nowhere. Whoever holds it decides where it goes.",
                  ""]
        return lines.joined(separator: "\n")
    }

    static func json<T: Encodable>(_ value: T, home: String) -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        guard let data = try? encoder.encode(value) else { return Data("{}".utf8) }
        return Data(DiagnosticsRedactor.redact(String(decoding: data, as: UTF8.self), home: home).utf8)
    }

    /// A log masked once more, line by line, in case it came from an older
    /// build or was edited.
    static func redactLines(_ text: String, home: String) -> String {
        text.split(separator: "\n", omittingEmptySubsequences: false)
            .map { DiagnosticsRedactor.redact(String($0), home: home) }
            .joined(separator: "\n")
    }

    /// The version in a command's `--version` output, like `2.1.309` or `0.164.0-beta.2`.
    static func version(in output: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: #"\d+\.\d+(?:\.\d+)?(?:-[0-9A-Za-z.]+)?"#),
              let match = regex.firstMatch(in: output, range: NSRange(output.startIndex..., in: output)),
              let range = Range(match.range, in: output) else { return nil }
        return String(output[range])
    }
}
