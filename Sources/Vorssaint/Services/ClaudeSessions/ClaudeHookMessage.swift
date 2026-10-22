// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// One hook event as Claude Code writes it to the hook's input, and the
/// answer a permission request takes back.
struct ClaudeHookMessage {
    let fact: ClaudeHookFact
    let transcript: String?
    /// The rules Claude Code offered to stop asking, handed back as they came
    /// when the person picks Always.
    let suggestions: Any?

    var isPermissionRequest: Bool {
        if case .permissionRequested = fact.event { return true }
        return false
    }

    /// Nil for input that is not a hook event. Text and arguments are kept
    /// only as the policy allows, except what a permission would run.
    static func decode(_ data: Data, requestID: String, now: Date, policy: ClaudeContentPolicy) -> ClaudeHookMessage? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let name = json["hook_event_name"] as? String,
              let session = json["session_id"] as? String, !session.isEmpty else { return nil }
        let tool = json["tool_name"] as? String ?? ""
        let input = json["tool_input"] as? [String: Any] ?? [:]
        let toolID = json["tool_use_id"] as? String ?? ""
        let agent = (json["agent_id"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        var suggestions: Any?
        let event: ClaudeHookFact.Event
        // Subagent events name the subagent they are about; any other event
        // carrying an agent comes from inside that subagent.
        var from = agent
        switch name {
        case "SessionStart": event = .sessionStarted
        case "SessionEnd": event = .sessionEnded
        case "UserPromptSubmit": event = .promptSubmitted
        case "PreToolUse":
            guard !toolID.isEmpty else { return nil }
            let summary = policy.previews ? ClaudeToolSummary.summary(name: tool, input: input) : nil
            event = .toolStarted(ClaudeToolUse(id: toolID, name: tool, summary: summary, started: now,
                                               inputKey: ClaudeToolSummary.inputKey(name: tool, input: input)))
        case "PostToolUse", "PostToolUseFailure":
            guard !toolID.isEmpty else { return nil }
            event = .toolFinished(id: toolID, failed: name == "PostToolUseFailure")
        case "PermissionRequest":
            let offered = json["permission_suggestions"] as? [Any] ?? []
            suggestions = offered.isEmpty ? nil : offered
            event = .permissionRequested(ClaudePermission(
                requestID: requestID, tool: tool.isEmpty ? "Tool" : tool,
                summary: ClaudeToolSummary.summary(name: tool, input: input),
                detail: ClaudeToolSummary.permissionDetail(name: tool, input: input),
                offersAlways: !offered.isEmpty, since: now,
                inputKey: ClaudeToolSummary.inputKey(name: tool, input: input)))
        case "Notification": event = .notification(type: json["notification_type"] as? String ?? "")
        case "Stop":
            let text = policy.previews ? (json["last_assistant_message"] as? String).map(preview) : nil
            event = .stopped(lastText: text ?? nil)
        case "SubagentStart":
            guard let agent else { return nil }
            event = .subagentStarted(id: agent, type: json["agent_type"] as? String ?? "Agent")
            from = nil
        case "SubagentStop":
            guard let agent else { return nil }
            event = .subagentStopped(id: agent)
            from = nil
        default: event = .other(name)
        }
        return ClaudeHookMessage(
            fact: ClaudeHookFact(sessionID: session, date: now, event: event, agentID: from),
            transcript: json["transcript_path"] as? String, suggestions: suggestions)
    }

    private static func preview(_ text: String) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let limit = ClaudeTranscriptReader.replyLimit
        return trimmed.count > limit ? String(trimmed.prefix(limit - 1)) + "…" : trimmed
    }

    enum Decision: Equatable {
        case allow, allowAlways, deny
    }

    static let denyMessage = "Denied from the Dynamic Island."

    /// What the hook prints for Claude Code. Always adds the rules Claude Code
    /// offered, so it stops asking for the same thing.
    static func reply(_ decision: Decision, suggestions: Any?) -> Data {
        var body: [String: Any]
        switch decision {
        case .allow: body = ["behavior": "allow"]
        case .allowAlways:
            body = ["behavior": "allow"]
            if let suggestions { body["updatedPermissions"] = suggestions }
        case .deny: body = ["behavior": "deny", "message": denyMessage]
        }
        let output: [String: Any] = ["hookSpecificOutput": ["hookEventName": "PermissionRequest", "decision": body]]
        return (try? JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])) ?? Data()
    }

    /// Whether the input is a permission request, told without decoding the rest.
    static func isPermissionRequest(_ data: Data) -> Bool {
        AgentLogParser.contains(data, #""hook_event_name":"PermissionRequest""#)
            || ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["hook_event_name"] as? String == "PermissionRequest"
    }

    /// The input is complete once it parses: the hook sends one JSON object
    /// and keeps the connection open for the answer.
    static func isComplete(_ buffer: Data) -> Bool {
        guard let last = buffer.last(where: { !($0 == 0x20 || $0 == 0x0A || $0 == 0x0D || $0 == 0x09) }), last == 0x7D else {
            return false
        }
        return (try? JSONSerialization.jsonObject(with: buffer)) != nil
    }
}
