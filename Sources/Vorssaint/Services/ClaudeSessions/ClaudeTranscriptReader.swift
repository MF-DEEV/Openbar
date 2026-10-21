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
                    let summary = policy.previews
                        ? ClaudeToolSummary.summary(name: name, input: block["input"] as? [String: Any] ?? [:]) : nil
                    facts.append(.toolStarted(ClaudeToolUse(id: id, name: name, summary: summary, started: date)))
                case "text":
                    if policy.previews, let text = preview(block["text"] as? String) { facts.append(.text(text, at: date)) }
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
                }
                if json["toolEndsTurn"] as? Bool == true { facts.append(.turnEnded(date, completed: true)) }
                break
            }
            guard json["isMeta"] as? Bool != true else { break }
            let texts = (content as? String).map { [$0] }
                ?? blocks.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }
            if texts.contains(where: { $0.hasPrefix("[Request interrupted by user") || $0.hasPrefix("<local-command-std") }) {
                facts.append(.turnEnded(date, completed: false))
            } else if !texts.isEmpty {
                facts.append(.turnBegan(date))
            }
        case "ai-title":
            if policy.previews, let title = preview(json["aiTitle"] as? String) { facts.append(.title(title)) }
        default: break
        }
        return facts
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

    private func preview(_ text: String?) -> String? {
        guard let text else { return nil }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return trimmed.count > Self.previewLimit ? String(trimmed.prefix(Self.previewLimit - 1)) + "…" : trimmed
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
