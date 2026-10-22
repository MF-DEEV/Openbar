// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Turns the lines of a Claude Code transcript into the facts the Sessions
/// section needs. Prompts, replies and tool arguments are decoded only when
/// the content policy allows previews; otherwise a line yields its times,
/// tool names and identifiers, branch, model and usage.
struct ClaudeTranscriptReader {
    var policy: ClaudeContentPolicy
    /// The subagent a `subagents/agent-<id>.jsonl` file belongs to.
    let subagent: String?
    /// Usage is read by the AI section's own reader so cost matches it.
    private var usageState = AgentLogState()

    init(policy: ClaudeContentPolicy, path: String) {
        self.policy = policy
        subagent = Self.subagent(of: path)
    }

    /// `<session>/subagents/agent-<id>.jsonl` names its subagent.
    static func subagent(of path: String) -> String? {
        guard AgentLogCursor.parent(of: path) != nil else { return nil }
        let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
        return name.hasPrefix("agent-") ? String(name.dropFirst(6)) : name
    }

    static let previewLimit = 280
    /// A reply is kept whole enough to read in the session's window.
    static let replyLimit = 6000

    mutating func parse(_ line: Data, now: Date) -> [ClaudeTranscriptFact] {
        var facts: [ClaudeTranscriptFact] = []
        if AgentLogParser.contains(line, #""type":"assistant""#) {
            for entry in ClaudeReader().parse(line, state: &usageState, now: now) {
                if case .usage(let key, let record, _) = entry {
                    facts.append(.usage(key: key, tokens: record.tokens, cost: record.cost ?? 0))
                }
            }
        }
        guard let json = AgentLogParser.object(line), let type = json["type"] as? String else { return facts }
        // A subagent run in the background ends without closing its own
        // transcript; the session hears of it as a task notification. Only
        // the records that deliver one count, never a reply quoting one.
        if subagent == nil, AgentLogParser.contains(line, "<task-notification>"), Self.deliversNotification(json, type: type),
           let ended = Self.finishedTask(line) {
            facts.append(.subagentEnded(id: ended.id, at: AgentLogParser.timestamp(json["timestamp"]) ?? now))
        }
        let date = AgentLogParser.timestamp(json["timestamp"]) ?? now
        if let branch = json["gitBranch"] as? String, !branch.isEmpty, branch != "HEAD" { facts.append(.branch(branch)) }
        if let subagent {
            facts.append(contentsOf: subagentFacts(json, type: type, id: subagent, date: date))
            return facts
        }
        switch type {
        case "assistant":
            guard json["isSidechain"] as? Bool != true, let message = json["message"] as? [String: Any] else { break }
            let model = AgentLogParser.native(message["model"] as? String ?? "")
            if !model.isEmpty, !model.hasPrefix("<") { facts.append(.model(model)) }
            for block in message["content"] as? [[String: Any]] ?? [] {
                switch block["type"] as? String {
                case "tool_use":
                    guard let id = block["id"] as? String, let name = block["name"] as? String else { continue }
                    let input = block["input"] as? [String: Any] ?? [:]
                    let summary = policy.previews ? ClaudeToolSummary.summary(name: name, input: input) : nil
                    facts.append(.toolStarted(ClaudeToolUse(id: id, name: name, summary: summary, started: date,
                                                            inputKey: ClaudeToolSummary.inputKey(name: name, input: input))))
                    if name == "Bash", input["run_in_background"] as? Bool == true {
                        let command = input["command"] as? String ?? ""
                        let description = input["description"] as? String
                        let label = policy.previews ? preview(description ?? command) : nil
                        facts.append(.backgroundStarted(toolID: id, service: ClaudeBackgroundKind.isService(command: command, description: description),
                                                        label: label, at: date))
                    }
                    // Stopping a background command ends it as its notice would.
                    if ["TaskStop", "KillShell", "KillBash"].contains(name),
                       let task = (input["task_id"] ?? input["shell_id"] ?? input["bash_id"]) as? String, !task.isEmpty {
                        facts.append(.subagentEnded(id: task, at: date))
                    }
                case "text":
                    if policy.previews, let text = preview(block["text"] as? String, limit: Self.replyLimit) {
                        facts.append(.text(text, at: date))
                    }
                default: break
                }
            }
            switch message["stop_reason"] as? String {
            case "end_turn", "stop_sequence", "max_tokens", "refusal":
                let failed = json["isApiErrorMessage"] as? Bool == true || model.hasPrefix("<")
                facts.append(.turnEnded(date, completed: !failed))
            default: break
            }
        case "user":
            guard json["isSidechain"] as? Bool != true else { break }
            let content = (json["message"] as? [String: Any])?["content"]
            let blocks = content as? [[String: Any]] ?? []
            let results = blocks.filter { $0["type"] as? String == "tool_result" }
            if !results.isEmpty {
                for result in results {
                    guard let id = result["tool_use_id"] as? String else { continue }
                    facts.append(.toolFinished(id: id, at: date, failed: result["is_error"] as? Bool == true))
                    if let task = (json["toolUseResult"] as? [String: Any])?["backgroundTaskId"] as? String, !task.isEmpty {
                        facts.append(.backgroundTask(toolID: id, task: task))
                    }
                }
                if json["toolEndsTurn"] as? Bool == true { facts.append(.turnEnded(date, completed: true)) }
                break
            }
            // The summary an idle compaction writes is no prompt and begins no turn.
            guard json["isMeta"] as? Bool != true, json["isCompactSummary"] as? Bool != true else { break }
            let texts = (content as? String).map { [$0] }
                ?? blocks.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }
            if texts.contains(where: { $0.hasPrefix("[Request interrupted by user") || $0.hasPrefix("<local-command-std") }) {
                facts.append(.turnEnded(date, completed: false))
            } else if texts.contains(where: { $0.hasPrefix("<command-name>") }) {
                // A slash command typed in the terminal, like /recap or /cost,
                // asks the model nothing by itself; one that does, like a
                // skill, begins its turn with the reply or the registry.
            } else if !texts.isEmpty {
                facts.append(.turnBegan(date))
                // Task notifications and the summary a compaction leaves
                // arrive as user lines but were never typed.
                if policy.previews, (json["origin"] as? [String: Any])?["kind"] as? String != "task-notification",
                   json["isCompactSummary"] as? Bool != true, let prompt = typed(texts) {
                    facts.append(.prompt(prompt, at: date))
                }
            }
        case "attachment":
            // A prompt typed while Claude worked joins the running turn.
            guard policy.previews, json["isSidechain"] as? Bool != true,
                  let attachment = json["attachment"] as? [String: Any],
                  attachment["type"] as? String == "queued_command" else { break }
            let prompt = attachment["prompt"]
            let texts = (prompt as? String).map { [$0] }
                ?? (prompt as? [[String: Any]] ?? []).compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }
            if let text = typed(texts) { facts.append(.prompt(text, at: date)) }
        case "ai-title":
            if policy.previews, let title = preview(json["aiTitle"] as? String) { facts.append(.title(title)) }
        default: break
        }
        return facts
    }

    static let finishedStatuses: Set<String> = ["completed", "failed", "killed", "stopped", "cancelled", "error"]

    static func deliversNotification(_ json: [String: Any], type: String) -> Bool {
        switch type {
        case "queue-operation": return true
        case "user": return (json["origin"] as? [String: Any])?["kind"] as? String == "task-notification"
        case "attachment": return (json["attachment"] as? [String: Any])?["type"] as? String == "queued_command"
        default: return false
        }
    }

    /// The task a notification line reports finished.
    static func finishedTask(_ line: Data) -> (id: String, status: String)? {
        let text = String(decoding: line, as: UTF8.self)
        guard let id = between(text, "<task-id>", "</task-id>"),
              let status = between(text, "<status>", "</status>"), finishedStatuses.contains(status) else { return nil }
        return (id, status)
    }

    private static func between(_ text: String, _ start: String, _ end: String) -> String? {
        guard let open = text.range(of: start), let close = text.range(of: end, range: open.upperBound..<text.endIndex)
        else { return nil }
        let value = text[open.upperBound..<close.lowerBound].trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty || value.count > 128 ? nil : value
    }

    private func subagentFacts(_ json: [String: Any], type: String, id: String, date: Date) -> [ClaudeTranscriptFact] {
        guard type == "assistant" || type == "user" else { return [] }
        if type == "assistant", let message = json["message"] as? [String: Any] {
            switch message["stop_reason"] as? String {
            case "end_turn", "stop_sequence", "max_tokens", "refusal": return [.subagentEnded(id: id, at: date)]
            default: break
            }
        }
        return [.subagentActivity(id: id, at: date)]
    }

    /// Text the person wrote; commands and notifications come wrapped in tags.
    private func typed(_ texts: [String]) -> String? {
        let text = texts.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.hasPrefix("<") else { return nil }
        return preview(text, limit: Self.replyLimit)
    }

    private func preview(_ text: String?, limit: Int = previewLimit) -> String? {
        guard let text else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return trimmed.count > limit ? String(trimmed.prefix(limit - 1)) + "…" : trimmed
    }

    // MARK: Side files

    /// The type and task of a subagent, from the `agent-<id>.meta.json` file
    /// beside its transcript.
    static func subagentInfo(_ data: Data, id: String, policy: ClaudeContentPolicy) -> ClaudeTranscriptFact? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return nil }
        let type = json["agentType"] as? String ?? "Agent"
        let description = policy.previews ? json["description"] as? String : nil
        return .subagentInfo(id: id, type: type.isEmpty ? "Agent" : type, description: description)
    }

    /// Progress of the task list Claude Code keeps in `tasks/<session>/`.
    static func tasks(_ files: [Data], policy: ClaudeContentPolicy) -> ClaudeTaskProgress? {
        var progress = ClaudeTaskProgress()
        for data in files {
            guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let status = json["status"] as? String, status != "deleted" else { continue }
            progress.total += 1
            if status == "completed" { progress.done += 1 }
            if status == "in_progress", policy.previews, progress.current == nil {
                progress.current = (json["activeForm"] as? String) ?? (json["subject"] as? String)
            }
        }
        return progress.total > 0 ? progress : nil
    }
}
