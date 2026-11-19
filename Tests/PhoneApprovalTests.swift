// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

/// Phone approvals through Telegram (#214): the cases Windows checks too, and
/// what a phone answer may and may not do.
enum PhoneApprovalTests {
    static let casesPath = "windows/src-tauri/src/phone/phone_cases.json"
    static let start = Date(timeIntervalSince1970: 1_800_000_000)

    static func session(_ json: [String: Any], asks: [ClaudePermission] = [], lastText: String? = nil) -> ClaudeSession {
        let provider = ClaudeSessionProvider(rawValue: json["provider"] as? String ?? "claude") ?? .claude
        let project = json["project"] as? String ?? "p"
        var session = ClaudeSession(id: "s-" + project, pid: 1, cwd: "/w/" + project, project: project,
                                    name: json["name"] as? String ?? "", branch: nil, model: nil, host: .unknown,
                                    phase: asks.isEmpty ? .idle : .waitingPermission, phaseSince: start, turnStarted: nil,
                                    waitingFor: nil, currentTool: nil, permission: asks.first, laterAsks: Array(asks.dropFirst()),
                                    timeline: [], subagents: [], tasks: nil, tokens: AgentTokens(), cost: 0, hooked: true,
                                    provider: provider)
        session.lastText = lastText
        return session
    }

    static func ask(_ json: [String: Any]) -> ClaudePermission {
        let repeated = json["repeat"] as? Int ?? 1
        let text = String(repeating: json["detail"] as? String ?? "", count: repeated)
        let content = PermissionContent(full: text, complete: true, originalLength: text.unicodeScalars.count)
        var ask = ClaudePermission(requestID: "r1", tool: json["tool"] as? String ?? "Bash", summary: nil, detail: content.preview,
                                   content: content, offersAlways: json["offersAlways"] as? Bool ?? false, since: start)
        ask.questions = (json["questions"] as? [[String: Any]] ?? []).map { q in
            ClaudeQuestion(question: q["question"] as? String ?? "", header: nil,
                           options: (q["options"] as? [[String: Any]] ?? []).map {
                               ClaudeQuestion.Option(label: $0["label"] as? String ?? "", description: $0["description"] as? String)
                           },
                           multiSelect: q["multiSelect"] as? Bool ?? false)
        }
        ask.planModes = (json["planModes"] as? [String] ?? []).compactMap(ClaudePlanMode.init(rawValue:))
        ask.diffs = (json["diffs"] as? [[String: Any]] ?? []).map { d in
            var diff = ClaudeFileDiff(path: d["path"] as? String ?? "",
                                      change: ClaudeFileDiff.Change(rawValue: d["change"] as? String ?? "") ?? .update)
            diff.lines = (d["lines"] as? [[String]] ?? []).map {
                ClaudeFileDiff.Line(kind: ClaudeFileDiff.Line.Kind(rawValue: $0[0]) ?? .context, text: $0[1])
            }
            return diff
        }
        return ask
    }

    static func outcome(_ json: [String: Any]) -> PhoneOutcome? {
        switch json["outcome"] as? String {
        case "allowed": return .allowed
        case "always": return .always
        case "denied": return .denied
        case "planApproved": return .planApproved(json["mode"] as? String)
        case "keptPlanning": return .keptPlanning
        case "answered": return .answered(json["answers"] as? [String] ?? [])
        case "skipped": return .skipped
        case "answeredElsewhere": return .answeredElsewhere
        case "gone": return .gone
        case "changed": return .changed
        case "failed": return .failed
        default: return nil
        }
    }

    static func run(_ suite: TestSuite) {
        guard let data = FileManager.default.contents(atPath: casesPath),
              let cases = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
            suite.run("phone approvals: shared cases") { suite.expect(false, "phone cases readable at \(casesPath)") }
            return
        }
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!

        suite.run("phone approvals: away, quiet hours, codes, callbacks") {
            for item in cases["away"] as? [[String: Any]] ?? [] {
                var settings = PhoneSettings()
                settings.awayMinutes = item["awayMinutes"] as? Int ?? 5
                settings.sendAlways = item["sendAlways"] as? Bool ?? false
                let got = PhoneAway.sends(settings, locked: item["locked"] as? Bool ?? false,
                                          idle: TimeInterval(item["idle"] as? Int ?? 0))
                suite.expect(got == item["expect"] as? Bool, "away \(item)")
            }
            for item in cases["quiet"] as? [[String: Any]] ?? [] {
                var settings = PhoneSettings()
                settings.quietHours = item["on"] as? Bool ?? false
                settings.quietFrom = item["from"] as? Int ?? 0
                settings.quietTo = item["to"] as? Int ?? 0
                suite.expect(PhoneAway.quiet(settings, minute: item["minute"] as? Int ?? 0) == item["expect"] as? Bool, "quiet \(item)")
            }
            for item in cases["codes"] as? [[String: Any]] ?? [] {
                let got = PhoneLinkCode.code(in: item["text"] as? String ?? "")
                suite.expect(got == item["expect"] as? String, "code in \(item["text"] ?? ""), got \(got ?? "nil")")
            }
            for item in cases["callbacks"] as? [[String: Any]] ?? [] {
                let got = PhoneCallback.parse(item["data"] as? String ?? "")
                suite.expect(got?.run == item["run"] as? String && (got == nil || got?.number == item["number"] as? Int),
                             "callback \(item["data"] ?? "")")
            }
            let code = PhoneLinkCode.make()
            suite.expect(code.count == PhoneLinkCode.length && PhoneLinkCode.code(in: code) == code, "a made code reads back")
            suite.expect(PhoneLinkCode.make() != PhoneLinkCode.make(), "codes differ")
            let run = PhoneCallback.newRun()
            suite.expect(PhoneCallback.parse(PhoneCallback.data(run: run, number: 42))?.number == 42
                         && PhoneCallback.data(run: run, number: 999_999).utf8.count <= 64, "button data fits Telegram's 64 bytes")
        }

        suite.run("phone approvals: escaping and clipping") {
            for item in cases["escape"] as? [[String: Any]] ?? [] {
                suite.expect(PhoneHTML.escape(item["text"] as? String ?? "") == item["expect"] as? String, "escape \(item)")
            }
            for item in cases["clip"] as? [[String: Any]] ?? [] {
                let got = PhoneHTML.clip(item["text"] as? String ?? "", utf16: item["limit"] as? Int ?? 0)
                suite.expect(got.text == item["shown"] as? String && got.left == item["left"] as? Int, "clip \(item)")
            }
        }

        suite.run("phone approvals: messages and buttons") {
            for item in cases["messages"] as? [[String: Any]] ?? [] {
                let name = item["name"] as? String ?? "?"
                let ask = ask(item["ask"] as? [String: Any] ?? [:])
                let sessionJSON = item["session"] as? [String: Any] ?? [:]
                let session = session(sessionJSON, asks: [ask])
                var picked: [Int: Set<Int>] = [:]
                for (key, value) in item["picked"] as? [String: [Int]] ?? [:] { picked[Int(key) ?? 0] = Set(value) }
                let message = PhoneMessages.ask(ask, of: session, name: sessionJSON["name"] as? String ?? "",
                                                hideCode: item["hideCode"] as? Bool ?? false, picked: picked)
                if let text = item["text"] as? String {
                    suite.expect(message.text == text, "\(name): text\n\(message.text)")
                }
                for part in item["contains"] as? [String] ?? [] {
                    suite.expect(message.text.contains(part), "\(name): holds \(part)")
                }
                if let most = item["maxLength"] as? Int {
                    suite.expect(message.text.utf16.count <= most, "\(name): \(message.text.utf16.count) ≤ \(most)")
                }
                if let buttons = item["buttons"] as? [[String]] {
                    let titles = message.buttons.map { $0.map(\.title) }
                    suite.expect(titles == buttons, "\(name): buttons \(titles)")
                }
                if let answers = item["answers"] as? [String: [String]] {
                    suite.expect(PhoneMessages.answers(ask, picked: picked) == answers, "\(name): answers")
                }
            }
        }

        suite.run("phone approvals: outcomes and ends") {
            for item in cases["outcomes"] as? [[String: Any]] ?? [] {
                guard let outcome = outcome(item) else { suite.expect(false, "outcome \(item)"); continue }
                let date = utc.startOfDay(for: start).addingTimeInterval(TimeInterval((item["minute"] as? Int ?? 0) * 60))
                let got = PhoneMessages.outcome(outcome, at: date, calendar: utc)
                suite.expect(got == item["expect"] as? String, "outcome \(item["outcome"] ?? ""): \(got)")
            }
            for item in cases["ended"] as? [[String: Any]] ?? [] {
                let kind = SessionSoundKind(rawValue: item["kind"] as? String ?? "") ?? .finished
                let json = item["session"] as? [String: Any] ?? [:]
                let session = session(json, lastText: item["lastText"] as? String)
                let got = PhoneMessages.ended(kind, of: session, name: json["name"] as? String ?? "",
                                              hideCode: item["hideCode"] as? Bool ?? false)
                suite.expect(got.text == item["expect"] as? String && got.buttons.isEmpty, "ended \(kind): \(got.text)")
            }
        }

        suite.run("phone approvals: the phone answers what its message showed") {
            let gate = PermissionInspectionGate()
            // Too long to read whole on the island without its viewer.
            let long = String(repeating: "a", count: PermissionContent.previewLimit + 10)
            let content = PermissionContent(full: long, complete: true, originalLength: long.unicodeScalars.count)
            let p = ClaudePermission(requestID: "r", tool: "Bash", summary: nil, detail: content.preview, content: content,
                                     offersAlways: false, since: start)
            let revision = content.revision(p)
            suite.expect(!gate.allows("s", p, revision: revision, token: nil), "the island needs the full command opened")
            suite.expect(gate.allows("s", p, revision: revision, token: nil, origin: .phone), "the phone allows what it showed")
            suite.expect(!gate.allows("s", p, revision: "other", token: nil, origin: .phone), "a changed request is never allowed")
            suite.expect(!gate.allows("s", p, revision: nil, token: nil, origin: .phone), "no revision, no allow")
            let partial = PermissionContent(full: "rm", complete: false, originalLength: nil)
            let q = ClaudePermission(requestID: "q", tool: "Bash", summary: nil, detail: partial.preview, content: partial,
                                     offersAlways: false, since: start)
            suite.expect(!gate.allows("s", q, revision: partial.revision(q), token: nil, origin: .phone),
                         "a command Openbar could not read whole is not allowed from the phone either")
            let none = ClaudePermission(requestID: "n", tool: "Bash", summary: nil, offersAlways: false, since: start)
            suite.expect(!gate.allows("s", none, revision: nil, token: nil, origin: .phone), "a request without content is not allowed")
        }

        suite.run("phone approvals: denials say they came from the phone") {
            func message(_ data: Data) -> String? {
                let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
                return ((json?["hookSpecificOutput"] as? [String: Any])?["decision"] as? [String: Any])?["message"] as? String
            }
            suite.expect(message(ClaudeHookMessage.reply(.deny, suggestions: nil, origin: .phone)) == ClaudeHookMessage.phoneDenyMessage,
                         "deny")
            suite.expect(message(ClaudeHookMessage.reply(.deny, suggestions: nil, input: ["questions": []], origin: .phone))
                         == ClaudeHookMessage.phoneSkipMessage, "skip")
            suite.expect(message(ClaudeHookMessage.reply(.deny, suggestions: nil, approved: ["plan": "x"], origin: .phone))
                         == ClaudeHookMessage.phonePlanDenyMessage, "plan")
            suite.expect(message(ClaudeHookMessage.reply(.deny, suggestions: nil)) == ClaudeHookMessage.denyMessage, "island unchanged")
        }

        suite.run("phone approvals: answers on the session page and reply targets") {
            let plan = ClaudePermission(requestID: "p", tool: ClaudeToolSummary.planTool, summary: nil, offersAlways: false, since: start)
            var question = ClaudePermission(requestID: "q", tool: ClaudeQuestion.tool, summary: nil, offersAlways: false, since: start)
            question.questions = [ClaudeQuestion(question: "Q?", header: nil, options: [], multiSelect: false)]
            let bash = ClaudePermission(requestID: "b", tool: "Bash", summary: "ls", offersAlways: true, since: start)
            let verdicts: [(ClaudePermission?, ClaudeHookMessage.Decision, ClaudePhoneAnswer.Verdict)] = [
                (bash, .allow, .allowed), (bash, .allowAlways, .always), (bash, .deny, .denied),
                (plan, .allow, .planApproved), (plan, .approvePlan(.auto), .planApproved), (plan, .deny, .keptPlanning),
                (question, .deny, .skipped), (question, .answer(["Q?": ["x"]]), .answered), (nil, .deny, .denied),
            ]
            for (p, decision, verdict) in verdicts {
                let entry = ClaudePhoneAnswer(p, request: p?.requestID ?? "x", decision: decision, at: start)
                suite.expect(entry.verdict == verdict, "\(p?.tool ?? "none") \(decision) → \(verdict)")
            }
            let log = PhoneAnswerLog()
            log.add(ClaudePhoneAnswer(bash, request: "b", decision: .allow, at: start), to: "s-web")
            let decorated = log.decorate([session(["project": "web"]), session(["project": "api"])])
            suite.expect(decorated[0].phoneAnswers.count == 1 && decorated[1].phoneAnswers.isEmpty, "answers go to their session")
            log.prune(listed: [], now: start.addingTimeInterval(601))
            suite.expect(log.decorate([session(["project": "web"])])[0].phoneAnswers.isEmpty, "a session long gone is forgotten")

            let idle = session(["project": "web"])
            var waiting = session(["project": "api"], asks: [bash])
            waiting.phase = .waitingPermission
            suite.expect(PhoneReplyTarget.session(replyingTo: "s-api", sessions: [idle, waiting]) == "s-api", "a reply goes where it points")
            suite.expect(PhoneReplyTarget.session(replyingTo: "gone", sessions: [idle]) == nil, "never to a session that left")
        }
    }
}
