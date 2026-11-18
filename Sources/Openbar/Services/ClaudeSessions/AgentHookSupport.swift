// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Devin CLI reads the hooks in Claude Code's settings by default
/// (`read_config_from.claude`), so the island's Claude Code hooks run in
/// Devin's sessions too. Those are told apart and let go: Devin's sessions
/// come through hooks of their own, and a Devin request held as Claude
/// Code's would be answered in a form Devin doesn't read.
enum AgentDevinClaudeHooks {
    /// Whether a hook in Claude Code's format came from Devin CLI: its process
    /// is Devin's, or its input has Devin's per-turn id and none of the
    /// transcript every Claude Code hook names.
    static func isDevin(_ payload: Data, peer: pid_t) -> Bool {
        if isDevinPayload(payload) { return true }
        guard peer > 0 else { return false }
        let agent = AgentHookProcess.agent(fromHook: peer, hint: 0)
        return agent > 0 && isDevinProcess(AgentHookProcess.name(of: agent))
    }

    static func isDevinPayload(_ payload: Data) -> Bool {
        guard AgentLogParser.contains(payload, "\"prompt_id\""),
              let json = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] else { return false }
        return json["prompt_id"] != nil && json["transcript_path"] == nil
    }

    static func isDevinProcess(_ name: String?) -> Bool {
        guard let name = name?.lowercased() else { return false }
        return name == "devin" || name.hasPrefix("devin-") || name.hasPrefix("devin.")
    }
}

/// Grok CLI runs the hooks in Claude Code's settings too (its `[compat.claude]`
/// hooks, on by default), handing them its own input. Those are let go:
/// Grok's sessions come through hooks of their own (#243).
enum AgentGrokClaudeHooks {
    /// Grok's input names the workspace root and its event in camelCase,
    /// which no Claude Code hook input does.
    static func isGrok(_ payload: Data) -> Bool {
        guard AgentLogParser.contains(payload, "\"workspaceRoot\""), AgentLogParser.contains(payload, "\"hookEventName\""),
              let json = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] else { return false }
        return json["workspaceRoot"] != nil && json["hookEventName"] != nil && json["sessionId"] != nil
    }
}

/// The model of a Grok CLI session, from the summary Grok keeps beside the
/// conversation its hooks name (`…/sessions/<folder>/<id>/updates.jsonl`).
/// The summary's other fields (title, last turn) are not read.
enum AgentGrokLog {
    static let sizeLimit = 1024 * 1024

    static func model(transcript: String) -> String? {
        let summary = URL(fileURLWithPath: transcript).deletingLastPathComponent().appending(path: "summary.json")
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: summary.path),
              ((attributes[.size] as? NSNumber)?.intValue ?? 0) <= sizeLimit,
              let data = FileManager.default.contents(atPath: summary.path) else { return nil }
        return model(summary: data)
    }

    static func model(summary data: Data) -> String? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let model = json["current_model_id"] as? String,
              !model.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return model
    }
}

/// The last reply of a Mistral Vibe session, read from its own log (the
/// folder its hooks name as `transcript_path`), since the hooks carry none.
/// Vibe keeps the conversation in JSON chunks under `chunks/`; the newest
/// holding history ends on the latest messages. Read only with previews on.
enum AgentVibeLog {
    /// Chunks larger than this are skipped rather than read on a hook.
    static let chunkLimit = 4 * 1024 * 1024
    static let chunksRead = 6

    static func lastReply(in folder: String) -> String? {
        let chunks = URL(fileURLWithPath: folder, isDirectory: true).appending(path: "chunks")
        let manager = FileManager.default
        guard let names = try? manager.contentsOfDirectory(atPath: chunks.path) else { return nil }
        let files = names.filter { $0.hasSuffix(".json") }.compactMap { name -> (url: URL, date: Date, size: Int)? in
            let url = chunks.appending(path: name)
            guard let attributes = try? manager.attributesOfItem(atPath: url.path),
                  let date = attributes[.modificationDate] as? Date else { return nil }
            return (url, date, (attributes[.size] as? NSNumber)?.intValue ?? 0)
        }
        for file in files.sorted(by: { $0.date > $1.date }).prefix(chunksRead) where file.size <= chunkLimit {
            guard let data = try? Data(contentsOf: file.url), AgentLogParser.contains(data, "\"history\"") else { continue }
            if let reply = lastReply(chunk: data) { return reply }
        }
        return nil
    }

    /// The text of the last assistant message in one chunk of history.
    static func lastReply(chunk: Data) -> String? {
        guard let entries = (try? JSONSerialization.jsonObject(with: chunk)) as? [[String: Any]] else { return nil }
        for entry in entries.reversed() {
            guard entry["source"] as? String == "history", let message = entry["message"] as? [String: Any],
                  message["role"] as? String == "assistant" else { continue }
            let parts = message["content"] as? [[String: Any]] ?? []
            let text = parts.filter { $0["type"] as? String == "text" }.compactMap { $0["text"] as? String }.joined(separator: "\n")
            if let reply = AgentHookMessage.preview(text) { return reply }
        }
        return nil
    }
}

/// The agent's last reply in a Google Antigravity conversation, read from
/// the transcript its hooks name
/// (`…/brain/<conversation>/.system_generated/logs/transcript_full.jsonl`),
/// since the hooks carry none. One step per line; `PLANNER_RESPONSE` steps
/// hold the model's text, empty on a step that only calls tools. Read only
/// with previews on, and only the file's end.
enum AgentAntigravityLog {
    static let tailBytes = 1024 * 1024
    static let names: Set<String> = ["transcript_full.jsonl", "transcript.jsonl"]

    static func lastReply(in path: String) -> String? {
        lastContent(in: path) { $0 == "PLANNER_RESPONSE" }.flatMap(AgentHookMessage.preview)
    }

    /// The content of the last step of a type that has any, in the file's last part.
    static func lastContent(in path: String, where matches: (String) -> Bool) -> String? {
        guard names.contains((path as NSString).lastPathComponent), let data = tail(path) else { return nil }
        return lastContent(lines: data, where: matches)
    }

    static func lastContent(lines data: Data, where matches: (String) -> Bool) -> String? {
        for line in data.split(separator: 0x0A).reversed() {
            guard let step = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any],
                  let type = step["type"] as? String, matches(type),
                  let content = step["content"] as? String,
                  !content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { continue }
            return content
        }
        return nil
    }

    private static func tail(_ path: String) -> Data? {
        guard let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        guard let size = try? handle.seekToEnd() else { return nil }
        let start = size > UInt64(tailBytes) ? size - UInt64(tailBytes) : 0
        try? handle.seek(toOffset: start)
        guard var data = try? handle.readToEnd() else { return nil }
        // A cut first line is not a step.
        if start > 0, let newline = data.firstIndex(of: 0x0A) { data = data[(newline + 1)...] }
        return data
    }
}
