// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

// Phone approvals through a Telegram bot (#214). What needs no network and no
// window lives here: the settings, when a message may go out, what it says
// and which buttons it carries, the linking code, and the record of what was
// answered from the phone. Windows follows the same cases in
// `windows/src-tauri/src/phone/messages.rs`, both checked against
// `windows/src-tauri/src/phone/phone_cases.json`.

/// What the person set for phone approvals.
struct PhoneSettings: Equatable {
    var enabled = false
    /// Minutes without keyboard or mouse input before messages go out.
    var awayMinutes = 5
    /// Messages go out even while the person sits at the computer.
    var sendAlways = false
    /// Between these minutes of the day messages arrive without a sound.
    var quietHours = false
    var quietFrom = 22 * 60
    var quietTo = 7 * 60
    /// Messages carry only the agent, the project and the kind of request.
    var hideCode = false
    /// A message when a session finishes or stops on an error.
    var tellEnds = true

    static let awayChoices = [1, 2, 5, 10, 15, 30]

    static func load(_ defaults: UserDefaults = .standard) -> PhoneSettings {
        var settings = PhoneSettings()
        settings.enabled = defaults.object(forKey: DefaultsKey.notchSessionsPhoneEnabled) as? Bool ?? false
        settings.awayMinutes = max(1, defaults.object(forKey: DefaultsKey.notchSessionsPhoneAwayMinutes) as? Int ?? 5)
        settings.sendAlways = defaults.object(forKey: DefaultsKey.notchSessionsPhoneSendAlways) as? Bool ?? false
        settings.quietHours = defaults.object(forKey: DefaultsKey.notchSessionsPhoneQuietHours) as? Bool ?? false
        settings.quietFrom = Self.minute(defaults.object(forKey: DefaultsKey.notchSessionsPhoneQuietFrom) as? Int ?? 22 * 60)
        settings.quietTo = Self.minute(defaults.object(forKey: DefaultsKey.notchSessionsPhoneQuietTo) as? Int ?? 7 * 60)
        settings.hideCode = defaults.object(forKey: DefaultsKey.notchSessionsPhoneHideCode) as? Bool ?? false
        settings.tellEnds = defaults.object(forKey: DefaultsKey.notchSessionsPhoneTellEnds) as? Bool ?? true
        return settings
    }

    private static func minute(_ value: Int) -> Int { min(max(value, 0), 24 * 60 - 1) }
}

/// The Telegram chat linked to this computer: only its person, in a private
/// chat with the bot, is listened to.
struct PhoneLink: Codable, Equatable {
    let chat: Int64
    let user: Int64
    /// How the person goes by in Telegram, to show in Settings.
    let name: String

    static func load(_ defaults: UserDefaults = .standard) -> PhoneLink? {
        guard let text = defaults.string(forKey: DefaultsKey.notchSessionsPhoneLink) else { return nil }
        return try? JSONDecoder().decode(PhoneLink.self, from: Data(text.utf8))
    }

    func save(_ defaults: UserDefaults = .standard) {
        guard let data = try? JSONEncoder().encode(self) else { return }
        defaults.set(String(decoding: data, as: UTF8.self), forKey: DefaultsKey.notchSessionsPhoneLink)
    }

    static func clear(_ defaults: UserDefaults = .standard) {
        defaults.removeObject(forKey: DefaultsKey.notchSessionsPhoneLink)
        defaults.removeObject(forKey: DefaultsKey.notchSessionsPhoneBot)
    }
}

/// When a message may go out, and whether it sounds.
enum PhoneAway {
    /// Away: the screen is locked, or nobody touched the keyboard or mouse for
    /// the minutes set; or the person asked for messages all the time.
    static func sends(_ settings: PhoneSettings, locked: Bool, idle: TimeInterval) -> Bool {
        settings.sendAlways || locked || idle >= Double(max(1, settings.awayMinutes)) * 60
    }

    /// Quiet hours, which may run past midnight: the message still goes out,
    /// without a sound.
    static func quiet(_ settings: PhoneSettings, minute: Int) -> Bool {
        guard settings.quietHours, settings.quietFrom != settings.quietTo else { return false }
        if settings.quietFrom < settings.quietTo { return minute >= settings.quietFrom && minute < settings.quietTo }
        return minute >= settings.quietFrom || minute < settings.quietTo
    }

    static func minute(of date: Date, calendar: Calendar = .current) -> Int {
        let parts = calendar.dateComponents([.hour, .minute], from: date)
        return (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
    }
}

/// The one-time code that links a Telegram chat: sent to the bot as is, or by
/// the `t.me/<bot>?start=<code>` link, which sends `/start <code>`.
enum PhoneLinkCode {
    /// Letters and digits that can't be mistaken for one another.
    static let alphabet = Array("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
    static let length = 8
    static let validFor: TimeInterval = 10 * 60
    /// Wrong codes allowed before the code stops working.
    static let tries = 5

    static func make() -> String {
        var generator = SystemRandomNumberGenerator()
        return String((0..<length).map { _ in alphabet.randomElement(using: &generator)! })
    }

    /// The code a message carries, in capitals without spaces or dashes.
    static func code(in text: String) -> String? {
        var body = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if body.lowercased().hasPrefix("/start") {
            body = String(body.dropFirst("/start".count))
            // "/start@bot CODE" in a group; private chats send "/start CODE".
            if body.hasPrefix("@") { body = String(body.drop { !$0.isWhitespace }) }
        }
        let code = body.uppercased().filter { !$0.isWhitespace && $0 != "-" }
        guard code.count == length, code.allSatisfy({ alphabet.contains($0) }) else { return nil }
        return code
    }

    /// The link that opens the bot in Telegram with the code filled in.
    static func link(bot: String, code: String) -> String { "https://t.me/\(bot)?start=\(code)" }
}

/// A button's data in Telegram (at most 64 bytes): which run of Openbar made
/// it and its number in that run, so a tap on a message an earlier run sent
/// can never answer a request of this one.
enum PhoneCallback {
    static func newRun() -> String {
        let letters = Array("abcdefghijkmnopqrstuvwxyz23456789")
        var generator = SystemRandomNumberGenerator()
        return String((0..<6).map { _ in letters.randomElement(using: &generator)! })
    }

    static func data(run: String, number: Int) -> String { "\(run):\(number)" }

    static func parse(_ data: String) -> (run: String, number: Int)? {
        let parts = data.split(separator: ":", omittingEmptySubsequences: false)
        guard parts.count == 2, !parts[0].isEmpty, let number = Int(parts[1]), number >= 0 else { return nil }
        return (String(parts[0]), number)
    }
}

/// What a button does.
enum PhoneAction: Equatable {
    case allow
    /// Claude Code's rules for next time; for Codex, every request of the turn under way.
    case always
    case deny
    /// A plan approved into this mode, or plainly when it offers none.
    case approvePlan(ClaudePlanMode?)
    case keepPlanning
    /// Picks (or for several, toggles) one option of one question.
    case pick(question: Int, option: Int)
    /// Sends the options picked so far.
    case send
    case skip
}

struct PhoneButton: Equatable {
    let title: String
    let action: PhoneAction
}

/// A message to the phone: HTML text Telegram renders, and rows of buttons.
struct PhoneMessage: Equatable {
    var text: String
    var buttons: [[PhoneButton]] = []
}

/// How a request ended, told at the foot of its message.
enum PhoneOutcome: Equatable {
    case allowed
    case always
    case denied
    case planApproved(String?)
    case keptPlanning
    case answered([String])
    case skipped
    case answeredElsewhere
    case gone
    case changed
    case failed
}

/// Escapes text for Telegram's HTML: only &, < and > mean anything there.
enum PhoneHTML {
    static func escape(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    /// The text cut to whole characters within a count of UTF-16 units, the
    /// length Telegram measures, with how many characters (Unicode scalars, as
    /// the island counts them) were left out.
    static func clip(_ text: String, utf16 limit: Int) -> (text: String, left: Int) {
        guard text.utf16.count > limit else { return (text, 0) }
        var used = 0
        var end = text.startIndex
        for index in text.indices {
            let size = text[index].utf16.count
            guard used + size <= limit else { break }
            used += size
            end = text.index(after: index)
        }
        let shown = String(text[..<end])
        return (shown, text.unicodeScalars.count - shown.unicodeScalars.count)
    }
}

/// What the messages say.
enum PhoneMessages {
    /// Telegram takes 4096 characters; the rest is room for the outcome line.
    static let limit = 3600
    /// The most diff lines a message shows.
    static let diffLines = 40
    /// The most of a plan, or of a session's last words, a message shows.
    static let proseLimit = 1500

    static func header(_ session: ClaudeSession, name: String) -> String {
        var line = "<b>\(PhoneHTML.escape(session.provider.displayName))</b> · \(PhoneHTML.escape(session.project))"
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty, name != session.project { line += "\n<i>\(PhoneHTML.escape(name))</i>" }
        return line
    }

    static func isPlan(_ ask: ClaudePermission) -> Bool { ask.tool == ClaudeToolSummary.planTool }

    /// What the request is, in a line that holds no code.
    static func kind(_ ask: ClaudePermission) -> String {
        if ask.asksQuestions { return ask.questions.count == 1 ? "Asks you a question." : "Asks you \(ask.questions.count) questions." }
        if isPlan(ask) { return "Has a plan for you to review." }
        if !ask.diffs.isEmpty {
            return ask.diffs.count == 1 ? "Wants to change a file." : "Wants to change \(ask.diffs.count) files."
        }
        switch ask.tool {
        case "Bash", "Command", "PowerShell": return "Wants to run a command."
        default: return "Wants to use \(ask.tool)."
        }
    }

    /// Whether one tap on an option answers: one question, one choice.
    static func answersOnTap(_ ask: ClaudePermission) -> Bool {
        ask.questions.count == 1 && !ask.questions[0].multiSelect
    }

    /// The message for one request. `picked` holds the options toggled so
    /// far for a question answered in more than one tap.
    static func ask(_ ask: ClaudePermission, of session: ClaudeSession, name: String, hideCode: Bool,
                    picked: [Int: Set<Int>] = [:]) -> PhoneMessage {
        var parts = [header(session, name: name), kind(ask)]
        if hideCode {
            parts.append(ask.asksQuestions ? "<i>Don’t send code is on, so answer this one on the computer.</i>"
                         : "<i>Hidden because Don’t send code is on.</i>")
        } else {
            let used = parts.joined(separator: "\n").utf16.count + 2
            parts.append(body(ask, room: max(200, limit - used)))
        }
        return PhoneMessage(text: parts.filter { !$0.isEmpty }.joined(separator: "\n\n"),
                            buttons: buttons(ask, of: session, hideCode: hideCode, picked: picked))
    }

    /// The request itself, cut to the room left.
    static func body(_ ask: ClaudePermission, room: Int) -> String {
        if ask.asksQuestions { return questions(ask.questions, room: room) }
        if isPlan(ask) {
            let (text, left) = PhoneHTML.clip(ask.detail.trimmingCharacters(in: .whitespacesAndNewlines), utf16: min(room, proseLimit))
            return PhoneHTML.escape(text) + (left > 0 ? "\n" + more(left, what: "plan") : "")
        }
        if !ask.diffs.isEmpty { return diff(ask.diffs, room: room) }
        let detail = ask.detail.trimmingCharacters(in: .newlines)
        guard !detail.isEmpty else { return "" }
        // The island's preview already marks what it left out; the phone
        // counts both.
        let (text, cut) = PhoneHTML.clip(detail, utf16: room - 80)
        let left = cut + (cut > 0 ? (ask.content?.remaining ?? 0) : 0)
        return "<pre>\(PhoneHTML.escape(text))</pre>" + (left > 0 ? "\n" + more(left, what: "request") : "")
    }

    static func more(_ left: Int, what: String) -> String {
        "<i>… \(left) more character\(left == 1 ? "" : "s"). The full \(what) is on the computer.</i>"
    }

    static func questions(_ questions: [ClaudeQuestion], room: Int) -> String {
        var lines: [String] = []
        for (index, question) in questions.enumerated() {
            let number = questions.count > 1 ? "\(index + 1). " : ""
            lines.append("<b>\(number)\(PhoneHTML.escape(question.question))</b>")
            for option in question.options {
                var line = "• \(PhoneHTML.escape(option.label))"
                if let description = option.description { line += " — \(PhoneHTML.escape(description))" }
                lines.append(line)
            }
            if question.multiSelect { lines.append("<i>Pick any, then Send.</i>") }
        }
        lines.append("<i>Or reply to this message with your own answer.</i>")
        let text = lines.joined(separator: "\n")
        guard text.utf16.count > room else { return text }
        // Escaped text can't be cut in the middle of an entity or a tag, so the
        // plain questions are shown instead.
        let plain = questions.map(\.question).joined(separator: "\n")
        let (cut, left) = PhoneHTML.clip(plain, utf16: room - 80)
        return PhoneHTML.escape(cut) + (left > 0 ? "\n" + more(left, what: "question") : "")
    }

    static func diff(_ diffs: [ClaudeFileDiff], room: Int) -> String {
        var lines: [String] = []
        var shown = 0
        var hidden = 0
        for file in diffs {
            lines.append("\(file.change == .delete ? "deleted" : file.change == .add ? "new" : "edit") \(file.path)")
            for line in file.lines {
                guard shown < diffLines else { hidden += 1; continue }
                shown += 1
                switch line.kind {
                case .added: lines.append("+ " + line.text)
                case .removed: lines.append("- " + line.text)
                case .context: lines.append("  " + line.text)
                case .gap: lines.append("  ⋯")
                }
            }
        }
        let (text, left) = PhoneHTML.clip(lines.joined(separator: "\n"), utf16: room - 80)
        let note = hidden > 0 || left > 0 ? "\n<i>The rest of the change is on the computer.</i>" : ""
        return "<pre>\(PhoneHTML.escape(text))</pre>" + note
    }

    static func planLabel(_ mode: ClaudePlanMode) -> String {
        switch mode {
        case .auto: return "Auto mode"
        case .acceptEdits: return "Accept edits"
        case .bypass: return "Bypass"
        case .manual: return "Manual"
        }
    }

    /// The buttons the island offers for the same request.
    static func buttons(_ ask: ClaudePermission, of session: ClaudeSession, hideCode: Bool,
                        picked: [Int: Set<Int>] = [:]) -> [[PhoneButton]] {
        if ask.asksQuestions {
            let skip = PhoneButton(title: "Skip", action: .skip)
            guard !hideCode else { return [[skip]] }
            var rows: [[PhoneButton]] = []
            let several = ask.questions.count > 1
            for (q, question) in ask.questions.enumerated() {
                for (o, option) in question.options.enumerated() {
                    let mark = picked[q]?.contains(o) == true ? "✓ " : ""
                    rows.append([PhoneButton(title: mark + (several ? "\(q + 1). " : "") + option.label,
                                             action: .pick(question: q, option: o))])
                }
            }
            if answersOnTap(ask) { return rows + [[skip]] }
            let complete = ask.questions.indices.allSatisfy { picked[$0]?.isEmpty == false }
            return rows + [complete ? [skip, PhoneButton(title: "Send", action: .send)] : [skip]]
        }
        if isPlan(ask) {
            var rows = [[PhoneButton(title: "Keep planning", action: .keepPlanning)]]
            if let first = ask.planModes.first {
                rows.append([PhoneButton(title: "Approve · " + planLabel(first), action: .approvePlan(first))])
                let others = ask.planModes.dropFirst().map { PhoneButton(title: planLabel($0), action: .approvePlan($0)) }
                if !others.isEmpty { rows.append(Array(others)) }
            } else {
                rows[0].append(PhoneButton(title: "Approve", action: .approvePlan(nil)))
            }
            return rows
        }
        var row = [PhoneButton(title: "Deny", action: .deny)]
        if session.codexServer || ask.offersAlways { row.append(PhoneButton(title: "Always", action: .always)) }
        row.append(PhoneButton(title: "Allow", action: .allow))
        return [row]
    }

    /// The answers picked and typed, keyed by question text, as the island sends them.
    static func answers(_ ask: ClaudePermission, picked: [Int: Set<Int>], typed: [Int: String] = [:]) -> [String: [String]]? {
        var byLabel: [Int: Set<String>] = [:]
        for (q, options) in picked where ask.questions.indices.contains(q) {
            let question = ask.questions[q]
            byLabel[q] = Set(options.compactMap { question.options.indices.contains($0) ? question.options[$0].label : nil })
        }
        return NotchClaudeSessionsSupport.answers(ask.questions, picked: byLabel, typed: typed)
    }

    /// The line a request's message ends with once it is settled.
    static func outcome(_ outcome: PhoneOutcome, at date: Date, calendar: Calendar = .current) -> String {
        let minute = PhoneAway.minute(of: date, calendar: calendar)
        let time = String(format: "%02d:%02d", minute / 60, minute % 60)
        let text: String
        switch outcome {
        case .allowed: text = "<b>Allowed from your phone</b>"
        case .always: text = "<b>Always allowed from your phone</b>"
        case .denied: text = "<b>Denied from your phone</b>"
        case .planApproved(let mode): text = "<b>Plan approved from your phone</b>" + (mode.map { " · " + PhoneHTML.escape($0) } ?? "")
        case .keptPlanning: text = "<b>Kept planning, from your phone</b>"
        case .answered(let answers):
            let joined = answers.joined(separator: ", ")
            text = "<b>Answered from your phone</b>" + (joined.isEmpty ? "" : ": " + PhoneHTML.escape(PhoneHTML.clip(joined, utf16: 200).text))
        case .skipped: text = "<b>Skipped from your phone</b>"
        case .answeredElsewhere: text = "<i>Answered on the computer.</i>"
        case .gone: text = "<i>No longer waiting.</i>"
        case .changed: text = "<i>The request changed. Look at it on the computer.</i>"
        case .failed: text = "<i>That didn’t reach the session. Look at it on the computer.</i>"
        }
        return text + " · " + time
    }

    /// A session finished its turn, stopped on an error or hit a usage limit.
    static func ended(_ kind: SessionSoundKind, of session: ClaudeSession, name: String, hideCode: Bool) -> PhoneMessage {
        let what: String
        switch kind {
        case .error: what = "Stopped on an error."
        case .limit: what = "Stopped at its usage limit."
        default: what = "Finished."
        }
        var parts = [header(session, name: name), what]
        if !hideCode, kind == .finished, let last = session.lastText?.trimmingCharacters(in: .whitespacesAndNewlines), !last.isEmpty {
            let (text, left) = PhoneHTML.clip(last, utf16: proseLimit)
            parts.append("<blockquote>\(PhoneHTML.escape(text))\(left > 0 ? "…" : "")</blockquote>")
        }
        parts.append("<i>Reply to this message to send it your next prompt.</i>")
        return PhoneMessage(text: parts.joined(separator: "\n\n"))
    }

    static func linked(computer: String) -> String {
        "<b>Linked to Openbar on \(PhoneHTML.escape(computer)).</b>\n\nWhile you’re away from the computer, sessions that need you show up here. Tap a button to answer, or reply to a message to write to its session."
    }

    static let test = "<b>Test message from Openbar.</b>\n\nPhone approvals are working, and requests will show up here while you’re away from the computer."
    static let wrongCode = "That code doesn’t match. Check the code Openbar shows in Settings."
    static let earlierRun = "This button is from before Openbar restarted. Look at the request on the computer."
    static let settled = "Already settled."
    static let whichSession = "Reply to a session’s message so Openbar knows where this goes."
    static let noSession = "No session can take a reply right now."

    static func sent(to name: String) -> String { "Sent to \(PhoneHTML.escape(name))." }

    static func replyFailed(_ failure: ClaudeReplyFailure, name: String) -> String {
        switch failure {
        case .waiting: return "\(PhoneHTML.escape(name)) is waiting on a request. Answer that first."
        case .unreachable: return "Couldn’t reach \(PhoneHTML.escape(name)). Look at it on the computer."
        }
    }
}

/// Where a typed message goes: the session whose message it replies to, else
/// the only session that can take a reply.
enum PhoneReplyTarget {
    static func session(replyingTo session: String?, sessions: [ClaudeSession]) -> String? {
        if let session { return sessions.contains { $0.id == session } ? session : nil }
        let open = sessions.filter { $0.id != ClaudeSessionSample.id && NotchClaudeSessionsSupport.canReply($0) }
        return open.count == 1 ? open[0].id : nil
    }
}

/// A request answered from the phone, listed on its session's page.
struct ClaudePhoneAnswer: Equatable, Identifiable {
    enum Verdict: Equatable {
        case allowed, always, denied, planApproved, keptPlanning, answered, skipped

        /// The request did not go ahead as asked.
        var refused: Bool { self == .denied || self == .keptPlanning || self == .skipped }
    }

    let id: String
    let tool: String
    let summary: String?
    let at: Date
    let verdict: Verdict

    init(id: String, tool: String, summary: String?, at: Date, verdict: Verdict) {
        self.id = id
        self.tool = tool
        self.summary = summary
        self.at = at
        self.verdict = verdict
    }

    init(_ permission: ClaudePermission?, request: String, decision: ClaudeHookMessage.Decision, at: Date) {
        let plan = permission?.tool == ClaudeToolSummary.planTool
        let question = permission?.asksQuestions == true
        let verdict: Verdict
        switch decision {
        case .allow: verdict = plan ? .planApproved : .allowed
        case .allowAlways: verdict = .always
        case .approvePlan: verdict = .planApproved
        case .deny: verdict = question ? .skipped : plan ? .keptPlanning : .denied
        case .answer: verdict = .answered
        }
        self.init(id: request, tool: permission?.tool ?? "Request", summary: permission?.summary, at: at, verdict: verdict)
    }
}

/// What was answered from the phone, by session, newest last; written from
/// the hook queue and Codex's connections alike, like `SessionAutoMode.Log`.
final class PhoneAnswerLog: @unchecked Sendable {
    static let shared = PhoneAnswerLog()
    static let kept = 30
    private let lock = NSLock()
    private var entries: [String: [ClaudePhoneAnswer]] = [:]

    func add(_ entry: ClaudePhoneAnswer, to session: String) {
        lock.withLock {
            var list = entries[session] ?? []
            list.append(entry)
            if list.count > Self.kept { list.removeFirst(list.count - Self.kept) }
            entries[session] = list
        }
    }

    func decorate(_ sessions: [ClaudeSession]) -> [ClaudeSession] {
        lock.withLock {
            sessions.map { session in
                guard let list = entries[session.id], list != session.phoneAnswers else { return session }
                var session = session
                session.phoneAnswers = list
                return session
            }
        }
    }

    func prune(listed ids: Set<String>, now: Date) {
        lock.withLock {
            entries = entries.filter { id, list in
                ids.contains(id) || (list.last.map { now.timeIntervalSince($0.at) < 600 } ?? false)
            }
        }
    }

    func clear() { lock.withLock { entries = [:] } }
}
