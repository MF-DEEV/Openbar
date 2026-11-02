// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// One event from Gemini CLI, Copilot CLI, Cursor or OpenCode, as the island
/// reads it. The hook the island installs wraps what the agent hands it in
/// an envelope naming the agent, the hook and the agent's process:
///
///     {"vorssaint": "gemini", "hook": "BeforeTool", "pid": 4242, "event": {…}}
///
/// OpenCode's plugin writes the same envelope with an event of its own
/// making (docs/more-session-agents.md). Prompts, replies and tool arguments
/// are kept only as the content policy allows, like Claude Code's.
struct AgentHookEvent: Equatable {
    enum Kind: Equatable {
        case started
        case ended
        /// The person sent a prompt and a turn began; its text with previews.
        case prompt(String?)
        /// The agent is at work, as OpenCode's busy status or Cursor's thoughts say.
        case working
        case toolStarted(ClaudeToolUse)
        /// A tool ended. Tools the agent gives no id are matched by name and input.
        case toolFinished(id: String?, name: String, inputKey: Int?, failed: Bool, started: Date?)
        /// A tool waits for the person's permission. Answerable only where the
        /// island can answer that agent.
        case permission(ClaudePermission)
        /// The request was answered, in the agent or here.
        case permissionSettled(String?)
        /// A question or other dialog of the agent's waits on the person.
        case question
        /// The agent wrote a reply; with previews.
        case reply(String)
        /// The turn ended; `lastText` is the reply it ended on, with previews.
        case stopped(lastText: String?, failed: Bool)
        case model(String)
        case title(String)
        case usage(key: String, tokens: AgentTokens, cost: Double)
    }

    let provider: ClaudeSessionProvider
    /// The agent's own id for the session.
    let sessionID: String
    /// The agent's process, which the hook was started by.
    let pid: Int32
    let cwd: String?
    let date: Date
    let kind: Kind

    /// The id the island lists the session by, apart from every other agent's.
    var id: String { AgentHookEvent.id(provider, sessionID) }

    static func id(_ provider: ClaudeSessionProvider, _ session: String) -> String { "\(provider.rawValue):\(session)" }
}

enum AgentHookMessage {
    /// The envelope's key, which no agent's own payload carries.
    static let key = "vorssaint"

    /// What one envelope holds: the events in it, and for a connection that
    /// waits on the island, what it waits for.
    struct Decoded {
        var events: [AgentHookEvent] = []
        /// An OpenCode permission request, answered on the same connection.
        var permission: (session: String, request: String)? = nil
        /// OpenCode's plugin listening for replies the person types here.
        var listens = false
        let provider: ClaudeSessionProvider
        let pid: Int32
    }

    /// Whether the input is an envelope from another agent, told cheaply.
    static func isEnvelope(_ data: Data) -> Bool {
        AgentLogParser.contains(data, #""vorssaint":"#)
    }

    /// Whether the hook waits for the island's answer: an OpenCode permission
    /// or its plugin listening. Every other hook is let go at once.
    static func waits(_ data: Data) -> Bool {
        guard isEnvelope(data), AgentLogParser.contains(data, "\"opencode\"") else { return false }
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              json[key] as? String == ClaudeSessionProvider.opencode.rawValue,
              let event = json["event"] as? [String: Any] else { return false }
        let kind = event["kind"] as? String
        return kind == "permission" || kind == "listen"
    }

    static func decode(_ data: Data, now: Date, policy: ClaudeContentPolicy) -> Decoded? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let name = json[key] as? String, let provider = ClaudeSessionProvider(rawValue: name),
              provider.reportsThroughHooks, let event = json["event"] as? [String: Any] else { return nil }
        let pid = Int32(clamping: (json["pid"] as? NSNumber)?.int64Value ?? 0)
        let hook = json["hook"] as? String ?? event["hook_event_name"] as? String ?? ""
        var decoded = Decoded(provider: provider, pid: pid)
        switch provider {
        case .gemini: decoded.events = gemini(hook, event, pid: pid, now: now, policy: policy)
        case .copilot: decoded.events = copilot(hook, event, pid: pid, now: now, policy: policy)
        case .cursor: decoded.events = cursor(hook, event, pid: pid, now: now, policy: policy)
        case .opencode: opencode(event, pid: pid, now: now, policy: policy, into: &decoded)
        case .claude, .codex: return nil
        }
        return decoded
    }

    // MARK: Gemini CLI

    /// Gemini CLI's hooks (docs/hooks/reference.md in its package): a
    /// permission it asks for is only heard of, and answered in the terminal.
    private static func gemini(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                               policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "session_id") else { return [] }
        let cwd = text(json, "cwd")
        let date = timestamp(json["timestamp"]) ?? now
        func event(_ kind: AgentHookEvent.Kind) -> [AgentHookEvent] {
            [AgentHookEvent(provider: .gemini, sessionID: session, pid: pid, cwd: cwd, date: date, kind: kind)]
        }
        let tool = AgentHookTools.name(.gemini, json["tool_name"] as? String ?? "")
        let input = arguments(json["tool_input"])
        switch hook {
        case "SessionStart": return event(.started)
        case "SessionEnd": return event(.ended)
        case "BeforeAgent": return event(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "AfterAgent":
            // Gemini CLI writes this when the turn ended with no text, as on an error.
            let reply = preview(json["prompt_response"]).flatMap { $0 == "[no response text]" ? nil : $0 }
            return event(.stopped(lastText: policy.previews ? reply : nil, failed: false))
        case "BeforeTool":
            return event(.toolStarted(toolUse(tool, input: input, at: date, policy: policy)))
        case "AfterTool":
            let response = json["tool_response"] as? [String: Any]
            let failed = response.map { $0["error"] != nil && !($0["error"] is NSNull) } ?? false
            return event(.toolFinished(id: nil, name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                                       failed: failed, started: nil))
        case "Notification" where json["notification_type"] as? String == "ToolPermission":
            let details = json["details"] as? [String: Any] ?? [:]
            let asked = AgentHookTools.name(.gemini, (details["toolName"] as? String) ?? (details["type"] as? String) ?? "")
            let detail = [details["command"], details["fileName"], details["filePath"], details["url"], json["message"]]
                .compactMap { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }.first ?? ""
            return event(.permission(ClaudePermission(
                requestID: UUID().uuidString, tool: asked.isEmpty ? "Tool" : asked,
                summary: ClaudeToolSummary.clip(detail.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
                detail: detail, offersAlways: false, since: date)))
        default: return []
        }
    }

    // MARK: Copilot CLI

    /// Copilot CLI's camelCase hooks (docs.github.com, CLI hooks reference).
    /// It hears of a permission prompt through a notification only.
    private static func copilot(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                                policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "sessionId") ?? text(json, "session_id") else { return [] }
        let cwd = text(json, "cwd")
        let date = timestamp(json["timestamp"]) ?? now
        func event(_ kind: AgentHookEvent.Kind) -> [AgentHookEvent] {
            [AgentHookEvent(provider: .copilot, sessionID: session, pid: pid, cwd: cwd, date: date, kind: kind)]
        }
        let tool = AgentHookTools.name(.copilot, json["toolName"] as? String ?? "")
        let input = arguments(json["toolArgs"])
        switch hook {
        case "sessionStart": return event(.started)
        case "sessionEnd": return event(.ended)
        case "userPromptSubmitted": return event(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "preToolUse": return event(.toolStarted(toolUse(tool, input: input, at: date, policy: policy)))
        case "postToolUse", "postToolUseFailure":
            let result = (json["toolResult"] as? [String: Any])?["resultType"] as? String
            return event(.toolFinished(id: nil, name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                                       failed: hook == "postToolUseFailure" || (result != nil && result != "success"),
                                       started: nil))
        case "agentStop": return event(.stopped(lastText: nil, failed: false))
        case "errorOccurred" where json["recoverable"] as? Bool == false:
            return event(.stopped(lastText: nil, failed: true))
        case "notification":
            switch json["notification_type"] as? String {
            case "permission_prompt":
                let message = [json["message"], json["title"]].compactMap { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }
                    .first ?? ""
                return event(.permission(ClaudePermission(
                    requestID: UUID().uuidString, tool: "Tool", summary: ClaudeToolSummary.clip(message),
                    detail: message, offersAlways: false, since: date)))
            case "elicitation_dialog": return event(.question)
            default: return []
            }
        default: return []
        }
    }

    // MARK: Cursor

    /// Cursor's hooks, in the app and in `cursor-agent` (cursor.com/docs/agent/hooks).
    /// Only hooks that ask nothing of their answer are installed, so Cursor's
    /// approvals stay in Cursor and none is ever held up by the island.
    private static func cursor(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                               policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "conversation_id") ?? text(json, "session_id") else { return [] }
        let cwd = (json["workspace_roots"] as? [String])?.first(where: { !$0.isEmpty }) ?? text(json, "cwd")
        let date = now
        var events: [AgentHookEvent] = []
        func add(_ kind: AgentHookEvent.Kind) {
            events.append(AgentHookEvent(provider: .cursor, sessionID: session, pid: pid, cwd: cwd, date: date, kind: kind))
        }
        if let model = text(json, "model") { add(.model(model)) }
        let tool = AgentHookTools.name(.cursor, json["tool_name"] as? String ?? "")
        switch hook {
        case "sessionStart": add(.started)
        case "sessionEnd": add(.ended)
        case "beforeSubmitPrompt": add(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "afterAgentThought": add(.working)
        case "afterAgentResponse":
            if policy.previews, let text = preview(json["text"]) { add(.reply(text)) } else { add(.working) }
        case "postToolUse", "postToolUseFailure":
            let input = arguments(json["tool_input"])
            let seconds = (json["duration"] as? NSNumber).map { $0.doubleValue / 1000 }
            var use = toolUse(tool, input: input, at: seconds.map { date.addingTimeInterval(-$0) } ?? date, policy: policy)
            use.finished = date
            use.failed = hook == "postToolUseFailure"
            add(.toolFinished(id: text(json, "tool_use_id"), name: tool, inputKey: use.inputKey,
                              failed: use.failed, started: use.started))
        case "stop": add(.stopped(lastText: nil, failed: (json["status"] as? String).map { $0 != "completed" } ?? false))
        default: break
        }
        return events
    }

    // MARK: OpenCode

    /// What the island's OpenCode plugin writes (AgentOpenCodePlugin.swift).
    private static func opencode(_ json: [String: Any], pid: Int32, now: Date, policy: ClaudeContentPolicy,
                                 into decoded: inout Decoded) {
        let kind = json["kind"] as? String ?? ""
        if kind == "listen" { decoded.listens = true; return }
        guard let session = text(json, "session") else { return }
        let cwd = text(json, "directory")
        let date = timestamp(json["at"]) ?? now
        func add(_ kind: AgentHookEvent.Kind) {
            decoded.events.append(AgentHookEvent(provider: .opencode, sessionID: session, pid: pid, cwd: cwd, date: date, kind: kind))
        }
        switch kind {
        case "created": add(.started)
        case "deleted": add(.ended)
        case "prompt": add(.prompt(policy.previews ? preview(json["text"]) : nil))
        case "status":
            switch json["status"] as? String {
            case "busy", "retry": add(.working)
            case "idle": add(.stopped(lastText: nil, failed: false))
            default: break
            }
        case "error": add(.stopped(lastText: nil, failed: true))
        case "tool":
            let tool = AgentHookTools.name(.opencode, json["tool"] as? String ?? "")
            let input = arguments(json["input"])
            let id = text(json, "call") ?? UUID().uuidString
            switch json["status"] as? String {
            case "running", "pending":
                var use = toolUse(tool, input: input, at: date, policy: policy)
                use = ClaudeToolUse(id: id, name: use.name, summary: use.summary, started: date, inputKey: use.inputKey)
                add(.toolStarted(use))
            case "completed", "error":
                add(.toolFinished(id: id, name: tool, inputKey: nil, failed: json["status"] as? String == "error",
                                  started: nil))
            default: break
            }
        case "permission":
            guard let request = text(json, "id") else { return }
            let tool = AgentHookTools.name(.opencode, json["permission"] as? String ?? "")
            let metadata = json["metadata"] as? [String: Any] ?? [:]
            let patterns = (json["patterns"] as? [String] ?? []).filter { !$0.isEmpty }
            let detail = (metadata["command"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                ?? (patterns.isEmpty ? ClaudeToolSummary.arguments(metadata) : patterns.joined(separator: "\n"))
            let always = (json["always"] as? [String] ?? []).contains { !$0.isEmpty }
            decoded.permission = (session, request)
            add(.permission(ClaudePermission(
                requestID: request, tool: tool.isEmpty ? "Tool" : tool,
                summary: ClaudeToolSummary.clip(detail.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
                detail: String(detail.prefix(ClaudeToolSummary.detailLimit)), offersAlways: always, since: date)))
        case "permissionReplied": add(.permissionSettled(text(json, "id")))
        case "question": add(.question)
        case "reply":
            if policy.previews, let text = preview(json["text"]) { add(.reply(text)) }
        case "message":
            if let model = text(json, "model") { add(.model(model)) }
            if let tokens = json["tokens"] as? [String: Any], let message = text(json, "id") {
                let cache = tokens["cache"] as? [String: Any] ?? [:]
                let usage = AgentTokens(input: int(tokens["input"]), cacheWrite: int(cache["write"]), cacheRead: int(cache["read"]),
                                        output: int(tokens["output"]) + int(tokens["reasoning"]), reasoning: int(tokens["reasoning"]))
                add(.usage(key: message, tokens: usage, cost: (json["cost"] as? NSNumber)?.doubleValue ?? 0))
            }
        case "title":
            if policy.previews, let title = preview(json["title"]) { add(.title(title)) }
        default: break
        }
    }

    // MARK: Answers

    /// What OpenCode's plugin passes on for a decision: once, always, or reject.
    static func opencodeReply(_ decision: ClaudeHookMessage.Decision) -> Data? {
        let reply: String
        switch decision {
        case .allow: reply = "once"
        case .allowAlways: reply = "always"
        case .deny: reply = "reject"
        case .answer: return nil
        }
        return try? JSONSerialization.data(withJSONObject: ["reply": reply], options: [.sortedKeys])
    }

    /// A prompt for OpenCode's listening plugin to send to one of its sessions.
    static func opencodePrompt(session: String, text: String) -> Data? {
        try? JSONSerialization.data(withJSONObject: ["prompt": ["session": session, "text": text]], options: [.sortedKeys])
    }

    // MARK: Reading

    static func toolUse(_ tool: String, input: [String: Any], at date: Date, policy: ClaudeContentPolicy) -> ClaudeToolUse {
        ClaudeToolUse(id: UUID().uuidString, name: tool.isEmpty ? "Tool" : tool,
                      summary: policy.previews ? AgentHookTools.summary(tool, input: input) : nil, started: date,
                      inputKey: ClaudeToolSummary.inputKey(name: tool, input: input))
    }

    /// Tool arguments, given as an object or as JSON text.
    static func arguments(_ value: Any?) -> [String: Any] {
        if let object = value as? [String: Any] { return object }
        if let text = value as? String, let data = text.data(using: .utf8),
           let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] { return object }
        return [:]
    }

    private static func text(_ json: [String: Any], _ key: String) -> String? {
        (json[key] as? String).flatMap { $0.isEmpty ? nil : $0 }
    }

    private static func int(_ value: Any?) -> Int { (value as? NSNumber)?.intValue ?? 0 }

    /// An ISO 8601 time, or milliseconds since 1970.
    static func timestamp(_ value: Any?) -> Date? {
        if let number = value as? NSNumber, number.doubleValue > 1_000_000_000_000 {
            return Date(timeIntervalSince1970: number.doubleValue / 1000)
        }
        guard let text = value as? String else { return nil }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: text) { return date }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: text)
    }

    static func preview(_ value: Any?) -> String? {
        guard let text = (value as? String)?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty else { return nil }
        let limit = ClaudeTranscriptReader.replyLimit
        return text.count > limit ? String(text.prefix(limit - 1)) + "…" : text
    }
}

/// The agents' tools by the names Claude Code gives the same ones, so they
/// read and are summed up alike in the island.
enum AgentHookTools {
    private static let names: [ClaudeSessionProvider: [String: String]] = [
        .gemini: ["run_shell_command": "Bash", "read_file": "Read", "read_many_files": "Read", "write_file": "Write",
                  "replace": "Edit", "edit": "Edit", "glob": "Glob", "search_file_content": "Grep", "grep_search": "Grep",
                  "list_directory": "LS", "web_fetch": "WebFetch", "google_web_search": "WebSearch",
                  "save_memory": "Memory", "write_todos": "TodoWrite", "exec": "Bash", "info": "Tool"],
        .copilot: ["bash": "Bash", "powershell": "PowerShell", "view": "Read", "create": "Write", "edit": "Edit",
                   "glob": "Glob", "grep": "Grep", "web_fetch": "WebFetch", "task": "Task", "ask_user": "AskUserQuestion"],
        .opencode: ["bash": "Bash", "read": "Read", "write": "Write", "edit": "Edit", "patch": "Edit", "glob": "Glob",
                    "grep": "Grep", "list": "LS", "webfetch": "WebFetch", "websearch": "WebSearch", "task": "Task",
                    "todowrite": "TodoWrite", "todoread": "TodoRead", "external_directory": "Folder"],
        .cursor: ["Shell": "Bash", "Read": "Read", "Write": "Write", "Edit": "Edit", "Grep": "Grep", "Glob": "Glob",
                  "Task": "Task", "Delete": "Delete"],
    ]

    static func name(_ provider: ClaudeSessionProvider, _ tool: String) -> String {
        if let known = names[provider]?[tool] { return known }
        if tool.hasPrefix("MCP:") { return String(tool.dropFirst(4)) }
        guard let first = tool.first else { return tool }
        return first.uppercased() + tool.dropFirst()
    }

    /// One line on what a tool works on, from the first field that says.
    static func summary(_ tool: String, input: [String: Any]) -> String? {
        if let claude = ClaudeToolSummary.summary(name: tool, input: input) { return claude }
        for key in ["command", "file_path", "filePath", "absolute_path", "path", "pattern", "url", "query", "description"] {
            guard let value = input[key] as? String else { continue }
            let line = value.split(whereSeparator: \.isNewline).first.map(String.init)?.trimmingCharacters(in: .whitespaces) ?? ""
            guard !line.isEmpty else { continue }
            let path = key.lowercased().contains("path")
            return ClaudeToolSummary.clip(path ? (line as NSString).lastPathComponent : line)
        }
        return nil
    }
}
