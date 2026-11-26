// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Background sessions, from what Claude Code 2.1.316 really prints and writes.
enum ClaudeBackgroundSessionTests {
    static func run(_ suite: TestSuite) {
        suite.run("background registry record") { record(suite) }
        suite.run("agent view list") { list(suite) }
        suite.run("ended sessions kept for a while") { ended(suite) }
        suite.run("background actions and names") { actions(suite) }
        suite.run("agent view switched off") { switches(suite) }
        suite.run("terminal screen from logs") { screen(suite) }
        suite.run("command lines") { lines(suite) }
        suite.run("recent folders") { folders(suite) }
        suite.run("hidden attach keystrokes") { keys(suite) }
        suite.run("new session agents and models") { models(suite) }
        suite.run("new session efforts") { efforts(suite) }
    }

    private static func json(_ text: String) -> Data { Data(text.utf8) }

    static func record(_ suite: TestSuite) {
        let bg = json(#"{"pid":18198,"sessionId":"a5b5e11e-ad5a-4451","cwd":"/tmp/one","kind":"bg","entrypoint":"cli","name":"vs-spike-one","jobId":"a5b5e11e","status":"idle","nameSource":"peer"}"#)
        suite.expect(ClaudeSessionRegistryReader.record(bg)?.job == "a5b5e11e", "a bg record names its job")
        let noJob = json(#"{"pid":1,"sessionId":"abcdef1234567","cwd":"/tmp/one","kind":"background"}"#)
        suite.expect(ClaudeSessionRegistryReader.record(noJob)?.job == "abcdef12", "the short id starts the session id")
        let interactive = json(#"{"pid":1,"sessionId":"abc","cwd":"/tmp/one","kind":"interactive"}"#)
        suite.expect(ClaudeSessionRegistryReader.record(interactive)?.job == nil, "a terminal session has no job")
        let old = json(#"{"pid":1,"sessionId":"abc","cwd":"/tmp/one"}"#)
        suite.expect(ClaudeSessionRegistryReader.record(old)?.job == nil, "older Claude Code writes no kind")
        suite.expect(ClaudeSessionHost.background(job: "a").acceptsReplies, "a background session takes replies")
    }

    static func list(_ suite: TestSuite) {
        let output = json(#"""
        [{"pid":17514,"cwd":"/x","kind":"interactive","sessionId":"i","name":"mine","status":"idle"},
         {"pid":18198,"id":"a5b5e11e","cwd":"/tmp/one","kind":"background","startedAt":1793486734159,"sessionId":"a5b5e11e-ad","name":"one","status":"idle","state":"done"},
         {"id":"b7f26484","cwd":"/tmp/two","kind":"background","startedAt":1793486730361,"sessionId":"b7f26484-ad","state":"something new"}]
        """#)
        let jobs = ClaudeBackgroundJobs.decode(output, configHome: "/h")
        suite.expect(jobs?.count == 2, "only background sessions: \(jobs?.count ?? -1)")
        suite.expect(jobs?[0].state == .done && jobs?[0].pid == 18198 && jobs?[0].configHome == "/h", "a live job")
        suite.expect(jobs?[1].ended == true && jobs?[1].state == nil && jobs?[1].name == "two", "an ended job, a state not known yet, no name")
        suite.expect(ClaudeBackgroundJobs.decode(json("'claude agents --json' is disabled"), configHome: nil) == nil, "an error isn't a list")
        suite.expect(ClaudeBackgroundJobs.decode(json("[]"), configHome: nil) == [], "an empty list is a list")
    }

    private static func job(_ id: String, pid: Int32? = nil, started: Date? = nil, state: ClaudeBackgroundState? = .done) -> ClaudeBackgroundJob {
        ClaudeBackgroundJob(id: id, sessionID: id + "-s", cwd: "/tmp/\(id)", name: id, state: state, pid: pid, started: started)
    }

    static func ended(_ suite: TestSuite) {
        let now = Date(timeIntervalSinceReferenceDate: 1_000_000)
        let old = now.addingTimeInterval(-2 * ClaudeBackgroundJobs.keptFor)
        let endings = ClaudeBackgroundJobs.endings(
            [job("seen", started: old), job("stale", started: old), job("alive", pid: 5, started: old)],
            known: [:], alive: ["seen"], now: now)
        suite.expect(endings["seen"] == now, "one seen alive ended just now")
        suite.expect(endings["stale"] == old, "one never seen alive ended by its start at the latest")
        suite.expect(endings["alive"] == nil, "a live job hasn't ended")
        let kept = ClaudeBackgroundJobs.kept([job("seen"), job("stale"), job("alive", pid: 5)], endings: endings, now: now)
        suite.expect(kept.map(\.id) == ["seen"], "old and live ones aren't kept: \(kept.map(\.id))")
        let many = (0..<9).map { job("j\($0)") }
        let times = Dictionary(uniqueKeysWithValues: many.enumerated().map { ($1.id, now.addingTimeInterval(-Double($0))) })
        let few = ClaudeBackgroundJobs.kept(many, endings: times, now: now)
        suite.expect(few.count == ClaudeBackgroundJobs.keptAtMost && few.first?.id == "j0", "the newest few")

        let row = ClaudeBackgroundJobs.session(job("r", state: .failed), ended: now, branch: "main")
        suite.expect(row.phase == .idle && row.pid == 0 && row.branch == "main" && row.host == .background(job: "r"), "an ended row is quiet")
        suite.expect(ClaudeSessionCounts([row]).isEmpty, "and counts as nothing on the strip")
        suite.expect(!NotchClaudeSessionsSupport.canReply(row), "it takes no reply until Open brings it back")
    }

    static func actions(_ suite: TestSuite) {
        let text = NotchClaudeSessionStrings.enUS
        func session(_ phase: ClaudeSessionPhase, _ state: ClaudeBackgroundState?, ended: Bool = false) -> ClaudeSession {
            var session = NotchSessionAgentTests.session("s", phase, provider: .claude)
            session.host = .background(job: "j")
            session.background = ClaudeBackgroundDetails(job: "j", state: state, ended: ended)
            return session
        }
        let plain = NotchSessionAgentTests.session("s", .idle, provider: .claude)
        suite.expect(!NotchClaudeSessionsSupport.canStop(plain) && !NotchClaudeSessionsSupport.canRemove(plain), "a terminal session has neither")
        suite.expect(NotchClaudeSessionsSupport.backgroundPhase(plain, text: text) == nil, "nor a background state")

        let working = session(.generating, .working)
        suite.expect(NotchClaudeSessionsSupport.canStop(working) && !NotchClaudeSessionsSupport.canRemove(working), "at work: stop, not remove")
        suite.expect(NotchClaudeSessionsSupport.canReply(working), "at work: a reply joins its queue")
        let done = session(.idle, .done)
        suite.expect(NotchClaudeSessionsSupport.canStop(done) && NotchClaudeSessionsSupport.canRemove(done), "done: both")
        let gone = session(.idle, .stopped, ended: true)
        suite.expect(!NotchClaudeSessionsSupport.canStop(gone) && NotchClaudeSessionsSupport.canRemove(gone), "ended: remove only")
        suite.expect(NotchClaudeSessionsSupport.backgroundPhase(gone, text: text) == text.backgroundStopped, "says stopped")
        suite.expect(NotchClaudeSessionsSupport.backgroundPhase(session(.idle, .failed, ended: true), text: text) == text.backgroundFailed, "says failed")

        let asking = session(.idle, .blocked)
        suite.expect(NotchClaudeSessionsSupport.backgroundPhase(asking, text: text) == text.backgroundNeedsReply, "blocked with no dialog waits for a reply")
        suite.expect(NotchClaudeSessionsSupport.canReply(asking), "and can get one")
        var dialog = session(.waitingPermission, .blocked)
        suite.expect(NotchClaudeSessionsSupport.backgroundPhase(dialog, text: text) == nil, "a permission says so itself")
        suite.expect(!NotchClaudeSessionsSupport.canReply(dialog), "keys never go into a dialog")
        dialog.phase = .idle
        dialog.waitingFor = "dialog open"
        suite.expect(!NotchClaudeSessionsSupport.canReply(dialog), "nor one the registry alone shows")

        var named = session(.idle, .done)
        named.name = "clarify-x-message"
        named.nameSource = "peer"
        named.title = "Some title"
        suite.expect(NotchClaudeSessionsSupport.name(named, custom: nil) == "clarify-x-message", "Agent View's name")
        suite.expect(NotchClaudeSessionsSupport.name(named, custom: "Mine") == "Mine", "the person's own name wins")
    }

    static func switches(_ suite: TestSuite) {
        suite.expect(ClaudeBackgroundJobs.settingsDisable(json(#"{"disableAgentView":true}"#)), "the setting")
        suite.expect(!ClaudeBackgroundJobs.settingsDisable(json(#"{"disableAgentView":false}"#)), "the setting off")
        suite.expect(ClaudeBackgroundJobs.settingsDisable(json(#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":"1"}}"#)), "the variable in settings")
        suite.expect(ClaudeBackgroundJobs.settingsDisable(json(#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":1}}"#)), "a number there too")
        suite.expect(!ClaudeBackgroundJobs.settingsDisable(json(#"{"env":{"CLAUDE_CODE_DISABLE_AGENT_VIEW":"0"}}"#)), "the variable off")
        suite.expect(!ClaudeBackgroundJobs.settingsDisable(json("not json")), "broken settings turn nothing off")
        let clean = ClaudeCLI.cleaned(["PATH": "/bin", "CLAUDECODE": "1", "CLAUDE_CODE_SESSION_ID": "x", "__CFBundleIdentifier": "app",
                                       ClaudeCLI.disableVariable: "1", ClaudeConfigHomes.variable: "/c"])
        suite.expect(clean == ["PATH": "/bin", ClaudeCLI.disableVariable: "1", ClaudeConfigHomes.variable: "/c"], "only the person's switches stay: \(clean)")
        let parsed = ClaudeCLI.parseEnvironment(Data("Welcome back\nPATH=/a:/b\0EMPTY=\0B=c=d\0nonsense\0".utf8))
        suite.expect(parsed == ["EMPTY": "", "B": "c=d"], "a profile's greeting costs the first variable, nothing else: \(parsed)")
    }

    static func screen(_ suite: TestSuite) {
        // Bytes as `claude logs` printed them.
        let sample = "\u{1B}7\u{1B}8\u{1B}[2J\u{1B}[H\u{1B}[H\r\u{1B}[1B\u{1B}[91m X\u{1B}[12G\u{1B}[39m\u{1B}[1mClaude Code\u{1B}[24G\u{1B}[22mv2.1.316\r\u{1B}[3B\u{1B}[100m❯ \u{1B}[97mSay hello\u{1B}[39m      \r\u{1B}[38B\u{1B}[49m·\u{1B}[3GBaking… \r\u{1B}[5Cend"
        let lines = ClaudeTerminalScreen.text(Data(sample.utf8)).components(separatedBy: "\n")
        suite.expect(lines == [" X         Claude Code v2.1.316", "", "❯ Say hello", "", "· Bakend…"], "placed where the cursor put it: \(lines)")
        suite.expect(ClaudeTerminalScreen.text(Data()) == "", "nothing")
        suite.expect(!ClaudeTerminalScreen.isReadable("⏺"), "a spinner alone isn't output to read")
        suite.expect(ClaudeTerminalScreen.isReadable("Claude Code\n\n❯ hi\n⏺ hello"), "a screen is")
        suite.expect(ClaudeTerminalScreen.text(Data("\u{1B}]0;title\u{07}a\u{1B}[?25lb\u{1B}[".utf8)) == "ab", "titles, modes and a cut-off code leave no marks")
        suite.expect(ClaudeTerminalScreen.text(Data("old\r\u{1B}[Knew\n\u{1B}[2Jx".utf8)).hasSuffix("x"), "clearing clears")
        suite.expect(ClaudeTerminalScreen.text(Data("abc\u{1B}[1G\u{1B}[Kz".utf8)) == "z", "clearing a line")
        suite.expect(ClaudeTerminalScreen.text(Data("\u{1B}[99A\u{1B}[99Dq".utf8)) == "q", "moves stop at the edge")
        suite.expect(ClaudeTerminalScreen.text(Data("e\u{301}x".utf8)).unicodeScalars.count == 2, "a combining mark takes no cell")
    }

    static func lines(_ suite: TestSuite) {
        let home = URL(fileURLWithPath: "/Users/me")
        suite.expect(ClaudeCLI.quoted("it's") == #"'it'\''s'"#, "a quote in a word")
        suite.expect(ClaudeCLI.line(executable: "/b/claude", arguments: ["attach", "j1"], configHome: "/Users/me/.claude", folder: nil, home: home)
                     == "'/b/claude' 'attach' 'j1'", "the standard config folder needs no variable")
        suite.expect(ClaudeCLI.line(executable: "/b/claude", arguments: ["--", "a $(x) 'b'"], configHome: "/Users/me/.claude-2", folder: "/t/my dir", home: home)
                     == #"cd '/t/my dir' && CLAUDE_CONFIG_DIR='/Users/me/.claude-2' '/b/claude' '--' 'a $(x) '\''b'\'''"#, "nothing in a prompt or path runs")
        suite.expect(ClaudeCLI.startArguments(prompt: "-x", background: true) == ["--bg", "--", "-x"], "a dashed prompt stays a prompt")
        suite.expect(ClaudeCLI.startArguments(prompt: "", background: true) == ["--bg"], "no prompt")
        suite.expect(ClaudeCLI.startArguments(prompt: nil, background: false) == [], "a terminal, no prompt")
        let env = ClaudeCLI.environment(["PATH": "/b", ClaudeConfigHomes.variable: "/other"], configHome: "/Users/me/.claude", home: home)
        suite.expect(env == ["PATH": "/b"], "the standard folder drops another's variable")
        let moved = ClaudeCLI.environment(["PATH": "/b"], configHome: "/c", home: home)
        suite.expect(moved[ClaudeConfigHomes.variable] == "/c", "a moved folder sets it")
        let found = ClaudeCLI.locate(path: "/nope:/yes", home: home) { $0 == "/yes/claude" || $0 == "/Users/me/.local/bin/claude" }
        suite.expect(found == "/yes/claude", "the PATH first")
        suite.expect(ClaudeCLI.locate(path: nil, home: home) { $0 == "/Users/me/.local/bin/claude" } == "/Users/me/.local/bin/claude", "then the installers' places")
        suite.expect(ClaudeCLI.locate(path: "/nope", home: home) { _ in false } == nil, "or none")
        suite.expect(ClaudeTerminalLauncher.script("x").hasPrefix("#!/bin/zsh -il\nrm -f -- \"$0\"\nx"), "a script removes itself first")
        var inGhostty = NotchSessionAgentTests.session("a", .idle, provider: .claude)
        inGhostty.host = .app("com.mitchellh.ghostty")
        inGhostty.lastActive = Date(timeIntervalSinceReferenceDate: 9)
        var inTerminal = NotchSessionAgentTests.session("b", .idle, provider: .claude)
        inTerminal.lastActive = Date(timeIntervalSinceReferenceDate: 1)
        var inIDE = inTerminal
        inIDE.host = .ide(bundle: ClaudeSessionHosts.antigravity, port: nil)
        inIDE.lastActive = Date(timeIntervalSinceReferenceDate: 20)
        suite.expect(ClaudeTerminalLauncher.preferred(from: [inTerminal, inGhostty, inIDE]) == "com.mitchellh.ghostty", "the terminal used last, an IDE being none")
        suite.expect(ClaudeTerminalLauncher.preferred(from: [inIDE]) == nil, "none: Terminal")
    }

    static func folders(_ suite: TestSuite) {
        let user = URL(fileURLWithPath: "/Users/me")
        suite.expect(ClaudeRecentFolders.configFile(home: user.appending(path: ".claude"), user: user).path == "/Users/me/.claude.json", "the standard folder's")
        suite.expect(ClaudeRecentFolders.configFile(home: user.appending(path: ".claude-2"), user: user).path == "/Users/me/.claude-2/.claude.json", "a moved one's")
        suite.expect(ClaudeRecentFolders.transcriptFolder("/private/tmp/vs-bg_spike.x/one") == "-private-tmp-vs-bg-spike-x-one", "as Claude Code names it")
        suite.expect(ClaudeRecentFolders.projects(in: json(#"{"projects":{"/a":{},"/b":{}}}"#)).sorted() == ["/a", "/b"], "the projects")
        suite.expect(ClaudeRecentFolders.projects(in: json("nope")).isEmpty, "a broken file has none")

        let root = FileManager.default.temporaryDirectory.appending(path: "vs-recent-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let home = root.appending(path: ".claude")
        let a = root.appending(path: "a").path, b = root.appending(path: "b").path
        for folder in [a, b] { try? FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true) }
        try? FileManager.default.createDirectory(at: home.appending(path: "projects"), withIntermediateDirectories: true)
        for (path, age) in [(a, 100.0), (b, 10.0)] {
            let transcripts = home.appending(path: "projects").appending(path: ClaudeRecentFolders.transcriptFolder(path))
            try? FileManager.default.createDirectory(at: transcripts, withIntermediateDirectories: true)
            try? FileManager.default.setAttributes([.modificationDate: Date().addingTimeInterval(-age)], ofItemAtPath: transcripts.path)
        }
        let config = ["projects": [a: [:], b: [:], root.appending(path: "gone").path: [:]] as [String: [String: String]]]
        FileManager.default.createFile(atPath: root.appending(path: ".claude.json").path,
                                       contents: try? JSONSerialization.data(withJSONObject: config))
        let read = ClaudeRecentFolders.read(homes: [home, root.appending(path: ".missing")], user: root)
        suite.expect(read.map(\.path) == [b, a], "the last worked in first, a gone folder left out: \(read.map(\.path))")
        suite.expect(read.first?.configHome == home.standardizedFileURL.path, "and which config folder it belongs to")
        let many = (0..<20).map { ClaudeRecentFolder(path: "/p\($0)", configHome: nil, used: nil) }
        suite.expect(ClaudeRecentFolders.ordered(many).count == ClaudeRecentFolders.shownAtMost, "only a few")
    }

    static func keys(_ suite: TestSuite) {
        func text(_ data: Data) -> String { String(decoding: data, as: UTF8.self) }
        let plain = ClaudeBackgroundReply.keystrokes("one\ntwo", images: [])
        suite.expect(plain.map { text($0.data) } == ["\u{1B}[200~one\ntwo\u{1B}[201~", "\r"], "a paste keeps line breaks, then Return alone")
        let both = ClaudeBackgroundReply.keystrokes("look", images: ["/a.png", "/b.png"])
        suite.expect(both.map { text($0.data) } == ["\u{1B}[200~/a.png\u{1B}[201~", "\u{1B}[200~/b.png\u{1B}[201~", "\u{1B}[200~look\u{1B}[201~", "\r"], "images first")
        suite.expect(both[1].pause == ClaudeTerminalScripts.imageSettle(2), "with time to read them")
        let only = ClaudeBackgroundReply.keystrokes("", images: ["/a.png"])
        suite.expect(only.count == 2, "an image alone")
    }

    static func models(_ suite: TestSuite) {
        // Claude Code
        suite.expect(ClaudeCLI.startArguments(prompt: "-x", background: true, model: "opus") == ["--bg", "--model", "opus", "--", "-x"],
                     "the model goes before the prompt")
        suite.expect(ClaudeCLI.startArguments(prompt: nil, background: false, model: "") == [], "an empty model is the default")
        let claude = NewSessionModels.claude(defaultName: "Default", current: "opus[1m]")
        suite.expect(claude.map(\.id) == [nil, "fable", "opus", "sonnet", "haiku"], "the default, then the aliases")
        suite.expect(claude.first?.detail == "Opus 5.5 (1M)", "the default names the exact model the settings pick")
        suite.expect(claude.dropFirst().map(\.name) == ["Fable 5.2", "Opus 5.5", "Sonnet 5.5", "Haiku 5.5"],
                     "without Claude Code's catalog, the bundled versions")
        suite.expect(NewSessionModels.claude(defaultName: "Default").first?.detail == nil, "no setting, no name")
        suite.expect(NewSessionModels.claudeName("claude-fable-5-1") == "Fable 5.1", "a full name reads with its version")
        suite.expect(NewSessionModels.claudeName("claude-opus-5-5[1m]") == "Opus 5.5 (1M)", "a full name with a 1M context")
        suite.expect(NewSessionModels.claudeName("my-proxy-model") == "my-proxy-model", "an unknown name stays")

        // Claude Code's cached model catalog
        let older = json(#"{"version":2,"fetchedAt":1,"catalog":{"config":{"models":[{"id":"claude-opus-5","name":"Opus 5"}],"provider_alias_targets":{"opus":{"default":"claude-opus-5"}}},"state":{"model":"claude-opus-5"}}}"#)
        let newer = json(#"{"version":2,"fetchedAt":2,"catalog":{"config":{"models":[{"id":"claude-opus-6","name":"Opus 6"},{"id":"claude-fable-6-1","name":"Fable 6.1 Preview"}],"provider_alias_targets":{"opus":{"default":"claude-opus-6","per_provider":{"gateway":"claude-opus-5"}},"fable":{"default":"claude-fable-6-1"},"sonnet":{"default":"claude-sonnet-6"}}},"state":{"model":"claude-fable-6-1"}}}"#)
        let catalog = NewSessionModels.claudeCatalog([older, newer, json("not json"), json(#"{"catalog":{}}"#)])
        suite.expect(catalog?.aliases["opus"] == "claude-opus-6", "the freshest catalog wins")
        let resolved = NewSessionModels.claude(defaultName: "Default", catalog: catalog)
        suite.expect(resolved.map(\.name) == ["Default", "Fable 6.1 Preview", "Opus 6", "Sonnet 6", "Haiku 5.5"],
                     "aliases name what the catalog points them to, read from the id when it has no name")
        suite.expect(resolved.first?.detail == "Fable 6.1 Preview", "no setting: the model Claude Code starts with")
        suite.expect(NewSessionModels.claude(defaultName: "Default", current: "default", catalog: catalog).first?.detail == "Fable 6.1 Preview",
                     "`default` in the settings is the account's")
        suite.expect(NewSessionModels.claude(defaultName: "Default", current: "sonnet", catalog: catalog).first?.detail == "Sonnet 6",
                     "the setting wins over the catalog's state")
        suite.expect(NewSessionModels.claudeCatalog([]) == nil && NewSessionModels.claudeCatalog([json("[]")]) == nil, "no catalog")
        suite.expect(NewSessionModels.claudeSetting(json(#"{"model":"sonnet","env":{}}"#)) == "sonnet", "settings name a model")
        suite.expect(NewSessionModels.claudeSetting(json(#"{"model":""}"#)) == nil, "an empty one is none")
        suite.expect(NewSessionModels.claudeSetting(json("not json")) == nil, "a broken file is none")

        // Codex's catalog, shaped as 0.164 writes models_cache.json
        let cache = json(#"{"fetched_at":"x","models":[{"slug":"gpt-b","display_name":"GPT B","visibility":"list","priority":2},{"slug":"hidden","display_name":"Hidden","visibility":"hide","priority":0},{"slug":"gpt-a","display_name":"GPT A","visibility":"list","priority":1},{"slug":"gpt-c","display_name":"","priority":2},{"slug":"gpt-a","display_name":"Again","visibility":"list","priority":9},{"display_name":"No slug","visibility":"list","priority":0}]}"#)
        let codexList = NewSessionModels.codexCatalog(cache)
        suite.expect(codexList.map(\.key) == ["gpt-a", "gpt-b", "gpt-c"], "listed ones in Codex's order, once each, hidden left out")
        suite.expect(codexList.last?.name == "gpt-c", "no display name shows the slug")
        suite.expect(NewSessionModels.codexCatalog(nil).isEmpty && NewSessionModels.codexCatalog(json("[]")).isEmpty, "no catalog, no models")
        let codex = NewSessionModels.codex(cache: cache, config: "model = \"gpt-b\"\n", defaultName: "Default")
        suite.expect(codex.first?.id == nil && codex.first?.detail == "GPT B", "the default names the configured model")
        suite.expect(NewSessionModels.codex(cache: cache, config: nil, defaultName: "Default").first?.detail == "GPT A",
                     "with nothing configured, Codex's first")
        suite.expect(NewSessionModels.codex(cache: nil, config: "model = 'gpt-z'", defaultName: "Default").map(\.detail) == ["gpt-z"],
                     "only the default when there is no catalog")

        // config.toml
        suite.expect(NewSessionModels.codexSetting("model = \"gpt-6.1-sol\"\nmodel_reasoning_effort = \"medium\"") == "gpt-6.1-sol", "the top-level model")
        suite.expect(NewSessionModels.codexSetting("model_reasoning_effort = \"high\"\n  model=\"m\" # mine") == "m", "spaces and a comment")
        suite.expect(NewSessionModels.codexSetting("[profiles.fast]\nmodel = \"p\"") == nil, "a profile's model isn't the default")
        suite.expect(NewSessionModels.codexSetting("model = gpt") == nil, "not a TOML string")
        suite.expect(NewSessionModels.codexSetting("model = \"\"") == nil, "an empty one")
        suite.expect(NewSessionModels.codexSetting("") == nil, "an empty file")

        // Codex's command line and thread
        let home = URL(fileURLWithPath: "/Users/me")
        suite.expect(CodexCLI.startArguments(prompt: "-x", model: "gpt-a") == ["-m", "gpt-a", "--", "-x"], "Codex's model and a dashed prompt")
        suite.expect(CodexCLI.startArguments(prompt: "", model: nil) == [], "Codex with nothing")
        suite.expect(CodexCLI.line(executable: "/b/codex", arguments: ["--", "a 'b'"], folder: "/t/my dir")
                     == #"cd '/t/my dir' && '/b/codex' '--' 'a '\''b'\'''"#, "nothing in a Codex prompt runs")
        suite.expect(CodexCLI.locate(path: "/x:/y", home: home) { $0 == "/y/codex" } == "/y/codex", "codex on the PATH")
        suite.expect(CodexCLI.locate(path: nil, home: home) { $0 == "/opt/homebrew/bin/codex" } == "/opt/homebrew/bin/codex", "or Homebrew's")
        suite.expect(CodexCLI.locate(path: "/x", home: home) { _ in false } == nil, "or none")
        let params = CodexSessionClient.threadStart(cwd: "/t", model: "gpt-a")
        suite.expect(params["cwd"] as? String == "/t" && params["model"] as? String == "gpt-a" && params.count == 2,
                     "a thread gets its folder and model, nothing that overrides the person's settings")
        suite.expect(CodexSessionClient.threadStart(cwd: "/t", model: nil).count == 1, "the default model sends none")
    }
    static func efforts(_ suite: TestSuite) {
        // Claude Code's catalog, shaped as 2.1.318 caches it
        let file = json(#"""
        {"fetchedAt":3,"catalog":{"config":{"models":[
          {"id":"claude-opus-5-5","name":"Opus 5.5","thinking":{"type":"effort","effort_options":[{"id":"low","name":"Low"},{"id":"medium","name":"Medium","badge":{"message":"Recommended","variant":"neutral"}},{"id":"high","name":"High"},{"id":"xhigh","name":"Extra"},{"id":"max","name":"Max","badge":{"message":"3.5× or more usage","variant":"warning"}}]},"runtime":{"default_effort":"medium"}},
          {"id":"claude-opus-4-6","name":"Opus 4.6","thinking":{"type":"effort","effort_options":[{"id":"low"},{"id":"high","name":"High","badge":{"message":"Recommended"}},{"id":"max","name":"Max"},{"id":"bad id; rm"},{"id":"low","name":"Again"}]}},
          {"id":"claude-haiku-4-5-20251001","name":"Haiku 4.5","thinking":{"type":"none"}},
          {"id":"claude-sonnet-5-5","name":"Sonnet 5.5"}],
          "provider_alias_targets":{"opus":{"default":"claude-opus-5-5"},"sonnet":{"default":"claude-sonnet-5-5"},"haiku":{"default":"claude-haiku-4-5-20251001"},"fable":{"default":"claude-opus-4-6"}}},
          "state":{"model":"claude-opus-4-6[1m]"}}}
        """#)
        let catalog = NewSessionModels.claudeCatalog([file])
        let opus = catalog?.efforts["claude-opus-5-5"] ?? []
        suite.expect(opus.map(\.id) == ["low", "medium", "high", "xhigh", "max"], "a model's levels in Claude Code's order")
        suite.expect(opus.map(\.name) == ["Low", "Medium", "High", "Extra", "Max"], "under Claude Code's names")
        suite.expect(opus[1].detail == "Recommended" && opus[4].detail == "3.5× or more usage" && opus[0].detail == nil,
                     "with Claude Code's own marks")
        suite.expect(catalog?.efforts["claude-opus-4-6"]?.map(\.id) == ["low", "high", "max"],
                     "only the levels the model takes, once each, nothing that isn't a plain word")
        suite.expect(catalog?.efforts["claude-opus-4-6"]?.first?.name == "Low", "a level with no name reads from its id")
        suite.expect(catalog?.efforts["claude-haiku-4-5-20251001"] == [], "a model that takes no effort has none")
        suite.expect(catalog?.efforts["claude-sonnet-5-5"] == nil, "a model the catalog says nothing about has none")
        suite.expect(catalog?.defaultEfforts["claude-opus-5-5"] == "medium" && catalog?.defaultEfforts["claude-opus-4-6"] == "high",
                     "the runtime default, else the Recommended one")

        let models = NewSessionModels.claude(defaultName: "Default", catalog: catalog)
        let byID = Dictionary(uniqueKeysWithValues: models.map { ($0.key, $0) })
        suite.expect(byID["opus"]?.efforts.count == 5 && byID["opus"]?.defaultEffort == "medium", "an alias gets its model's levels")
        suite.expect(byID["haiku"]?.efforts.isEmpty == true && byID["haiku"]?.defaultEffort == nil, "no levels, no Effort")
        suite.expect(byID["sonnet"]?.efforts.isEmpty == true, "nothing known, no Effort")
        suite.expect(byID[""]?.efforts.map(\.id) == ["low", "high", "max"] && byID[""]?.defaultEffort == "high",
                     "the default model, a full name with its context, gets that model's levels")
        suite.expect(NewSessionModels.claude(defaultName: "Default").allSatisfy { $0.efforts.isEmpty },
                     "without the catalog nothing is offered")

        // What the settings start a model at
        // As Claude Code 2.1.318 reads them: only per model, max never, nothing from a top-level effortLevel.
        let settings = NewSessionModels.claudeEffortSetting(json(#"{"effortLevel":"low","modelSettings":{"claude-opus-5-5":{"effortLevel":"xhigh"},"claude-opus-4-6":{"effortLevel":"xhigh"},"claude-sonnet-5-5":{"effortLevel":"max"},"x":{"effortLevel":"a b"}}}"#))
        suite.expect(settings.perModel == ["claude-opus-5-5": "xhigh", "claude-opus-4-6": "xhigh"], "one per model, low to xhigh")
        let set = NewSessionModels.claude(defaultName: "Default", catalog: catalog, effortSettings: settings)
        let setByID = Dictionary(uniqueKeysWithValues: set.map { ($0.key, $0) })
        suite.expect(setByID["opus"]?.defaultEffort == "xhigh", "the model's own setting first")
        suite.expect(setByID["fable"]?.defaultEffort == "high", "a level the model lacks leaves the model's default")
        suite.expect(NewSessionModels.claudeEffortSetting(json("not json")) == NewSessionModels.ClaudeEffortSettings(), "a broken file says nothing")

        // Codex's levels, shaped as 0.172 writes models_cache.json
        let cache = json(#"{"models":[{"slug":"gpt-a","display_name":"GPT A","visibility":"list","priority":1,"default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low","description":"Fast"},{"effort":"medium","description":"x"},{"effort":"xhigh","description":"Extra high reasoning"},{"effort":"ultra","description":"y"},{"effort":""}]},{"slug":"gpt-b","display_name":"GPT B","visibility":"list","priority":2,"default_reasoning_level":"medium","supported_reasoning_levels":["low","medium"]},{"slug":"gpt-c","visibility":"list","priority":3}]}"#)
        let codex = NewSessionModels.codex(cache: cache, config: nil, defaultName: "Default")
        let a = codex[1]
        suite.expect(a.efforts.map(\.id) == ["low", "medium", "xhigh", "ultra"], "Codex's levels for the model")
        suite.expect(a.efforts.map(\.name) == ["Low", "Medium", "Extra high", "Ultra"], "named as Codex's picker names them")
        suite.expect(a.efforts.first?.detail == "default" && a.defaultEffort == "low", "its default marked")
        suite.expect(codex[2].efforts.map(\.id) == ["low", "medium"] && codex[2].defaultEffort == "medium", "plain strings too")
        suite.expect(codex[3].efforts.isEmpty, "a model with no levels has none")
        suite.expect(codex[0].efforts == a.efforts && codex[0].defaultEffort == "low", "the default model is Codex's first")
        let configured = NewSessionModels.codex(cache: cache, config: "model = \"gpt-b\"\nmodel_reasoning_effort = \"low\"\n", defaultName: "Default")
        suite.expect(configured[0].efforts.map(\.id) == ["low", "medium"] && configured[0].defaultEffort == "low",
                     "the configured model and effort")
        let lacking = NewSessionModels.codex(cache: cache, config: "model_reasoning_effort = \"ultra\"", defaultName: "Default")
        suite.expect(lacking[1].defaultEffort == "ultra" && lacking[2].defaultEffort == "medium",
                     "a configured level the model lacks leaves the model's own")
        suite.expect(NewSessionModels.codexSetting("model_reasoning_effort = \"high\"\nmodel = \"m\"", key: "model_reasoning_effort") == "high",
                     "the configured effort")
        suite.expect(NewSessionModels.codexSetting("[profiles.x]\nmodel_reasoning_effort = \"high\"", key: "model_reasoning_effort") == nil,
                     "a profile's effort isn't the default")

        // How each agent takes it
        suite.expect(ClaudeCLI.startArguments(prompt: "go", background: true, model: "opus", effort: "high", worktree: "w",
                                              allowed: ["Bash(ls:*)"])
                     == ["--bg", "--model", "opus", "--effort", "high", "--worktree", "w", "--allowedTools", "Bash(ls:*)", "--", "go"],
                     "Claude Code's effort before the options that take several values")
        suite.expect(ClaudeCLI.startArguments(prompt: "go", background: false, effort: nil) == ["--", "go"], "the default adds nothing")
        suite.expect(ClaudeCLI.startArguments(prompt: nil, background: false, effort: "x; rm") == [], "nothing that isn't a level")
        suite.expect(CodexCLI.startArguments(prompt: "go", model: "gpt-a", effort: "xhigh")
                     == ["-m", "gpt-a", "-c", "model_reasoning_effort=xhigh", "--", "go"], "Codex's effort as a setting for the run")
        suite.expect(CodexCLI.startArguments(prompt: "go", model: nil, effort: "") == ["--", "go"], "Codex's default adds nothing")
        let input: [[String: Any]] = [["type": "text", "text": "go"]]
        let first = CodexSessionClient.firstTurn(thread: "t1", input: input, effort: "ultra")
        suite.expect(first["threadId"] as? String == "t1" && first["effort"] as? String == "ultra" && first.count == 3,
                     "a background thread's first turn carries the effort")
        suite.expect(CodexSessionClient.firstTurn(thread: "t1", input: input, effort: nil).count == 2, "the default sends none")
        suite.expect(CodexSessionClient.firstTurn(thread: "t1", input: input, effort: "a b").count == 2, "nor anything that isn't a level")

        suite.expect(NewSessionChoices.effortKey(agent: "claude", model: "") == "claude|", "the default model keeps its own")
    }
}
