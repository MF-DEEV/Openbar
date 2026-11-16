// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import Foundation

/// Export diagnostics (#230): the masking every system shares, the log and
/// journal formats, hook states told apart by install records, and that the
/// file holds exactly the parts the screen before saving lists.
enum DiagnosticsTests {
    static func run(_ suite: TestSuite) {
        suite.run("diagnostics masking matches the shared fixture") { masking(suite) }
        suite.run("diagnostics log lines and files") { log(suite) }
        suite.run("diagnostics journal keeps names, never content") { journal(suite) }
        suite.run("diagnostics hook states from install records") { hookStates(suite) }
        suite.run("diagnostics hook records against a real install") { installs(suite) }
        suite.run("diagnostics report files match the chosen parts") { report(suite) }
        suite.run("diagnostics strings") { strings(suite) }
    }

    private struct Fixture: Decodable {
        struct Case: Decodable { let `in`: String; let out: String; let home: String? }
        let home: String
        let cases: [Case]
    }

    static func masking(_ suite: TestSuite) {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appending(path: "Fixtures/diagnostics-redaction.json")
        guard let data = try? Data(contentsOf: url), let fixture = try? JSONDecoder().decode(Fixture.self, from: data) else {
            return suite.expect(false, "the shared masking fixture reads")
        }
        suite.expect(fixture.cases.count >= 25, "the fixture has its cases")
        for item in fixture.cases {
            let home = item.home ?? fixture.home
            let masked = DiagnosticsRedactor.redact(item.in, home: home)
            suite.expect(masked == item.out, "masks \(item.in.debugDescription): got \(masked.debugDescription), expected \(item.out.debugDescription)")
            suite.expect(DiagnosticsRedactor.redact(item.out, home: home) == item.out,
                         "masking twice changes nothing: \(item.out.debugDescription)")
        }
        suite.expect(DiagnosticsRedactor.redact("", home: "/Users/jane").isEmpty, "empty stays empty")
        suite.expect(DiagnosticsRedactor.redact("root /", home: "/") == "root /", "a home of / masks nothing")
    }

    static func log(_ suite: TestSuite) {
        let date = Date(timeIntervalSince1970: 1_763_307_002.123) // 2025-11-16T15:30:02.123Z
        let line = DiagnosticsLog.line(.info, "sessions", "two\nlines in /Users/jane/x with sk-ant-api03-abcdefghij",
                                       at: date, home: "/Users/jane")
        suite.expect(line == "2025-11-16T15:30:02.123Z INFO  [sessions] two lines in ~/x with <secret>\n",
                     "one masked line, UTC with milliseconds: \(line.debugDescription)")
        suite.expect(DiagnosticsLog.line(.error, "a", "b", at: date, home: "/x").contains(" ERROR [a] b"), "levels are five wide")
        suite.expect(DiagnosticsLog.day(date) == "2025-11-16" && DiagnosticsLog.fileName(day: "2025-11-16") == "openbar-2025-11-16.log",
                     "files are named after the UTC day")
        suite.expect(DiagnosticsLog.dayOf("openbar-2025-11-16.log") == "2025-11-16" && DiagnosticsLog.dayOf("other.log") == nil
                     && DiagnosticsLog.dayOf("openbar-2025-11-16.log.partial") == nil, "only log files have a day")
        let names = ["openbar-2025-11-13.log", "openbar-2025-11-16.log", "openbar-2025-11-14.log", "openbar-2025-11-15.log", "notes.txt"]
        suite.expect(DiagnosticsLog.recent(names, days: 3, now: date) == ["openbar-2025-11-14.log", "openbar-2025-11-15.log", "openbar-2025-11-16.log"],
                     "the last three days, oldest first")

        let folder = FileManager.default.temporaryDirectory.appending(path: "openbar-log-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let old = folder.appending(path: "openbar-2025-11-01.log")
        FileManager.default.createFile(atPath: old.path, contents: Data("old\n".utf8))
        let kept = folder.appending(path: "openbar-2025-11-12.log")
        FileManager.default.createFile(atPath: kept.path, contents: Data("kept\n".utf8))
        let writer = DiagnosticsLog(folder: folder, home: "/Users/jane")
        writer.write(.info, "app", "started", now: date)
        writer.write(.warn, "hooks", "gemini missing", now: date.addingTimeInterval(1))
        writer.flush()
        let today = (try? String(contentsOf: folder.appending(path: "openbar-2025-11-16.log"), encoding: .utf8)) ?? ""
        suite.expect(today.components(separatedBy: "\n").filter { !$0.isEmpty }.count == 2
                     && today.contains("[app] started") && today.contains("WARN  [hooks] gemini missing"),
                     "lines land in the day's file in order")
        suite.expect(!FileManager.default.fileExists(atPath: old.path), "a file older than a week is deleted")
        suite.expect(FileManager.default.fileExists(atPath: kept.path), "a file within the week stays")
    }

    static func journal(_ suite: TestSuite) {
        let tool = ClaudeToolUse(id: "t1", name: "Bash", summary: "rm -rf ~/secret-project", started: Date())
        suite.expect(DiagnosticsJournal.label(AgentHookEvent.Kind.toolStarted(tool)) == ("toolStarted", "Bash"), "a tool keeps its name only")
        suite.expect(DiagnosticsJournal.label(.prompt("CANARY-PROMPT")) == ("promptSubmitted", nil), "a prompt keeps no text")
        suite.expect(DiagnosticsJournal.label(.reply("CANARY-REPLY")) == ("reply", nil), "a reply keeps no text")
        suite.expect(DiagnosticsJournal.label(.stopped(lastText: "CANARY", failed: true)) == ("stopped", "failed"), "a stop keeps no text")
        suite.expect(DiagnosticsJournal.label(.title("CANARY title")) == ("title", nil), "a title keeps no text")
        suite.expect(DiagnosticsJournal.label(ClaudeHookFact.Event.stopped(lastText: "CANARY")) == ("stopped", nil),
                     "Claude's stop keeps no text")
        suite.expect(DiagnosticsJournal.label(ClaudeHookFact.Event.other("PreCompact")) == ("other", "PreCompact"),
                     "an event name is kept")
        suite.expect(DiagnosticsJournal.safe("rm -rf /tmp") == nil && DiagnosticsJournal.safe("mcp__github__create_issue") != nil
                     && DiagnosticsJournal.safe(String(repeating: "a", count: 65)) == nil && DiagnosticsJournal.safe("zażółć") == nil,
                     "only plain names pass")
        suite.expect(DiagnosticsJournal.shortSession("gemini:abcdef12-3456-7890") == "abcdef12"
                     && DiagnosticsJournal.shortSession("3f2504e0-4f89-11d3") == "3f2504e0"
                     && DiagnosticsJournal.shortSession("host:a/b") == "hostab", "sessions are cut to eight plain characters")

        let journal = DiagnosticsJournal(log: nil)
        for index in 0..<(DiagnosticsJournal.capacity + 5) {
            journal.record(DiagnosticsJournal.make(agent: .claude, event: ("toolStarted", "Bash"), session: "s\(index)",
                                                   from: .generating, to: .runningTool, at: Date()))
        }
        let kept = journal.recent()
        suite.expect(kept.count == DiagnosticsJournal.capacity && kept.first?.session == "s5", "the oldest events go first")
        let event = DiagnosticsJournal.make(agent: .gemini, event: ("stopped", nil), session: "gemini:x", from: .generating,
                                            to: .finished, at: Date(timeIntervalSince1970: 0))
        suite.expect(event.agent == "gemini" && event.from == "generating" && event.to == "finished"
                     && event.time == "1970-01-01T00:00:00.000Z", "an event names agent, states and time")
    }

    static func hookStates(_ suite: TestSuite) {
        let record = HookRecords.Record(installedAt: Date(), fingerprint: "abc")
        suite.expect(HookRecords.derive(.installed, record: nil, current: nil) == .installed, "installed stays installed")
        suite.expect(HookRecords.derive(.missing, record: nil, current: nil) == .missing, "never installed is missing")
        suite.expect(HookRecords.derive(.missing, record: record, current: nil) == .removedByHand,
                     "installed by Openbar and gone is removed by hand")
        suite.expect(HookRecords.derive(.outdated, record: record, current: "abc") == .outdated,
                     "as Openbar wrote it, but older, is outdated")
        suite.expect(HookRecords.derive(.outdated, record: record, current: "xyz") == .changedByHand,
                     "otherwise than Openbar wrote it is changed by hand")
        suite.expect(HookRecords.derive(.outdated, record: nil, current: "xyz") == .outdated,
                     "hooks from before records can't be told apart")
        suite.expect(HookRecords.derive(.unreadable, record: record, current: nil) == .unreadable, "unreadable stays unreadable")

        let marker = "# openbar-hook com.openbar.app"
        func json(_ object: Any) -> Data { try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]) }
        let ours: [String: Any] = ["hooks": ["SessionStart": [["hooks": [["command": "nc -U a; exit 0 \(marker)"]]]]]]
        var theirs = ours
        theirs["theme"] = "dark"
        let first = HookRecords.fingerprint(of: json(ours), marker: marker)
        suite.expect(first != nil && first == HookRecords.fingerprint(of: json(theirs), marker: marker),
                     "the person's own settings don't change Openbar's fingerprint")
        let changed: [String: Any] = ["hooks": ["SessionStart": [["hooks": [["command": "nc -U b; exit 0 \(marker)"]]]]]]
        suite.expect(HookRecords.fingerprint(of: json(changed), marker: marker) != first, "a changed command does")
        suite.expect(HookRecords.fingerprint(of: json(["theme": "dark"]), marker: marker) == nil
                     && HookRecords.fingerprint(of: nil, marker: marker) == nil, "no marker, or no file, has no fingerprint")
        suite.expect(HookRecords.fingerprint(of: Data("x".utf8), marker: marker, wholeFile: true)
                     != HookRecords.fingerprint(of: Data("y".utf8), marker: marker, wholeFile: true), "an own file is taken whole")
        let toml = "[[hooks]]\nname = \"mine\"\n\n\(marker) start: added\n[[hooks]]\ntimeout = 5.0\n\(marker) end\n"
        let edited = toml.replacingOccurrences(of: "timeout = 5.0", with: "timeout = 9.0")
        suite.expect(HookRecords.fingerprint(of: Data(toml.utf8), marker: marker)
                     != HookRecords.fingerprint(of: Data(edited.utf8), marker: marker), "a marked block is taken line by line")
        suite.expect(HookRecords.fingerprint(of: Data(toml.utf8), marker: marker)
                     == HookRecords.fingerprint(of: Data(("[x]\n" + toml).utf8), marker: marker), "lines outside it are not")

        let suiteName = "com.openbar.tests.diagnostics-records"
        guard let defaults = UserDefaults(suiteName: suiteName) else { return suite.expect(false, "test defaults") }
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let file = FileManager.default.temporaryDirectory.appending(path: "openbar-hooks-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: file) }
        try? json(ours).write(to: file)
        HookRecords.noteInstalled("agent:gemini", file: file, marker: marker, in: defaults)
        suite.expect(HookRecords.all(in: defaults)["agent:gemini"]?.fingerprint == first, "an install is recorded")
        let linked = FileManager.default.temporaryDirectory.appending(path: "openbar-linked-\(UUID().uuidString).json")
        try? FileManager.default.createSymbolicLink(at: linked, withDestinationURL: file)
        let linkedID = HookRecords.claudeID(linked)
        try? FileManager.default.removeItem(at: linked)
        suite.expect(HookRecords.claudeID(linked) == linkedID, "a settings link deleted by hand still names its record")
        HookRecords.forget("agent:gemini", in: defaults)
        suite.expect(HookRecords.all(in: defaults).isEmpty && defaults.object(forKey: DefaultsKey.diagnosticsHookRecords) == nil,
                     "a removal by Openbar forgets it")
    }

    /// Gemini CLI's hooks put in by the installer, then taken out and
    /// changed by hand, in a throwaway home.
    static func installs(_ suite: TestSuite) {
        let home = FileManager.default.temporaryDirectory.appending(path: "openbar-diag-home-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        try? FileManager.default.createDirectory(at: home.appending(path: ".gemini"), withIntermediateDirectories: true)
        let suiteName = "com.openbar.tests.diagnostics-install"
        guard let defaults = UserDefaults(suiteName: suiteName) else { return suite.expect(false, "test defaults") }
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let bundle = "com.openbar.app.dev", socket = "/tmp/openbar-test.sock"
        let settings = AgentHookInstaller.geminiSettings(home: home, environment: [:])
        guard (try? AgentHookInstaller.install(.gemini, home: home, socket: socket, bundle: bundle)) != nil else {
            return suite.expect(false, "the installer writes Gemini's hooks")
        }
        guard let file = AgentHookInstaller.hookFile(.gemini, home: home, bundle: bundle), file.url == settings else {
            return suite.expect(false, "Gemini's hook file is its settings")
        }
        HookRecords.noteInstalled(HookRecords.agentID(.gemini), file: file.url, marker: AgentHookInstaller.marker(bundle: bundle),
                                  in: defaults)
        func state(_ socket: String) -> HookRecords.State {
            AgentHookInstaller.diagnosticsState(.gemini, home: home, socket: socket, bundle: bundle,
                                                records: HookRecords.all(in: defaults))
        }
        suite.expect(state(socket) == .installed, "freshly installed")
        suite.expect(state("/tmp/other.sock") == .outdated, "the same hooks seen by a newer Openbar are outdated")

        let original = (try? Data(contentsOf: settings)) ?? Data()
        let text = String(decoding: original, as: UTF8.self).replacingOccurrences(of: "openbar-test.sock", with: "elsewhere.sock")
        try? Data(text.utf8).write(to: settings)
        suite.expect(state(socket) == .changedByHand, "a command edited by hand is changed by hand")

        try? Data(#"{"theme":"dark"}"#.utf8).write(to: settings)
        suite.expect(state(socket) == .removedByHand, "hooks taken out by hand are removed by hand")

        HookRecords.forget(HookRecords.agentID(.gemini), in: defaults)
        suite.expect(state(socket) == .missing, "after Openbar removed them itself, they are just missing")
    }

    static func report(_ suite: TestSuite) {
        let home = "/Users/jane"
        let input = DiagnosticsInput(
            system: DiagnosticsSystem(app: .init(name: "Openbar", version: "3.5.0", channel: "stable", developerBuild: false),
                                      os: .init(name: "macOS", version: "26.1.0"), cpu: "Apple M3 (arm64)",
                                      language: "en-US",
                                      displays: [.init(width: 1512, height: 982, scale: 2, main: true, builtIn: true, notch: true)]),
            features: [DiagnosticsFeature(id: "sessions", group: "island", on: true)],
            agents: [DiagnosticsAgent(id: "gemini", name: "Gemini CLI", installed: true, version: "0.20.0",
                                      hooks: [.init(state: "removedByHand", configPath: "/Users/jane/.gemini/settings.json")])],
            events: [DiagnosticsEvent(time: "2025-11-16T15:30:02.123Z", agent: "claude", event: "toolStarted", detail: "Bash",
                                      session: "abcdef12", from: "generating", to: "runningTool")],
            hosts: DiagnosticsHosts(terminals: ["Terminal"], editors: [], inUse: ["Terminal": 1],
                                    claudeAccounts: ["jane@example.com"], codexAccounts: [], remoteHosts: [.init(label: "box", state: "off")]),
            logs: [("openbar-2025-11-16.log", "2025-11-16T15:30:02.123Z ERROR [hooks] failed in /Users/jane/.claude: token=CANARY123 sk-ant-api03-CANARYKEY99\n")])
        let now = Date(timeIntervalSince1970: 1_763_307_002)
        let all = DiagnosticsReport.files(parts: Set(DiagnosticsPart.allCases), input: input, platform: "macOS", now: now, home: home)
        let paths = all.map(\.path)
        suite.expect(Set(paths) == ["README.txt", "manifest.json", "system.json", "features.json", "agents.json", "events.json",
                                    "hosts.json", "logs/openbar-2025-11-16.log"], "every part has its file: \(paths)")
        let manifest = all.first { $0.path == "manifest.json" }.flatMap { try? JSONDecoder().decode(DiagnosticsManifest.self, from: $0.data) }
        suite.expect(manifest?.files == paths.sorted() && manifest?.parts == DiagnosticsPart.allCases.map(\.rawValue)
                     && manifest?.format == 1, "the manifest lists exactly the files and parts written")
        let everything = all.map { String(decoding: $0.data, as: UTF8.self) }.joined(separator: "\n")
        for canary in ["CANARY123", "CANARYKEY99", "/Users/jane", "jane@example.com"] {
            suite.expect(!everything.contains(canary), "nothing in the file carries \(canary)")
        }
        suite.expect(everything.contains("removedByHand") && everything.contains("~/.gemini/settings.json"),
                     "a hook removed by hand shows, with its file under ~")

        let required = DiagnosticsReport.files(parts: [], input: input, platform: "macOS", now: now, home: home)
        suite.expect(Set(required.map(\.path)) == ["README.txt", "manifest.json", "system.json", "features.json", "agents.json"],
                     "with every switch off only the parts always in are written")
        let readme = required.first { $0.path == "README.txt" }.map { String(decoding: $0.data, as: UTF8.self) } ?? ""
        suite.expect(readme.contains("Left out before saving:") && readme.contains("- logs/:") && readme.contains("- events.json:"),
                     "the README says what was left out")
        let noLogs = DiagnosticsReport.files(parts: [.events, .hosts], input: input, platform: "macOS", now: now, home: home)
        suite.expect(!noLogs.contains { $0.path.hasPrefix("logs/") }, "logs switched off leave no logs folder")

        suite.expect(DiagnosticsReport.version(in: "2.1.309 (Claude Code)") == "2.1.309"
                     && DiagnosticsReport.version(in: "codex-cli 0.164.0\n") == "0.164.0"
                     && DiagnosticsReport.version(in: "v1.2.3-beta.1") == "1.2.3-beta.1"
                     && DiagnosticsReport.version(in: "no version here") == nil, "versions are read from --version output")
        suite.expect(DiagnosticsReport.fileName(now: now, timeZone: TimeZone(identifier: "UTC")!) == "openbar-diagnostics-20251116-1530.zip",
                     "the file is named after the minute it was made")
    }

    static func strings(_ suite: TestSuite) {
        for language in AppLanguage.allCases {
            let text = FeatureStrings.diagnostics(language)
            let values = Mirror(reflecting: text).children.compactMap { $0.value as? String }
            suite.expect(values.allSatisfy { !$0.isEmpty && !$0.contains("\u{2014}") && !$0.contains("...") },
                         "no empty strings, em dashes or three periods (\(language.rawValue))")
            suite.expect(text.eventsCaptionFormat.components(separatedBy: "%d").count == 2
                         && text.logsCaptionFormat.components(separatedBy: "%d").count == 2
                         && text.failedFormat.components(separatedBy: "%@").count == 2, "formats take their one value (\(language.rawValue))")
            suite.expect([text.exportButton, text.save, text.saveAndReport, text.saving].allSatisfy { $0.hasSuffix("…") },
                         "buttons that open a panel, and work under way, end with … (\(language.rawValue))")
            suite.expect(!values.contains { $0.contains("«") || $0.contains("„") || $0.contains("\"") }, "no quotes (\(language.rawValue))")
        }
    }
}
