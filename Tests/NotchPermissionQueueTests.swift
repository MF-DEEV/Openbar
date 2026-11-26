// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

/// The queue of every waiting ask (#269): its order, what Allow all takes,
/// and the page opened on an ask behind another.
enum NotchPermissionQueueTests {
    static let start = Date(timeIntervalSince1970: 1_800_000_000)

    static func ask(_ id: String, _ seconds: TimeInterval, tool: String = "Bash", text: String = "ls",
                    risk: PermissionRisk = .safe, always: Bool = false, complete: Bool = true) -> ClaudePermission {
        let content = PermissionContent(full: text, complete: complete, originalLength: text.unicodeScalars.count)
        var p = ClaudePermission(requestID: id, tool: tool, summary: text, detail: content.preview, content: content,
                                 offersAlways: always, since: start.addingTimeInterval(seconds))
        p.risk = risk
        return p
    }

    static func session(_ id: String, _ asks: [ClaudePermission], provider: ClaudeSessionProvider = .claude) -> ClaudeSession {
        ClaudeSession(id: id, pid: 1, cwd: "/w/\(id)", project: id, name: id, branch: nil, model: nil, host: .unknown,
                      phase: asks.isEmpty ? .idle : .waitingPermission, phaseSince: start, turnStarted: nil,
                      waitingFor: nil, currentTool: nil, permission: asks.first, laterAsks: Array(asks.dropFirst()),
                      timeline: [], subagents: [], tasks: nil, tokens: AgentTokens(), cost: 0, hooked: true,
                      provider: provider)
    }

    static func run(_ suite: TestSuite) {
        suite.run("permission queue: order, threshold and Allow all") {
            let one = NotchPermissionQueue([session("a", [ask("a1", 0)]), session("b", [])])
            suite.expect(!one.shows && one.count == 1, "one ask keeps its own page")

            var question = ask("c1", 3, tool: ClaudeQuestion.tool)
            question.questions = [ClaudeQuestion(question: "Which?", header: nil, options: [], multiSelect: false)]
            let sessions = [
                session("a", [ask("a1", 5), ask("a2", 1, text: "rm -rf x", risk: .deletes, always: true)]),
                session("b", [ask("b1", 2, tool: ClaudeToolSummary.planTool, text: "# Plan")]),
                session("c", [question]),
                session("d", [ask("d1", 4, text: "echo hi", complete: false)]),
                session("e", [ask("e1", 6, text: "npm test", risk: .unassessed)]),
                session("f", [ask("f1", 7, text: "pwd")], provider: .codex),
            ]
            let queue = NotchPermissionQueue(sessions)
            suite.expect(queue.shows && queue.count == 7, "every ask of every session is a row")
            suite.expect(queue.rows.map(\.ask.requestID) == ["a2", "b1", "c1", "d1", "a1", "e1", "f1"],
                         "oldest first across sessions: \(queue.rows.map(\.ask.requestID))")
            suite.expect(queue.allowAll.map(\.ask.requestID) == ["a1", "f1"], "only safe asks answered here join Allow all")
            suite.expect(queue.heldBack == 5, "the rest wait one by one")
            let byID = Dictionary(uniqueKeysWithValues: queue.rows.map { ($0.ask.requestID, $0) })
            suite.expect(byID["a2"]?.answersHere == true && byID["a2"]?.joinsAllowAll == false, "a risky ask keeps its own buttons")
            suite.expect(byID["b1"]?.answersHere == false && byID["c1"]?.answersHere == false && byID["d1"]?.answersHere == false,
                         "plans, questions and commands to read whole open their page")
            suite.expect(byID["a2"]?.offersAlways == true && byID["a1"]?.offersAlways == false && byID["f1"]?.offersAlways == true,
                         "Always where the agent offers it, and on Codex")
            suite.expect(queue.keyTarget?.ask.requestID == "a2", "a key answers the oldest ask that is not a question")
            suite.expect(byID["a2"]?.line == "rm -rf x", "the row shows the command")
        }

        suite.run("permission queue: Allow all's list keeps the whole ask (#281)") {
            var edit = ask("e1", 1, tool: "Edit", text: "/repo/a.swift")
            edit.diffs = [ClaudeFileDiff(path: "/repo/a.swift")]
            let s = session("a", [edit], provider: .codex)
            let entry = BatchApproval(batch: "b1", session: s, ask: edit, at: start)
            suite.expect(entry.agent == "codex" && entry.session == "a" && entry.project == "a" && entry.tool == "Edit"
                         && entry.detail == "/repo/a.swift" && entry.paths == nil && entry.at == start.timeIntervalSince1970,
                         "session, agent, tool and path, and no path list that repeats the detail")
            var multi = ask("m1", 1, tool: "MultiEdit", text: "2 files")
            var moved = ClaudeFileDiff(path: "/repo/b.swift")
            moved.movedTo = "/repo/c.swift"
            multi.diffs = [ClaudeFileDiff(path: "/repo/a.swift"), moved]
            suite.expect(BatchApproval(batch: "b", session: s, ask: multi, at: start).paths
                         == ["/repo/a.swift", "/repo/b.swift", "/repo/c.swift"], "every file a change touches, a move's target too")
            let long = String(repeating: "é", count: BatchApproval.detailLimit + 5)
            let cut = BatchApproval(batch: "b", session: s, ask: ask("l1", 1, text: long), at: start).detail
            suite.expect(cut.unicodeScalars.count == BatchApproval.detailLimit + 1 && cut.hasSuffix("…"), "a huge command is cut")
            let full = BatchApproval(batch: "b", session: s, ask: ask("f1", 1, text: "echo one\necho two"), at: start)
            suite.expect(full.detail == "echo one\necho two", "the whole text, not the row's first line")

            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("openbar-batch-\(UUID().uuidString)")
            defer { try? FileManager.default.removeItem(at: dir) }
            let url = dir.appendingPathComponent("sessions/batch-approvals.json")
            let log = BatchApprovalLog(url: url, container: dir)
            for i in 0..<(BatchApprovalLog.cap + 3) {
                log.record(BatchApproval(batch: "b\(i / 2)", session: s, ask: ask("r\(i)", 1, text: "echo \(i)"),
                                         at: start.addingTimeInterval(Double(i))))
            }
            log.flush()
            let kept = BatchApprovalLog.load(url)
            suite.expect(kept.count == BatchApprovalLog.cap && kept.first?.detail == "echo \(BatchApprovalLog.cap + 2)",
                         "newest first, capped at \(BatchApprovalLog.cap): \(kept.count)")
            let mode = (try? FileManager.default.attributesOfItem(atPath: url.path))?[.posixPermissions] as? Int
            suite.expect(mode == 0o600, "owner-only file")
            log.clear()
            log.flush()
            suite.expect(!FileManager.default.fileExists(atPath: url.path) && BatchApprovalLog.load(url).isEmpty, "Clear empties it")
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? Data("{oops".utf8).write(to: url)
            suite.expect(BatchApprovalLog.load(url).isEmpty, "a broken file reads as empty")
        }

        suite.run("permission queue: a new ask joins at the foot") {
            let before = NotchPermissionQueue([session("a", [ask("a1", 1)]), session("b", [ask("b1", 2)])])
            let after = NotchPermissionQueue([session("b", [ask("b1", 2), ask("b2", 9)]), session("a", [ask("a1", 1)])])
            suite.expect(Array(after.rows.prefix(before.count)).map(\.id) == before.rows.map(\.id),
                         "the rows on screen keep their places when the sessions reorder")
            suite.expect(after.rows.last?.ask.requestID == "b2", "and the new ask is last")
        }

        suite.run("permission queue: a page opened on an ask behind another") {
            let s = session("a", [ask("a1", 1), ask("a2", 2), ask("a3", 3)])
            let shown = s.showing("a3")
            suite.expect(shown.permission?.requestID == "a3" && shown.laterAsks.map(\.requestID) == ["a1", "a2"],
                         "the chosen ask is on the card, the others behind it")
            suite.expect(s.showing("gone") == s && s.showing(nil) == s && s.showing("a1") == s,
                         "an answered or first ask leaves the page as it was")
            suite.expect(s.ask("a2")?.requestID == "a2" && s.ask("x") == nil, "asks are found by id")
        }

        suite.run("permission queue: risks are judged once per ask and follow rule changes") {
            var reads = 0
            var rules = PermissionRules(rules: [])
            let cache = PermissionRiskCache { _, _ in reads += 1; return rules }
            let sessions = [session("a", [ask("a1", 1, text: "ls", risk: .unassessed), ask("a2", 2, text: "rm x", risk: .unassessed)])]
            let judged = cache.assess(sessions, now: start)
            suite.expect(judged[0].asks.map(\.risk) == [.safe, .deletes], "each ask gets its risk: \(judged[0].asks.map(\.risk))")
            _ = cache.assess(sessions, now: start.addingTimeInterval(1))
            suite.expect(reads == 1, "rules are read again only after a while")
            rules = PermissionRules(rules: ["Bash(ls)"])
            let later = cache.assess(sessions, now: start.addingTimeInterval(PermissionRiskCache.rulesLifetime + 1))
            suite.expect(reads == 2 && later[0].asks.first?.risk == .rule, "a new rule judges the asks again")
            var remote = session("r", [ask("r1", 1, text: "ls", risk: .unassessed)])
            remote.host = .remote(alias: "box", tmux: false)
            suite.expect(cache.assess([remote], now: start).first?.permission?.risk == .safe && reads == 2,
                         "a host's sessions are judged without reading this Mac's settings")
        }
    }
}
