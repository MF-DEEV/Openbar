// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Commands a new session may run from its first turn without asking (#270):
/// the project's build, test and lint, and git's read-only three, plus any
/// the person adds. Claude Code gets them as `--allowedTools` rules; Codex
/// threads get their approvals answered "for this session" by the island.
/// Anything else still asks on the island. The Windows and Linux app does
/// the same in safe_commands.rs; both run `safe_command_cases.json`.
enum SafeCommands {
    enum Kind: String, Codable, CaseIterable {
        case build, test, lint, git, custom

        /// The SF Symbol its chip shows.
        var symbol: String {
            switch self {
            case .build: return "hammer"
            case .test: return "checkmark.seal"
            case .lint: return "wand.and.stars"
            case .git: return "arrow.triangle.branch"
            case .custom: return "terminal"
            }
        }
    }

    struct Entry: Codable, Hashable, Identifiable {
        let command: String
        let kind: Kind
        var id: String { command }
    }

    /// The agents whose sessions can start with the list.
    static func supports(_ provider: ClaudeSessionProvider) -> Bool {
        provider == .claude || provider == .codex
    }

    static let git = ["git status", "git diff", "git log"].map { Entry(command: $0, kind: .git) }

    /// What a folder suggests, read from the files at its top: a package
    /// manager's scripts, Cargo, SwiftPM, Go or make; then git's three.
    static func suggested(for folder: String, fileManager: FileManager = .default) -> [Entry] {
        let root = URL(fileURLWithPath: folder)
        func has(_ name: String) -> Bool { fileManager.fileExists(atPath: root.appendingPathComponent(name).path) }
        var found: [Entry] = []
        func add(_ command: String, _ kind: Kind) {
            if !found.contains(where: { $0.command == command }) { found.append(Entry(command: command, kind: kind)) }
        }
        if let data = try? Data(contentsOf: root.appendingPathComponent("package.json")),
           let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] {
            let scripts = json["scripts"] as? [String: Any] ?? [:]
            let manager = has("bun.lockb") || has("bun.lock") ? "bun"
                : has("pnpm-lock.yaml") ? "pnpm" : has("yarn.lock") ? "yarn" : "npm"
            for (script, kind) in [("build", Kind.build), ("test", .test), ("lint", .lint), ("typecheck", .lint)]
            where scripts[script] != nil {
                add("\(manager) run \(script)", kind)
            }
        }
        if has("Cargo.toml") {
            add("cargo build", .build); add("cargo test", .test); add("cargo clippy", .lint)
        }
        if has("Package.swift") {
            add("swift build", .build); add("swift test", .test)
        }
        if has("go.mod") {
            add("go build", .build); add("go test", .test); add("go vet", .lint)
        }
        if let makefile = try? String(contentsOf: root.appendingPathComponent("Makefile"), encoding: .utf8) {
            let targets = Set(makefile.split(separator: "\n").compactMap { line -> String? in
                guard let colon = line.firstIndex(of: ":"), !line.hasPrefix("\t"), !line.hasPrefix(" ") else { return nil }
                return String(line[..<colon]).trimmingCharacters(in: .whitespaces)
            })
            if !found.contains(where: { $0.kind == .build }) { add("make", .build) }
            if targets.contains("test") { add("make test", .test) }
            if targets.contains("lint") { add("make lint", .lint) }
        }
        return found + git
    }

    /// Characters that chain, redirect or substitute: a command with any of
    /// them is never one of the list, so `git status; rm -rf ~` still asks.
    static let joiners: Set<Character> = [";", "&", "|", "<", ">", "`", "$", "(", ")", "\n", "\r", "{", "}"]

    /// A command a person may add: one line, plain words, no joiners, short.
    static func cleaned(_ text: String) -> String? {
        let words = text.split(whereSeparator: \.isWhitespace)
        let command = words.joined(separator: " ")
        guard !command.isEmpty, command.count <= 80, !command.contains(where: joiners.contains),
              !command.contains("*"), !command.contains("\""), !command.contains("'"), !command.contains("\\")
        else { return nil }
        return command
    }

    /// Claude Code's rules for the list: each a prefix of a Bash command.
    static func claudeRules(_ commands: [String]) -> [String] {
        commands.compactMap(cleaned).map { "Bash(\($0):*)" }
    }

    /// Whether a command a Codex approval names is one of the list: its
    /// script, unwrapped from the shell Codex runs it in, has no joiners and
    /// starts with all the words of an entry.
    static func matches(_ command: String, allowed: [String]) -> Bool {
        let script = CodexSessionState.approvalScript(command) ?? command
        guard !script.contains(where: joiners.contains), let words = CodexSessionState.shellWords(script), !words.isEmpty
        else { return false }
        return allowed.compactMap(cleaned).contains { entry in
            let wanted = entry.split(separator: " ").map(String.init)
            return words.count >= wanted.count && Array(words.prefix(wanted.count)) == wanted
        }
    }
}

/// Which Codex threads run with a list, and lists waiting for the thread a
/// terminal is about to start. Guarded by its owner's lock.
struct SafeCommandGrants {
    struct Waiting {
        let cwd: String
        let commands: [String]
        let until: Date
        /// The threads known when it was made: the terminal's one is new.
        let known: Set<String>
    }

    private(set) var threads: [String: [String]] = [:]
    private(set) var waiting: [Waiting] = []

    static let claimWindow: TimeInterval = 120

    mutating func grant(_ thread: String, _ commands: [String]) {
        guard !commands.isEmpty else { return }
        threads[thread] = commands
    }

    mutating func expect(cwd: String, commands: [String], now: Date, known: Set<String>) {
        guard !commands.isEmpty else { return }
        waiting.append(Waiting(cwd: Self.standard(cwd), commands: commands, until: now.addingTimeInterval(Self.claimWindow),
                               known: known))
    }

    /// The list a thread runs with: its own, or a waiting one it is the
    /// first new thread in that folder to claim.
    mutating func commands(for thread: String, cwd: String, now: Date) -> [String]? {
        if let own = threads[thread] { return own }
        waiting.removeAll { $0.until < now }
        guard let index = waiting.firstIndex(where: { $0.cwd == Self.standard(cwd) && !$0.known.contains(thread) })
        else { return nil }
        let claimed = waiting.remove(at: index)
        threads[thread] = claimed.commands
        return claimed.commands
    }

    private static func standard(_ path: String) -> String {
        URL(fileURLWithPath: path).standardizedFileURL.path
    }
}
