// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Alibaba's Qwen Code (`qwen`, #239). Its hooks take Claude Code's format
/// in `~/.qwen/settings.json` (`QWEN_HOME` moves the folder), and it runs
/// them itself, with its own process as their parent.
///
/// Unlike Claude Code's, its permission hook runs before its own dialog
/// shows, and the dialog only opens once the hook has answered nothing: so
/// the island holds the request and answers it, and lets it go unanswered
/// whenever the person should answer in Qwen Code instead (Open, the island
/// going away). A question (`ask_user_question`) and a plan
/// (`exit_plan_mode`) are never allowed by a hook: their hook is let go at
/// once, Qwen Code shows its dialog, and the island answers it with keys
/// where the terminal takes them.
enum AgentQwen {
    /// Every event the island hears. `Notification` carries
    /// `permission_prompt` and `idle_prompt`; `StopFailure` ends a turn an
    /// error ended, in place of `Stop`.
    static let events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                         "PostToolUseFailure", "PermissionRequest", "Notification", "Stop", "StopFailure"]

    static let questionTool = "ask_user_question"
    static let planTool = "exit_plan_mode"

    /// Tools whose dialog Qwen Code shows whatever a hook answers.
    static let askedInQwen: Set<String> = [questionTool, planTool]

    /// The modes Qwen Code's plan dialog offers after "restore previous
    /// mode", in its order: auto-accept edits, then approve each.
    static let planModes: [ClaudePlanMode] = [.acceptEdits, .manual]

    // MARK: Places

    /// Qwen Code's folder: `QWEN_HOME`, or `~/.qwen`.
    static func folder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        environment["QWEN_HOME"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home.appending(path: ".qwen")
    }

    static func settings(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        folder(home: home, environment: environment).appending(path: "settings.json")
    }

    // MARK: Hooks

    /// The hook for one event. Qwen Code reads a command hook's timeout in
    /// seconds, but any value of 1000 or more in milliseconds, so the
    /// permission hook's long wait is written in milliseconds.
    static func entry(_ event: String, socket: String, bundle: String) -> [String: Any] {
        let timeout = event == "PermissionRequest" ? ClaudeHookInstaller.permissionTimeout * 1000 : AgentHookInstaller.eventTimeout
        return ["type": "command", "name": AgentHookInstaller.ownName(bundle), "timeout": timeout,
                "command": AgentHookInstaller.command(.qwen, event: event, socket: socket, bundle: bundle)]
    }

    /// What the permission hook prints: allow or deny. A question or a plan
    /// is never answered here.
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

    /// Whether the terminal sends keys in one burst with a return after
    /// them (Terminal's `do script`), rather than one by one.
    static func oneBurst(_ host: ClaudeSessionHost) -> Bool {
        if case .terminal = host { return true }
        return false
    }

    /// Whether the request is answered in Qwen Code's own dialog, by keys:
    /// a question, a plan, or a tool's once its hook was let go.
    static func answeredByKeys(_ asked: ClaudePermission) -> Bool {
        asked.asksQuestions || asked.tool == ClaudeToolSummary.planTool || asked.inAgentDialog
    }

    /// How long Qwen Code waits for its permission hook: its message bus
    /// gives up on a request after 60 seconds and shows its own dialog.
    /// The island lets the hook go a little before, and answers that dialog.
    static let holdLimit: TimeInterval = 55

    /// The keys that answer Qwen Code's own dialog for a question or a plan.
    ///
    /// Qwen Code reads one key from each burst a terminal sends: arrows,
    /// Space or Escape sent together with anything else are lost. So each
    /// key goes out on its own, a moment apart. Terminal can't do that: it
    /// types whatever it sends in one go and adds a return. There a
    /// choice's number, which picks it at once, answers a single question
    /// with one choice, and a plan's number approves it or keeps planning;
    /// anything else is answered in Qwen Code (`keyable`).
    ///
    /// Elsewhere, each question opens on its first choice: arrows down to
    /// the one picked and Return takes it; a multiple choice is ticked with
    /// Space on each, then Return. "Type something…" comes after the
    /// choices and takes the text typed on it. With more than one question,
    /// a last tab asks to submit, its Submit already picked. A plan's dialog
    /// offers to restore the mode before, to auto-accept edits, to approve
    /// each and to keep planning, always those four. Escape skips a
    /// question. Two returns at Qwen Code's empty prompt would send the
    /// reply it suggests, so no answer ends on a spare one. Nil for an
    /// answer the dialog can't be given by keys.
    static func keys(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission,
                     oneBurst: Bool = false) -> [ClaudeTypingStep]? {
        if asked.tool == ClaudeToolSummary.planTool {
            let choice: Int
            switch decision {
            case .allow, .allowAlways: choice = 2
            case .approvePlan(let mode):
                guard let index = planModes.firstIndex(of: mode) else { return nil }
                choice = index + 2
            // "No, keep planning", the last of four.
            case .deny: choice = 4
            case .answer: return nil
            }
            return oneBurst ? [.keys([.digit(choice)])] : apart(Array(repeating: .down, count: choice - 1) + [nil])
        }
        if asked.inAgentDialog && !asked.asksQuestions {
            // A tool's dialog: "Yes, allow once" is picked first, Escape says no.
            switch decision {
            case .allow: return oneBurst ? [.keys([.digit(1)])] : apart([nil])
            case .deny: return oneBurst ? nil : apart([.escape])
            default: return nil
            }
        }
        guard asked.asksQuestions else { return nil }
        switch decision {
        case .deny: return oneBurst ? nil : apart([.escape])
        case .answer(let answers):
            return oneBurst ? questionDigit(answers, asked.questions) : questionKeys(answers, asked.questions)
        default: return nil
        }
    }

    /// Whether the island can answer the request with keys at all in a
    /// terminal that sends them in one burst (Terminal): a plan, or a single
    /// question with one choice to pick.
    static func keyable(_ asked: ClaudePermission, oneBurst: Bool) -> Bool {
        guard answeredByKeys(asked) else { return false }
        guard oneBurst else { return true }
        if asked.tool == ClaudeToolSummary.planTool || (asked.inAgentDialog && !asked.asksQuestions) { return true }
        return asked.questions.count == 1 && !asked.questions[0].multiSelect && asked.questions[0].options.count <= 9
    }

    /// Keys pressed one by one, a moment apart; nil stands for Return.
    private static func apart(_ keys: [ClaudeKey?]) -> [ClaudeTypingStep] {
        keys.flatMap { key -> [ClaudeTypingStep] in [key.map { .keys([$0]) } ?? .enter, .wait(keyGap)] }
    }

    static let keyGap = 0.06

    /// A single question's choice, as its number.
    private static func questionDigit(_ answers: [String: [String]], _ questions: [ClaudeQuestion]) -> [ClaudeTypingStep]? {
        guard questions.count == 1, !questions[0].multiSelect, answers.count == 1,
              let picked = answers.values.first, picked.count == 1,
              let index = questions[0].options.firstIndex(where: { $0.label == picked[0] }), index < 9 else { return nil }
        return [.keys([.digit(index + 1)])]
    }

    /// Each question's answer as keys, in the dialog's order.
    static func questionKeys(_ answers: [String: [String]], _ questions: [ClaudeQuestion]) -> [ClaudeTypingStep]? {
        guard !questions.isEmpty else { return nil }
        var steps: [ClaudeTypingStep] = []
        for question in questions {
            let picked = (answers[question.question] ?? (questions.count == 1 && answers.count == 1 ? answers.values.first : nil))?
                .filter { !$0.isEmpty } ?? []
            guard !picked.isEmpty else { return nil }
            let labels = question.options.map(\.label)
            if question.multiSelect {
                // Only its choices: a typed answer beside ticks isn't one the keys can give.
                var indices: [Int] = []
                for label in picked {
                    guard let index = labels.firstIndex(of: label) else { return nil }
                    indices.append(index)
                }
                var keys: [ClaudeKey?] = []
                var at = 0
                for index in Set(indices).sorted() {
                    keys += Array(repeating: .down, count: index - at) + [.space]
                    at = index
                }
                steps += apart(keys + [nil])
            } else if let index = labels.firstIndex(of: picked[0]) {
                steps += apart(Array(repeating: .down, count: index) + [nil])
            } else {
                // Typed on "Type something…", which follows the choices.
                steps += apart(Array(repeating: .down, count: labels.count))
                steps += [.paste(picked[0]), .wait(0.1), .enter]
            }
            steps.append(.wait(0.3))
        }
        // Several questions end on a tab that asks to submit them.
        if questions.count > 1 { steps.append(.enter) }
        return steps
    }

    // MARK: Log

    /// The model of the session's last reply, from its chat log
    /// (`projects/<project>/chats/<session>.jsonl`): for a session the
    /// island first hears after its start, which named the model. Only the
    /// log's end is read, and only the model's name is kept.
    static func model(transcript path: String) -> String? {
        guard let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        let start = size > 262_144 ? size - 262_144 : 0
        try? handle.seek(toOffset: start)
        guard let data = try? handle.readToEnd(), let text = String(data: data, encoding: .utf8) else { return nil }
        for line in text.split(separator: "\n").reversed() where line.contains(#""type":"assistant""#) {
            guard let json = (try? JSONSerialization.jsonObject(with: Data(line.utf8))) as? [String: Any],
                  let model = json["model"] as? String, !model.isEmpty else { continue }
            return model
        }
        return nil
    }

    // MARK: Turns

    /// What tells Qwen Code's turns apart from what it does after them.
    ///
    /// Once a turn stops, Qwen Code may run a helper of its own (the one
    /// that keeps its memory) under the same session and prompt, whose tool
    /// hooks would open the turn again: tools of a prompt that already
    /// stopped are dropped. Its `idle_prompt` comes each time the model
    /// stops answering, also just before a tool of the same turn runs, so
    /// it ends the turn only when no tool has started just before it, or
    /// when a dialog was waiting: then the person cancelled it with Escape,
    /// which brings no `Stop`.
    struct Turns {
        private var stopped: [String: String] = [:]
        private var toolStarted: [String: Date] = [:]
        private var asked: Set<String> = []

        static let toolGrace: TimeInterval = 2

        enum Fact: Equatable {
            case prompt
            case tool
            case asked
            case stopped
            case idle
            case other
        }

        /// Whether an event of the session's goes on to the island.
        mutating func keep(_ session: String, turn: String?, fact: Fact, at now: Date) -> Bool {
            switch fact {
            case .prompt:
                toolStarted[session] = nil
                asked.remove(session)
                return true
            case .tool:
                if let turn, stopped[session] == turn { return false }
                // A tool that starts or ends means no dialog waits any more.
                toolStarted[session] = now
                asked.remove(session)
                return true
            case .asked:
                if let turn, stopped[session] == turn { return false }
                asked.insert(session)
                return true
            case .stopped:
                if let turn { stopped[session] = turn }
                toolStarted[session] = nil
                asked.remove(session)
                return true
            case .idle:
                if asked.remove(session) != nil { toolStarted[session] = nil; return true }
                if let started = toolStarted[session], now.timeIntervalSince(started) < Self.toolGrace { return false }
                toolStarted[session] = nil
                return true
            case .other:
                return true
            }
        }

        mutating func forget(_ session: String) {
            stopped[session] = nil
            toolStarted[session] = nil
            asked.remove(session)
        }
    }
}
