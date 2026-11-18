// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CryptoKit
import Foundation

/// Puts the island's hooks into Gemini CLI, Copilot CLI, Cursor, Mistral
/// Vibe, Devin CLI and Grok CLI, and its plugin into OpenCode and Pi, and takes them
/// out again, only when the person presses Install or Remove. Where the
/// agent keeps every hook in one file (Gemini's settings, Cursor's
/// hooks.json, Vibe's hooks.toml, Devin's config.json) the island's entries
/// go beside the person's own, carry its marker, and the file is copied
/// aside first, as for Claude Code. Where the agent reads a folder of files
/// (Copilot's and Grok's hooks, OpenCode's plugins, Pi's extensions) the
/// island writes a file of its own.
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
    /// Mistral Vibe's hook types. `post_agent` runs after subagents' turns
    /// too, so the end of the person's turn is heard from `notification`.
    static let vibeEvents = ["session_start", "session_end", "pre_agent", "pre_tool", "post_tool", "notification"]
    /// Devin CLI's events, in Claude Code's format. `PermissionRequest` is
    /// only heard: the hook answers nothing, so Devin asks in its terminal.
    static let devinEvents = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                              "PermissionRequest", "Stop"]
    /// Google Antigravity's lifecycle hooks (its hooks.json guide). It has no
    /// session or prompt hooks: a turn is heard from the model's invocations.
    static let antigravityEvents = ["PreInvocation", "PreToolUse", "PostToolUse", "PostInvocation", "Stop"]
    /// Antigravity's tool hooks sit in groups with a matcher; the rest are flat.
    static let antigravityToolEvents: Set<String> = ["PreToolUse", "PostToolUse"]
    /// Codex's hooks (codex-rs/hooks), in Claude Code's format, written into
    /// a Codex home other than the one the island hears through Codex's
    /// app-server (#236). A permission is only heard: Codex asks in its terminal.
    static let codexEvents = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                              "PermissionRequest", "Stop"]
    /// Grok CLI's events (xAI's Grok Build, user guide "Hooks"), in a file of
    /// the island's own in its `hooks` folder, without a matcher, which Grok
    /// refuses on events that aren't a tool's. Each only tells: Grok goes on
    /// whatever the hook answers, and asks in its terminal.
    static let grokEvents = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                             "PostToolUseFailure", "PermissionDenied", "Notification", "Stop", "StopFailure", "StopCancelled"]
    /// Events Codex and Claude Code match against a tool name.
    static let claudeToolEvents: Set<String> = ["PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest"]
    /// What a tool with Claude Code style hooks gets: Claude Code's events,
    /// each only heard.
    static let customEvents = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                               "PostToolUseFailure", "PermissionRequest", "Notification", "Stop"]

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

    static func piFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["PI_CODING_AGENT_DIR"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
            ?? home.appending(path: ".pi/agent")
    }

    static func piExtension(home: URL, bundle: String,
                            environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        piFolder(home: home, environment: environment).appending(path: "extensions/\(ownName(bundle)).js")
    }

    static func vibeFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["VIBE_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".vibe")
    }

    static func vibeHooks(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        vibeFolder(home: home, environment: environment).appending(path: "hooks.toml")
    }

    static func devinConfig(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        let config = environment["XDG_CONFIG_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
            ?? home.appending(path: ".config")
        return config.appending(path: "devin/config.json")
    }

    /// Grok CLI's folder: `GROK_HOME`, or `~/.grok`.
    static func grokFolder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["GROK_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".grok")
    }

    static func grokHooks(home: URL, bundle: String, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        grokFolder(home: home, environment: environment).appending(path: "hooks/\(ownName(bundle)).json")
    }

    /// Antigravity's global hooks, read by its app, its IDE and `agy` alike.
    static func antigravityHooks(home: URL) -> URL { home.appending(path: ".gemini/config/hooks.json") }

    /// The agents whose config folder can be named by hand, beside the one
    /// their home variable or default names (#236).
    static let foldersByHand: [ClaudeSessionProvider] = [.codex, .gemini, .copilot, .pi, .vibe, .grok]

    /// The agent's hook file in a config folder the person added (#236): the
    /// folder Gemini keeps as `.gemini`, Copilot's `COPILOT_HOME`, Pi's
    /// `PI_CODING_AGENT_DIR`, Vibe's `VIBE_HOME`, a `CODEX_HOME`, a `GROK_HOME`.
    static func file(_ provider: ClaudeSessionProvider, folder: URL, bundle: String) -> URL? {
        switch provider {
        case .gemini: return folder.appending(path: "settings.json")
        case .copilot: return folder.appending(path: "hooks/\(ownName(bundle)).json")
        case .pi: return folder.appending(path: "extensions/\(ownName(bundle)).js")
        case .vibe: return folder.appending(path: "hooks.toml")
        case .codex: return folder.appending(path: "hooks.json")
        case .grok: return folder.appending(path: "hooks/\(ownName(bundle)).json")
        default: return nil
        }
    }

    /// The agent's hook file: in `folder` when one is given, else where the
    /// agent reads it by default; nil for an agent with nothing installed.
    static func file(_ provider: ClaudeSessionProvider, home: URL, bundle: String, folder: URL? = nil) -> URL? {
        if let folder { return file(provider, folder: folder, bundle: bundle) }
        return hookFile(provider, home: home, bundle: bundle)?.url
    }

    /// A short name for a config folder, which its hooks carry so the island
    /// knows which folder a session runs from without writing the path into
    /// a command.
    static func folderTag(_ folder: URL) -> String {
        let digest = SHA256.hash(data: Data(ClaudeAccounts.canonical(folder.path).utf8))
        return digest.prefix(6).map { String(format: "%02x", $0) }.joined()
    }

    /// Whether a tag is one the island made: only these go into a command.
    static func isTag(_ text: String) -> Bool {
        !text.isEmpty && text.count <= 40 && text.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") }
    }

    /// The folder whose presence says the agent was used on this Mac.
    static func agentFolder(_ provider: ClaudeSessionProvider, home: URL,
                            environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
        switch provider {
        case .gemini: return geminiSettings(home: home, environment: environment).deletingLastPathComponent()
        case .copilot: return copilotFolder(home: home, environment: environment)
        case .cursor: return cursorHooks(home: home).deletingLastPathComponent()
        case .opencode: return opencodeFolder(home: home, environment: environment)
        case .pi: return piFolder(home: home, environment: environment)
        case .vibe: return vibeFolder(home: home, environment: environment)
        case .devin: return devinConfig(home: home, environment: environment).deletingLastPathComponent()
        case .antigravity: return antigravityHooks(home: home).deletingLastPathComponent()
        case .grok: return grokFolder(home: home, environment: environment)
        case .codex:
            return environment["CODEX_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
                ?? home.appending(path: ".codex")
        case .claude, .custom: return nil
        }
    }

    /// Whether the agent is installed: its command on the login shell's
    /// `PATH` or where its installers put it, or, for Cursor, OpenCode and
    /// Google Antigravity, their app; for Pi, its command and the folder Pi
    /// makes on first run.
    /// A folder like `~/.gemini` or `~/.copilot` is not enough: other apps
    /// (Antigravity, VS Code) make those too.
    static func isPresent(_ provider: ClaudeSessionProvider, home: URL, path: String?,
                          isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:),
                          exists: (String) -> Bool = FileManager.default.fileExists(atPath:)) -> Bool {
        if AgentCLI.locate(provider, path: path, home: home, isExecutable: isExecutable) != nil {
            // Pi's command has a name other tools take too; Pi's own folder says it is Pi's.
            return provider != .pi || exists(home.appending(path: ".pi/agent").path)
        }
        let apps: [String]
        switch provider {
        case .cursor: apps = ["Cursor.app"]
        case .opencode: apps = ["OpenCode.app"]
        case .antigravity: apps = ["Antigravity.app", "Antigravity IDE.app"]
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
        plan(found: found, shown: shown, states: states.mapValues { HookRecords.derive(HookRecords.Base($0), record: nil, current: nil) })
    }

    /// The same, once the records say more (#236): hooks the island put in
    /// that someone took out come back only while lost hooks are put back
    /// by themselves, hooks changed by hand are the person's, and an agent
    /// the person pressed Remove for on the Hooks page is left as it is.
    static func plan(found: Set<ClaudeSessionProvider>, shown: Set<ClaudeSessionProvider>,
                     states: [ClaudeSessionProvider: HookRecords.State], optedOut: Set<ClaudeSessionProvider> = [],
                     repair: Bool = true) -> (install: [ClaudeSessionProvider], remove: [ClaudeSessionProvider]) {
        var install: [ClaudeSessionProvider] = []
        var remove: [ClaudeSessionProvider] = []
        for provider in ClaudeSessionProvider.hookAgents {
            let state = states[provider] ?? .missing
            if shown.contains(provider) {
                guard found.contains(provider), !optedOut.contains(provider) else { continue }
                switch state {
                case .missing, .outdated: install.append(provider)
                case .removedByHand: if repair { install.append(provider) }
                case .installed, .changedByHand, .unreadable: break
                }
            } else if state == .installed || state == .outdated || state == .changedByHand {
                remove.append(provider)
            }
        }
        return (install, remove)
    }

    // MARK: Commands

    static func marker(bundle: String) -> String { ClaudeHookInstaller.marker(bundle: bundle) }

    /// What Antigravity reads from the island's hook. It refuses a tool call
    /// whose `PreToolUse` hook prints nothing, `{}`, fails or runs out of
    /// time, so that hook hands every call back to Antigravity's own check
    /// (`ask`, which keeps its grants and Always Allow); the others print an
    /// empty answer, which asks nothing of Antigravity.
    static func antigravityAnswer(_ event: String) -> String {
        event == "PreToolUse" ? #"{"decision":"ask"}"# : "{}"
    }

    /// The hook for one event: the agent's input inside the island's envelope,
    /// with the process that ran the hook as a hint of the agent's.
    /// `via` names the config folder (Codex) or the tool (one with Claude
    /// Code style hooks) the hook was written for (#236).
    static func command(_ provider: ClaudeSessionProvider, event: String, socket: String, bundle: String,
                        via: String? = nil) -> String {
        let quoted = "'" + socket.replacingOccurrences(of: "'", with: "'\\''") + "'"
        let tag = via.flatMap { isTag($0) ? "\"\(AgentHookMessage.viaKey)\":\"\($0)\"," : nil } ?? ""
        let head = "{\"\(AgentHookMessage.key)\":\"\(provider.rawValue)\",\"hook\":\"\(event)\",\"pid\":%d,\(tag)\"event\":"
        if provider == .antigravity {
            // The answer goes out first, whatever the island does, and the
            // island gets at most two seconds: Antigravity waits on its hooks.
            return "printf '\(antigravityAnswer(event))'; { printf '\(head)' \"$PPID\"; cat; printf '}'; } "
                + "| /usr/bin/nc -w 2 -U \(quoted) >/dev/null 2>&1; exit 0 \(marker(bundle: bundle))"
        }
        var command = "{ printf '\(head)' \"$PPID\"; cat; printf '}'; } | /usr/bin/nc -U \(quoted) 2>/dev/null"
        if provider == .cursor, cursorContinues.contains(event) { command += "; printf '{\"continue\":true}'" }
        return command + "; exit 0 \(marker(bundle: bundle))"
    }

    // MARK: Vibe's hooks.toml

    /// The lines that open and close the island's entries in hooks.toml.
    static func vibeStart(bundle: String) -> String { "\(marker(bundle: bundle)) start: added by the island, removed by it" }
    static func vibeEnd(bundle: String) -> String { "\(marker(bundle: bundle)) end" }

    /// The island's `[[hooks]]` entries, between its two marker lines. Names
    /// carry the app's own, so the Developer build's never clash with them.
    static func vibeBlock(socket: String, bundle: String) -> String {
        var lines = [vibeStart(bundle: bundle)]
        for event in vibeEvents {
            lines += ["[[hooks]]",
                      "name = \(tomlString("\(ownName(bundle))-\(event)"))",
                      "type = \(tomlString(event))",
                      "command = \(tomlString(command(.vibe, event: event, socket: socket, bundle: bundle)))",
                      "timeout = \(eventTimeout).0"]
        }
        lines.append(vibeEnd(bundle: bundle))
        return lines.joined(separator: "\n") + "\n"
    }

    /// A TOML basic string.
    static func tomlString(_ text: String) -> String {
        var out = "\""
        for scalar in text.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\t": out += "\\t"
            case "\r": out += "\\r"
            default:
                if scalar.value < 0x20 || scalar.value == 0x7F { out += String(format: "\\u%04X", scalar.value) }
                else { out.unicodeScalars.append(scalar) }
            }
        }
        return out + "\""
    }

    /// The file without the island's entries; nil when it had none.
    static func vibeRemoving(_ text: String, bundle: String) -> String? {
        let start = vibeStart(bundle: bundle), end = vibeEnd(bundle: bundle)
        var lines = text.components(separatedBy: "\n")
        var changed = false
        while let first = lines.firstIndex(of: start) {
            let last = lines[first...].firstIndex(of: end) ?? lines.index(before: lines.endIndex)
            lines.removeSubrange(first...last)
            changed = true
        }
        guard changed else { return nil }
        var result = lines.joined(separator: "\n")
        while result.hasSuffix("\n\n") { result.removeLast() }
        return result.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "" : result
    }

    /// The file with the island's entries at its end, after the person's own.
    static func vibeInstalling(_ text: String?, socket: String, bundle: String) -> String {
        var base = text.map { vibeRemoving($0, bundle: bundle) ?? $0 } ?? ""
        if !base.isEmpty, !base.hasSuffix("\n") { base += "\n" }
        if !base.isEmpty { base += "\n" }
        return base + vibeBlock(socket: socket, bundle: bundle)
    }

    /// Whether hooks.toml keeps its hooks in a form the island's tables
    /// can't sit beside (`hooks = [...]`), so it is left alone.
    static func vibeInline(_ text: String) -> Bool {
        text.components(separatedBy: "\n").contains { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix("hooks") else { return false }
            return trimmed.dropFirst("hooks".count).trimmingCharacters(in: .whitespaces).hasPrefix("=")
        }
    }

    static func vibeText(_ url: URL) throws -> String? {
        guard let data = try ClaudeHookInstaller.contents(url) else { return nil }
        guard let text = String(data: data, encoding: .utf8) else { throw Failure.unreadable }
        return text
    }

    /// Changes hooks.toml in one step, copying it aside first.
    static func updateVibe(_ link: URL, _ change: (String?) -> String?) throws {
        let url = link.resolvingSymlinksInPath()
        for _ in 0..<3 {
            let manager = FileManager.default
            let before = (try? manager.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
            let original = try ClaudeHookInstaller.contents(url)
            let text = try vibeText(url)
            guard let changed = change(text), changed != text else { return }
            let after = (try? manager.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
            guard before == after, (try? ClaudeHookInstaller.contents(url)) == original else { continue }
            try ClaudeHookInstaller.replace(url, original: original, with: Data(changed.utf8), now: Date())
            return
        }
        throw Failure.busy
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

    /// The state of the agent's hooks where it reads them by default, or in
    /// `folder`, one the person added (#236).
    static func state(_ provider: ClaudeSessionProvider, home: URL, socket: String, bundle: String,
                      folder: URL? = nil) -> State {
        guard let url = file(provider, home: home, bundle: bundle, folder: folder) else { return .missing }
        do {
            switch provider {
            case .gemini:
                return groupsState(try ClaudeHookInstaller.read(url), events: geminiEvents, bundle: bundle) {
                    command(.gemini, event: $0, socket: socket, bundle: bundle)
                }
            case .cursor:
                let settings = try ClaudeHookInstaller.read(url)
                let hooks = settings["hooks"] as? [String: Any] ?? [:]
                let current = cursorEvents.allSatisfy { event in
                    (hooks[event] as? [[String: Any]] ?? []).contains {
                        $0["command"] as? String == command(.cursor, event: event, socket: socket, bundle: bundle)
                    }
                }
                if current { return .installed }
                return flatHasAny(settings, bundle: bundle) ? .outdated : .missing
            case .copilot:
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == copilotFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .grok:
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == grokFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .opencode:
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == opencodeFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .pi:
                guard let data = try ClaudeHookInstaller.contents(url) else { return .missing }
                return data == piFile(socket: socket, bundle: bundle) ? .installed : .outdated
            case .vibe:
                guard let text = try vibeText(url) else { return .missing }
                if text.contains(vibeBlock(socket: socket, bundle: bundle)) { return .installed }
                if text.contains(marker(bundle: bundle)) { return .outdated }
                return vibeInline(text) ? .unreadable : .missing
            case .devin:
                return groupsState(try ClaudeHookInstaller.read(url), events: devinEvents, bundle: bundle) {
                    command(.devin, event: $0, socket: socket, bundle: bundle)
                }
            case .antigravity:
                let settings = try ClaudeHookInstaller.read(url)
                if let own = settings[ownName(bundle)] as? [String: Any],
                   NSDictionary(dictionary: own).isEqual(to: antigravityEntry(socket: socket, bundle: bundle)) {
                    return .installed
                }
                return antigravityRemoving(from: settings, bundle: bundle).count != settings.count ? .outdated : .missing
            case .codex:
                guard let folder else { return .missing }
                let tag = folderTag(folder)
                return groupsState(try ClaudeHookInstaller.read(url), events: codexEvents, bundle: bundle) {
                    command(.codex, event: $0, socket: socket, bundle: bundle, via: tag)
                }
            case .claude, .custom:
                return .missing
            }
        } catch {
            return .unreadable
        }
    }

    /// Installed when every event holds the current command; outdated when
    /// some other command of the island's is there.
    static func groupsState(_ settings: [String: Any], events: [String], bundle: String,
                            command: (String) -> String) -> State {
        if groupsHold(settings, events: events, command: command) { return .installed }
        return ClaudeHookInstaller.hasAny(in: settings, bundle: bundle) ? .outdated : .missing
    }

    // MARK: Changing

    static func install(_ provider: ClaudeSessionProvider, home: URL, socket: String, bundle: String,
                        folder: URL? = nil) throws {
        guard let url = file(provider, home: home, bundle: bundle, folder: folder) else { return }
        switch provider {
        case .gemini:
            try ClaudeHookInstaller.update(url) { settings in
                installingGroups(into: settings, events: geminiEvents, bundle: bundle) { event in
                    ["type": "command", "name": "openbar", "timeout": eventTimeout * 1000,
                     "command": command(.gemini, event: event, socket: socket, bundle: bundle)]
                }
            }
        case .cursor:
            try ClaudeHookInstaller.update(url) { settings in
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
            try writeOwn(copilotFile(socket: socket, bundle: bundle), to: url, bundle: bundle)
        case .grok:
            try writeOwn(grokFile(socket: socket, bundle: bundle), to: url, bundle: bundle)
        case .opencode:
            try writeOwn(opencodeFile(socket: socket, bundle: bundle), to: url, bundle: bundle)
        case .pi:
            try writeOwn(piFile(socket: socket, bundle: bundle), to: url, bundle: bundle)
        case .vibe:
            if let text = try vibeText(url), vibeInline(text) { throw Failure.unreadable }
            try updateVibe(url) { vibeInstalling($0, socket: socket, bundle: bundle) }
        case .devin:
            try ClaudeHookInstaller.update(url) { settings in
                var settings = installingGroups(into: settings, events: devinEvents, bundle: bundle) { event in
                    ["type": "command", "timeout": eventTimeout,
                     "command": command(.devin, event: event, socket: socket, bundle: bundle)]
                }
                // Devin writes its own file with a version; a new one gets the same.
                if settings["version"] == nil { settings["version"] = 1 }
                return settings
            }
        case .antigravity:
            try ClaudeHookInstaller.update(url) { settings in
                var settings = antigravityRemoving(from: settings, bundle: bundle)
                settings[ownName(bundle)] = antigravityEntry(socket: socket, bundle: bundle)
                return settings
            }
        case .codex:
            guard let folder else { return }
            let tag = folderTag(folder)
            try ClaudeHookInstaller.update(url) { settings in
                installingGroups(into: settings, events: codexEvents, bundle: bundle, matched: claudeToolEvents) { event in
                    // Codex holds SessionEnd to 3 seconds and warns of more.
                    ["type": "command", "timeout": event == "SessionEnd" ? 3 : eventTimeout,
                     "command": command(.codex, event: event, socket: socket, bundle: bundle, via: tag)]
                }
            }
        case .claude, .custom:
            break
        }
    }

    static func remove(_ provider: ClaudeSessionProvider, home: URL, bundle: String, folder: URL? = nil) throws {
        guard let url = file(provider, home: home, bundle: bundle, folder: folder) else { return }
        switch provider {
        case .gemini, .devin, .codex:
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try ClaudeHookInstaller.update(url) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
        case .cursor:
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try ClaudeHookInstaller.update(url) { flatRemoving(from: $0, bundle: bundle) }
        case .copilot, .opencode, .pi, .grok:
            try removeOwn(url, bundle: bundle)
        case .vibe:
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try updateVibe(url) { text in text.flatMap { vibeRemoving($0, bundle: bundle) } }
        case .antigravity:
            guard try ClaudeHookInstaller.contents(url) != nil else { return }
            try ClaudeHookInstaller.update(url) { antigravityRemoving(from: $0, bundle: bundle) }
        case .claude, .custom:
            break
        }
    }

    // MARK: Tools with Claude Code style hooks (#236)

    /// A tool's hooks: Claude Code's events in Claude Code's groups, each
    /// command naming the tool.
    static func customState(_ url: URL, tool: String, socket: String, bundle: String) -> State {
        do {
            return groupsState(try ClaudeHookInstaller.read(url), events: customEvents, bundle: bundle) {
                command(.custom, event: $0, socket: socket, bundle: bundle, via: tool)
            }
        } catch {
            return .unreadable
        }
    }

    static func installCustom(_ url: URL, tool: String, socket: String, bundle: String) throws {
        try ClaudeHookInstaller.update(url) { settings in
            installingGroups(into: settings, events: customEvents, bundle: bundle, matched: claudeToolEvents) { event in
                ["type": "command", "timeout": eventTimeout,
                 "command": command(.custom, event: event, socket: socket, bundle: bundle, via: tool)]
            }
        }
    }

    static func removeCustom(_ url: URL, bundle: String) throws {
        guard try ClaudeHookInstaller.contents(url) != nil else { return }
        try ClaudeHookInstaller.update(url) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
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

    /// The island's groups after the person's own; events in `matched` get
    /// a matcher for every tool, as Claude Code and Codex want.
    static func installingGroups(into settings: [String: Any], events: [String], bundle: String,
                                 matched: Set<String> = [], entry: (String) -> [String: Any]) -> [String: Any] {
        var settings = ClaudeHookInstaller.removing(from: settings, bundle: bundle)
        var hooks = settings["hooks"] as? [String: Any] ?? [:]
        for event in events {
            var group: [String: Any] = ["hooks": [entry(event)]]
            if matched.contains(event) { group["matcher"] = "*" }
            hooks[event] = (hooks[event] as? [[String: Any]] ?? []) + [group]
        }
        settings["hooks"] = hooks
        return settings
    }

    // MARK: Antigravity's named hooks

    /// The island's named hook in Antigravity's hooks.json: every event's
    /// command, the tool events in a group matching every tool.
    static func antigravityEntry(socket: String, bundle: String) -> [String: Any] {
        var entry: [String: Any] = [:]
        for event in antigravityEvents {
            let handler: [String: Any] = ["type": "command", "timeout": eventTimeout,
                                          "command": command(.antigravity, event: event, socket: socket, bundle: bundle)]
            entry[event] = antigravityToolEvents.contains(event) ? [["matcher": "*", "hooks": [handler]]] : [handler]
        }
        return entry
    }

    /// The hooks.json without the island's named hooks: every top-level
    /// entry one of whose commands carries the marker, whatever it is called.
    static func antigravityRemoving(from settings: [String: Any], bundle: String) -> [String: Any] {
        let marker = marker(bundle: bundle)
        func carries(_ value: Any) -> Bool {
            if let object = value as? [String: Any] {
                if (object["command"] as? String)?.contains(marker) == true { return true }
                return object.values.contains(where: carries)
            }
            if let list = value as? [Any] { return list.contains(where: carries) }
            return false
        }
        return settings.filter { !carries($0.value) }
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

    /// Grok CLI's hooks file of the island's own: every event's command in a
    /// group with no matcher.
    static func grokFile(socket: String, bundle: String) -> Data {
        var hooks: [String: Any] = [:]
        for event in grokEvents {
            hooks[event] = [["hooks": [["type": "command", "timeout": eventTimeout,
                                        "command": command(.grok, event: event, socket: socket, bundle: bundle)]]]]
        }
        var data = (try? JSONSerialization.data(withJSONObject: ["hooks": hooks],
                                                options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])) ?? Data()
        data.append(0x0A)
        return data
    }

    /// Pi's extension, with the socket written in.
    static func piFile(socket: String, bundle: String) -> Data {
        Data(opencodeSource(AgentPiExtension.source, socket: socket, bundle: bundle).utf8)
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

    /// Writes a file of the island's own, never over one that is not, and
    /// keeps a copy of what was there first (#236).
    private static func writeOwn(_ data: Data, to url: URL, bundle: String) throws {
        let existing = try ClaudeHookInstaller.contents(url)
        if let existing, existing.range(of: Data(marker(bundle: bundle).utf8)) == nil {
            throw Failure.unwritable("\(url.lastPathComponent) is not the island's")
        }
        guard existing != data else { return }
        HookFolders.noteMade(url.deletingLastPathComponent())
        try ClaudeHookInstaller.replace(url, original: existing, with: data, now: Date())
    }

    /// Takes a file of the island's own away, a copy kept beside it.
    private static func removeOwn(_ url: URL, bundle: String) throws {
        guard let existing = try ClaudeHookInstaller.contents(url) else { return }
        guard existing.range(of: Data(marker(bundle: bundle).utf8)) != nil else { return }
        try ClaudeHookInstaller.keepCopy(of: existing, beside: url, now: Date())
        do { try FileManager.default.removeItem(at: url) } catch { throw Failure.unwritable(error.localizedDescription) }
    }
}
