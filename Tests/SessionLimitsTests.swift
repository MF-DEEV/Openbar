// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Sessions stopped by a plan limit, and the limits shown beside the sessions.
enum SessionLimitsTests {
    static func run(_ suite: TestSuite) {
        suite.run("session limits transcript") { transcript(suite) }
        suite.run("session limits engine") { engine(suite) }
        suite.run("session limits codex") { codex(suite) }
        suite.run("session limits support") { support(suite) }
    }

    static let t0 = Date(timeIntervalSince1970: 1_792_600_000)
    static func at(_ seconds: TimeInterval) -> Date { t0.addingTimeInterval(seconds) }

    /// As Claude Code 2.1.316 wrote it when the five-hour window ran out
    /// (a real record, trimmed).
    static let sessionLimit = #"{"parentUuid":"p","isSidechain":false,"type":"assistant","uuid":"u","timestamp":"2026-10-20T14:55:45.284Z","message":{"id":"m","model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","stop_sequence":"","type":"message","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"You've hit your session limit · resets 5pm (Europe/Warsaw)"}]},"requestId":"req_1","apiError":"usage_limit_reached","apiErrorParams":{"rate_limit_info":{"status":"rejected","resetsAt":1792508400,"rateLimitType":"five_hour","overageStatus":"rejected","isUsingOverage":false}},"quotaLimits":{"status":"rejected","resetsAt":1792508400,"rateLimitType":"five_hour"},"error":"rate_limit","isApiErrorMessage":true,"apiErrorStatus":429,"sessionId":"s1"}"#
    /// A weekly cap, told only through `quotaLimits`.
    static let weeklyLimit = #"{"type":"assistant","timestamp":"2026-10-20T15:00:00.000Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","content":[{"type":"text","text":"You've hit your weekly limit"}]},"error":"rate_limit","isApiErrorMessage":true,"quotaLimits":{"status":"rejected","resetsAt":1792900000,"rateLimitType":"seven_day"},"sessionId":"s1"}"#
    /// A passing rate limit, which Claude Code retries: no plan window.
    static let rateLimited = #"{"type":"assistant","timestamp":"2026-10-20T15:01:00.000Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","content":[{"type":"text","text":"API Error: Rate limit reached"}]},"error":"rate_limit","isApiErrorMessage":true,"sessionId":"s1"}"#
    /// A reply that only talks about limits.
    static let talk = #"{"type":"assistant","timestamp":"2026-10-20T15:02:00.000Z","message":{"model":"claude-opus-5-5","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"usage_limit_reached is what Claude Code writes"}]},"apiError":"usage_limit_reached","sessionId":"s1"}"#

    static func parse(_ lines: [String]) -> [ClaudeTranscriptFact] {
        var reader = ClaudeTranscriptReader(policy: ClaudeContentPolicy(previews: false), path: "/x/projects/p/s1.jsonl")
        return lines.flatMap { reader.parse(Data($0.utf8), now: Date(timeIntervalSince1970: 0)) }
    }

    static func stops(_ facts: [ClaudeTranscriptFact]) -> [ClaudeLimitStop] {
        facts.compactMap { if case .limitReached(let stop) = $0 { return stop }; return nil }
    }

    // MARK: Transcript

    static func transcript(_ suite: TestSuite) {
        let facts = parse([sessionLimit])
        let stop = stops(facts).first
        suite.expect(stop?.at == AgentLogParser.timestamp("2026-10-20T14:55:45.284Z"), "the limit is dated by its record")
        suite.expect(stop?.resetsAt == Date(timeIntervalSince1970: 1_792_508_400), "the reset comes from rate_limit_info")
        suite.expect(stop?.kind == .session, "five_hour is the session window")
        let ended = facts.firstIndex { if case .turnEnded(_, let completed) = $0 { return !completed }; return false }
        let reached = facts.firstIndex { if case .limitReached = $0 { return true }; return false }
        suite.expect(reached != nil && ended != nil && reached! < ended!, "the limit comes before the failed end of its turn")

        let weekly = stops(parse([weeklyLimit])).first
        suite.expect(weekly?.kind == .weekly && weekly?.resetsAt == Date(timeIntervalSince1970: 1_792_900_000),
                     "a rejected seven_day window in quotaLimits is a weekly stop")
        suite.expect(stops(parse([rateLimited])).isEmpty, "a rate limit with no plan window is no stop")
        suite.expect(stops(parse([talk])).isEmpty, "a reply that is no API error is no stop")
        suite.expect(ClaudeLimitStop.kind(claude: "seven_day_opus") == .weekly && ClaudeLimitStop.kind(claude: "overage") == .other
                     && ClaudeLimitStop.kind(claude: nil) == nil, "window kinds follow rateLimitType")
    }

    // MARK: Engine

    static func record(status: ClaudeRegistryStatus, changed: TimeInterval) -> ClaudeSessionRecord {
        ClaudeSessionRecord(pid: 100, sessionID: "s1", cwd: "/Users/me/code/app", name: "app-1", status: status,
                            waitingFor: nil, statusUpdated: at(changed), started: t0, version: "2.1.316")
    }

    static func engine(_ suite: TestSuite) {
        let resets = at(3600)
        let engine = ClaudeSessionEngine()
        engine.ingest(records: [record(status: .busy, changed: 0)], now: at(0))
        engine.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .limitReached(ClaudeLimitStop(at: at(5), resetsAt: resets, kind: .session)),
                                               .turnEnded(at(5), completed: false)], now: at(5))
        suite.expect(engine.snapshot(now: at(6)).sessions.first?.phase == .limited,
                     "a turn the limit ended reads as limited before the registry settles")
        engine.ingest(records: [record(status: .idle, changed: 5.2)], now: at(6))
        let limited = engine.snapshot(now: at(10)).sessions
        suite.expect(limited.first?.phase == .limited, "and after it goes idle")
        suite.expect(limited.first?.limitStop?.resetsAt == resets && limited.first?.limitStop?.kind == .session,
                     "the stop says when it resets")
        suite.expect(ClaudeSessionCounts(limited).limited == 1 && ClaudeSessionCounts(limited).finished == 0
                     && !ClaudeSessionCounts(limited).isEmpty, "it is counted apart from the finished ones")
        suite.expect(engine.snapshot(now: at(3601)).sessions.first?.phase == .idle, "the reset passed, it is idle")

        // Claude Code goes on by itself at the reset, or the person writes.
        let resumed = ClaudeSessionEngine()
        resumed.ingest(records: [record(status: .idle, changed: 5)], now: at(5))
        resumed.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .limitReached(ClaudeLimitStop(at: at(5), resetsAt: resets, kind: .session)),
                                                .turnEnded(at(5), completed: false)], now: at(5))
        resumed.ingest(records: [record(status: .busy, changed: 30)], now: at(30))
        suite.expect(resumed.snapshot(now: at(31)).sessions.first?.phase == .generating, "busy again, it works")
        resumed.ingest(sessionID: "s1", facts: [.turnBegan(at(30)), .turnEnded(at(40), completed: true)], now: at(40))
        resumed.ingest(records: [record(status: .idle, changed: 40)], now: at(40))
        let after = resumed.snapshot(now: at(41)).sessions.first
        suite.expect(after?.phase == .finished && after?.limitStop == nil, "the turn after it finishes as usual")

        // A registry-only blip, like a recap, does not lift the stop.
        let blip = ClaudeSessionEngine()
        blip.ingest(records: [record(status: .idle, changed: 5)], now: at(5))
        blip.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .limitReached(ClaudeLimitStop(at: at(5), resetsAt: resets)),
                                             .turnEnded(at(5), completed: false)], now: at(5))
        blip.ingest(records: [record(status: .busy, changed: 20)], now: at(20))
        blip.ingest(records: [record(status: .idle, changed: 22)], now: at(22))
        suite.expect(blip.snapshot(now: at(23)).sessions.first?.phase == .limited, "a busy moment with nothing done keeps it limited")

        // No reset known: it holds for a session window at most.
        let unknown = ClaudeSessionEngine()
        unknown.ingest(records: [record(status: .idle, changed: 5)], now: at(5))
        unknown.ingest(sessionID: "s1", facts: [.turnBegan(at(1)), .limitReached(ClaudeLimitStop(at: at(5))),
                                                .turnEnded(at(5), completed: false)], now: at(5))
        suite.expect(unknown.snapshot(now: at(60)).sessions.first?.phase == .limited, "a stop with no reset shows")
        suite.expect(unknown.snapshot(now: at(5 + ClaudeLimitStop.unknownResetHold + 1)).sessions.first?.phase == .idle,
                     "and lets go after a session window")

        // An older stop read late never replaces a newer one.
        let late = ClaudeSessionEngine()
        late.ingest(records: [record(status: .idle, changed: 50)], now: at(50))
        late.ingest(sessionID: "s1", facts: [.turnBegan(at(40)), .limitReached(ClaudeLimitStop(at: at(50), resetsAt: at(9000))),
                                             .turnEnded(at(50), completed: false)], now: at(50))
        late.ingest(sessionID: "s1", facts: [.limitReached(ClaudeLimitStop(at: at(10), resetsAt: at(100)))], now: at(51))
        suite.expect(late.snapshot(now: at(200)).sessions.first?.limitStop?.resetsAt == at(9000), "the newest stop holds")
    }

    // MARK: Codex

    static func codex(_ suite: TestSuite) {
        func state() -> CodexSessionState {
            let state = CodexSessionState()
            state.upsertThread(["id": "t1", "cwd": "/Users/vors/app", "createdAt": 100, "updatedAt": 200, "status": ["type": "idle"]])
            return state
        }
        func phase(_ state: CodexSessionState, _ seconds: TimeInterval) -> ClaudeSessionPhase? {
            state.threads["t1"].map { CodexSessionState.phase($0, now: at(seconds)) }
        }
        let active: [String: Any] = ["threadId": "t1", "status": ["type": "active", "activeFlags": []]]
        let idle: [String: Any] = ["threadId": "t1", "status": ["type": "idle"]]
        let failed: [String: Any] = ["threadId": "t1", "turn": [
            "id": "turn1", "items": [], "status": "failed", "completedAt": t0.timeIntervalSince1970 + 20,
            "error": ["message": "You’ve hit your usage limit.", "codexErrorInfo": "usageLimitExceeded"],
        ]]

        let limited = state()
        limited.notify("thread/status/changed", params: active, now: at(10))
        limited.notify("turn/completed", params: failed, now: at(20))
        limited.notify("thread/status/changed", params: idle, now: at(20))
        suite.expect(phase(limited, 21) == .limited, "a turn failed on usageLimitExceeded is limited, not done")
        let session = limited.snapshot(now: at(21), previews: false).sessions.first
        suite.expect(session?.phase == .limited && session?.limitStop?.at == at(20) && session?.limitStop?.resetsAt == nil,
                     "Codex names no reset; the limits reading has it")
        limited.notify("thread/status/changed", params: active, now: at(40))
        suite.expect(phase(limited, 41) == .generating, "a new turn lifts it")

        let other = state()
        other.notify("thread/status/changed", params: active, now: at(10))
        var serverError = failed
        serverError["turn"] = ["id": "turn1", "items": [], "status": "failed", "completedAt": t0.timeIntervalSince1970 + 20,
                               "error": ["message": "boom", "codexErrorInfo": "serverOverloaded"]]
        other.notify("turn/completed", params: serverError, now: at(20))
        other.notify("thread/status/changed", params: idle, now: at(20))
        suite.expect(phase(other, 21) != .limited, "another failure is no limit")

        let read = CodexSessionState()
        read.upsertThread(["id": "t1", "cwd": "/Users/vors/app", "status": ["type": "idle"], "turns": [
            ["id": "turn0", "items": [], "status": "completed", "completedAt": t0.timeIntervalSince1970 + 5],
            ["id": "turn1", "items": [], "status": "failed", "completedAt": t0.timeIntervalSince1970 + 20,
             "error": ["message": "limit", "codexErrorInfo": "usageLimitExceeded"]],
        ]])
        suite.expect(read.threads["t1"].map { CodexSessionState.phase($0, now: at(30)) } == .limited,
                     "a thread read after its last turn hit the limit is limited")
    }

    // MARK: Support

    static func window(_ id: String, _ kind: AgentLimitWindow.Kind, used: Double, minutes: Int?, resets: Date?) -> AgentLimitWindow {
        AgentLimitWindow(id: id, kind: kind, minutes: minutes, scope: nil, usedPercent: used, resetsAt: resets)
    }

    static func support(_ suite: TestSuite) {
        let counts = ClaudeSessionCounts(working: 2, workingCodex: 1, waiting: 1, limited: 2, finished: 1)
        let reading = NotchClaudeSessionsSupport.stripReading(counts).map(\.phase)
        suite.expect(reading == [.waitingPermission, .generating, .generating, .limited, .finished],
                     "limited sessions count after the ones at work: \(reading)")
        suite.expect(NotchClaudeSessionsSupport.rank(.limited) > NotchClaudeSessionsSupport.rank(.generating)
                     && NotchClaudeSessionsSupport.rank(.limited) < NotchClaudeSessionsSupport.rank(.finished),
                     "stopped sessions sort after the working ones and before results")
        let plain = NotchClaudeSessionsSupport.stripReadingWidth(counts, textSize: 12)
        let withLimit = NotchClaudeSessionsSupport.stripReadingWidth(counts, limit: "8%", textSize: 12)
        suite.expect(withLimit > plain + 20, "the limit's ring and share widen the strip")
        suite.expect(NotchClaudeSessionsSupport.stripReadingWidth(ClaudeSessionCounts(), limit: "8%", textSize: 12) > 0,
                     "a limit alone still takes room")
        suite.expect(NotchClaudeSessionsSupport.stripText(counts, limit: "8%").hasSuffix("◔ 8%"),
                     "the measured text has the share, so its change remeasures the strip")

        var snapshot = AgentUsageSnapshot(loaded: true, now: at(0))
        snapshot.limits[.claude] = AgentLimits(provider: .claude, windows: [
            window("claude.weekly", .weekly, used: 97, minutes: 10_080, resets: at(86_400 * 2)),
            window("claude.session", .session, used: 30, minutes: 300, resets: at(3600)),
            AgentLimitWindow(id: "claude.opus", kind: .weekly, minutes: 10_080, scope: "Opus", usedPercent: 99, resetsAt: at(9000)),
        ], observedAt: at(0), source: .account)
        snapshot.limits[.codex] = AgentLimits(provider: .codex, windows: [
            window("codex.primary", .session, used: 100, minutes: 300, resets: at(1800)),
        ], observedAt: at(0), source: .sessionLog)
        snapshot.limits[.opencode] = AgentLimits(provider: .opencode, windows: [
            window("opencode.x", .session, used: 99, minutes: 300, resets: at(60)),
        ], observedAt: at(0), source: .sessionLog)

        let groups = NotchSessionLimitsSupport.groups(snapshot, now: at(1))
        suite.expect(groups.map(\.provider) == [.claude, .codex], "only Claude and Codex, in that order")
        suite.expect(groups.first?.windows.map(\.id) == ["claude.session", "claude.weekly"],
                     "the session window first, a model's own left to the AI page")

        // The strip's most used window, as the AI strip's, counts a model's
        // own allowance too; without one the weekly cap is the binding one.
        var planWide = snapshot
        planWide.limits[.claude]?.windows.removeAll { $0.scope != nil }
        let session = NotchSessionLimitsSupport.stripLimit(planWide, agents: [.claude], focus: .session, threshold: 80, now: at(1))
        suite.expect(session?.window.id == "claude.weekly",
                     "a chosen session window under the threshold gives way to a spent weekly one")
        let model = NotchSessionLimitsSupport.stripLimit(snapshot, agents: [.claude], focus: .mostUsed, threshold: 80, now: at(1))
        suite.expect(model?.window.id == "claude.opus", "a model's own allowance nearly spent counts as the most used")
        let quiet = NotchSessionLimitsSupport.stripLimit(planWide, agents: [.claude], focus: .session, threshold: 99, now: at(1))
        suite.expect(quiet == nil, "nothing past the threshold, nothing on the strip")
        let both = NotchSessionLimitsSupport.stripLimit(snapshot, agents: [.claude, .codex], focus: .mostUsed, threshold: 80, now: at(1))
        suite.expect(both?.provider == .codex, "the most used among the agents with sessions")
        suite.expect(NotchSessionLimitsSupport.stripLimit(snapshot, agents: [.opencode], focus: .mostUsed, threshold: 1, now: at(1)) == nil,
                     "agents Sessions does not follow never show")
        suite.expect(NotchSessionLimitsSupport.stripShare(window("w", .session, used: 92, minutes: 300, resets: nil), display: .remaining,
                                                          locale: Locale(identifier: "en_US")) == "8%", "the share is what is left by default")

        var stopped = ClaudeSession(id: "t1", pid: 0, cwd: "/a", project: "a", name: "a", branch: nil, model: nil, host: .unknown,
                                    phase: .limited, phaseSince: at(0), timeline: [], subagents: [], tokens: AgentTokens(),
                                    cost: 0, hooked: true)
        stopped.provider = .codex
        stopped.limitStop = ClaudeLimitStop(at: at(0))
        suite.expect(NotchSessionLimitsSupport.reset(of: stopped, snapshot: snapshot, now: at(1)) == at(1800),
                     "with no reset of its own, the spent window's reset tells when it goes on")
        stopped.limitStop = ClaudeLimitStop(at: at(0), resetsAt: at(500))
        suite.expect(NotchSessionLimitsSupport.reset(of: stopped, snapshot: snapshot, now: at(1)) == at(500), "its own reset wins")
        suite.expect(NotchSessionLimitsSupport.reset(of: stopped, snapshot: snapshot, now: at(600)) == nil, "a reset gone by tells nothing")

        var working = stopped
        working.phase = .generating
        var waiting = stopped
        waiting.phase = .waitingInput
        var claude = stopped
        claude.provider = .claude
        claude.phase = .runningTool
        suite.expect(NotchSessionLimitsSupport.running([stopped, working, waiting, claude], provider: .codex) == 2,
                     "a warning counts the agent's sessions at work or waiting, not stopped ones or the other agent's")

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let noon = Date(timeIntervalSince1970: 1_792_670_400) // 12:00 UTC
        let english = Locale(identifier: "en_US")
        suite.expect(NotchSessionLimitsSupport.resetText(noon.addingTimeInterval(7200), now: noon, locale: english, calendar: calendar)
                     == { var style = Date.FormatStyle(date: .omitted, time: .shortened).locale(english)
                          style.timeZone = calendar.timeZone
                          return noon.addingTimeInterval(7200).formatted(style) }(), "today, only the time")
        let later = NotchSessionLimitsSupport.resetText(noon.addingTimeInterval(86_400 * 2), now: noon, locale: english, calendar: calendar)
        let weekday = calendar.shortWeekdaySymbols[calendar.component(.weekday, from: noon.addingTimeInterval(86_400 * 2)) - 1]
        suite.expect(later.contains(weekday) && !later.contains("Nov"), "within the week, the weekday: \(later)")
        let far = NotchSessionLimitsSupport.resetText(noon.addingTimeInterval(86_400 * 9), now: noon, locale: english, calendar: calendar)
        suite.expect(far.contains("Nov") || far.contains("Oct"), "past it, the date: \(far)")
        suite.expect(NotchClaudeSessionsSupport.motion(stopped, now: at(1)) == .doze, "a stopped session's Clawd sleeps it off")

        // Whose limits they are: the email in Codex's identity token, nothing else.
        let claims = Data(#"{"email":"me@example.com","sub":"x"}"#.utf8).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        let auth = Data(#"{"auth_mode":"chatgpt","tokens":{"id_token":"h.\#(claims).s","access_token":"secret"}}"#.utf8)
        suite.expect(AgentAccountIdentity.codexEmail(auth) == "me@example.com", "the account's email comes from the id token")
        suite.expect(AgentAccountIdentity.codexEmail(Data(#"{"OPENAI_API_KEY":"sk-x","tokens":null}"#.utf8)) == nil,
                     "an API key has no account email")
        suite.expect(AgentAccountIdentity.codexEmail(Data(#"{"tokens":{"id_token":"nonsense"}}"#.utf8)) == nil,
                     "an unreadable token gives nothing")
    }
}
