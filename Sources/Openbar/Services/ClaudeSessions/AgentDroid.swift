// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Factory's Droid (`droid`, #242). Its hooks take Claude Code's format, in
/// `~/.factory/settings.json` under `hooks` or in `~/.factory/hooks.json`
/// keyed by event (`FACTORY_HOME_OVERRIDE` moves the home the folder sits
/// in). An event `hooks.json` names replaces the same event in
/// `settings.json`, so each of the island's entries goes where its event is
/// read: beside the person's own in `hooks.json` when that file has the
/// event, else in `settings.json`. Nothing the person wrote is shadowed.
///
/// Droid has no permission hook: a call it asks about is told by the tool's
/// `PreToolUse`, then a `permission_prompt` notification naming no tool. Its
/// prompt takes Return for the "Yes, allow" it has picked and, with
/// "No, cancel" last, arrow keys down to that and Return for no; arrows stop
/// at the ends. A question (`AskUser`) shows an `elicitation_dialog` whose
/// choices take their number, and a spec (`ExitSpecMode`) a dialog whose
/// first choice proceeds, then asks the autonomy to go on with. Droid reads
/// keys that come in one burst one by one, so Terminal's `do script`, which
/// adds a return, works too; but Escape with anything after it is read as
/// another key, so Escape is never sent with a return.
enum AgentDroid {
    /// Every event the island hears. Droid has no `PostToolUseFailure`:
    /// `PostToolUse` comes for a failed command too. A turn the person
    /// stops ends with an `idle_prompt` notification and no `Stop`.
    static let events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                         "Notification", "Stop"]
    static let toolEvents: Set<String> = ["PreToolUse", "PostToolUse"]

    static let questionTool = "AskUser"
    static let planTool = "ExitSpecMode"

    /// The autonomy a spec is approved into, in the order of Droid's second
    /// dialog: Manual approvals (1), Low (2, edits), Medium (3). High, which
    /// runs anything, is never picked from the island.
    static let planModes: [ClaudePlanMode] = [.auto, .acceptEdits, .manual]

    // MARK: Places

    /// Droid's folder: `.factory` in `FACTORY_HOME_OVERRIDE`, or in the home folder.
    static func folder(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        let base = environment["FACTORY_HOME_OVERRIDE"].flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) } ?? home
        return base.appending(path: ".factory")
    }

    static func settings(home: URL, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        folder(home: home, environment: environment).appending(path: "settings.json")
    }

    /// The hooks file Droid reads in a folder: `hooks.json`, or the older
    /// `hooks/hooks.json` while there is no `hooks.json`.
    static func hooksFile(in folder: URL, exists: (String) -> Bool = FileManager.default.fileExists(atPath:)) -> URL {
        let file = folder.appending(path: "hooks.json")
        let legacy = folder.appending(path: "hooks/hooks.json")
        return !exists(file.path) && exists(legacy.path) ? legacy : file
    }

    // MARK: Hooks

    /// The hook for one event.
    static func entry(_ event: String, socket: String, bundle: String) -> [String: Any] {
        ["type": "command", "timeout": AgentHookInstaller.eventTimeout,
         "command": AgentHookInstaller.command(.droid, event: event, socket: socket, bundle: bundle)]
    }

    /// Where each event's entry goes: `hooks.json` when it has the event, else `settings.json`.
    static func inHooksFile(_ event: String, hooks: [String: Any]) -> Bool { hooks[event] != nil }

    /// The state of the island's hooks in a Droid folder.
    static func state(folder: URL, socket: String, bundle: String) throws -> AgentHookInstaller.State {
        let settings = try ClaudeHookInstaller.read(folder.appending(path: "settings.json"))
        let hooks = try ClaudeHookInstaller.read(hooksFile(in: folder))
        let fromSettings = settings["hooks"] as? [String: Any] ?? [:]
        let current = events.allSatisfy { event in
            let groups = inHooksFile(event, hooks: hooks) ? hooks[event] : fromSettings[event]
            return holds(groups, entry(event, socket: socket, bundle: bundle))
        }
        if current { return .installed }
        let marker = AgentHookInstaller.marker(bundle: bundle)
        return ClaudeHookInstaller.hasAny(in: settings, marker: marker) || ClaudeHookInstaller.hasAny(in: ["hooks": hooks], marker: marker)
            ? .outdated : .missing
    }

    static func holds(_ groups: Any?, _ entry: [String: Any]) -> Bool {
        (groups as? [[String: Any]] ?? []).contains { group in
            (group["hooks"] as? [[String: Any]] ?? []).contains { NSDictionary(dictionary: $0).isEqual(to: entry) }
        }
    }

    /// Puts the island's entries in, each where its event is read, after
    /// taking any older ones out of both files.
    static func install(folder: URL, socket: String, bundle: String) throws {
        let hooksURL = hooksFile(in: folder)
        // Read both first: an unreadable one is left alone, and so is the other.
        let hooks = try ClaudeHookInstaller.read(hooksURL)
        _ = try ClaudeHookInstaller.read(folder.appending(path: "settings.json"))
        let marker = AgentHookInstaller.marker(bundle: bundle)
        let inHooks = Set(events.filter { inHooksFile($0, hooks: removing(from: hooks, marker: marker)) })
        try ClaudeHookInstaller.update(hooksURL) { file in
            adding(to: removing(from: file, marker: marker), events: events.filter(inHooks.contains), socket: socket, bundle: bundle)
        }
        try ClaudeHookInstaller.update(folder.appending(path: "settings.json")) { settings in
            var settings = settings
            let own = removing(from: settings["hooks"] as? [String: Any] ?? [:], marker: marker)
            let added = adding(to: own, events: events.filter { !inHooks.contains($0) }, socket: socket, bundle: bundle)
            settings["hooks"] = added.isEmpty ? nil : added
            return settings
        }
    }

    /// Takes the island's entries out of both files, a copy of each kept.
    static func remove(folder: URL, bundle: String) throws {
        let marker = AgentHookInstaller.marker(bundle: bundle)
        let hooksURL = hooksFile(in: folder)
        if try ClaudeHookInstaller.contents(hooksURL) != nil {
            try ClaudeHookInstaller.update(hooksURL) { removing(from: $0, marker: marker) }
        }
        let settingsURL = folder.appending(path: "settings.json")
        if try ClaudeHookInstaller.contents(settingsURL) != nil {
            try ClaudeHookInstaller.update(settingsURL) { settings in
                guard let hooks = settings["hooks"] as? [String: Any] else { return settings }
                var settings = settings
                let kept = removing(from: hooks, marker: marker)
                settings["hooks"] = kept.isEmpty ? nil : kept
                return settings
            }
        }
    }

    /// An event map without the hooks whose command carries `marker`. An
    /// event the island's removal empties goes; one the person left empty
    /// stays, since an empty list in `hooks.json` clears the event in
    /// `settings.json`. Keys that aren't events, as `hooksDisabled`, stay.
    static func removing(from hooks: [String: Any], marker: String) -> [String: Any] {
        var hooks = hooks
        for (event, value) in hooks {
            guard let groups = value as? [[String: Any]], !groups.isEmpty else { continue }
            var removed = false
            let kept = groups.compactMap { group -> [String: Any]? in
                guard let entries = group["hooks"] as? [[String: Any]] else { return group }
                let remaining = entries.filter { ($0["command"] as? String)?.contains(marker) != true }
                guard remaining.count != entries.count else { return group }
                removed = true
                if remaining.isEmpty { return nil }
                var group = group
                group["hooks"] = remaining
                return group
            }
            guard removed else { continue }
            hooks[event] = kept.isEmpty ? nil : kept
        }
        return hooks
    }

    /// The island's groups after the person's own; tool events match every tool.
    static func adding(to hooks: [String: Any], events: [String], socket: String, bundle: String) -> [String: Any] {
        var hooks = hooks
        for event in events {
            var group: [String: Any] = ["hooks": [entry(event, socket: socket, bundle: bundle)]]
            if toolEvents.contains(event) { group["matcher"] = "*" }
            hooks[event] = (hooks[event] as? [[String: Any]] ?? []) + [group]
        }
        return hooks
    }

    // MARK: Questions

    /// The questions in an `AskUser` call's questionnaire: numbered
    /// `[question]` lines, `(multi)` at the end of one for several picks,
    /// `[topic]` its short label, `[option]` each choice. Nil when one has
    /// no question or fewer than two choices.
    static func questions(in questionnaire: String) -> [ClaudeQuestion]? {
        var questions: [ClaudeQuestion] = []
        var current: (text: String, multi: Bool, header: String?, options: [ClaudeQuestion.Option])?
        func close() -> Bool {
            guard let open = current else { return true }
            current = nil
            guard !open.text.isEmpty, open.options.count >= 2 else { return false }
            questions.append(ClaudeQuestion(question: open.text, header: open.header, options: open.options,
                                            multiSelect: open.multi))
            return true
        }
        for raw in questionnaire.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if let range = line.range(of: "[question]") {
                guard close() else { return nil }
                var text = line[range.upperBound...].trimmingCharacters(in: .whitespaces)
                let multi = text.lowercased().hasSuffix("(multi)")
                if multi { text = String(text.dropLast("(multi)".count)).trimmingCharacters(in: .whitespaces) }
                current = (text, multi, nil, [])
            } else if line.hasPrefix("[topic]"), current != nil {
                let topic = line.dropFirst("[topic]".count).trimmingCharacters(in: .whitespaces)
                current?.header = topic.isEmpty ? nil : topic
            } else if line.hasPrefix("[option]"), current != nil {
                let label = line.dropFirst("[option]".count).trimmingCharacters(in: .whitespaces)
                if !label.isEmpty { current?.options.append(ClaudeQuestion.Option(label: label)) }
            }
        }
        guard close(), !questions.isEmpty else { return nil }
        return questions
    }

    // MARK: Asks

    /// What Droid's prompt would ask about the tool its `PreToolUse` named:
    /// the command or file change, a spec to approve, or the questions it
    /// asks. Shown like Claude Code's, whatever the previews, since the
    /// person decides on it. Nil for a question the island can't show.
    static func ask(_ tool: String, raw: String, input: [String: Any], cwd: String?, at date: Date) -> ClaudePermission? {
        if raw == questionTool {
            guard let text = input["questionnaire"] as? String, let questions = questions(in: text) else { return nil }
            var permission = ClaudePermission(requestID: UUID().uuidString, tool: ClaudeQuestion.tool,
                                              summary: ClaudeToolSummary.clip(questions[0].question), offersAlways: false, since: date)
            permission.questions = questions
            return permission
        }
        var permission = AgentHookMessage.grokAsk(tool, input: input, cwd: cwd, at: date)
        if raw == planTool { permission.planModes = planModes }
        return permission
    }

    // MARK: Keys

    /// The keys that answer Droid's prompt, nil for an answer it can't be
    /// given by keys. `oneBurst` is a terminal that sends them together with
    /// a return after them (Terminal's `do script`): there nothing ends on
    /// Escape, which the return would turn into another key.
    static func keys(_ decision: ClaudeHookMessage.Decision, _ asked: ClaudePermission,
                     oneBurst: Bool = false) -> [ClaudeTypingStep]? {
        if asked.tool == ClaudeToolSummary.planTool {
            switch decision {
            case .allow, .allowAlways: return planKeys(planModes[0])
            case .approvePlan(let mode): return planModes.contains(mode) ? planKeys(mode) : nil
            // Escape cancels the spec and keeps planning.
            case .deny: return oneBurst ? nil : [.keys([.escape])]
            case .answer: return nil
            }
        }
        if asked.asksQuestions {
            switch decision {
            // Escape stops the agent rather than skipping the question.
            case .deny: return oneBurst ? nil : [.keys([.escape])]
            case .answer(let answers): return questionKeys(answers, asked.questions)
            default: return nil
            }
        }
        switch decision {
        case .allow: return [.enter]
        // "No, cancel" is last; the arrows stop there.
        case .deny: return [.keys(Array(repeating: .down, count: denyDowns)), .enter]
        default: return nil
        }
    }

    /// Enough downs to reach the last of Droid's choices from the first.
    static let denyDowns = 5

    /// "Proceed with the proposal", then the autonomy to go on with.
    private static func planKeys(_ mode: ClaudePlanMode) -> [ClaudeTypingStep]? {
        let digit: Int
        switch mode {
        case .manual: digit = 1
        case .acceptEdits: digit = 2
        case .auto: digit = 3
        case .bypass: return nil
        }
        return [.keys([.digit(1)]), .wait(0.4), .keys([.digit(digit)])]
    }

    /// Each question's answer in the dialog's order: a single choice is its
    /// number, which moves on by itself; a multiple choice ticks each number,
    /// then Tab moves on. Several questions, or a multiple choice, end on a
    /// review whose Return submits them. Only listed choices, nine at most.
    static func questionKeys(_ answers: [String: [String]], _ questions: [ClaudeQuestion]) -> [ClaudeTypingStep]? {
        guard !questions.isEmpty else { return nil }
        var keys: [ClaudeKey] = []
        for question in questions {
            let picked = (answers[question.question] ?? (questions.count == 1 && answers.count == 1 ? answers.values.first : nil))?
                .filter { !$0.isEmpty } ?? []
            guard !picked.isEmpty else { return nil }
            let labels = question.options.map(\.label)
            var numbers: [Int] = []
            for label in picked {
                guard let index = labels.firstIndex(of: label), index < 9 else { return nil }
                numbers.append(index + 1)
            }
            if question.multiSelect {
                keys += Array(Set(numbers)).sorted().map { .digit($0) } + [.tab]
            } else {
                guard numbers.count == 1 else { return nil }
                keys.append(.digit(numbers[0]))
            }
        }
        let review = questions.count > 1 || questions.contains(where: \.multiSelect)
        return [.keys(keys)] + (review ? [.wait(0.3), .enter] : [])
    }

    // MARK: Log

    /// A session's own settings beside its transcript
    /// (`sessions/<folder>/<id>.settings.json`) name its model.
    static func model(transcript path: String, home: URL,
                      environment: [String: String] = ProcessInfo.processInfo.environment) -> String? {
        guard path.hasSuffix(".jsonl") else { return nil }
        let sidecar = String(path.dropLast(".jsonl".count)) + ".settings.json"
        guard let data = small(sidecar), let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let model = json["model"] as? String, !model.isEmpty else { return nil }
        guard model.hasPrefix("custom:") else { return model }
        return customName(model, settings: small(settings(home: home, environment: environment).path)) ?? model
    }

    /// The name a custom model is shown by: Droid calls the `n`th of
    /// `customModels` `custom:<its displayName, dashed>-<n>`.
    static func customName(_ id: String, settings data: Data?) -> String? {
        guard let data, let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let models = json["customModels"] as? [[String: Any]],
              let dash = id.lastIndex(of: "-"), let index = Int(id[id.index(after: dash)...]),
              models.indices.contains(index) else { return nil }
        let model = models[index]
        let name = (model["displayName"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? model["model"] as? String
        return name.flatMap { $0.isEmpty ? nil : $0 }
    }

    /// The text of the reply a `Stop` names by its message id, from the
    /// end of the transcript. Read only with previews on.
    static func reply(transcript path: String, message: String) -> String? {
        guard let handle = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        let start = size > 524_288 ? size - 524_288 : 0
        try? handle.seek(toOffset: start)
        guard let data = try? handle.readToEnd() else { return nil }
        let needle = Data(message.utf8)
        for line in data.split(separator: 0x0A).reversed() where line.range(of: needle) != nil {
            guard let json = (try? JSONSerialization.jsonObject(with: Data(line))) as? [String: Any],
                  json["id"] as? String == message, let body = json["message"] as? [String: Any],
                  body["role"] as? String == "assistant" else { continue }
            let parts = body["content"] as? [[String: Any]] ?? []
            let text = parts.filter { $0["type"] as? String == "text" }.compactMap { $0["text"] as? String }.joined(separator: "\n")
            return AgentHookMessage.preview(text)
        }
        return nil
    }

    private static func small(_ path: String) -> Data? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: path),
              ((attributes[.size] as? NSNumber)?.intValue ?? 0) <= 1_048_576 else { return nil }
        return FileManager.default.contents(atPath: path)
    }
}
