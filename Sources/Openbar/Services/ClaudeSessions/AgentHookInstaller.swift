// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Puts the island's hooks into Gemini CLI, Copilot CLI and Cursor, and its
/// plugin into OpenCode, and takes them out again, only when the person
/// presses Install or Remove. Where the agent keeps every hook in one file
/// (Gemini's settings, Cursor's hooks.json) the island's entries go beside
/// the person's own, carry its marker, and the file is copied aside first,
/// as for Claude Code. Where the agent reads a folder of files (Copilot's
/// hooks, OpenCode's plugins) the island writes a file of its own.
///
/// Each hook is the system's `nc` writing the event, wrapped in an envelope
/// naming the agent and the hook, to the island's socket. When the app is
/// not running it connects to nothing and prints nothing, and the agent
/// carries on as if there were no hook. None of the hooks installed asks
/// anything of its answer, so none ever holds the agent up.
enum AgentHookInstaller {
    static let eventTimeout = 5

    static let geminiEvents = ["SessionStart", "SessionEnd", "BeforeAgent", "AfterAgent", "BeforeTool", "AfterTool",
                               "Notification"]
    static let copilotEvents = ["sessionStart", "sessionEnd", "userPromptSubmitted", "preToolUse", "postToolUse",
                                "postToolUseFailure", "agentStop", "errorOccurred", "notification"]
    /// Cursor blocks an action when a hook that may allow or deny it answers
    /// with nothing, so only hooks that only tell are installed. The prompt's
    /// hook answers that the prompt goes on.
    static let cursorEvents = ["sessionStart", "sessionEnd", "beforeSubmitPrompt", "afterAgentThought", "afterAgentResponse",
                               "postToolUse", "postToolUseFailure", "stop"]
    static let cursorContinues: Set<String> = ["beforeSubmitPrompt"]

    typealias Failure = ClaudeHookInstaller.Failure

    // MARK: Places

    static func geminiSettings(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        let base = environment["GEMINI_CLI_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home
        return base.appending(path: ".gemini/settings.json")
    }

    /// Files of the island's own are named after the app, so the Developer
    /// build and the release never write over each other's.
    static func ownName(_ bundle: String) -> String {
        (LegacyIdentity.isLegacy(bundle) ? "vorssaint-" : "openbar-") + bundle
    }

    static func copilotFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["COPILOT_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".copilot")
    }

    static func copilotHooks(home: URL, bundle: String,
                             environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        copilotFolder(home: home, environment: environment).appending(path: "hooks/\(ownName(bundle)).json")
    }

    static func cursorHooks(home: URL) -> URL { home.appending(path: ".cursor/hooks.json") }

    static func opencodeFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        let config = environment["XDG_CONFIG_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
            ?? home.appending(path: ".config")
        return config.appending(path: "opencode")
    }

    static func opencodePlugin(home: URL, bundle: String,
                               environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        opencodeFolder(home: home, environment: environment).appending(path: "plugins/\(ownName(bundle)).js")
    }

    /// The folder whose presence says the agent was used on this Mac.
    static func agentFolder(_ provider: ClaudeSessionProvider, home: URL,
                            environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
        switch provider {
        case .gemini: return geminiSettings(home: home, environment: environment).deletingLastPathComponent()
        case .copilot: return copilotFolder(home: home, environment: environment)
        case .cursor: return cursorHooks(home: home).deletingLastPathComponent()
        case .opencode: return opencodeFolder(home: home, environment: environment)
        case .claude, .codex: return nil
        }
    }

    /// Whether the agent is installed: its command on the login shell's
    /// `PATH` or where its installers put it, or, for Cursor and OpenCode,
    /// their app. A folder like `~/.gemini` or `~/.copilot` is not enough:
    /// other apps (Antigravity, VS Code) make those too.
    static func isPresent(_ provider: ClaudeSessionProvider, home: URL, path: String?,
                          isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:),
                          exists: (String) -> Bool = FileManager.default.fileExists(atPath:)) -> Bool {
        if AgentCLI.locate(provider, path: path, home: home, isExecutable: isExecutable) != nil { return true }
        let apps: [String]
        switch provider {
        case .cursor: apps = ["Cursor.app"]
        case .opencode: apps = ["OpenCode.app"]
        default: apps = []
        }
        return apps.contains { app in ["/Applications", home.appending(path: "Applications").path].contains { exists("\($0)/\(app)") } }
    }

    /// What keeping the hooks in step asks: each found agent whose switch is
    /// on gets its hooks or plugin, or an update of them; an agent whose
    /// switch is off has the island's taken out. An agent not found is left
    /// alone, and a file that could not be read is never touched.
    static func plan(found: Set<ClaudeSessionProvider>, shown: Set<ClaudeSessionProvider>,
                     states: [ClaudeSessionProvider: State]) -> (install: [ClaudeSessionProvider], remove: [ClaudeSessionProvider]) {
        var install: [ClaudeSessionProvider] = []
        var remove: [ClaudeSessionProvider] = []
        for provider in ClaudeSessionProvider.hookAgents {
            let state = states[provider] ?? .missing
            if shown.contains(provider) {
                if found.contains(provider), state == .missing || state == .outdated { install.append(provider) }
            } else if state == .installed || state == .outdated {
                remove.append(provider)
            }
        }
        return (install, remove)
    }

    // MARK: Commands

    static func marker(bundle: String) -> String { ClaudeHookInstaller.marker(bundle: bundle) }

    /// The hook for one event: the agent's input inside the island's envelope,
    /// with the process that ran the hook as a hint of the agent's.
    static func command(_ provider: ClaudeSessionProvider, event: String, socket: String, bundle: String) -> String {
        let quoted = "'" + socket.replacingOccurrences(of: "'", with: "'\\''") + "'"
        let head = "{\"\(AgentHookMessage.key)\":\"\(provider.rawValue)\",\"hook\":\"\(event)\",\"pid\":%d,\"event\":"
        var command = "{ printf '\(head)' \"$PPID\"; cat; printf '}'; } | /usr/bin/nc -U \(quoted) 2>/dev/null"
        if provider == .cursor, cursorContinues.contains(event) { command += "; printf '{\"continue\":true}'" }
        return command + "; exit 0 \(marker(bundle: bundle))"
    }

    // MARK: State

    enum State: Equatable {
        case installed
        /// The island's hooks are there, but for another socket or an older set.
        case outdated
        case missing
        /// The agent's file could not be read; nothing is changed in it.
        case unreadable
    }

    static func state(_ provider: ClaudeSessionProvider, home: URL, socket: String, bundle: String) -> State {
        do {
            switch provider {
            case .gemini:
                let settings = try ClaudeHookInstaller.read(geminiSettings(home: home))
                if groupsHold(settings, events: geminiEvents, command: { command(.gemini, event: $0, socket: socket, bundle: bundle) }) {
                    return .installed
                }
                return ClaudeHookInstaller.hasAny(in: settings, bundle: bundle) ? .outdated : .missing
            case .cursor:
                let settings = try ClaudeHookInstaller.read(cursorHooks(home: home))
                let hooks = settings["hooks"] as? [String: Any] ?? [:]
                let current = cursorEvents.allSatisfy { event in
                    (hooks[event] as? [[String: Any]] ?? []).contains {
                        $0["command"] as? String == command(.cursor, event: event, socket: socket, bundle: bundle)
                    }
                }
                if current { return .installed }
                return flatHasAny(settings, bundle: bundle) ? .outdated : .missing
            case .copilot:
                let url = copilotHooks(home: home, bundle: bundle)
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == copilotFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .opencode:
                let url = opencodePlugin(home: home, bundle: bundle)
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == opencodeFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .claude, .codex:
                return .missing
            }
        } catch {
            return .unreadable
        }
    }

    // MARK: Changing

    static func install(_ provider: ClaudeSessionProvider, home: URL, socket: String, bundle: String) throws {
        switch provider {
        case .gemini:
            try ClaudeHookInstaller.update(geminiSettings(home: home)) { settings in
                installingGroups(into: settings, events: geminiEvents, bundle: bundle) { event in
                    ["type": "command", "name": "openbar", "timeout": eventTimeout * 1000,
                     "command": command(.gemini, event: event, socket: socket, bundle: bundle)]
                }
            }
        case .cursor:
            try ClaudeHookInstaller.update(cursorHooks(home: home)) { settings in
                var settings = flatRemoving(from: settings, bundle: bundle)
                if settings["version"] == nil { settings["version"] = 1 }
                var hooks = settings["hooks"] as? [String: Any] ?? [:]
                for event in cursorEvents {
                    let entry: [String: Any] = ["command": command(.cursor, event: event, socket: socket, bundle: bundle),
                                                "timeout": eventTimeout]
                    hooks[event] = (hooks[event] as? [[String: Any]] ?? []) + [entry]
                }
                settings["hooks"] = hooks
                return settings
            }
        case .copilot:
            try writeOwn(copilotFile(socket: socket, bundle: bundle), to: copilotHooks(home: home, bundle: bundle), bundle: bundle)
        case .opencode:
            try writeOwn(opencodeFile(socket: socket, bundle: bundle), to: opencodePlugin(home: home, bundle: bundle), bundle: bundle)
        case .claude, .codex:
            break
        }
    }

    static func remove(_ provider: ClaudeSessionProvider, home: URL, bundle: String) throws {
        switch provider {
        case .gemini:
            let url = geminiSettings(home: home)
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try ClaudeHookInstaller.update(url) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
        case .cursor:
            let url = cursorHooks(home: home)
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try ClaudeHookInstaller.update(url) { flatRemoving(from: $0, bundle: bundle) }
        case .copilot:
            try removeOwn(copilotHooks(home: home, bundle: bundle), bundle: bundle)
        case .opencode:
            try removeOwn(opencodePlugin(home: home, bundle: bundle), bundle: bundle)
        case .claude, .codex:
            break
        }
    }

    /// Takes every agent's hooks out, as the app is uninstalled.
    static func removeAll(home: URL, bundle: String) {
        for provider in ClaudeSessionProvider.hookAgents { try? remove(provider, home: home, bundle: bundle) }
    }

    // MARK: Gemini's groups

    static func groupsHold(_ settings: [String: Any], events: [String], command: (String) -> String) -> Bool {
        let hooks = settings["hooks"] as? [String: Any] ?? [:]
        return events.allSatisfy { event in
            (hooks[event] as? [[String: Any]] ?? []).contains { group in
                (group["hooks"] as? [[String: Any]] ?? []).contains { $0["command"] as? String == command(event) }
            }
        }
    }

    static func installingGroups(into settings: [String: Any], events: [String], bundle: String,
                                 entry: (String) -> [String: Any]) -> [String: Any] {
        var settings = ClaudeHookInstaller.removing(from: settings, bundle: bundle)
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        for event in events {
            hooks[event] = (hooks[event] as? [[String: Any]] ?? []) + [["hooks": [entry(event)]]]
        }
        settings["hooks"] = hooks
        return settings
    }

    // MARK: Cursor's list

    static func flatHasAny(_ settings: [String: Any], bundle: String) -> Bool {
        let marker = marker(bundle: bundle)
        return (settings["hooks"] as? [String: Any] ?? [:]).values.contains { value in
            (value as? [[String: Any]] ?? []).contains { ($0["command"] as? String)?.contains(marker) == true }
        }
    }

    /// Cursor's hooks without the island's; events left empty go too.
    static func flatRemoving(from settings: [String: Any], bundle: String) -> [String: Any] {
        let marker = marker(bundle: bundle)
        var settings = settings
        guard var hooks = settings["hooks"] as? [String: Any] else { return settings }
        for (event, value) in hooks {
            guard let entries = value as? [[String: Any]] else { continue }
            let kept = entries.filter { ($0["command"] as? String)?.contains(marker) != true }
            guard kept.count != entries.count else { continue }
            hooks[event] = kept.isEmpty ? nil : kept
        }
        settings["hooks"] = hooks
        return settings
    }

    // MARK: Files of the island's own

    static func copilotFile(socket: String, bundle: String) -> Data {
        var hooks: [String: Any] = [:]
        for event in copilotEvents {
            hooks[event] = [["type": "command", "bash": command(.copilot, event: event, socket: socket, bundle: bundle),
                             "timeoutSec": eventTimeout]]
        }
        let file: [String: Any] = ["version": 1, "hooks": hooks]
        var data = (try? JSONSerialization.data(withJSONObject: file, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]))
            ?? Data()
        data.append(0x0A)
        return data
    }

    /// The plugin, with the socket written in.
    static func opencodeFile(socket: String, bundle: String) -> Data {
        Data(opencodeSource(AgentOpenCodePlugin.source, socket: socket, bundle: bundle).utf8)
    }

    static func opencodeSource(_ template: String, socket: String, bundle: String) -> String {
        let literal = (try? JSONSerialization.data(withJSONObject: [socket], options: [.withoutEscapingSlashes]))
            .map { String(decoding: $0, as: UTF8.self).dropFirst().dropLast() }.map(String.init) ?? "\"\""
        return template
            .replacingOccurrences(of: "\"__OPENBAR_SOCKET__\"", with: literal)
            .replacingOccurrences(of: "__OPENBAR_MARKER__", with: marker(bundle: bundle))
    }

    /// Writes a file of the island's own, never over one that is not.
    private static func writeOwn(_ data: Data, to url: URL, bundle: String) throws {
        if let existing = try ClaudeHookInstaller.contents(url), existing.range(of: Data(marker(bundle: bundle).utf8)) == nil {
            throw Failure.unwritable("\(url.lastPathComponent) is not the island's")
        }
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: url, options: [.atomic])
        } catch {
            throw Failure.unwritable(error.localizedDescription)
        }
    }

    private static func removeOwn(_ url: URL, bundle: String) throws {
        guard let existing = try ClaudeHookInstaller.contents(url) else { return }
        guard existing.range(of: Data(marker(bundle: bundle).utf8)) != nil else { return }
        do { try FileManager.default.removeItem(at: url) } catch { throw Failure.unwritable(error.localizedDescription) }
    }
}
