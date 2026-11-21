// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation
import CoreFoundation

/// One event from Gemini CLI, Copilot CLI, Cursor, OpenCode, Pi, Mistral
/// Vibe or Devin CLI, as the island reads it. The hook the island installs wraps what the agent hands it in
/// an envelope naming the agent, the hook and the agent's process:
///
///     {"openbar": "gemini", "hook": "BeforeTool", "pid": 4242, "event": {…}}
///
/// OpenCode's plugin and Pi's extension write the same envelope with an
/// event of their own making (docs/more-session-agents.md). Prompts, replies and tool arguments
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
        /// A permission or question the island answers itself, over the
        /// connection that holds it (Vibe's app-server proxy).
        case heldPermission(ClaudePermission)
        /// The request was answered, in the agent or here.
        case permissionSettled(String?)
        /// A question or other dialog of the agent's waits on the person.
        case question
        /// The agent wrote a reply; with previews.
        case reply(String)
        /// The turn ended; `lastText` is the reply it ended on, with previews.
        case stopped(lastText: String?, failed: Bool)
        case model(String)
        /// What a permission would ask of the tool about to run, kept until
        /// the agent tells it asks (Grok CLI names the tool only before).
        case mayAsk(ClaudePermission)
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
    static let key = "openbar"
    /// The config folder's tag or tool id a hook was written for (#236).
    static let viaKey = "via"

    /// What one envelope holds: the events in it, and for a connection that
    /// waits on the island, what it waits for.
    struct Decoded {
        var events: [AgentHookEvent] = []
        /// An OpenCode permission request, answered on the same connection.
        var permission: (session: String, request: String)? = nil
        /// OpenCode's plugin or Pi's extension listening for replies the person types here.
        var listens = false
        /// The folder of the session's own log, where the agent names one (Vibe).
        var transcript: String? = nil
        /// A session Vibe's app-server proxy runs, which then answers its approvals.
        var attached: String? = nil
        /// The events come from the proxy, not from Vibe's hooks.
        var fromProxy = false
        /// The config folder's tag (Codex) or tool id the hook names (#236).
        var via: String? = nil
        /// The input of a question the hook holds, handed back whole with
        /// the answers (ZCode).
        var input: [String: Any]? = nil
        let provider: ClaudeSessionProvider
        let pid: Int32
    }

    /// Whether the input is an envelope from another agent, told cheaply.
    static func isEnvelope(_ data: Data) -> Bool {
        AgentLogParser.contains(data, #""openbar":"#)
    }

    /// Whether the hook waits for the island's answer: an OpenCode or ZCode
    /// permission, or OpenCode's plugin or Pi's extension listening. Every
    /// other hook is let go at once.
    static func waits(_ data: Data) -> Bool {
        guard isEnvelope(data), AgentLogParser.contains(data, "\"opencode\"") || AgentLogParser.contains(data, "\"pi\"")
            || AgentLogParser.contains(data, "\"proxy\"") || AgentLogParser.contains(data, "\"zcode\"")
        else { return false }
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let name = json[key] as? String, let provider = ClaudeSessionProvider(rawValue: name),
              let event = json["event"] as? [String: Any] else { return false }
        let kind = event["kind"] as? String
        // Vibe's proxy holds the connection of an approval or a question, which the island answers.
        if provider == .vibe { return json["hook"] as? String == "proxy" && (kind == "approval" || kind == "question") }
        // ZCode's permission hook prints the island's answer.
        if provider == .zcode { return json["hook"] as? String == "PermissionRequest" }
        guard provider.reportsThroughPlugin else { return false }
        return kind == "listen" || (provider == .opencode && kind == "permission")
    }

    static func decode(_ data: Data, now: Date, policy: ClaudeContentPolicy) -> Decoded? {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let name = json[key] as? String, let provider = ClaudeSessionProvider(rawValue: name),
              let event = json["event"] as? [String: Any] else { return nil }
        let via = (json[viaKey] as? String).flatMap { AgentHookInstaller.isTag($0) ? $0 : nil }
        // Codex and the tools added by hand are heard only through hooks naming their folder or tool.
        guard provider.reportsThroughHooks || ((provider == .codex || provider == .custom) && via != nil) else { return nil }
        let pid = Int32(clamping: (json["pid"] as? NSNumber)?.int64Value ?? 0)
        let hook = json["hook"] as? String ?? event["hook_event_name"] as? String ?? ""
        var decoded = Decoded(provider: provider, pid: pid)
        decoded.via = via
        switch provider {
        case .gemini: decoded.events = gemini(hook, event, pid: pid, now: now, policy: policy)
        case .copilot: decoded.events = copilot(hook, event, pid: pid, now: now, policy: policy)
        case .cursor: decoded.events = cursor(hook, event, pid: pid, now: now, policy: policy)
        case .opencode, .pi: plugin(provider, event, pid: pid, now: now, policy: policy, into: &decoded)
        case .vibe:
            if hook == "proxy" {
                vibeProxy(event, pid: pid, now: now, policy: policy, into: &decoded)
            } else {
                decoded.events = vibe(hook, event, pid: pid, now: now, policy: policy)
                decoded.transcript = text(event, "transcript_path")
            }
        case .devin: decoded.events = claudeStyle(.devin, hook, event, pid: pid, now: now, policy: policy)
        case .codex, .custom:
            guard let via else { return nil }
            decoded.events = claudeStyle(provider, hook, event, pid: pid, now: now, policy: policy, via: via)
        case .antigravity:
            decoded.events = antigravity(hook, event, pid: pid, now: now, policy: policy)
            decoded.transcript = text(event, "transcriptPath")
        case .grok:
            decoded.events = grok(hook, event, pid: pid, now: now, policy: policy)
            decoded.transcript = text(event, "transcriptPath")
        case .zcode: zcode(hook, event, pid: pid, now: now, policy: policy, into: &decoded)
        case .claude: return nil
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
                detail: detail, offersAlways: false, since: date, diffs: ClaudeFileDiffs.gemini(details))))
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

    // MARK: Mistral Vibe

    /// Mistral Vibe's hooks.toml hooks (its bundled `vibe` skill, "Hooks").
    /// A notification only tells of an approval or a question, answered in
    /// Vibe; the end of the person's turn is heard from it too.
    private static func vibe(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                             policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "session_id") else { return [] }
        // A subagent's hooks name the session it works for; its tools are the parent's.
        if text(json, "parent_session_id") != nil { return [] }
        let cwd = text(json, "cwd")
        var events: [AgentHookEvent] = []
        func add(_ kind: AgentHookEvent.Kind) {
            events.append(AgentHookEvent(provider: .vibe, sessionID: session, pid: pid, cwd: cwd, date: now, kind: kind))
        }
        let raw = json["tool_name"] as? String ?? ""
        let tool = AgentHookTools.name(.vibe, raw)
        let input = arguments(json["tool_input"])
        switch hook {
        case "session_start":
            add(.started)
            if let model = text(json, "model") { add(.model(model)) }
        case "session_end": add(.ended)
        case "pre_agent": add(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "pre_tool":
            var use = toolUse(tool, input: input, at: now, policy: policy)
            if let id = text(json, "tool_call_id") {
                use = ClaudeToolUse(id: id, name: use.name, summary: use.summary, started: now, inputKey: use.inputKey)
            }
            add(.toolStarted(use))
        case "post_tool":
            add(.toolFinished(id: text(json, "tool_call_id"), name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                              failed: json["tool_status"] as? String == "failure", started: nil))
        case "notification":
            switch json["notification_type"] as? String {
            case "approval_requested":
                let asked = AgentHookTools.name(.vibe, json["tool_name"] as? String ?? "")
                let message = text(json, "message") ?? ""
                add(.permission(ClaudePermission(
                    requestID: UUID().uuidString, tool: asked.isEmpty ? "Tool" : asked,
                    summary: ClaudeToolSummary.clip(message), detail: message, offersAlways: false, since: now)))
            case "question_asked": add(.question)
            case "turn_completed": add(.stopped(lastText: nil, failed: false))
            case "turn_failed": add(.stopped(lastText: nil, failed: true))
            default: break
            }
        default: break
        }
        return events
    }

    // MARK: Mistral Vibe's app-server proxy

    /// What the proxy a New session runs Vibe's server through tells the
    /// island (`VibeProxy`): a session it serves, an approval or question the
    /// island can answer, and one answered elsewhere.
    private static func vibeProxy(_ json: [String: Any], pid: Int32, now: Date, policy: ClaudeContentPolicy,
                                  into decoded: inout Decoded) {
        decoded.fromProxy = true
        guard let session = text(json, "session"), let request = text(json, "request") ?? (json["kind"] as? String == "attach" ? "" : nil)
        else { return }
        let cwd = text(json, "cwd")
        // The proxy's pid is not the agent's: the hooks name that.
        func add(_ kind: AgentHookEvent.Kind) {
            decoded.events.append(AgentHookEvent(provider: .vibe, sessionID: session, pid: 0, cwd: cwd, date: now, kind: kind))
        }
        switch json["kind"] as? String {
        case "attach":
            decoded.attached = session
        case "settled":
            add(.permissionSettled(request))
        case "approval":
            let raw = json["tool"] as? String ?? ""
            let tool = AgentHookTools.name(.vibe, raw)
            let input = (json["input"] as? [String: Any]) ?? [:]
            let detail = [input["command"], input["file_path"], input["path"], input["url"]]
                .compactMap { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }.first
                ?? (input.isEmpty ? "" : ClaudeToolSummary.arguments(input))
            decoded.permission = (session, request)
            let content = PermissionContent(full: detail, complete: true, originalLength: detail.unicodeScalars.count)
            add(.heldPermission(ClaudePermission(
                requestID: request, tool: tool.isEmpty ? "Tool" : tool,
                summary: ClaudeToolSummary.clip(detail.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
                detail: content.preview, content: content, offersAlways: json["always"] as? Bool ?? false, since: now)))
        case "question":
            let questions = (json["questions"] as? [[String: Any]] ?? []).compactMap { item -> ClaudeQuestion? in
                guard let asked = item["question"] as? String, !asked.isEmpty else { return nil }
                let options = (item["options"] as? [[String: Any]] ?? []).compactMap { option -> ClaudeQuestion.Option? in
                    guard let label = option["label"] as? String, !label.isEmpty else { return nil }
                    return ClaudeQuestion.Option(label: label, description: (option["description"] as? String).flatMap { $0.isEmpty ? nil : $0 })
                }
                return ClaudeQuestion(question: asked, header: (item["header"] as? String).flatMap { $0.isEmpty ? nil : $0 },
                                      options: options, multiSelect: item["multiSelect"] as? Bool ?? false)
            }
            guard !questions.isEmpty else { return }
            decoded.permission = (session, request)
            add(.heldPermission(ClaudePermission(
                requestID: request, tool: ClaudeQuestion.tool, summary: ClaudeToolSummary.clip(questions[0].question),
                offersAlways: false, since: now, questions: questions)))
        default: break
        }
    }

    /// What the proxy passes on to Vibe for the person's decision on a request.
    static func vibeReply(_ decision: ClaudeHookMessage.Decision, questions: [ClaudeQuestion] = []) -> Data? {
        let reply: [String: Any]
        switch decision {
        case .allow, .approvePlan: reply = ["decision": "approve"]
        case .allowAlways: reply = ["decision": "approve_for_session"]
        case .deny:
            reply = questions.isEmpty ? ["decision": "deny", "feedback": ClaudeHookMessage.denyMessage] : ["cancelled": true]
        case .answer(let answers):
            let given = questions.compactMap { question -> [String: Any]? in
                guard let picked = answers[question.question], !picked.isEmpty else { return nil }
                let text = picked.joined(separator: ", ")
                let listed = Set(question.options.map(\.label))
                return ["question": question.question, "answer": text, "isOther": !picked.allSatisfy(listed.contains)]
            }
            guard !given.isEmpty, given.count == questions.count else { return nil }
            reply = ["answers": given]
        }
        return try? JSONSerialization.data(withJSONObject: reply, options: [.sortedKeys])
    }

    // MARK: Devin CLI

    /// Hooks in Claude Code's format: Devin CLI's (docs.devin.ai, "Lifecycle
    /// Hooks"), whose payloads carry no folder (the island reads the
    /// agent's), and those the Hooks page writes for a second Codex home or a
    /// tool added by hand (#236), whose session ids carry `via` so they stay
    /// apart from Codex's server and from each other. A permission is heard
    /// and answered in the agent.
    private static func claudeStyle(_ provider: ClaudeSessionProvider, _ hook: String, _ json: [String: Any], pid: Int32,
                                    now: Date, policy: ClaudeContentPolicy, via: String? = nil) -> [AgentHookEvent] {
        guard let own = text(json, "session_id") else { return [] }
        let session = via.map { "\($0):\(own)" } ?? own
        let cwd = text(json, "cwd")
        var events: [AgentHookEvent] = []
        func add(_ kind: AgentHookEvent.Kind) {
            events.append(AgentHookEvent(provider: provider, sessionID: session, pid: pid, cwd: cwd, date: now, kind: kind))
        }
        let raw = json["tool_name"] as? String ?? ""
        let tool = AgentHookTools.name(provider, raw)
        let input = arguments(json["tool_input"])
        switch hook {
        case "SessionStart": add(.started)
        case "SessionEnd": add(.ended)
        case "UserPromptSubmit": add(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "PreToolUse":
            // Devin's question to the person is a tool of its own.
            if raw == "ask_user_question" { add(.question) } else { add(.toolStarted(toolUse(tool, input: input, at: now, policy: policy))) }
        case "PostToolUse":
            if raw == "ask_user_question" { add(.permissionSettled(nil)); break }
            let response = json["tool_response"] as? [String: Any]
            add(.toolFinished(id: nil, name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                              failed: response?["success"] as? Bool == false, started: nil))
        case "PermissionRequest":
            let detail = [input["command"], input["file_path"], input["path"], input["url"]]
                .compactMap { ($0 as? String).flatMap { $0.isEmpty ? nil : $0 } }.first
                ?? (input.isEmpty ? "" : ClaudeToolSummary.arguments(input))
            add(.permission(ClaudePermission(
                requestID: UUID().uuidString, tool: tool.isEmpty ? "Tool" : tool,
                summary: ClaudeToolSummary.clip(detail.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
                detail: detail, offersAlways: false, since: now)))
        case "PostToolUseFailure":
            add(.toolFinished(id: nil, name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                              failed: true, started: nil))
        case "Stop":
            let last = policy.previews ? preview(json["last_assistant_message"]) : nil
            add(.stopped(lastText: last, failed: false))
        default: break
        }
        if provider != .devin, let model = text(json, "model") { add(.model(model)) }
        return events
    }

    // MARK: ZCode

    /// ZCode's hooks (zcode.z.ai/en/docs/hooks), in Claude Code's format
    /// with camelCase copies beside. Its permission hook runs while ZCode's
    /// own prompt is up: the island's answer, printed by the hook, wins if it
    /// comes first, and ZCode stops the hook when the person answers there.
    /// A question is a permission for `AskUserQuestion`, answered with the
    /// question's input and the answers in it. It has no hook at its end:
    /// a session goes when its process does.
    private static func zcode(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                              policy: ClaudeContentPolicy, into decoded: inout Decoded) {
        guard let session = text(json, "session_id") ?? text(json, "sessionId") else { return }
        let cwd = text(json, "cwd")
        func add(_ kind: AgentHookEvent.Kind) {
            decoded.events.append(AgentHookEvent(provider: .zcode, sessionID: session, pid: pid, cwd: cwd, date: now, kind: kind))
        }
        let tool = AgentHookTools.name(.zcode, json["tool_name"] as? String ?? json["toolName"] as? String ?? "")
        let input = arguments(json["tool_input"] ?? json["toolInput"])
        let call = text(json, "tool_use_id") ?? text(json, "toolCallId")
        let asks = tool == ClaudeQuestion.tool
        let event = text(json, "hook_event_name") ?? text(json, "hookEventName") ?? hook
        switch event {
        case "SessionStart":
            add(.started)
            if let model = zcodeModel(text(json, "model")) { add(.model(model)) }
        case "UserPromptSubmit": add(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "PreToolUse":
            // A question is told by its permission request.
            if asks { break }
            var use = toolUse(tool, input: input, at: now, policy: policy)
            if let call { use = ClaudeToolUse(id: call, name: use.name, summary: use.summary, started: now, inputKey: use.inputKey) }
            add(.toolStarted(use))
        case "PermissionRequest":
            let request = call ?? text(json, "requestId") ?? UUID().uuidString
            let questions = asks ? ClaudeQuestion.questions(in: input) ?? [] : []
            // A question the island can't show is answered in ZCode.
            if asks && questions.isEmpty { add(.question); break }
            decoded.permission = (session, request)
            if !questions.isEmpty { decoded.input = input }
            // ZCode's tools take Claude Code's names and arguments, so the
            // request reads as Claude Code's, whatever the previews.
            let full = ClaudeToolSummary.permissionDetail(name: tool, input: input)
            let content = PermissionContent(full: full, complete: tool != "Bash" || (input["command"] as? String).map { !$0.isEmpty } == true,
                                            originalLength: full.unicodeScalars.count)
            add(.heldPermission(ClaudePermission(
                requestID: request, tool: tool.isEmpty ? "Tool" : tool,
                summary: questions.first.map { ClaudeToolSummary.clip($0.question) } ?? ClaudeToolSummary.summary(name: tool, input: input),
                detail: content.preview, content: content, offersAlways: false, since: now,
                inputKey: ClaudeToolSummary.inputKey(name: tool, input: input), questions: questions,
                diffs: ClaudeFileDiffs.diffs(tool: tool, input: input, cwd: cwd))))
        case "PostToolUse", "PostToolUseFailure":
            // The tool ran or failed: its request, if any, was answered, here or in ZCode.
            add(.permissionSettled(call))
            if asks { break }
            add(.toolFinished(id: call, name: tool, inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                              failed: event == "PostToolUseFailure", started: nil))
        case "Stop":
            add(.stopped(lastText: policy.previews ? preview(json["last_assistant_message"] ?? json["responseText"]) : nil,
                         failed: false))
        default: break
        }
    }

    /// What a ZCode session's folder is called. A task started in ZCode's
    /// app without a project runs in a folder of the app's own,
    /// `~/.zcode/workspace/default` or `draft-<id>`, named for nobody.
    static func zcodeProject(_ cwd: String) -> String {
        let parts = cwd.split(separator: "/")
        if parts.count >= 3, parts[parts.count - 3] == ".zcode", parts[parts.count - 2] == "workspace" { return zcodeTask }
        return AgentLogParser.projectName(cwd)
    }
    static let zcodeTask = "ZCode task"

    /// The model ZCode names as `provider/model`, without the provider.
    static func zcodeModel(_ named: String?) -> String? {
        guard let named, !named.isEmpty else { return nil }
        guard let slash = named.firstIndex(of: "/") else { return named }
        let model = named[named.index(after: slash)...]
        return model.isEmpty ? nil : String(model)
    }

    /// What ZCode's permission hook prints for the person's decision, as
    /// Claude Code's does. Answers go back in the question's own input, each
    /// as one text, as ZCode keeps them; a skipped question is a deny.
    static func zcodeReply(_ decision: ClaudeHookMessage.Decision, input: [String: Any]? = nil) -> Data? {
        let body: [String: Any]
        switch decision {
        case .allow, .allowAlways, .approvePlan: body = ["behavior": "allow"]
        case .deny: body = ["behavior": "deny", "message": input != nil ? ClaudeHookMessage.skipMessage : ClaudeHookMessage.denyMessage]
        case .answer(let answers):
            guard var updated = input else { return nil }
            updated["answers"] = answers.mapValues { $0.joined(separator: ", ") }
            body = ["behavior": "allow", "updatedInput": updated]
        }
        let output: [String: Any] = ["hookSpecificOutput": ["hookEventName": "PermissionRequest", "decision": body]]
        return try? JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
    }

    // MARK: Grok CLI

    /// Grok CLI's hooks (xAI's Grok Build, user guide "Hooks"): camelCase
    /// input with snake_case event names. A permission is told in two steps,
    /// the tool's `PreToolUse` and then a `permission_prompt` notification
    /// with no tool in it; the island answers it with the keys of Grok's
    /// prompt. A question is a tool of its own, answered in Grok. A turn
    /// ends with `Stop`, or without a done with `StopFailure` or `StopCancelled`; the `Stop` Grok
    /// runs as it shuts down ends nothing.
    private static func grok(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                             policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "sessionId") else { return [] }
        let cwd = text(json, "cwd") ?? text(json, "workspaceRoot")
        var events: [AgentHookEvent] = []
        func add(_ kind: AgentHookEvent.Kind) {
            events.append(AgentHookEvent(provider: .grok, sessionID: session, pid: pid, cwd: cwd, date: now, kind: kind))
        }
        let raw = json["toolName"] as? String ?? ""
        let tool = AgentHookTools.name(.grok, raw)
        let input = arguments(json["toolInput"])
        let event = text(json, "hookEventName") ?? hook
        switch event {
        case "session_start", "SessionStart":
            add(.started)
            if let model = text(json, "modelId") { add(.model(model)) }
        case "session_end", "SessionEnd": add(.ended)
        case "user_prompt_submit", "UserPromptSubmit": add(.prompt(policy.previews ? preview(json["prompt"]) : nil))
        case "pre_tool_use", "PreToolUse":
            if raw == "ask_user_question" { add(.question); break }
            var use = toolUse(tool, input: input, at: now, policy: policy)
            if let id = text(json, "toolUseId") {
                use = ClaudeToolUse(id: id, name: use.name, summary: use.summary, started: now, inputKey: use.inputKey)
            }
            add(.toolStarted(use))
            add(.mayAsk(grokAsk(tool, input: input, cwd: cwd, truncated: json["toolInputTruncated"] as? Bool == true, at: now)))
        case "post_tool_use", "PostToolUse", "post_tool_use_failure", "PostToolUseFailure":
            // A tool that ran or failed was let through, here or in Grok.
            add(.permissionSettled(nil))
            if raw == "ask_user_question" { break }
            add(.toolFinished(id: text(json, "toolUseId"), name: tool,
                              inputKey: ClaudeToolSummary.inputKey(name: tool, input: input),
                              failed: event.hasPrefix("post_tool_use_failure") || event == "PostToolUseFailure", started: nil))
        case "permission_denied", "PermissionDenied":
            add(.permissionSettled(nil))
        case "notification", "Notification":
            switch text(json, "notificationType") {
            case "permission_prompt":
                // Filled in from the tool the session's last PreToolUse named.
                add(.permission(ClaudePermission(requestID: UUID().uuidString, tool: "", summary: nil,
                                                 offersAlways: false, since: now)))
            case "elicitation_dialog": add(.question)
            default: break
            }
        case "stop", "Stop":
            if text(json, "reason") == "shutdown" { break }
            add(.stopped(lastText: policy.previews ? preview(json["lastAssistantMessage"]) : nil, failed: false))
        case "stop_failure", "StopFailure":
            add(.stopped(lastText: nil, failed: true))
        case "stop_cancelled", "StopCancelled":
            // Interrupted or refused: the turn ends, but nothing was done.
            add(.stopped(lastText: nil, failed: true))
        default: break
        }
        return events
    }

    /// What Grok's prompt would ask about the tool: its command, or the
    /// file it changes. Shown on the card like Claude Code's, whatever the
    /// previews, since the person decides on it.
    static func grokAsk(_ tool: String, input: [String: Any], cwd: String?, truncated: Bool = false,
                        at date: Date) -> ClaudePermission {
        // Grok's shell, write and search_replace take Claude Code's arguments
        // under Claude Code's names, so the request reads as Claude Code's.
        let full = ClaudeToolSummary.permissionDetail(name: tool, input: input)
        let complete = !truncated && (tool != "Bash" || (input["command"] as? String).map { !$0.isEmpty } == true)
        let content = PermissionContent(full: full, complete: complete, originalLength: complete ? full.unicodeScalars.count : nil)
        var permission = ClaudePermission(
            requestID: UUID().uuidString, tool: tool.isEmpty ? "Tool" : tool,
            summary: ClaudeToolSummary.clip(full.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
            detail: content.preview, content: content, offersAlways: false, since: date)
        permission.inputKey = ClaudeToolSummary.inputKey(name: tool, input: input)
        permission.diffs = ClaudeFileDiffs.diffs(tool: tool, input: input, cwd: cwd)
        return permission
    }

    // MARK: Google Antigravity

    /// Google Antigravity's lifecycle hooks (the hooks.json guide its
    /// language server carries), the same in its app, its IDE and `agy`. The
    /// input is camelCase, a session is a conversation, and there is no hook
    /// for a prompt or for a permission: a turn's first model invocation
    /// (`invocationNum` 0, with the prompt as `lastUserInput`) opens it,
    /// `Stop` ends it, and approvals are asked and answered in Antigravity.
    /// A tool Antigravity refused never reports back; `Stop` closes it.
    private static func antigravity(_ hook: String, _ json: [String: Any], pid: Int32, now: Date,
                                    policy: ClaudeContentPolicy) -> [AgentHookEvent] {
        guard let session = text(json, "conversationId") else { return [] }
        let cwd = (json["workspacePaths"] as? [String])?.first(where: { !$0.isEmpty }) ?? text(json, "cwd")
        var events: [AgentHookEvent] = []
        func add(_ kind: AgentHookEvent.Kind) {
            events.append(AgentHookEvent(provider: .antigravity, sessionID: session, pid: pid, cwd: cwd, date: now, kind: kind))
        }
        if let model = text(json, "modelName"), model.lowercased() != "auto" { add(.model(model)) }
        // A tool's hooks share the step it runs as, which ties the two together.
        let step = (json["stepIdx"] as? NSNumber).map { "step-\($0.intValue)" }
        switch hook {
        case "PreInvocation":
            if (json["invocationNum"] as? NSNumber)?.intValue == 0 {
                add(.prompt(policy.previews ? preview(json["lastUserInput"]) : nil))
            } else {
                add(.working)
            }
        case "PreToolUse":
            let call = json["toolCall"] as? [String: Any] ?? [:]
            let tool = AgentHookTools.name(.antigravity, call["name"] as? String ?? "")
            let input = AgentHookTools.antigravityInput(arguments(call["args"]))
            var use = toolUse(tool, input: input, at: now, policy: policy)
            if let step { use = ClaudeToolUse(id: step, name: use.name, summary: use.summary, started: now, inputKey: use.inputKey) }
            add(.toolStarted(use))
        case "PostToolUse":
            // It names the tool too, for a call whose start the island missed.
            let call = json["toolCall"] as? [String: Any] ?? [:]
            let tool = AgentHookTools.name(.antigravity, call["name"] as? String ?? "")
            add(.toolFinished(id: step, name: tool, inputKey: nil, failed: text(json, "error") != nil, started: nil))
        case "Stop":
            // As "NO_TOOL_CALL" for a turn that ended well; a failed one says so.
            let reason = (text(json, "terminationReason") ?? "").lowercased()
            add(.stopped(lastText: nil, failed: reason.contains("error") || text(json, "error") != nil))
        default: break
        }
        return events
    }

    // MARK: OpenCode and Pi

    /// What the island's OpenCode plugin (AgentOpenCodePlugin.swift) and Pi
    /// extension (AgentPiExtension.swift) write.
    private static func plugin(_ provider: ClaudeSessionProvider, _ json: [String: Any], pid: Int32, now: Date,
                               policy: ClaudeContentPolicy, into decoded: inout Decoded) {
        let kind = json["kind"] as? String ?? ""
        if kind == "listen" { decoded.listens = true; return }
        guard let session = text(json, "session") else { return }
        let cwd = text(json, "directory")
        let date = timestamp(json["at"]) ?? now
        func add(_ kind: AgentHookEvent.Kind) {
            decoded.events.append(AgentHookEvent(provider: provider, sessionID: session, pid: pid, cwd: cwd, date: date, kind: kind))
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
            let tool = AgentHookTools.name(provider, json["tool"] as? String ?? "")
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
        case "permission" where provider == .opencode:
            guard let request = text(json, "id") else { return }
            let tool = AgentHookTools.name(provider, json["permission"] as? String ?? "")
            let metadata = json["metadata"] as? [String: Any] ?? [:]
            let patterns = json["patterns"] as? [String] ?? []
            let detail = (metadata["command"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                ?? (patterns.isEmpty ? ClaudeToolSummary.arguments(metadata) : patterns.joined(separator: "\n"))
            let always = (json["always"] as? [String] ?? []).contains { !$0.isEmpty }
            decoded.permission = (session, request)
            let command = metadata["command"] as? String
            // JSONSerialization bridges both booleans and numbers through NSNumber.
            // A numeric 1 is not an explicit completeness acknowledgement.
            let lengthNumber = metadata["commandLength"] as? NSNumber
            let length = lengthNumber.flatMap { number -> Int? in
                guard CFGetTypeID(number) != CFBooleanGetTypeID(),
                      let value = metadata["commandLength"] as? Int, value >= 0 else { return nil }
                return value
            }
            let completeFlag = (metadata["commandComplete"] as? NSNumber).map {
                CFGetTypeID($0) == CFBooleanGetTypeID() && $0.boolValue
            } ?? false
            let shell = tool == "Bash" || metadata["command"] != nil
            let detailFlag = (json["permissionContentComplete"] as? NSNumber).map {
                CFGetTypeID($0) == CFBooleanGetTypeID() && $0.boolValue
            } ?? false
            let completeDetail = detailFlag && (json["metadata"] == nil || json["metadata"] is [String: Any])
                && (json["patterns"] == nil || json["patterns"] is [String])
            let detailLength = (json["permissionDetailLength"] as? NSNumber).flatMap { n -> Int? in
                guard CFGetTypeID(n) != CFBooleanGetTypeID(), let value = json["permissionDetailLength"] as? Int, value >= 0 else { return nil }
                return value
            }
            let complete = shell ? (completeFlag && command.map { !$0.isEmpty && length == $0.unicodeScalars.count } == true)
                : (completeDetail && (json["permissionDetailLength"] == nil || detailLength == detail.unicodeScalars.count))
            let content = PermissionContent(full: detail, complete: complete,
                                            originalLength: shell ? length : (detailLength ?? (complete ? detail.unicodeScalars.count : nil)))
            add(.permission(ClaudePermission(
                requestID: request, tool: tool.isEmpty ? "Tool" : tool,
                summary: ClaudeToolSummary.clip(detail.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""),
                detail: content.preview, content: content, offersAlways: always, since: date,
                diffs: tool == "Edit" && !shell ? ClaudeFileDiffs.opencode(metadata) : [])))
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
        case "model":
            if let model = text(json, "model") { add(.model(model)) }
        default: break
        }
    }

    // MARK: Answers

    /// What OpenCode's plugin passes on for a decision: once, always, or reject.
    static func opencodeReply(_ decision: ClaudeHookMessage.Decision) -> Data? {
        let reply: String
        switch decision {
        case .allow, .approvePlan: reply = "once"
        case .allowAlways: reply = "always"
        case .deny: reply = "reject"
        case .answer: return nil
        }
        return try? JSONSerialization.data(withJSONObject: ["reply": reply], options: [.sortedKeys])
    }

    /// A prompt for OpenCode's listening plugin or Pi's extension to send to one of its sessions.
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
        .pi: ["bash": "Bash", "read": "Read", "write": "Write", "edit": "Edit", "grep": "Grep", "find": "Glob", "ls": "LS"],
        // Vibe names a tool with its group, as `file_system.bash`; the last part is matched.
        .vibe: ["bash": "Bash", "process_start": "Bash", "read_file": "Read", "write_file": "Write", "search_replace": "Edit",
                "edit_file": "Edit", "grep": "Grep", "glob": "Glob", "list_dir": "LS", "web_fetch": "WebFetch",
                "web_search": "WebSearch", "todo": "TodoWrite", "agents_spawn": "Task", "ask_user_question": "AskUserQuestion"],
        .antigravity: ["run_command": "Bash", "view_file": "Read", "view_file_outline": "Read", "view_code_item": "Read",
                       "write_to_file": "Write", "replace_file_content": "Edit", "multi_replace_file_content": "Edit",
                       "grep_search": "Grep", "find_by_name": "Glob", "list_dir": "LS", "read_url_content": "WebFetch",
                       "search_web": "WebSearch", "command_status": "BashOutput", "send_command_input": "Bash",
                       "manage_task": "BashOutput"],
        .grok: ["run_terminal_command": "Bash", "read_file": "Read", "write": "Write", "search_replace": "Edit",
                "list_dir": "LS", "grep": "Grep", "todo_write": "TodoWrite", "spawn_subagent": "Task",
                "get_command_or_subagent_output": "BashOutput", "kill_command_or_subagent": "KillShell",
                "web_fetch": "WebFetch", "web_search": "WebSearch", "ask_user_question": "AskUserQuestion",
                "enter_plan_mode": "EnterPlanMode", "exit_plan_mode": "ExitPlanMode"],
        // ZCode's tools take Claude Code's names; its patch tool is an edit, its subagent a task.
        .zcode: ["ApplyPatch": "Edit", "Agent": "Task"],
        .devin: ["exec": "Bash", "read": "Read", "write": "Write", "edit": "Edit", "apply_patch": "Edit", "grep": "Grep",
                 "glob": "Glob", "webfetch": "WebFetch", "todo_write": "TodoWrite", "run_subagent": "Task",
                 "notebook_read": "Read", "notebook_edit": "Edit", "exit_plan_mode": "ExitPlanMode"],
        // Codex's hooks name its shell `Bash` already; its patch tool is an edit.
        .codex: ["apply_patch": "Edit", "shell": "Bash", "local_shell": "Bash", "exec_command": "Bash",
                 "update_plan": "TodoWrite", "view_image": "Read", "web_search": "WebSearch"],
    ]

    static func name(_ provider: ClaudeSessionProvider, _ tool: String) -> String {
        if let known = names[provider]?[tool] { return known }
        if provider == .vibe, let dot = tool.lastIndex(of: "."), !tool.hasPrefix("mcp_") {
            return name(provider, String(tool[tool.index(after: dot)...]))
        }
        if tool.hasPrefix("mcp__") { return String(tool.dropFirst(5)) }
        if tool.hasPrefix("MCP:") { return String(tool.dropFirst(4)) }
        guard let first = tool.first else { return tool }
        return first.uppercased() + tool.dropFirst()
    }

    /// Antigravity's tool arguments under the names Claude Code gives them,
    /// so they are summed up alike; its own one-line summary, when it wrote
    /// one, comes last.
    static func antigravityInput(_ args: [String: Any]) -> [String: Any] {
        var input = args
        let renames = ["CommandLine": "command", "AbsolutePath": "file_path", "TargetFile": "file_path",
                       "Query": "pattern", "SearchPath": "path", "DirectoryPath": "path", "SearchDirectory": "path",
                       "Pattern": "pattern", "Url": "url", "toolSummary": "description"]
        for (from, to) in renames where input[to] == nil {
            if let value = args[from] { input[to] = value }
        }
        return input
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
