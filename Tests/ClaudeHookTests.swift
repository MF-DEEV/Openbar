// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

enum ClaudeHookTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions hook messages") { messages(suite) }
        suite.run("sessions hook server") { server(suite) }
        suite.run("sessions hook installer") { installer(suite) }
        suite.run("sessions replies") { replies(suite) }
    }

    static let now = Date(timeIntervalSince1970: 1_792_600_000)

    static func decode(_ json: String, previews: Bool = false) -> ClaudeHookMessage? {
        ClaudeHookMessage.decode(Data(json.utf8), requestID: "r1", now: now, policy: ClaudeContentPolicy(previews: previews))
    }

    // MARK: Messages

    static func messages(_ suite: TestSuite) {
        let base = #""session_id":"s1","transcript_path":"/x/s1.jsonl","cwd":"/a""#
        let permission = decode(#"{\#(base),"hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"rm -rf build","description":"Clean"},"permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash"}]}]}"#)
        suite.expect(permission?.isPermissionRequest == true && permission?.transcript == "/x/s1.jsonl",
                     "a permission request is recognized with its transcript")
        if case .permissionRequested(let ask)? = permission?.fact.event {
            suite.expect(ask.tool == "Bash" && ask.summary == "rm -rf build" && ask.offersAlways && ask.requestID == "r1",
                         "it shows the command even with previews off, and offers Always when Claude Code does")
        } else {
            suite.expect(false, "a permission request decodes as one")
        }

        let pre = decode(#"{\#(base),"hook_event_name":"PreToolUse","tool_name":"Edit","tool_use_id":"t1","tool_input":{"file_path":"/a/b.swift"}}"#)
        suite.expect(pre?.fact.event == .toolStarted(ClaudeToolUse(id: "t1", name: "Edit", summary: nil, started: now)),
                     "a tool start keeps its name but not its file while previews are off")
        let shown = decode(#"{\#(base),"hook_event_name":"PreToolUse","tool_name":"Edit","tool_use_id":"t1","tool_input":{"file_path":"/a/b.swift"}}"#,
                           previews: true)
        suite.expect(shown?.fact.event == .toolStarted(ClaudeToolUse(id: "t1", name: "Edit", summary: "b.swift", started: now)),
                     "and its file with previews on")
        suite.expect(decode(#"{\#(base),"hook_event_name":"PostToolUseFailure","tool_use_id":"t1"}"#)?.fact.event
                     == .toolFinished(id: "t1", failed: true), "a failed tool finishes as failed")
        suite.expect(decode(#"{\#(base),"hook_event_name":"Stop","last_assistant_message":"SECRET"}"#)?.fact.event
                     == .stopped(lastText: nil), "a stop drops the reply while previews are off")
        suite.expect(decode(#"{\#(base),"hook_event_name":"Stop","last_assistant_message":"  Done.  "}"#, previews: true)?.fact.event
                     == .stopped(lastText: "Done."), "and keeps it trimmed with previews on")
        let start = decode(#"{\#(base),"hook_event_name":"SubagentStart","agent_id":"a1","agent_type":"Explore"}"#)
        suite.expect(start?.fact.event == .subagentStarted(id: "a1", type: "Explore") && start?.fact.agentID == nil,
                     "a subagent starting is about that subagent, not from it")
        let inside = decode(#"{\#(base),"hook_event_name":"PreToolUse","tool_name":"Grep","tool_use_id":"t2","agent_id":"a1"}"#)
        suite.expect(inside?.fact.agentID == "a1", "a tool a subagent runs is marked as its own")
        suite.expect(decode(#"{\#(base),"hook_event_name":"Notification","notification_type":"idle_prompt"}"#)?.fact.event
                     == .notification(type: "idle_prompt"), "notifications keep their type")
        suite.expect(decode(#"{"hook_event_name":"Stop"}"#) == nil && decode("not json") == nil,
                     "input without a session or not JSON is ignored")

        func json(_ data: Data) -> [String: Any] {
            ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["hookSpecificOutput"] as? [String: Any] ?? [:]
        }
        let allow = json(ClaudeHookMessage.reply(.allow, suggestions: nil))
        suite.expect(allow["hookEventName"] as? String == "PermissionRequest"
                     && (allow["decision"] as? [String: Any])?["behavior"] as? String == "allow", "allow answers in Claude Code's shape")
        let deny = (json(ClaudeHookMessage.reply(.deny, suggestions: nil))["decision"] as? [String: Any]) ?? [:]
        suite.expect(deny["behavior"] as? String == "deny" && deny["message"] as? String == ClaudeHookMessage.denyMessage,
                     "deny tells Claude where it was denied")
        let always = (json(ClaudeHookMessage.reply(.allowAlways, suggestions: [["type": "addRules"]]))["decision"] as? [String: Any]) ?? [:]
        suite.expect((always["updatedPermissions"] as? [[String: Any]])?.first?["type"] as? String == "addRules",
                     "always hands back the rules Claude Code offered")

        suite.expect(ClaudeHookMessage.isComplete(Data(#"{"a":{"b":1}}  "#.utf8)), "a whole object is complete")
        suite.expect(!ClaudeHookMessage.isComplete(Data(#"{"a":{"b":1}"#.utf8))
                     && !ClaudeHookMessage.isComplete(Data(#"{"a":"}"#.utf8)), "a cut one is not")

        suite.expect(NotchClaudeSessionsSupport.decision(key: "y", commandOnly: true) == .allow
                     && NotchClaudeSessionsSupport.decision(key: "N", commandOnly: true) == .deny
                     && NotchClaudeSessionsSupport.decision(key: "y", commandOnly: false) == nil
                     && NotchClaudeSessionsSupport.decision(key: "k", commandOnly: true) == nil,
                     "⌘Y allows and ⌘N denies, nothing else")
    }

    // MARK: Server

    /// Runs the hook command Claude Code would, against the server.
    static func hook(_ socket: String, input: String, timeout: TimeInterval = 5) -> (output: String, status: Int32) {
        let command = ClaudeHookInstaller.command(socket: socket, bundle: "com.vorssaint.tests")
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", command]
        let stdin = Pipe(), stdout = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return ("", -1) }
        stdin.fileHandleForWriting.write(Data(input.utf8))
        try? stdin.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning, Date() < deadline { usleep(10_000) }
        if process.isRunning { process.terminate(); process.waitUntilExit() }
        let output = String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        return (output, process.terminationStatus)
    }

    static func server(_ suite: TestSuite) {
        // Socket paths are short; the temporary folder of a test run is not.
        let folder = "/tmp/vs-hook-\(getpid())"
        let path = folder + "/hooks.sock"
        defer { try? FileManager.default.removeItem(atPath: folder) }
        let queue = DispatchQueue(label: "test.claude-hooks")
        let lock = NSLock()
        var received: [String] = []
        var held: ClaudeHookServer.Connection?
        let server = ClaudeHookServer(path: path, queue: queue) { payload, connection in
            let message = ClaudeHookMessage.decode(payload, requestID: "r", now: now, policy: ClaudeContentPolicy())
            lock.withLock { received.append(String(decoding: payload, as: UTF8.self)) }
            guard let message, message.isPermissionRequest else { return connection.finish() }
            held = connection
            // The person answers a moment later.
            queue.asyncAfter(deadline: .now() + 0.3) {
                connection.finish(ClaudeHookMessage.reply(.allow, suggestions: nil))
            }
        }
        suite.expect(queue.sync { server.start() }, "the hook server listens")
        var info = stat()
        suite.expect(stat(path, &info) == 0 && info.st_mode & 0o077 == 0, "only its owner can reach the socket")

        let event = hook(path, input: #"{"session_id":"s1","hook_event_name":"PreToolUse","tool_use_id":"t1","tool_name":"Bash"}"#)
        suite.expect(event.status == 0 && event.output.isEmpty, "an event is taken and the hook prints nothing")
        let asked = hook(path, input: #"{"session_id":"s1","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"ls"}}"#)
        let decided = ((try? JSONSerialization.jsonObject(with: Data(asked.output.utf8))) as? [String: Any])?["hookSpecificOutput"]
            as? [String: Any]
        suite.expect(asked.status == 0 && (decided?["decision"] as? [String: Any])?["behavior"] as? String == "allow",
                     "a permission waits and prints the island's answer for Claude Code: \(asked.output)")
        suite.expect(lock.withLock { received.count } == 2, "both events arrived whole")
        suite.expect(queue.sync { held?.isClosed } == true, "the answered connection is closed")

        // A hook whose answer never comes still ends when Claude Code kills it.
        let late = ClaudeHookServer(path: folder + "/late.sock", queue: queue) { _, connection in
            held = connection
        }
        _ = queue.sync { late.start() }
        let killed = hook(folder + "/late.sock",
                          input: #"{"session_id":"s1","hook_event_name":"PermissionRequest","tool_name":"Bash"}"#, timeout: 0.5)
        suite.expect(killed.output.isEmpty, "an unanswered hook prints nothing")
        usleep(100_000)
        suite.expect(queue.sync { held?.peerIsGone } == true, "and the server can tell it went away")
        queue.sync { held?.finish() }

        queue.sync { server.stop(); late.stop() }
        suite.expect(!FileManager.default.fileExists(atPath: path), "stopping removes the socket")
        let missing = hook(path, input: #"{"session_id":"s1","hook_event_name":"Stop"}"#)
        suite.expect(missing.status == 0 && missing.output.isEmpty, "without the app the hook ends quietly and succeeds")
        suite.expect(!ClaudeHookServer.fits("/" + String(repeating: "x", count: 120)), "too long a path is refused")
    }

    // MARK: Installer

    static func installer(_ suite: TestSuite) {
        let folder = FileManager.default.temporaryDirectory.appending(path: "claude-settings-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let url = folder.appending(path: "settings.json")
        let own = #"{"model":"opus","hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"my-guard.sh"}]}]},"permissions":{"allow":["Bash(ls)"]}}"#
        try? Data(own.utf8).write(to: url)
        let bundle = "com.vorssaint.tests"
        let command = ClaudeHookInstaller.command(socket: "/Users/me/Library/Application Support/x/claude hooks.sock", bundle: bundle)
        suite.expect(command.contains("'/Users/me/Library/Application Support/x/claude hooks.sock'")
                     && command.hasSuffix(ClaudeHookInstaller.marker(bundle: bundle)), "the socket path is quoted and the entry marked")
        suite.expect(ClaudeHookInstaller.command(socket: "/a'b", bundle: bundle).contains(#"'/a'\''b'"#), "quotes in the path are escaped")

        let install = { (settings: [String: Any]) in ClaudeHookInstaller.installing(into: settings, command: command, bundle: bundle) }
        do { try ClaudeHookInstaller.update(url, now: now, install) } catch { suite.expect(false, "installing failed: \(error)") }
        let installed = (try? ClaudeHookInstaller.read(url)) ?? [:]
        suite.expect(ClaudeHookInstaller.isInstalled(in: installed, command: command), "every event gets the hook")
        let hooks = installed["hooks"] as? [String: Any] ?? [:]
        let pre = hooks["PreToolUse"] as? [[String: Any]] ?? []
        suite.expect(pre.count == 2 && (pre.first?["hooks"] as? [[String: Any]])?.first?["command"] as? String == "my-guard.sh",
                     "the person's own hooks stay first and untouched")
        suite.expect(installed["model"] as? String == "opus" && (installed["permissions"] as? [String: Any]) != nil,
                     "and so do the other settings")
        let permission = (hooks["PermissionRequest"] as? [[String: Any]])?.first
        suite.expect(((permission?["hooks"] as? [[String: Any]])?.first?["timeout"] as? Int) == ClaudeHookInstaller.permissionTimeout
                     && permission?["matcher"] as? String == "*", "a permission waits as long as Claude Code allows")
        let backups = (try? FileManager.default.contentsOfDirectory(atPath: folder.path))?.filter { $0.hasSuffix(".bak") } ?? []
        suite.expect(backups.count == 1, "the old file is kept aside")
        if let backup = backups.first {
            suite.expect(FileManager.default.contents(atPath: folder.appending(path: backup).path) == Data(own.utf8),
                         "with exactly what it held")
        }

        do { try ClaudeHookInstaller.update(url, now: now.addingTimeInterval(1), install) } catch { suite.expect(false, "\(error)") }
        let again = (try? ClaudeHookInstaller.read(url)) ?? [:]
        let count = ((again["hooks"] as? [String: Any])?["Stop"] as? [[String: Any]])?.count
        suite.expect(count == 1, "installing again adds nothing")

        let other = ClaudeHookInstaller.installing(into: again, command: "x # vorssaint-hook com.other", bundle: "com.other")
        let removed = ClaudeHookInstaller.removing(from: other, bundle: bundle)
        suite.expect(!ClaudeHookInstaller.hasAny(in: removed, bundle: bundle) && ClaudeHookInstaller.hasAny(in: removed, bundle: "com.other"),
                     "removing takes only this app's entries")
        do {
            try ClaudeHookInstaller.update(url, now: now.addingTimeInterval(2)) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
        } catch { suite.expect(false, "\(error)") }
        let restored = (try? ClaudeHookInstaller.read(url)) ?? [:]
        suite.expect(NSDictionary(dictionary: restored).isEqual(to: (try? JSONSerialization.jsonObject(with: Data(own.utf8))) as? [String: Any] ?? [:]),
                     "removing restores the settings as they were")

        let broken = folder.appending(path: "broken.json")
        try? Data("{ \"model\": ".utf8).write(to: broken)
        do {
            try ClaudeHookInstaller.update(broken, install)
            suite.expect(false, "a file that is not JSON is refused")
        } catch {
            suite.expect(error as? ClaudeHookInstaller.Failure == .unreadable, "a file that is not JSON is refused")
        }
        suite.expect(FileManager.default.contents(atPath: broken.path) == Data("{ \"model\": ".utf8), "and left as it was")

        let fresh = folder.appending(path: "new/settings.json")
        do { try ClaudeHookInstaller.update(fresh, install) } catch { suite.expect(false, "\(error)") }
        suite.expect(ClaudeHookInstaller.isInstalled(in: (try? ClaudeHookInstaller.read(fresh)) ?? [:], command: command),
                     "a missing settings file is created")

        let target = folder.appending(path: "dotfiles-settings.json")
        try? Data(own.utf8).write(to: target)
        let link = folder.appending(path: "linked.json")
        try? FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        do { try ClaudeHookInstaller.update(link, install) } catch { suite.expect(false, "\(error)") }
        let attributes = try? FileManager.default.attributesOfItem(atPath: link.path)
        suite.expect(attributes?[.type] as? FileAttributeType == .typeSymbolicLink
                     && ClaudeHookInstaller.isInstalled(in: (try? ClaudeHookInstaller.read(target)) ?? [:], command: command),
                     "linked settings are changed where they live and the link stays")
    }

    // MARK: Replies

    static func replies(_ suite: TestSuite) {
        let script = ClaudeTerminalScripts.type("say \"hi\"\nthen \\ go", tty: "/dev/ttys004")
        suite.expect(script.contains(#"tty of theTab is "/dev/ttys004""#), "the reply goes to the tab on the session's device")
        suite.expect(script.contains("\"say \\\"hi\\\"\nthen \\\\ go\""), "its quotes and backslashes are escaped and lines kept")
        suite.expect(script.contains(#"marker & "[200~""#) && script.contains(#"marker & "[201~""#),
                     "it is typed as a paste, so a line break does not send it early")
        suite.expect(ClaudeTerminalScripts.focus(tty: "/dev/ttys004").contains("set selected of theTab to true"),
                     "opening selects that tab")
        suite.expect(ClaudeTerminalBridge.shell(of: getpid()) == getppid(), "a session's shell is its parent process")

        // Two IDE windows listen; only the one holding the terminal says yes.
        let folder = URL(fileURLWithPath: "/tmp/vs-bridge-\(getpid())")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let queue = DispatchQueue(label: "test.claude-bridge")
        let lock = NSLock()
        var requests: [[String: Any]] = []
        func window(_ name: String, shell: Int) -> ClaudeHookServer {
            ClaudeHookServer(path: folder.appending(path: name).path, queue: queue) { payload, connection in
                let request = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] ?? [:]
                lock.withLock { requests.append(request) }
                let mine = request["shellPid"] as? Int == shell
                connection.finish(Data((mine ? #"{"ok":true}"# : #"{"ok":false,"error":"missing"}"#).utf8 + [0x0A]))
            }
        }
        let first = window("1.sock", shell: 111), second = window("2.sock", shell: 222)
        suite.expect(queue.sync { first.start() && second.start() }, "the windows listen")
        try? Data().write(to: folder.appending(path: "stale.sock"))
        suite.expect(ClaudeTerminalBridge.send("Ship it", shell: 222, folder: folder), "the window with the terminal types the reply")
        let sent = lock.withLock { requests.first { $0["shellPid"] as? Int == 222 } }
        suite.expect(sent?["command"] as? String == "send" && sent?["text"] as? String == "Ship it", "the request says what to type")
        suite.expect(!ClaudeTerminalBridge.send("x", shell: 333, folder: folder), "no window holding the terminal means no reply")
        suite.expect(ClaudeTerminalBridge.focus(shell: 111, folder: folder), "a window shows its terminal on request")
        queue.sync { first.stop(); second.stop() }
        suite.expect(!ClaudeTerminalBridge.focus(shell: 111, folder: folder), "with the IDE closed nothing answers")
        suite.expect(!ClaudeTerminalBridge.ask(["command": "ping"], folder: folder.appending(path: "missing")),
                     "a missing folder is no answer")
    }
}
