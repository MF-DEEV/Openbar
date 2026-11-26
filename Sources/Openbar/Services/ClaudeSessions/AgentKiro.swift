// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Amazon's Kiro CLI (`kiro-cli`, #238). Kiro CLI V3 loads every hooks file
/// in `~/.kiro/hooks` (`KIRO_HOME` moves the folder) for any agent the
/// person starts, so the island writes one file of its own there. Its hooks
/// run in Kiro's agent server, a `node` process under the terminal UI, and
/// get Claude Code's fields on stdin; tools carry Kiro's own names.
///
/// Its permission hook runs after Kiro's policy and before its dialog, and
/// the dialog only opens once the hook answered nothing, with no time limit
/// of its own: so the island holds a tool's request and answers it, and lets
/// it go whenever the person should answer in Kiro (Open, the island going
/// away). Its dialog then takes Return for "Allow once", the first choice,
/// and Down and Return for "Deny once"; Escape there cancels the whole turn.
///
/// What a prompt's or a session's hook prints goes into the model's context,
/// so those print nothing. A turn cancelled with Escape brings no `Stop`:
/// the session's own log tells it.
enum AgentKiro {
    /// Every event the island hears, named as Kiro's hook files name them.
    static let events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PermissionRequest",
                         "PostToolUse", "Stop"]

    /// Events Kiro matches against a tool; `*` takes every tool.
    static let toolEvents: Set<String> = ["PreToolUse", "PermissionRequest", "PostToolUse"]

    // MARK: Places

    /// Kiro CLI's folder: `KIRO_HOME`, or `~/.kiro`. The Kiro IDE keeps its
    /// files there too, so only the command says Kiro CLI is here.
    static func folder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["KIRO_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".kiro")
    }

    /// The island's own hooks file in Kiro's global `hooks` folder.
    static func hooks(home: URL, bundle: String, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        hooks(folder: folder(home: home, environment: environment), bundle: bundle)
    }

    static func hooks(folder: URL, bundle: String) -> URL {
        folder.appending(path: "hooks/\(AgentHookInstaller.ownName(bundle)).json")
    }

    // MARK: Hooks

    /// The island's hooks file (kiro.dev/docs/hooks): a hook for each event,
    /// tool events matching every tool. Only the permission hook waits, as
    /// long as Claude Code's does; Kiro reads timeouts in seconds.
    static func file(socket: String, bundle: String) -> Data {
        let hooks: [[String: Any]] = events.map { event in
            var hook: [String: Any] = [
                "name": "\(AgentHookInstaller.ownName(bundle))-\(event)",
                "trigger": event,
                "action": ["type": "command",
                           "command": AgentHookInstaller.command(.kiro, event: event, socket: socket, bundle: bundle)],
                "timeout": event == "PermissionRequest" ? ClaudeHookInstaller.permissionTimeout : AgentHookInstaller.eventTimeout,
                "enabled": true,
            ]
            if toolEvents.contains(event) { hook["matcher"] = "*" }
            return hook
        }
        var data = (try? JSONSerialization.data(withJSONObject: ["version": "v1", "hooks": hooks],
                                                options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])) ?? Data()
        data.append(0x0A)
        return data
    }

    /// What the permission hook prints: allow or deny, Kiro's decision.
    static func reply(_ decision: ClaudeHookMessage.Decision) -> Data? {
        let body: [String: Any]
        switch decision {
        case .allow, .allowAlways: body = ["behavior": "allow"]
        case .deny: body = ["behavior": "deny", "message": ClaudeHookMessage.denyMessage]
        case .approvePlan, .answer: return nil
        }
        let output: [String: Any] = ["hookSpecificOutput": ["hookEventName": "PermissionRequest", "decision": body]]
        return try? JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
    }

    // MARK: Keys

    /// The keys that answer Kiro's own dialog, once the island let its hook
    /// go: Return takes "Allow once", picked first; Down and Return take
    /// "Deny once" below it. Both work sent in one burst.
    static func keys(_ decision: ClaudeHookMessage.Decision) -> [ClaudeTypingStep]? {
        switch decision {
        case .allow: return [.enter]
        case .deny: return [.keys([.down]), .enter]
        default: return nil
        }
    }

    // MARK: Process

    /// The process the session's terminal shows: Kiro's hooks run in its
    /// agent server, a `node` its terminal UI starts without a terminal of
    /// its own, so the first process above it that has one. It lives as
    /// long as the session and tells the terminal, its tmux pane too.
    static func terminalProcess(of agent: Int32, tty: (Int32) -> String? = ClaudeSessionHosts.tty(of:),
                                parent: (Int32) -> Int32? = AgentHookProcess.parent(of:)) -> Int32 {
        var pid = agent
        for _ in 0..<3 {
            guard pid > 1, tty(pid) == nil, let up = parent(pid) else { break }
            pid = up
        }
        return pid > 1 && tty(pid) != nil ? pid : agent
    }

    // MARK: Tools

    /// A tool's input in Claude Code's words, so the card, the list and a
    /// file's diff read it as they read Claude Code's: Kiro names a file
    /// `path`, a written file's text `text`, an edit's `oldStr`/`newStr`.
    static func claudeInput(_ raw: String, _ input: [String: Any]) -> [String: Any] {
        var out = input
        if let path = input["path"] as? String, input["file_path"] == nil { out["file_path"] = path }
        switch raw {
        case "fs_write", "write_file":
            if let text = input["text"] as? String, input["content"] == nil { out["content"] = text }
        case "str_replace", "replace_text":
            if let old = input["oldStr"] as? String { out["old_string"] = old }
            if let new = input["newStr"] as? String { out["new_string"] = new }
        default: break
        }
        return out
    }

    /// Whether a tool's result says the person cancelled its dialog with
    /// Escape, which ends the turn without a `Stop`.
    static func cancelled(_ response: Any?) -> Bool {
        (response as? String)?.contains("The tool call was cancelled before it was approved") == true
    }

    // MARK: Log

    /// The session's folder, `sessions/<workspace>/<id>` in Kiro's folder.
    static func sessionFolder(_ id: String, folder: URL) -> URL? {
        guard id.hasPrefix("sess_"), !id.contains("/"), !id.contains("..") else { return nil }
        let sessions = folder.appending(path: "sessions")
        let workspaces = (try? FileManager.default.contentsOfDirectory(atPath: sessions.path)) ?? []
        for workspace in workspaces where !workspace.hasPrefix(".") {
            let candidate = sessions.appending(path: workspace).appending(path: id)
            if FileManager.default.fileExists(atPath: candidate.appending(path: "messages.jsonl").path) { return candidate }
        }
        return nil
    }

    /// What the session's `session.json` says: its title and its model, as
    /// Kiro names them ("auto" when the server picks one per turn).
    static func details(sessionFolder: URL) -> (title: String?, model: String?) {
        guard let data = try? Data(contentsOf: sessionFolder.appending(path: "session.json")),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return (nil, nil) }
        let title = (json["title"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let model = (json["modelId"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        return (title, model)
    }

    /// The end of the session's last turn in its `messages.jsonl`: whether
    /// it ended, cancelled or not, and the reply it said last. Only the
    /// log's end is read; the text is kept only when `text` is true.
    struct LastTurn: Equatable {
        var ended = false
        var cancelled = false
        var reply: String?
        var at: Date?
    }

    static func lastTurn(log: URL, text: Bool) -> LastTurn {
        guard let handle = FileHandle(forReadingAtPath: log.path) else { return LastTurn() }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        let start = size > 262_144 ? size - 262_144 : 0
        try? handle.seek(toOffset: start)
        guard let data = try? handle.readToEnd() else { return LastTurn() }
        return lastTurn(String(decoding: data, as: UTF8.self), text: text)
    }

    static func lastTurn(_ log: String, text: Bool) -> LastTurn {
        var turn = LastTurn()
        // From the end: the turn's end, then what it said before it, up to its start.
        for line in log.split(separator: "\n").reversed() {
            guard let json = (try? JSONSerialization.jsonObject(with: Data(line.utf8))) as? [String: Any],
                  let payload = json["payload"] as? [String: Any], let type = payload["type"] as? String else { continue }
            switch type {
            case "turn_end":
                if turn.ended { return turn }
                turn.ended = true
                turn.cancelled = payload["stopReason"] as? String == "cancelled"
                turn.at = (json["timestamp"] as? String).flatMap(Self.date)
            case "turn_start", "user":
                return turn
            case "assistant" where payload["operationType"] as? String == "Say":
                guard turn.reply == nil, let said = payload["content"] as? String,
                      !said.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, said != "..." else { continue }
                if text { turn.reply = said }
            default: continue
            }
        }
        return turn
    }

    private static func date(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: text) ?? ISO8601DateFormatter().date(from: text)
    }
}
