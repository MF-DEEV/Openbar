// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation
import ImageIO

enum ClaudeSessionReaderTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions registry") { registry(suite) }
        suite.run("sessions config folders") { configHomes(suite) }
        suite.run("sessions transcript") { transcript(suite) }
        suite.run("sessions prompt images") { promptImages(suite) }
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
        let renamed = ClaudeSessionRegistryReader.record(data(#"{"pid":7,"sessionId":"s","cwd":"/a","name":"bombs","nameSource":"user"}"#))
        suite.expect(renamed?.name == "bombs" && renamed?.nameSource == "user" && idle?.nameSource == nil,
                     "where a session's name came from is read")
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

    // MARK: CLAUDE_CONFIG_DIR

    static func configHomes(_ suite: TestSuite) {
        let files = FileManager.default
        let home = files.temporaryDirectory.appending(path: "vs-config-homes-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? files.removeItem(at: home) }
        for folder in [".claude/sessions", ".claude-2/sessions", ".claude-old", ".config/claude/sessions", "elsewhere/state"] {
            try? files.createDirectory(at: home.appending(path: folder), withIntermediateDirectories: true)
        }
        try? data("{}").write(to: home.appending(path: ".claude.json"))
        try? files.createSymbolicLink(at: home.appending(path: ".claude-link"), withDestinationURL: home.appending(path: ".claude"))
        let started = Date(timeIntervalSince1970: 1_792_605_025)
        try? data(#"{"pid":4242,"sessionId":"second-account","cwd":"/a","procStart":"Wed Oct 21 17:50:25 2026"}"#)
            .write(to: home.appending(path: ".claude-2/sessions/4242.json"))

        let homes = ClaudeSessionRegistryReader.homes(home).map(\.lastPathComponent)
        suite.expect(homes == [".claude", "claude", ".claude-2"],
                     "a .claude… folder beside ~/.claude holding a session registry is read after the standard two")
        suite.expect(!homes.contains(".claude-old") && !homes.contains(".claude.json"),
                     "folders without a registry and files are not")
        suite.expect(!homes.contains(".claude-link"), "a link to a folder already read is not read twice")
        let read = ClaudeSessionRegistryReader.read(ClaudeSessionRegistryReader.folders(home)) { $0 == 4242 ? started : nil }
        suite.expect(read.map(\.sessionID) == ["second-account"],
                     "a session started with CLAUDE_CONFIG_DIR=~/.claude-2 is listed")

        let named = ClaudeConfigHomes.folders(named: [home.appending(path: "elsewhere/state").path, "~/elsewhere",
                                                      " relative/claude", "/no/such/folder", ""], home: home)
        suite.expect(named.map(\.path) == [home.appending(path: "elsewhere/state").path, home.path + "/elsewhere"],
                     "the variable may name any folder, absolute or under ~/, that exists")
        let merged = ClaudeConfigHomes.merge([home.appending(path: ".claude")],
                                             [home.appending(path: ".claude-link"), home.appending(path: ".claude/"),
                                              home.appending(path: ".claude-2")])
        suite.expect(merged.map(\.lastPathComponent) == [".claude", ".claude-2"],
                     "a folder named twice, or through a link, is read once")
        suite.expect(ClaudeSessionRegistryReader.folders(URL(fileURLWithPath: "/Users/nobody-here")).count == 2,
                     "a home with no extra folders reads the standard two")
    }

    // MARK: Prompt images

    /// A real PNG of the given size, as a paste would carry.
    static func png(width: Int, height: Int) -> Data {
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.setFillColor(CGColor(red: 0.2, green: 0.6, blue: 0.9, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        let output = NSMutableData()
        let destination = CGImageDestinationCreateWithData(output, "public.png" as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, context.makeImage()!, nil)
        CGImageDestinationFinalize(destination)
        return output as Data
    }

    static func imageBlock(_ data: String, type: String = "base64", media: String = "image/png") -> String {
        #"{"type":"image","source":{"type":"\#(type)","media_type":"\#(media)","data":"\#(data)"}}"#
    }

    static func userLine(_ blocks: [String], at time: String = "2026-10-21T18:00:00.000Z", extra: String = "") -> String {
        #"{"type":"user","message":{"role":"user","content":[\#(blocks.joined(separator: ","))]},"timestamp":"\#(time)"\#(extra)}"#
    }

    static func promptImages(_ suite: TestSuite) {
        let at = AgentLogParser.timestamp("2026-10-21T18:00:00.000Z")!
        let wide = png(width: 2000, height: 1000).base64EncodedString()
        let small = png(width: 40, height: 30).base64EncodedString()
        func prompts(_ facts: [ClaudeTranscriptFact]) -> [(text: String, images: [ClaudeChatImage])] {
            facts.compactMap { if case .prompt(let text, _, let images) = $0 { return (text, images) }; return nil }
        }

        let alone = userLine([imageBlock(wide)])
        let read = parse([alone], previews: true)
        let shown = prompts(read)
        suite.expect(read.contains(.turnBegan(at)) && shown.count == 1 && shown.first?.text == "" && shown.first?.images.count == 1,
                     "a prompt of one image alone begins a turn and joins the conversation without text")
        if case .preview(let data)? = shown.first?.images.first?.source,
           let source = CGImageSourceCreateWithData(data as CFData, nil),
           let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any] {
            let width = properties[kCGImagePropertyPixelWidth] as? Int, height = properties[kCGImagePropertyPixelHeight] as? Int
            suite.expect(width == ClaudeChatImage.previewSide && height == ClaudeChatImage.previewSide / 2,
                         "the image is kept as a small preview of its own shape: \(width ?? 0)x\(height ?? 0)")
            suite.expect(data.count < wide.utf8.count, "smaller than what the transcript holds")
        } else {
            suite.expect(false, "the image is kept as a preview")
        }
        let hidden = parse([alone], previews: false)
        suite.expect(hidden.contains(.turnBegan(at)) && prompts(hidden).isEmpty,
                     "with previews off the image still begins its turn but is never read")

        let both = prompts(parse([userLine([#"{"type":"text","text":"[Image #1] what broke?"}"#, imageBlock(small)])],
                                 previews: true))
        suite.expect(both.first?.text == "[Image #1] what broke?" && both.first?.images.count == 1,
                     "a prompt keeps its words and its image together")

        let unread = [
            imageBlock("https://example.com/a.png", type: "url"),
            #"{"type":"image","source":{"type":"file","file_id":"f1"}}"#,
            imageBlock(small, media: "text/plain"),
            imageBlock("bm90IGFuIGltYWdl"),
            #"{"type":"image"}"#,
        ]
        let skipped = parse([userLine(unread)], previews: true)
        suite.expect(prompts(skipped).isEmpty && skipped.contains(.turnBegan(at)),
                     "addresses, files, other kinds and bytes that are no image are never read")
        let mixed = prompts(parse([userLine(unread + [imageBlock(small)])], previews: true))
        suite.expect(mixed.first?.images.count == 1, "beside them a real image still shows")

        let many = prompts(parse([userLine(Array(repeating: imageBlock(small), count: 20))], previews: true))
        suite.expect(many.first?.images.count == ClaudeChatImage.perPrompt, "a prompt shows at most sixteen images")

        // A real image padded past the limit is left out undecoded.
        var padded = png(width: 40, height: 30)
        padded.append(Data(count: ClaudeChatImage.largestInline / 4 * 3 + 3))
        suite.expect(ClaudeChatImage.inline(base64: padded.base64EncodedString(), mediaType: "image/png") == nil
                     && ClaudeChatImage.inline(base64: small, mediaType: "image/png") != nil,
                     "inline data past its limit is left out")
        suite.expect(ClaudeChatImage.inline(dataURL: "data:image/png;base64,\(small)") != nil
                     && ClaudeChatImage.inline(dataURL: "https://example.com/a.png") == nil
                     && ClaudeChatImage.inline(dataURL: "data:image/png,\(small)") == nil,
                     "only base64 data addresses are read, never a web one")

        let queued = #"{"type":"attachment","attachment":{"type":"queued_command","prompt":[{"type":"text","text":"Also this"},\#(imageBlock(small))]},"timestamp":"2026-10-21T18:01:00.000Z"}"#
        let joined = prompts(parse([queued], previews: true))
        suite.expect(joined.first?.text == "Also this" && joined.first?.images.count == 1,
                     "a prompt queued while Claude works brings its image too")
        let notice = userLine([#"{"type":"text","text":"<task-notification>done</task-notification>"}"#, imageBlock(small)],
                              extra: #","origin":{"kind":"task-notification"}"#)
        suite.expect(prompts(parse([notice], previews: true)).isEmpty, "a task notification is no prompt, with images or not")
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
        let key = ClaudeToolSummary.inputKey(name: "Bash", input: ["command": "echo SPIKE_OK > spike.txt", "description": "Write"])
        suite.expect(facts.contains(.toolStarted(ClaudeToolUse(id: "toolu_1", name: "Bash", summary: nil,
                                                               started: date("2026-10-21T17:50:44.000Z"), inputKey: key))),
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
        suite.expect(open.contains(.text("I ran it. SENTINEL-reply", at: date("2026-10-21T17:50:52.900Z"), endsTurn: true))
                     && open.contains(.title("SENTINEL-title")), "with previews on replies and titles are read")
        let onTheWay = #"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Checking the build."}],"stop_reason":"tool_use"},"timestamp":"2026-10-21T17:50:43.000Z"}"#
        let twoTexts = #"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First."},{"type":"text","text":"Last."}],"stop_reason":"end_turn"},"timestamp":"2026-10-21T17:50:53.000Z"}"#
        let failed = #"{"type":"assistant","isApiErrorMessage":true,"message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"API Error: 529"}],"stop_reason":"stop_sequence"},"timestamp":"2026-10-21T17:50:54.000Z"}"#
        let refused = #"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"Now the calibration."}],"stop_reason":"refusal"},"timestamp":"2026-10-21T17:50:55.000Z"}"#
        let cut = #"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"The list goes"}],"stop_reason":"max_tokens"},"timestamp":"2026-10-21T17:50:56.000Z"}"#
        let ends = parse([onTheWay, twoTexts, failed, refused, cut], previews: true).compactMap { fact -> String? in
            if case .text(let text, _, let endsTurn) = fact { return "\(text)\(endsTurn ? "!" : "")" }
            return nil
        }
        suite.expect(ends == ["Checking the build.", "First.", "Last.!", "API Error: 529", "Now the calibration.", "The list goes"],
                     "only the last text of a turn that ended as it should ends it, not a refusal or a cut-off: \(ends)")
        suite.expect(open.contains { if case .toolStarted(let tool) = $0 { return tool.summary == "echo SPIKE_OK > spike.txt" }; return false },
                     "and tool commands")

        suite.expect(!facts.contains { if case .prompt = $0 { return true }; return false }, "with previews off no prompt is read")
        let typed = parse([prompt], previews: true)
        suite.expect(typed.contains(.prompt("Use the Bash tool to run exactly: echo SPIKE_OK > spike.txt", at: date("2026-10-21T17:50:38.622Z"))),
                     "with previews on a typed prompt joins the conversation")
        let midTurn = #"{"isSidechain":false,"type":"attachment","attachment":{"type":"queued_command","prompt":[{"type":"text","text":"Also run lint"},{"type":"image","source":{}}]},"timestamp":"2026-10-21T17:52:00.000Z"}"#
        let summary = #"{"type":"user","isCompactSummary":true,"message":{"role":"user","content":"This session is being continued"},"timestamp":"2026-10-21T17:53:00.000Z"}"#
        let command = #"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"},"timestamp":"2026-10-21T17:54:00.000Z"}"#
        let longReply = #"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"\#(String(repeating: "a", count: 7000))"}]},"timestamp":"2026-10-21T17:55:00.000Z"}"#
        let said = parse([midTurn, summary, command, longReply], previews: true).compactMap { fact -> String? in
            switch fact {
            case .prompt(let text, _, _), .text(let text, _, _): return text
            default: return nil
            }
        }
        suite.expect(!parse([summary], previews: false).contains { if case .turnBegan = $0 { return true }; return false },
                     "an idle compaction's summary begins no turn")
        let slash = #"{"type":"user","message":{"role":"user","content":"<command-name>/recap</command-name>\n<command-message>recap</command-message>"},"timestamp":"2026-10-22T10:26:11.114Z"}"#
        suite.expect(!parse([slash], previews: true).contains { if case .turnBegan = $0 { return true }; return false },
                     "a slash command like /recap begins no turn")
        suite.expect(said.first == "Also run lint" && said.count == 2,
                     "a prompt typed while Claude works counts; summaries and commands do not: \(said.map { $0.prefix(20) })")
        suite.expect(said.last?.count == ClaudeTranscriptReader.replyLimit && said.last?.hasSuffix("…") == true,
                     "a very long reply is cut at its limit")

        let started = #"{"type":"assistant","timestamp":"2026-10-22T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"tb1","name":"Bash","input":{"command":"npx expo start --port 8088","description":"Run own Metro","run_in_background":true}}]}}"#
        let given = #"{"type":"user","timestamp":"2026-10-22T10:00:01.000Z","toolUseResult":{"backgroundTaskId":"b9x"},"message":{"role":"user","content":[{"tool_use_id":"tb1","type":"tool_result","content":"Command running in background with ID: b9x."}]}}"#
        let stopped = #"{"type":"assistant","timestamp":"2026-10-22T10:05:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"tk","name":"TaskStop","input":{"task_id":"b9x"}}]}}"#
        let jobs = parse([started, given, stopped], previews: true)
        suite.expect(jobs.contains(.backgroundStarted(toolID: "tb1", service: true, label: "Run own Metro", at: date("2026-10-22T10:00:00.000Z")))
                     && jobs.contains(.backgroundTask(toolID: "tb1", task: "b9x"))
                     && jobs.contains(.subagentEnded(id: "b9x", at: date("2026-10-22T10:05:00.000Z"))),
                     "a background command, the id it was given and its stop are read")
        suite.expect(parse([started], previews: false).contains(.backgroundStarted(toolID: "tb1", service: true, label: nil,
                                                                                  at: date("2026-10-22T10:00:00.000Z"))),
                     "with previews off it is still told apart, without its words")
        let sent = #"{"type":"user","timestamp":"2026-10-22T13:14:15.571Z","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"a758ef93113160fa0","description":"Map friends tab and add flow"},"message":{"role":"user","content":[{"tool_use_id":"ta1","type":"tool_result","content":[{"type":"text","text":"Async agent launched successfully."}]}]}}"#
        let waited = #"{"type":"user","timestamp":"2026-10-22T13:14:20.000Z","toolUseResult":{"status":"completed","agentId":"a1","description":"Look around"},"message":{"role":"user","content":[{"tool_use_id":"ta2","type":"tool_result","content":"Done."}]}}"#
        let agents = parse([sent, waited], previews: true)
        suite.expect(agents.contains(.backgroundStarted(toolID: "ta1", service: false, label: "Map friends tab and add flow",
                                                        at: date("2026-10-22T13:14:15.571Z")))
                     && agents.contains(.backgroundTask(toolID: "ta1", task: "a758ef93113160fa0")),
                     "a subagent sent to the background is work Claude waits for, under its own id")
        suite.expect(!agents.contains { if case .backgroundStarted(let id, _, _, _) = $0 { return id == "ta2" }; return false },
                     "a subagent that answered before the turn went on is not")
        suite.expect(parse([sent], previews: false).contains(.backgroundStarted(toolID: "ta1", service: false, label: nil,
                                                                               at: date("2026-10-22T13:14:15.571Z"))),
                     "with previews off without what it does")

        let notice = #"{"type":"user","origin":{"kind":"task-notification"},"timestamp":"2026-10-21T19:06:34.760Z","message":{"role":"user","content":"<task-notification>\n<task-id>a05df</task-id>\n<status>completed</status>\n</task-notification>"},"sessionId":"s1"}"#
        let queued = #"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-10-21T19:06:30.000Z","content":"<task-notification><task-id>b77</task-id><status>failed</status></task-notification>"}"#
        let quoted = #"{"type":"assistant","timestamp":"2026-10-21T19:07:00.000Z","message":{"role":"assistant","content":[{"type":"text","text":"<task-notification><task-id>zz</task-id><status>completed</status>"}]}}"#
        let running = #"{"type":"queue-operation","timestamp":"2026-10-21T19:06:00.000Z","content":"<task-notification><task-id>c1</task-id><status>running</status></task-notification>"}"#
        let notified = parse([notice, queued, quoted, running], previews: false)
        suite.expect(!parse([notice], previews: true).contains { if case .prompt = $0 { return true }; return false },
                     "a task notification is not something the person typed")
        suite.expect(notified.contains(.subagentEnded(id: "a05df", at: date("2026-10-21T19:06:34.760Z")))
                     && notified.contains(.subagentEnded(id: "b77", at: date("2026-10-21T19:06:30.000Z"))),
                     "a task notification ends the background subagent it names")
        suite.expect(!notified.contains { if case .subagentEnded(let id, _) = $0 { return id == "zz" || id == "c1" }; return false },
                     "a reply quoting one, or a task still running, ends nothing")

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

    /// Issue #121: every terminal is told apart by what the session's own
    /// environment says, and reached by its device, pane or window.
    static func terminalHosts(_ suite: TestSuite) {
        let tmux = ["TERM_PROGRAM": "tmux", "TMUX": "/private/tmp/tmux-501/default,51484,0", "TMUX_PANE": "%1",
                    "__CFBundleIdentifier": "com.apple.Terminal"]
        let pane = ClaudeTmuxPane(tmux: tmux["TMUX"]!, pane: "%1")
        suite.expect(pane?.socket == "/private/tmp/tmux-501/default" && pane?.server == 51484 && pane?.pane == "%1",
                     "tmux names its socket, server and pane")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys029", port: nil, environment: tmux)
                     == .tmux(pane!), "a session in tmux in Terminal is a tmux pane, not a Terminal tab it never has")
        suite.expect(ClaudeTmuxPane(tmux: "relative,1,0", pane: "%1") == nil && ClaudeTmuxPane(tmux: "/s,x,0", pane: "%1") == nil
                     && ClaudeTmuxPane(tmux: "/s,4,0", pane: "1") == nil && ClaudeTmuxPane(tmux: "/s,4,0", pane: "%1;x") == nil
                     && ClaudeTmuxPane(tmux: "/s,1,0", pane: "%1") == nil, "a malformed tmux variable names no pane")
        var editorFromTmux = tmux
        editorFromTmux["TERM_PROGRAM"] = "vscode"
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys030", port: 4000,
                                                 environment: editorFromTmux, owner: "com.microsoft.VSCode")
                     == .ide(bundle: "com.microsoft.VSCode", port: 4000),
                     "an editor opened from tmux keeps tmux's variables, and its terminal is still the editor's")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.google.antigravity-ide", tty: "/dev/ttys030", port: nil,
                                                 environment: ["TERM_PROGRAM": "vscode"], owner: "com.microsoft.VSCode")
                     == .ide(bundle: "com.microsoft.VSCode", port: nil),
                     "VS Code opened from Antigravity's terminal is VS Code, as macOS holds it responsible")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.todesktop.230313mzl4w4u92", tty: "/dev/ttys030", port: nil,
                                                 environment: ["TERM_PROGRAM": "vscode"])
                     == .ide(bundle: "com.todesktop.230313mzl4w4u92", port: nil), "Cursor's terminal goes through the bridge")
        suite.expect(ClaudeSessionHosts.classify(bundle: nil, tty: "/dev/ttys030", port: nil,
                                                 environment: ["TERM_PROGRAM": "vscode"], owner: "com.example.unknown-editor")
                     == .app("com.example.unknown-editor"), "an editor the island doesn't know is only opened")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.googlecode.iterm2", tty: "/dev/ttys011", port: nil)
                     == .iterm(tty: "/dev/ttys011"), "a session in iTerm2 is found by its device")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys011", port: nil,
                                                 environment: ["TERM_PROGRAM": "iTerm.app"])
                     == .iterm(tty: "/dev/ttys011"), "iTerm2 started from Terminal says so itself")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.mitchellh.ghostty", tty: "/dev/ttys012", port: nil)
                     == .ghostty(tty: "/dev/ttys012"), "a session in Ghostty is found by its device")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.github.wez.wezterm", tty: "/dev/ttys013", port: nil,
                                                 environment: ["TERM_PROGRAM": "WezTerm", "WEZTERM_PANE": "7",
                                                               "WEZTERM_UNIX_SOCKET": "/tmp/wez/gui-sock-1"])
                     == .wezterm(pane: 7, socket: "/tmp/wez/gui-sock-1"), "a session in WezTerm keeps its pane and socket")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.github.wez.wezterm", tty: "/dev/ttys013", port: nil)
                     == .app("com.github.wez.wezterm"), "WezTerm without a pane id is only opened")
        let kitty = ClaudeSessionHosts.classify(bundle: "net.kovidgoyal.kitty", tty: "/dev/ttys014", port: nil,
                                                environment: ["KITTY_WINDOW_ID": "3"])
        let remote = ClaudeSessionHosts.classify(bundle: "net.kovidgoyal.kitty", tty: "/dev/ttys014", port: nil,
                                                 environment: ["KITTY_WINDOW_ID": "3", "KITTY_LISTEN_ON": "unix:/tmp/kitty-9"])
        suite.expect(kitty == .kitty(window: 3, socket: nil) && !kitty.acceptsReplies
                     && remote == .kitty(window: 3, socket: "unix:/tmp/kitty-9") && remote.acceptsReplies,
                     "kitty takes replies only with remote control on")
        // Seen live: kitty started from Antigravity's terminal keeps TERM_PROGRAM=vscode.
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.google.antigravity-ide", tty: "/dev/ttys033", port: 20133,
                                                 environment: ["TERM_PROGRAM": "vscode", "TERM": "xterm-kitty", "KITTY_WINDOW_ID": "1",
                                                               "KITTY_LISTEN_ON": "unix:/tmp/vs-kitty-63317"])
                     == .kitty(window: 1, socket: "unix:/tmp/vs-kitty-63317"), "kitty is kitty even with an editor's TERM_PROGRAM")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.microsoft.VSCode", tty: "/dev/ttys033", port: nil,
                                                 environment: ["TERM_PROGRAM": "vscode", "TERM": "alacritty"])
                     == .app("org.alacritty")
                     && ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys033", port: nil,
                                                    environment: ["TERM_PROGRAM": "Apple_Terminal", "TERM": "xterm-ghostty"])
                     == .ghostty(tty: "/dev/ttys033"), "Alacritty and Ghostty are told by their TERM too")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.microsoft.VSCode", tty: "/dev/ttys033", port: nil,
                                                 environment: ["TERM_PROGRAM": "vscode", "TERM": "xterm-256color", "KITTY_WINDOW_ID": "1"])
                     == .ide(bundle: "com.microsoft.VSCode", port: nil), "an editor opened from kitty is still the editor")
        let warp = ClaudeSessionHosts.classify(bundle: "dev.warp.Warp-Stable", tty: "/dev/ttys015", port: nil,
                                               environment: ["TERM_PROGRAM": "WarpTerminal"])
        suite.expect(warp == .app("dev.warp.Warp-Stable") && !warp.acceptsReplies && warp.bundle == "dev.warp.Warp-Stable",
                     "Warp can only be raised, and the island knows which app it is")
        let text = NotchClaudeSessionStrings.enUS
        suite.expect(text.replyUnavailable(for: .app("dev.warp.Warp-Stable")) == "Replies aren’t available in Warp yet"
                     && text.replyUnavailable(for: .kitty(window: 3, socket: nil)) == text.replyKittyRemote
                     && text.replyUnavailable(for: .unknown) == text.replyUnavailable,
                     "a terminal replies can't reach is named")
        var named = pane!
        named.outer = "com.mitchellh.ghostty"
        suite.expect(ClaudeSessionHost.tmux(named).displayName == "tmux in Ghostty"
                     && ClaudeSessionHost.tmux(pane!).displayName == "tmux"
                     && ClaudeSessionHost.iterm(tty: "/dev/ttys1").displayName != nil, "the host is named as the person knows it")

        let clients = "/dev/ttys001\t100\t$1\t50\n/dev/ttys002\t200\t$0\t40\n/dev/ttys003\t300\t$0\t90\nbad line\n"
        suite.expect(ClaudeTmux.client(from: clients, session: "$0") == ClaudeTmuxClient(tty: "/dev/ttys003", pid: 300, elsewhere: false),
                     "the terminal on the pane's session used last shows it")
        suite.expect(ClaudeTmux.client(from: clients, session: "$5") == ClaudeTmuxClient(tty: "/dev/ttys003", pid: 300, elsewhere: true)
                     && ClaudeTmux.client(from: "", session: "$0") == nil,
                     "with none on that session the last one switches to it; with none at all nothing does")

        let wez = Data(#"[{"window_id":0,"tab_id":1,"pane_id":7,"tty_name":"/dev/ttys013","window_title":"claude"},{"window_id":0,"tab_id":2,"pane_id":8}]"#.utf8)
        suite.expect(ClaudeWezTerm.panes(from: wez) == [
            ClaudeWezTerm.Pane(pane: 7, window: 0, tab: 1, tty: "/dev/ttys013", windowTitle: "claude"),
            ClaudeWezTerm.Pane(pane: 8, window: 0, tab: 2, tty: nil, windowTitle: nil),
        ] && ClaudeWezTerm.panes(from: Data("nope".utf8)).isEmpty, "WezTerm's pane list is read with each pane's device")
        let kittyList = Data(#"[{"tabs":[{"windows":[{"id":3,"pid":500,"foreground_processes":[{"pid":501},{"pid":502}]},{"id":4,"pid":600}]}]}]"#.utf8)
        suite.expect(ClaudeKitty.pids(in: kittyList, window: 3) == [500, 501, 502] && ClaudeKitty.pids(in: kittyList, window: 9).isEmpty,
                     "a kitty window runs the session when it lists its process")

        suite.expect(ClaudeTyping.steps("hi\nthere") == [.paste("hi\nthere"), .wait(0.08), .enter],
                     "a reply is one paste and a return")
        let withImages = ClaudeTyping.steps("look", images: ["/a.png", "/b.png"])
        suite.expect(withImages == [.paste("/a.png"), .wait(0.06), .paste("/b.png"), .wait(0.06),
                                    .wait(ClaudeTerminalScripts.imageSettle(2)), .paste("look"), .wait(0.08), .enter]
                     && ClaudeTyping.steps("", images: ["/a.png"]).filter { $0 == .paste("") }.isEmpty,
                     "images go first, each a paste of its path, and the text waits for them")
        let iterm = ClaudeTTYTerminalScripts.itermType(ClaudeTyping.steps("say \"hi\""), tty: "/dev/ttys011")
        suite.expect(iterm.contains(#"{"/dev/ttys011", "ttys011"}"#) && iterm.contains(#"\"hi\""#)
                     && iterm.contains("newline no") && iterm.contains("character id 13"),
                     "iTerm2 is typed into by device, with the reply quoted and the return its own")
        let ghostty = ClaudeTTYTerminalScripts.ghosttyFocus(tty: "ttys012")
        suite.expect(ghostty.contains(#"{"/dev/ttys012", "ttys012"}"#) && ghostty.contains("focus theTerminal"),
                     "Ghostty is found by device too")

        let folder = FileManager.default.temporaryDirectory.appending(path: "vs-ide-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let app = folder.appending(path: "Cursor.app")
        let resources = app.appending(path: "Contents/Resources/app")
        try? FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        try? Data(#"{"applicationName":"cursor","dataFolderName":".cursor"}"#.utf8).write(to: resources.appending(path: "product.json"))
        let home = folder.appending(path: "home")
        let places = ClaudeIDE.places(app: app, home: home)
        suite.expect(places?.cli == resources.appending(path: "bin/cursor") && places?.extensions == home.appending(path: ".cursor/extensions"),
                     "an editor's command line and extensions folder come from its product.json")
        try? Data(#"{"applicationName":"../x","dataFolderName":".cursor"}"#.utf8).write(to: resources.appending(path: "product.json"))
        suite.expect(ClaudeIDE.places(app: app, home: home) == nil, "a product.json naming a way out of the folder is not followed")
    }

    static func hosts(_ suite: TestSuite) {
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys004", port: nil)
                     == .terminal(tty: "/dev/ttys004"), "a session in Terminal is reached by its device")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: nil, port: nil) == .app("com.apple.Terminal"),
                     "without a device only the app can be brought forward")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.google.antigravity-ide", tty: "/dev/ttys009", port: 53142)
                     == .ide(bundle: "com.google.antigravity-ide", port: 53142), "a session in Antigravity keeps its IDE port")
        terminalHosts(suite)
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
