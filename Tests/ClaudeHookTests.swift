// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
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
        let editKey = ClaudeToolSummary.inputKey(name: "Edit", input: ["file_path": "/a/b.swift"])
        suite.expect(pre?.fact.event == .toolStarted(ClaudeToolUse(id: "t1", name: "Edit", summary: nil, started: now, inputKey: editKey)),
                     "a tool start keeps its name but not its file while previews are off")
        let shown = decode(#"{\#(base),"hook_event_name":"PreToolUse","tool_name":"Edit","tool_use_id":"t1","tool_input":{"file_path":"/a/b.swift"}}"#,
                           previews: true)
        suite.expect(shown?.fact.event == .toolStarted(ClaudeToolUse(id: "t1", name: "Edit", summary: "b.swift", started: now, inputKey: editKey)),
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

        // An event answered at once, after the hook already stopped writing,
        // closes its descriptor once: a file opened meanwhile stays open.
        var victim: Int32 = -1
        let quick = ClaudeHookServer(path: folder + "/quick.sock", queue: queue) { _, connection in
            connection.finish()
            victim = open("/dev/null", O_RDONLY)
        }
        _ = queue.sync { quick.start() }
        for _ in 0..<5 { _ = hook(folder + "/quick.sock", input: #"{"session_id":"s1","hook_event_name":"Stop"}"#) }
        usleep(100_000)
        let stillOpen = queue.sync { victim >= 0 && fcntl(victim, F_GETFD) != -1 }
        suite.expect(stillOpen, "answering an event never closes someone else's file")
        queue.sync { if victim >= 0 { close(victim) }; quick.stop() }

        // A large answer arrives whole.
        // Past the socket's buffer, within the test pipe's.
        let big = String(repeating: "a", count: 40 << 10)
        let wide = ClaudeHookServer(path: folder + "/wide.sock", queue: queue) { _, connection in
            DispatchQueue.global().async { connection.finish(Data(#"{"x":"\#(big)"}"#.utf8)) }
        }
        _ = queue.sync { wide.start() }
        let large = hook(folder + "/wide.sock", input: #"{"session_id":"s1","hook_event_name":"PermissionRequest"}"#)
        suite.expect(large.output.count == big.count + 8, "a 40 KB answer, written from another queue, arrives whole")
        queue.sync { wide.stop() }

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

        let locked = folder.appending(path: "locked.json")
        try? Data(own.utf8).write(to: locked)
        chmod(locked.path, 0)
        do {
            try ClaudeHookInstaller.update(locked, install)
            suite.expect(false, "a settings file that cannot be read is never written over")
        } catch {
            suite.expect(error as? ClaudeHookInstaller.Failure == .unreadable, "a settings file that cannot be read is refused")
        }
        chmod(locked.path, 0o644)
        suite.expect(FileManager.default.contents(atPath: locked.path) == Data(own.utf8), "and keeps everything it held")

        let dangling = folder.appending(path: "dangling.json")
        try? FileManager.default.createSymbolicLink(at: dangling, withDestinationURL: folder.appending(path: "gone.json"))
        suite.expect((try? ClaudeHookInstaller.update(dangling, install)) == nil
                     && !FileManager.default.fileExists(atPath: folder.appending(path: "gone.json").path),
                     "a link to a missing file is not replaced by a new one")

        let none = folder.appending(path: "none/settings.json")
        try? ClaudeHookInstaller.update(none) { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
        suite.expect(!FileManager.default.fileExists(atPath: none.path), "removing hooks from no settings writes nothing")

        let pruned = folder.appending(path: "pruned/settings.json")
        try? FileManager.default.createDirectory(at: pruned.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(own.utf8).write(to: pruned)
        for step in 0..<5 {
            let toggle: ([String: Any]) -> [String: Any] = step % 2 == 0 ? install
                : { ClaudeHookInstaller.removing(from: $0, bundle: bundle) }
            try? ClaudeHookInstaller.update(pruned, now: now.addingTimeInterval(TimeInterval(step)), toggle)
        }
        let kept = (try? FileManager.default.contentsOfDirectory(atPath: pruned.deletingLastPathComponent().path))?
            .filter { $0.hasSuffix(".bak") } ?? []
        let modes = kept.compactMap {
            (try? FileManager.default.attributesOfItem(atPath: pruned.deletingLastPathComponent().appending(path: $0).path))?[.posixPermissions] as? Int
        }
        suite.expect(kept.count == ClaudeHookInstaller.backups && modes.allSatisfy { $0 == 0o600 },
                     "only the newest copies are kept, readable by their owner alone: \(kept.count) \(modes)")

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
        let pictured = ClaudeTerminalScripts.type("What is it?", images: ["/tmp/a b.png", "/tmp/c.jpg"], tty: "/dev/ttys004")
        let opening = pictured.range(of: "do script"), closing = pictured.range(of: "do script", options: .backwards)
        suite.expect(opening != nil && closing != nil && opening!.lowerBound < closing!.lowerBound
                     && pictured.contains(#"marker & "[200~" & "/tmp/a b.png" & marker & "[201~" & marker & "[200~" & "/tmp/c.jpg" & marker & "[201~" & marker & "[200~""#),
                     "images go first, each a paste of its own path, with a paste left open for Terminal's return")
        suite.expect(pictured.contains("delay \(ClaudeTerminalScripts.imageSettle(2))")
                     && pictured.contains(#"do script ("What is it?" & marker & "[201~") in theTab"#),
                     "then, once Claude Code has read them, the text closes the paste and its return sends it all")
        suite.expect(ClaudeTerminalScripts.focus(tty: "/dev/ttys004").contains("set selected of theTab to true"),
                     "opening selects that tab")
        suite.expect(ClaudeTerminalBridge.shell(of: getpid()) == getppid(), "a session's shell is its parent process")
        let open = ["/Users/me/Projekty/Nexus-Code", "/Users/me/Projekty"]
        suite.expect(ClaudeIDELock.openFolder(cwd: "/Users/me/Projekty/Nexus-Code/app", folders: open) == "/Users/me/Projekty/Nexus-Code",
                     "Open asks the IDE for the deepest open folder holding the session")
        suite.expect(ClaudeIDELock.openFolder(cwd: "/Users/me/Projekty-old/x", folders: open) == nil,
                     "and never for one not open there, which would open a new window")
        suite.expect(ClaudeIDELock.windowHint(cwd: "/Users/me/Projekty/Nexus-Code", folders: open) == "Nexus-Code",
                     "the window is named after that folder")

        // Two IDE windows listen; only the one holding the terminal says yes.
        let folder = URL(fileURLWithPath: "/tmp/vs-bridge-\(getpid())")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let queue = DispatchQueue(label: "test.claude-bridge")
        let lock = NSLock()
        var requests: [[String: Any]] = []
        func window(_ name: String, shell: Int, images: Bool = true) -> ClaudeHookServer {
            ClaudeHookServer(path: folder.appending(path: name).path, queue: queue) { payload, connection in
                let request = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] ?? [:]
                lock.withLock { requests.append(request) }
                let mine = request["shellPid"] as? Int == shell
                let answer = !mine ? #"{"ok":false,"error":"missing"}"#
                    : request["command"] as? String == "send-images" && !images ? #"{"ok":false,"error":"unknown"}"# : #"{"ok":true}"#
                connection.finish(Data(answer.utf8 + [0x0A]))
            }
        }
        let first = window("1.sock", shell: 111), second = window("2.sock", shell: 222)
        let old = window("3.sock", shell: 444, images: false)
        suite.expect(queue.sync { first.start() && second.start() && old.start() }, "the windows listen")
        try? Data().write(to: folder.appending(path: "stale.sock"))
        suite.expect(ClaudeTerminalBridge.send("Ship it", shell: 222, folder: folder), "the window with the terminal types the reply")
        let sent = lock.withLock { requests.first { $0["shellPid"] as? Int == 222 } }
        suite.expect(sent?["command"] as? String == "send" && sent?["text"] as? String == "Ship it", "the request says what to type")
        suite.expect(!ClaudeTerminalBridge.send("x", shell: 333, folder: folder), "no window holding the terminal means no reply")
        suite.expect(ClaudeTerminalBridge.send("Look", images: ["/tmp/a.png"], shell: 222, folder: folder) == nil,
                     "images go to the window holding the terminal")
        let pictured2 = lock.withLock { requests.last { $0["command"] as? String == "send-images" && $0["shellPid"] as? Int == 222 } }
        suite.expect(pictured2?["images"] as? [String] == ["/tmp/a.png"] && pictured2?["text"] as? String == "Look",
                     "with the text that follows them")
        suite.expect(ClaudeTerminalBridge.send("Look", images: ["/tmp/a.png"], shell: 444, folder: folder) == nil,
                     "a window still on an extension from before images is not a failure")
        let named = lock.withLock { requests.last { $0["command"] as? String == "send" && $0["shellPid"] as? Int == 444 } }
        suite.expect(named?["text"] as? String == "Look\n\n[Image: /tmp/a.png]",
                     "the reply names the image's file there, for Claude to read: \(named?["text"] ?? "none")")
        suite.expect(ClaudeTerminalBridge.naming(["/tmp/a.png", "/tmp/b.png"], in: "") == "[Image: /tmp/a.png]\n[Image: /tmp/b.png]",
                     "images alone are named one to a line")
        suite.expect(ClaudeTerminalBridge.send("Look", images: ["/tmp/a.png"], shell: 555, folder: folder) == .unreachable,
                     "and no window holding the terminal is unreachable")
        suite.expect(ClaudeTerminalBridge.focus(shell: 111, folder: folder), "a window shows its terminal on request")
        queue.sync { first.stop(); second.stop(); old.stop() }

        let board = NSPasteboard(name: NSPasteboard.Name("vorssaint-test-\(getpid())"))
        defer { board.releaseGlobally() }
        board.clearContents()
        board.setString("plain words", forType: .string)
        suite.expect(!ClaudeImageAttachments.holdsImages(board), "a text paste stays text")
        let image = NSImage(size: NSSize(width: 8, height: 8))
        image.lockFocus(); NSColor.systemBlue.setFill(); NSRect(x: 0, y: 0, width: 8, height: 8).fill(); image.unlockFocus()
        board.clearContents()
        board.writeObjects([image])
        let saved = ClaudeImageAttachments.images(from: board)
        suite.expect(ClaudeImageAttachments.holdsImages(board) && saved.count == 1 && saved.first?.pathExtension == "png"
                     && saved.first.map { FileManager.default.fileExists(atPath: $0.path) } == true,
                     "a copied picture becomes a PNG file Claude Code can read")
        let photo = folder.appending(path: "photo.jpg"), notes = folder.appending(path: "notes.txt")
        try? Data([0xFF, 0xD8, 0xFF]).write(to: photo)
        try? Data("x".utf8).write(to: notes)
        board.clearContents()
        board.writeObjects([photo as NSURL, notes as NSURL])
        suite.expect(ClaudeImageAttachments.images(from: board) == [photo], "a copied image file is used where it lies; others are left out")
        saved.forEach { try? FileManager.default.removeItem(at: $0) }

        let root = folder.appending(path: "claude-tmp").path
        let session = URL(fileURLWithPath: root).appending(path: "-Users-me-app").appending(path: "s1")
        let transcript = "/Users/me/.claude/projects/-Users-me-app/s1.jsonl"
        suite.expect(ClaudeImageAttachments.sessionFolder(transcript: transcript, session: "s1", root: root) == nil,
                     "no session folder means the images are named where they lie")
        try? FileManager.default.createDirectory(at: session, withIntermediateDirectories: true)
        let found = ClaudeImageAttachments.sessionFolder(transcript: transcript, session: "s1", root: root)
        suite.expect(found?.standardizedFileURL.path == session.standardizedFileURL.path, "the session's folder sits under the project's")
        let copied = ClaudeImageAttachments.copies([photo], in: found)
        suite.expect(copied.count == 1 && copied[0].deletingLastPathComponent().lastPathComponent == "images"
                     && copied[0].pathExtension == "jpg" && FileManager.default.fileExists(atPath: copied[0].path),
                     "images are copied into its images folder, where Claude reads without asking")
        suite.expect(ClaudeImageAttachments.copies([photo], in: nil) == [photo], "without a folder they stay where they are")
        suite.expect(!ClaudeTerminalBridge.focus(shell: 111, folder: folder), "with the IDE closed nothing answers")
        suite.expect(!ClaudeTerminalBridge.ask(["command": "ping"], folder: folder.appending(path: "missing")),
                     "a missing folder is no answer")
    }
}
