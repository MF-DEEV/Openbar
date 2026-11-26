// SPDX-License-Identifier: GPL-3.0-or-later
import Foundation

/// Safe commands and per-project choices for New session (#270).
enum SafeCommandsTests {
    static let casesPath = "windows/src-tauri/src/sessions/safe_command_cases.json"

    static func run(_ suite: TestSuite) {
        suite.run("safe commands: shared cases") {
            guard let data = FileManager.default.contents(atPath: casesPath),
                  let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let allowed = json["allowed"] as? [String],
                  let matches = json["matches"] as? [[String: Any]],
                  let cleaned = json["cleaned"] as? [[String: Any]] else {
                suite.expect(false, "safe command cases readable at \(casesPath)")
                return
            }
            for item in matches {
                let command = item["command"] as? String ?? ""
                let expect = item["expect"] as? Bool ?? false
                suite.expect(SafeCommands.matches(command, allowed: allowed) == expect,
                             "`\(command)` \(expect ? "is" : "is not") on the list")
            }
            for item in cleaned {
                let text = item["text"] as? String ?? ""
                suite.expect(SafeCommands.cleaned(text) == item["expect"] as? String,
                             "`\(text)` cleans to \(item["expect"] ?? "nil")")
            }
        }

        suite.run("safe commands: Claude rules and arguments") {
            suite.expect(SafeCommands.claudeRules(["git status", "a; b", "npm run test"])
                         == ["Bash(git status:*)", "Bash(npm run test:*)"], "one prefix rule per clean command")
            suite.expect(ClaudeCLI.startArguments(prompt: "go", background: true, allowed: ["Bash(git diff:*)"])
                         == ["--bg", "--allowedTools", "Bash(git diff:*)", "--", "go"], "the rules come before the prompt")
            suite.expect(ClaudeCLI.startArguments(prompt: nil, background: false, allowed: [])
                         == [], "no rules, no flag")
            suite.expect(SafeCommands.supports(.claude) && SafeCommands.supports(.codex) && !SafeCommands.supports(.gemini),
                         "only agents with a session allow list offer it")
        }

        suite.run("safe commands: suggested by the project") {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("safe-\(UUID().uuidString)")
            defer { try? FileManager.default.removeItem(at: root) }
            try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            let package = #"{"scripts":{"build":"tsc","test":"vitest","lint":"eslint .","dev":"vite"}}"#
            try? package.write(to: root.appendingPathComponent("package.json"), atomically: true, encoding: .utf8)
            try? "".write(to: root.appendingPathComponent("pnpm-lock.yaml"), atomically: true, encoding: .utf8)
            try? "[package]".write(to: root.appendingPathComponent("Cargo.toml"), atomically: true, encoding: .utf8)
            let found = SafeCommands.suggested(for: root.path).map(\.command)
            suite.expect(found == ["pnpm run build", "pnpm run test", "pnpm run lint", "cargo build", "cargo test",
                                   "cargo clippy", "git status", "git diff", "git log"], "scripts, Cargo and git: \(found)")
            let empty = SafeCommands.suggested(for: root.appendingPathComponent("missing").path).map(\.command)
            suite.expect(empty == ["git status", "git diff", "git log"], "a bare folder suggests git's three")
            let make = root.appendingPathComponent("m")
            try? FileManager.default.createDirectory(at: make, withIntermediateDirectories: true)
            try? "all:\n\tcc x\ntest: all\n\t./t\n".write(to: make.appendingPathComponent("Makefile"), atomically: true, encoding: .utf8)
            suite.expect(SafeCommands.suggested(for: make.path).map(\.command).prefix(2) == ["make", "make test"],
                         "make and its test target")
        }

        suite.run("safe commands: Codex grants") {
            var grants = SafeCommandGrants()
            let now = Date(timeIntervalSince1970: 1_800_000_000)
            grants.grant("t1", ["git diff"])
            suite.expect(grants.commands(for: "t1", cwd: "/w", now: now) == ["git diff"], "a thread keeps its own list")
            grants.expect(cwd: "/w/app/", commands: ["make"], now: now, known: ["old"])
            suite.expect(grants.commands(for: "old", cwd: "/w/app", now: now) == nil, "a thread from before doesn't claim it")
            suite.expect(grants.commands(for: "t2", cwd: "/w/other", now: now) == nil, "nor one in another folder")
            suite.expect(grants.commands(for: "t3", cwd: "/w/app", now: now.addingTimeInterval(SafeCommandGrants.claimWindow + 1)) == nil,
                         "nor a new one after the window")
            grants.expect(cwd: "/w/app", commands: ["make"], now: now, known: [])
            suite.expect(grants.commands(for: "t4", cwd: "/w/app", now: now.addingTimeInterval(5)) == ["make"],
                         "the first new thread there claims it")
            suite.expect(grants.commands(for: "t5", cwd: "/w/app", now: now.addingTimeInterval(6)) == nil, "only once")
        }

        suite.run("safe commands: a Codex thread's approvals") { codexClient(suite) }

        suite.run("new session choices: per project, the oldest dropped") {
            let defaults = UserDefaults(suiteName: "com.openbar.tests.new-session")!
            defer { defaults.removePersistentDomain(forName: "com.openbar.tests.new-session") }
            defaults.removePersistentDomain(forName: "com.openbar.tests.new-session")
            let start = Date(timeIntervalSince1970: 1_800_000_000)
            var choices = NewSessionChoices(agent: "codex", models: ["codex": "gpt-x"], worktree: false, background: false,
                                            safe: true, safeOff: ["git log"], safeCustom: ["make check"])
            NewSessionChoiceStore.save(choices, for: "/w/a", now: start, defaults)
            let read = NewSessionChoiceStore.load("/w/a", defaults)
            suite.expect(read?.agent == "codex" && read?.safeCustom == ["make check"] && read?.worktree == false
                         && read?.used == start, "a project's choices come back")
            suite.expect(NewSessionChoiceStore.load("/w/b", defaults) == nil, "another project has none")
            choices.agent = "claude"
            for index in 0..<NewSessionChoiceStore.limit {
                NewSessionChoiceStore.save(choices, for: "/w/p\(index)", now: start.addingTimeInterval(Double(index + 1)), defaults)
            }
            suite.expect(NewSessionChoiceStore.all(defaults).count == NewSessionChoiceStore.limit
                         && NewSessionChoiceStore.load("/w/a", defaults) == nil, "past the limit the least recent goes")
            let old = #"{"/w/x":{"agent":"claude"}}"#.data(using: .utf8)!
            defaults.set(old, forKey: DefaultsKey.notchNewSessionByProject)
            suite.expect(NewSessionChoiceStore.load("/w/x", defaults)?.safe == nil, "an older entry reads its missing fields as unset")
            suite.expect(NewSessionChoiceStore.load("/w/x", defaults)?.efforts == nil, "and has no efforts (#289)")
            let effortKey = NewSessionChoices.effortKey(agent: "codex", model: "gpt-x")
            NewSessionChoiceStore.save(NewSessionChoices(agent: "codex", efforts: [effortKey: "xhigh"]), for: "/w/e", now: start, defaults)
            suite.expect(NewSessionChoiceStore.load("/w/e", defaults)?.efforts?[effortKey] == "xhigh", "an effort comes back per agent and model")
            suite.expect(NewSessionChoiceStore.lastEfforts(defaults).isEmpty, "no effort remembered yet")
            NewSessionChoiceStore.rememberEffort("high", for: "claude|opus", defaults)
            NewSessionChoiceStore.rememberEffort("", for: "claude|", defaults)
            NewSessionChoiceStore.rememberEffort("low", for: "claude|opus", defaults)
            suite.expect(NewSessionChoiceStore.lastEfforts(defaults) == ["claude|opus": "low", "claude|": ""],
                         "the last effort of all projects, per agent and model")
        }
    }

    /// A terminal's Codex thread claims the list; its matching command is
    /// answered for the session, anything else waits on the island.
    static func codexClient(_ suite: TestSuite) {
        guard let daemon = CodexSessionTests.FakeDaemon() else { return suite.expect(false, "temporary home made") }
        defer { daemon.finish() }
        suite.expect(daemon.listen(), "the fake daemon listens")
        let client = CodexSessionClient(home: daemon.home, clientVersion: "9.9", retry: 0.1)
        client.auto = SessionAutoMode.Box()
        client.letThroughLog = SessionAutoMode.Log()
        client.expectSafeCommands(cwd: "/Users/me/code/app", commands: ["git diff", "npm run test"])
        client.start {}
        defer { client.stop() }
        guard daemon.accept() else { return suite.expect(false, "the island connects") }
        func next(_ method: String) -> [String: Any]? {
            for _ in 0..<6 {
                guard let message = daemon.message() else { return nil }
                if message["method"] as? String == method { return message }
            }
            return nil
        }
        let initialize = next("initialize")
        daemon.send(["id": initialize?["id"] ?? 0, "result": ["userAgent": "codex_cli_rs/0.164.0"]])
        let list = next("thread/loaded/list")
        daemon.send(["id": list?["id"] ?? 0, "result": ["data": ["t1"]]])
        let read = next("thread/read")
        let thread: [String: Any] = ["id": "t1", "cwd": "/Users/me/code/app", "status": ["type": "active", "activeFlags": []]]
        daemon.send(["id": read?["id"] ?? 0, "result": ["thread": thread]])
        let resume = next("thread/resume")
        daemon.send(["id": resume?["id"] ?? 0, "result": ["thread": thread, "approvalPolicy": "on-request"]])
        daemon.send(["id": 7, "method": "item/commandExecution/requestApproval",
                     "params": ["threadId": "t1", "turnId": "u1", "itemId": "i1",
                                "command": "/bin/zsh -lc 'git diff --stat'", "cwd": "/Users/me/code/app"]])
        var answer: [String: Any]?
        for _ in 0..<4 {
            guard let message = daemon.message() else { break }
            if message["method"] == nil, message["id"] as? Int == 7 { answer = message; break }
        }
        suite.expect((answer?["result"] as? [String: Any])?["decision"] as? String == "acceptForSession",
                     "a safe command is accepted for the session: \(CodexSessionTests.json(answer))")
        daemon.send(["id": 8, "method": "item/commandExecution/requestApproval",
                     "params": ["threadId": "t1", "turnId": "u1", "itemId": "i2",
                                "command": "/bin/zsh -lc 'git diff; rm -rf ~'", "cwd": "/Users/me/code/app"]])
        suite.expect(CodexSessionTests.eventually { client.waits("t1", requestID: "codex:8") },
                     "a chained one waits on the island")
    }
}
