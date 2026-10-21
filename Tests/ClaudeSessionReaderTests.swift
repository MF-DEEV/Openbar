// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClaudeSessionReaderTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions registry") { registry(suite) }
        suite.run("sessions transcript") { transcript(suite) }
        suite.run("sessions hosts") { hosts(suite) }
    }

    static func data(_ text: String) -> Data { Data(text.utf8) }

    // MARK: Registry

    static func registry(_ suite: TestSuite) {
        let waiting = data("""
        {"pid":85842,"sessionId":"99c35831","cwd":"/tmp/spike/work","startedAt":1792605026467,\
        "procStart":"Wed Oct 21 17:50:25 2026","version":"2.1.306","kind":"interactive","pidDomain":"darwin",\
        "messagingSocketPath":"/tmp/cc-socks/85842.sock","name":"work-1f","status":"waiting",\
        "waitingFor":"permission prompt","updatedAt":1792605040000,"statusUpdatedAt":1792605039000}
        """)
        let record = ClaudeSessionRegistryReader.record(waiting)
        suite.expect(record?.pid == 85842 && record?.sessionID == "99c35831" && record?.cwd == "/tmp/spike/work"
                     && record?.name == "work-1f", "a registry record gives the process, session, folder and name")
        suite.expect(record?.status == .waiting && record?.waitingFor == "permission prompt",
                     "and whether the session waits, and why")
        suite.expect(record?.statusUpdated == Date(timeIntervalSince1970: 1_792_605_039),
                     "the status change time is read from milliseconds")
        let started = Date(timeIntervalSince1970: 1_792_605_025)
        suite.expect(record?.started == started, "procStart is read as UTC")

        let idle = ClaudeSessionRegistryReader.record(data(#"{"pid":7,"sessionId":"s","cwd":"/a","status":"idle","waitingFor":"x"}"#))
        suite.expect(idle?.waitingFor == nil && idle?.name == "a", "an idle record keeps no reason and is named after its folder")
        let unknown = ClaudeSessionRegistryReader.record(data(#"{"pid":7,"sessionId":"s","cwd":"/a","status":"thinking"}"#))
        suite.expect(unknown != nil && unknown?.status == nil, "an unknown status leaves the transcript to decide")
        for (label, json) in [
            ("partial", #"{"pid":7,"sessionId":"#), ("no session", #"{"pid":7,"cwd":"/a"}"#),
            ("other machine", #"{"pid":7,"sessionId":"s","cwd":"/a","pidDomain":"linux"}"#),
            ("bad pid", #"{"pid":-3,"sessionId":"s","cwd":"/a"}"#),
        ] {
            suite.expect(ClaudeSessionRegistryReader.record(data(json)) == nil, "a \(label) record is skipped")
        }

        let current = record!
        suite.expect(ClaudeSessionRegistryReader.isCurrent(current, started: started.addingTimeInterval(0.6)),
                     "a record matches the process started that second")
        suite.expect(!ClaudeSessionRegistryReader.isCurrent(current, started: started.addingTimeInterval(-3600)),
                     "a record left by a killed session does not match a reused process number")
        suite.expect(!ClaudeSessionRegistryReader.isCurrent(current, started: nil), "nor a process that is gone")

        // Read from a folder, as the service does.
        let folder = FileManager.default.temporaryDirectory.appending(path: "claude-sessions-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        try? waiting.write(to: folder.appending(path: "85842.json"))
        try? data(#"{"pid":90,"sessionId":"gone","cwd":"/b","procStart":"Wed Oct 21 10:00:00 2026"}"#)
            .write(to: folder.appending(path: "90.json"))
        try? data("{}").write(to: folder.appending(path: "85842.abc.key"))
        let read = ClaudeSessionRegistryReader.read([folder]) { pid in pid == 85842 ? started : nil }
        suite.expect(read.map(\.sessionID) == ["99c35831"], "only records of running processes are listed")
        try? data(#"{"pid":85843,"sessionId":"#).write(to: folder.appending(path: "85843.json"))
        let scan = ClaudeSessionRegistryReader.scan([folder]) { pid in pid == 85842 || pid == 85843 ? started : nil }
        suite.expect(scan.records.map(\.pid) == [85842] && scan.unreadable == [85843],
                     "a running session's record caught mid-write is reported, not dropped silently")
        let home = URL(fileURLWithPath: "/Users/me")
        suite.expect(ClaudeSessionRegistryReader.folders(home).map(\.path)
                     == ["/Users/me/.claude/sessions", "/Users/me/.config/claude/sessions"],
                     "both Claude Code homes are read")
    }

    // MARK: Transcript

    static let prompt = #"{"isSidechain":false,"type":"user","message":{"role":"user","content":"Use the Bash tool to run exactly: echo SPIKE_OK > spike.txt"},"timestamp":"2026-10-21T17:50:38.622Z","origin":{"kind":"human"},"cwd":"/tmp/spike/work","sessionId":"s1","gitBranch":"feat/sessions"}"#
    static let toolCall = #"{"isSidechain":false,"message":{"model":"claude-haiku-5-5","id":"msg_1","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo SPIKE_OK > spike.txt","description":"Write"}}],"stop_reason":"tool_use","usage":{"input_tokens":2,"cache_creation_input_tokens":48651,"cache_read_input_tokens":28058,"output_tokens":103}},"requestId":"req_1","type":"assistant","timestamp":"2026-10-21T17:50:44.000Z","sessionId":"s1","cwd":"/tmp/spike/work","gitBranch":"feat/sessions"}"#
    static let toolResult = #"{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"(Bash completed with no output)","is_error":true}]},"timestamp":"2026-10-21T17:50:50.507Z","sessionId":"s1"}"#
    static let reply = #"{"isSidechain":false,"message":{"model":"claude-haiku-5-5","id":"msg_2","type":"message","role":"assistant","content":[{"type":"text","text":"I ran it. SENTINEL-reply"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":76000,"output_tokens":40}},"requestId":"req_2","type":"assistant","timestamp":"2026-10-21T17:50:52.900Z","sessionId":"s1"}"#
    static let title = #"{"type":"ai-title","aiTitle":"SENTINEL-title","sessionId":"s1"}"#
    static let interrupt = #"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"timestamp":"2026-10-21T17:51:00.000Z","sessionId":"s1"}"#
    static let meta = #"{"type":"user","isMeta":true,"message":{"role":"user","content":"<system-reminder>x</system-reminder>"},"timestamp":"2026-10-21T17:51:01.000Z","sessionId":"s1"}"#

    static func parse(_ lines: [String], previews: Bool, path: String = "/x/projects/p/s1.jsonl") -> [ClaudeTranscriptFact] {
        var reader = ClaudeTranscriptReader(policy: ClaudeContentPolicy(previews: previews), path: path)
        return lines.flatMap { reader.parse(data($0), now: Date(timeIntervalSince1970: 0)) }
    }

    static func transcript(_ suite: TestSuite) {
        let date = { (text: String) in AgentLogParser.timestamp(text)! }
        let facts = parse([prompt, toolCall, toolResult, reply, title, interrupt, meta], previews: false)
        suite.expect(facts.contains(.turnBegan(date("2026-10-21T17:50:38.622Z"))), "a typed prompt begins a turn")
        suite.expect(facts.contains(.branch("feat/sessions")), "the branch comes from the transcript")
        suite.expect(facts.contains(.model("claude-haiku-5-5")), "and the model")
        suite.expect(facts.contains(.toolStarted(ClaudeToolUse(id: "toolu_1", name: "Bash", summary: nil,
                                                               started: date("2026-10-21T17:50:44.000Z")))),
                     "a tool call starts that tool, without its command while previews are off")
        suite.expect(facts.contains(.toolFinished(id: "toolu_1", at: date("2026-10-21T17:50:50.507Z"), failed: true)),
                     "its result finishes it and says when it failed")
        suite.expect(facts.contains(.turnEnded(date("2026-10-21T17:50:52.900Z"), completed: true)), "end_turn ends the turn")
        suite.expect(facts.contains(.turnEnded(date("2026-10-21T17:51:00.000Z"), completed: false)),
                     "an interruption ends it unfinished")
        suite.expect(!facts.contains(.turnBegan(date("2026-10-21T17:51:01.000Z"))), "meta lines begin nothing")
        suite.expect(!String(describing: facts).contains("SENTINEL"), "with previews off no text or title is read")
        let usage = facts.compactMap { fact -> AgentTokens? in
            if case .usage(_, let tokens, _) = fact { return tokens }
            return nil
        }
        suite.expect(usage.count == 2 && usage.first?.output == 103, "usage is read for each reply")

        let open = parse([toolCall, reply, title], previews: true)
        suite.expect(open.contains(.text("I ran it. SENTINEL-reply", at: date("2026-10-21T17:50:52.900Z")))
                     && open.contains(.title("SENTINEL-title")), "with previews on replies and titles are read")
        suite.expect(open.contains { if case .toolStarted(let tool) = $0 { return tool.summary == "echo SPIKE_OK > spike.txt" }; return false },
                     "and tool commands")

        let path = "/x/projects/p/s1/subagents/agent-a7.jsonl"
        suite.expect(ClaudeTranscriptReader.subagent(of: path) == "a7", "a subagent file names its subagent")
        let sub = parse([toolCall.replacingOccurrences(of: #""isSidechain":false"#, with: #""isSidechain":true"#),
                         reply.replacingOccurrences(of: #""isSidechain":false"#, with: #""isSidechain":true"#)],
                        previews: true, path: path)
        suite.expect(sub.contains(.subagentActivity(id: "a7", at: date("2026-10-21T17:50:44.000Z")))
                     && sub.contains(.subagentEnded(id: "a7", at: date("2026-10-21T17:50:52.900Z"))),
                     "a subagent's lines tell when it works and ends")
        suite.expect(!sub.contains { if case .toolStarted = $0 { return true }; return false },
                     "its tools stay out of the session's timeline")
        suite.expect(sub.contains { if case .usage = $0 { return true }; return false }, "its usage counts toward the session")

        let meta = data(#"{"agentType":"Explore","description":"Find NotchModule"}"#)
        suite.expect(ClaudeTranscriptReader.subagentInfo(meta, id: "a7", policy: ClaudeContentPolicy())
                     == .subagentInfo(id: "a7", type: "Explore", description: nil), "a subagent's type is read without its task")
        let tasks = ClaudeTranscriptReader.tasks([
            data(#"{"id":"1","subject":"A","activeForm":"Doing A","status":"completed"}"#),
            data(#"{"id":"2","subject":"B","activeForm":"Doing B","status":"in_progress"}"#),
            data(#"{"id":"3","subject":"C","status":"pending"}"#), data(#"{"id":"4","status":"deleted"}"#), data("{"),
        ], policy: ClaudeContentPolicy(previews: true))
        suite.expect(tasks == ClaudeTaskProgress(done: 1, total: 3, current: "Doing B"),
                     "task progress counts finished tasks and names the one under way")
        suite.expect(ClaudeTranscriptReader.tasks([], policy: ClaudeContentPolicy()) == nil, "no tasks shows no progress")
    }

    // MARK: Hosts

    static func hosts(_ suite: TestSuite) {
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys004", port: nil)
                     == .terminal(tty: "/dev/ttys004"), "a session in Terminal is reached by its device")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: nil, port: nil) == .app("com.apple.Terminal"),
                     "without a device only the app can be brought forward")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.google.antigravity-ide", tty: "/dev/ttys009", port: 53142)
                     == .antigravity(port: 53142), "a session in Antigravity keeps its IDE port")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.mitchellh.ghostty", tty: nil, port: nil) == .app("com.mitchellh.ghostty")
                     && ClaudeSessionHosts.classify(bundle: nil, tty: nil, port: nil) == .unknown, "other apps are only opened")

        var bytes: [UInt8] = []
        withUnsafeBytes(of: Int32(2)) { bytes.append(contentsOf: $0) }
        for part in ["/usr/bin/claude", "", "", "claude", "--resume", "TERM_PROGRAM=Apple_Terminal",
                     "__CFBundleIdentifier=com.apple.Terminal", "EQ=a=b", "", "junk"] {
            bytes.append(contentsOf: Array(part.utf8))
            bytes.append(0)
        }
        let environment = ClaudeSessionHosts.parseArguments(bytes)
        suite.expect(environment == ["TERM_PROGRAM": "Apple_Terminal", "__CFBundleIdentifier": "com.apple.Terminal", "EQ": "a=b"],
                     "the environment follows the arguments and ends at an empty entry: \(environment)")
        suite.expect(ClaudeSessionHosts.parseArguments([1, 0]).isEmpty, "a short buffer reads as nothing")
        let own = ClaudeSessionHosts.environment(of: getpid(), keys: ["PATH", "HOME"])
        suite.expect(own["PATH"] == ProcessInfo.processInfo.environment["PATH"] && own.count <= 2,
                     "only the asked-for variables of a live process's environment are kept")
        suite.expect(ClaudeSessionHosts.environment(of: getpid()).keys.allSatisfy(ClaudeSessionHosts.hostKeys.contains),
                     "by default only where the session runs is kept")
        suite.expect(ClaudeSessionRegistryReader.processStart(getpid()) != nil
                     && ClaudeSessionRegistryReader.processStart(-1) == nil, "a running process has a start time")
    }
}
