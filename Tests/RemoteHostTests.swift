// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Foundation

/// Remote hosts over SSH (issue #189): the pure parts, and the host script
/// itself run by this Mac's `sh` against a throwaway home.
enum RemoteHostTests {
    static func run(_ suite: TestSuite) {
        suite.run("remote hosts aliases and ssh") { aliases(suite) }
        suite.run("remote hosts answers") { answers(suite) }
        suite.run("remote hosts transcripts") { transcripts(suite) }
        suite.run("remote hosts clock") { clock(suite) }
        suite.run("remote hosts hook installer") { installer(suite) }
        suite.run("remote hosts script") { script(suite) }
    }

    static let bundle = "com.openbar.app.dev"

    static func aliases(_ suite: TestSuite) {
        suite.expect(RemoteHost.isValid("openbar-linux") && RemoteHost.isValid("dev@box.example.com")
                     && RemoteHost.isValid("[::1]"), "ssh config aliases and user@host count")
        suite.expect(!RemoteHost.isValid("-oProxyCommand=x") && !RemoteHost.isValid("a b") && !RemoteHost.isValid("")
                     && !RemoteHost.isValid("box;rm") && !RemoteHost.isValid("bóx"),
                     "nothing ssh would read as an option, and no spaces or shell characters")
        let args = RemoteSSH.link("box", remoteSocket: "/home/dev/.openbar/a.sock", localSocket: "/Users/me/Library/x y.sock")
        let dash = args.firstIndex(of: "--") ?? -1
        suite.expect(args.contains("BatchMode=yes") && args.contains("ExitOnForwardFailure=yes")
                     && args.contains("/home/dev/.openbar/a.sock:/Users/me/Library/x y.sock"),
                     "the link never prompts, fails when the forward can't be made, and forwards the host socket to the island's")
        suite.expect(dash > 0 && args[dash + 1] == "box" && args.last == RemoteSSH.loader,
                     "the alias comes after --, and the host only runs the loader")
        suite.expect(!RemoteSSH.loader.contains("\\") && RemoteSSH.loader.hasPrefix("exec sh -c '"),
                     "the loader reads the same in fish, zsh, bash and sh")
        suite.expect(RemoteSSH.socketName(bundle: bundle) == "com.openbar.app.dev.sock"
                     && RemoteSSH.socketName(bundle: "a/../b c") == "a..bc.sock", "the socket name has no path in it")
        suite.expect(RemoteBackoff.delay(attempt: 1) == 2 && RemoteBackoff.delay(attempt: 3) == 8
                     && RemoteBackoff.delay(attempt: 9) == 30, "tries again sooner first, then every half minute")
        suite.expect(RemoteSSH.reason("Warning: x\nssh: connect to host box port 22: Connection refused\n", status: 255)
                     == "ssh: connect to host box port 22: Connection refused"
                     && RemoteSSH.reason("", status: 255) == "The host can’t be reached (ssh exited with 255)", "the reason is ssh's own last line")

        let folder = FileManager.default.temporaryDirectory.appending(path: "openbar-remote-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        try? FileManager.default.createDirectory(at: folder.appending(path: ".ssh/conf.d"), withIntermediateDirectories: true)
        try? "Host box other\n  HostName 10.0.0.2\nHost *.internal !bad\nInclude conf.d/extra\nHost=eq\n"
            .write(to: folder.appending(path: ".ssh/config"), atomically: true, encoding: .utf8)
        try? "Host fromInclude\n".write(to: folder.appending(path: ".ssh/conf.d/extra"), atomically: true, encoding: .utf8)
        suite.expect(RemoteHostPrefs.configAliases(home: folder) == ["box", "other", "eq", "fromInclude"],
                     "aliases come from the config and its includes, without patterns")

        let defaults = UserDefaults(suiteName: "com.openbar.tests.remote-hosts")!
        defaults.removePersistentDomain(forName: "com.openbar.tests.remote-hosts")
        defer { defaults.removePersistentDomain(forName: "com.openbar.tests.remote-hosts") }
        suite.expect(RemoteHostPrefs.active(in: defaults).isEmpty && !RemoteHostPrefs.isEnabled(in: defaults), "off by default")
        RemoteHostPrefs.save([RemoteHost(alias: "box"), RemoteHost(alias: "box"), RemoteHost(alias: "-x")], in: defaults)
        suite.expect(RemoteHostPrefs.hosts(in: defaults).map(\.alias) == ["box"] && RemoteHostPrefs.active(in: defaults).isEmpty,
                     "hosts are kept once each, and reached only with the switch on")
        defaults.set(true, forKey: DefaultsKey.notchSessionsRemoteHostsEnabled)
        suite.expect(RemoteHostPrefs.active(in: defaults).map(\.alias) == ["box"], "the switch turns them on")
    }

    static func b64(_ text: String) -> String { RemoteAnswer.encode(text) }

    static let linuxRecord = #"{"pid":1128,"sessionId":"5a3bef52-775f-4659-8b4b-099b67ea877e","cwd":"/home/dev/project","startedAt":1794042383051,"procStart":"12807","version":"2.1.319","kind":"interactive","pidDomain":"linux:91bc82a9:pid:[4026531836]","tmux":"main:@0.%0","name":"project-6c","nameSource":"derived","status":"waiting","waitingFor":"permission prompt","updatedAt":1794042383085,"statusUpdatedAt":1794042383085}"#

    static func answers(_ suite: TestSuite) {
        let info = RemoteAnswer.probe(["H \(b64("/home/dev"))", "U Linux", "C nc \(b64("/usr/bin/nc"))", "M yes", "A yes", "I no"])
        suite.expect(info == RemoteHostInfo(home: "/home/dev", system: "Linux", client: .nc("/usr/bin/nc"), tmux: true,
                                            claude: true, hooksInstalled: false), "the probe says what the host has")
        suite.expect(RemoteAnswer.probe(["H \(b64("/home/dev"))", "C none", "M no"])?.client == nil,
                     "a host with no client still connects, without hooks")
        suite.expect(RemoteAnswer.probe(["E name"]) == nil && RemoteAnswer.probe(["U Linux"]) == nil,
                     "a refused or partial probe is no connection")

        suite.expect(ClaudeSessionRegistryReader.record(Data(linuxRecord.utf8)) == nil,
                     "a Linux record is never one of this Mac's own")
        let lines = ["R 1128 \(b64(linuxRecord)) \(b64("/home/dev/.claude")) \(b64("/tmp/tmux-1000/default")) %0 \(b64("/dev/pts/0"))",
                     "R 77 \(b64(linuxRecord)) \(b64("/home/dev/.claude")) - - -",
                     "J 5a3bef52-775f-4659-8b4b-099b67ea877e \(b64("/home/dev/.claude/projects/-home-dev-project/5a3bef52.jsonl"))",
                     "T \(b64("/home/dev/a b.jsonl")) 10 \(Data("{}\n".utf8).base64EncodedString())",
                     "G \(b64("/home/dev/gone.jsonl"))", "junk"]
        let snapshot = RemoteAnswer.snapshot(lines, alias: "box")
        let record = snapshot.records.first
        suite.expect(snapshot.records.count == 1 && record?.pid == 1128 && record?.status == .waiting
                     && record?.waitingFor == "permission prompt" && record?.configHome == "/home/dev/.claude",
                     "a remote record reads like a local one; one whose pid disagrees with its file is dropped")
        suite.expect(record?.host == .remote(alias: "box", tmux: true) && record?.host.remoteAlias == "box"
                     && record?.host.acceptsReplies == true, "a session in tmux on the host takes replies")
        suite.expect(ClaudeSessionHost.remote(alias: "box", tmux: false).acceptsReplies == false
                     && ClaudeSessionHost.remote(alias: "box", tmux: false).bundle == nil
                     && ClaudeSessionHost.remote(alias: "box", tmux: true).displayName == "tmux on box",
                     "outside tmux replies can't reach it, and it names its host")
        suite.expect(snapshot.transcripts["5a3bef52-775f-4659-8b4b-099b67ea877e"] == "/home/dev/.claude/projects/-home-dev-project/5a3bef52.jsonl"
                     && snapshot.chunks == [RemoteChunk(path: "/home/dev/a b.jsonl", offset: 10, bytes: Data("{}\n".utf8))]
                     && snapshot.gone == ["/home/dev/gone.jsonl"], "transcripts, pieces of them, and gone ones")
        suite.expect(RemoteAnswer.outcome(["O"]) == nil && RemoteAnswer.outcome(["F waiting"]) == .waiting
                     && RemoteAnswer.outcome(["F unreachable"]) == .unreachable && RemoteAnswer.outcome(nil) == .unreachable,
                     "a reply's outcome")
        let text = NotchClaudeSessionStrings.enUS
        suite.expect(text.replyUnavailable(for: .remote(alias: "box", tmux: false)) == RemoteHostStrings.replyNeedsTmux,
                     "the reply field says why")
    }

    static func clock(_ suite: TestSuite) {
        let sent = Date(timeIntervalSince1970: 1_794_045_700)
        let received = sent.addingTimeInterval(0.2)
        let hostAhead = sent.addingTimeInterval(3600.1)
        let offset = RemoteClock.offset(sent: sent, received: received, host: hostAhead)
        suite.expect(abs(offset - -3600) < 0.001, "an hour ahead is an hour taken off, measured at the request's middle")
        suite.expect(RemoteClock.offset(sent: sent, received: received, host: sent.addingTimeInterval(0.15)) == 0
                     && RemoteClock.offset(sent: sent, received: received, host: nil) == 0,
                     "the request's own time, or no host time, leaves times as they are")
        let lines = ["N \(Int(hostAhead.timeIntervalSince1970 * 1000))",
                     "R 1128 \(b64(linuxRecord)) \(b64("/home/dev/.claude")) - - -"]
        let read = RemoteAnswer.snapshot(lines, alias: "box")
        suite.expect(read.hostTime.map { abs($0.timeIntervalSince(hostAhead)) < 0.001 } == true, "the snapshot gives the host's clock")
        let shifted = RemoteClock.shifted(read, by: offset)
        let before = read.records.first?.statusUpdated ?? .distantPast
        suite.expect(shifted.clockOffset == offset
                     && abs((shifted.records.first?.statusUpdated ?? .distantPast).timeIntervalSince(before) - offset) < 0.001,
                     "the registry's times move to this Mac's clock, so a hook's permission isn't taken for answered")
        let line = Data(#"{"type":"user","timestamp":"2026-11-07T10:00:00.500Z","sessionId":"s"}"#.utf8)
        let moved = (try? JSONSerialization.jsonObject(with: RemoteClock.line(line, by: -3600))) as? [String: Any]
        suite.expect(moved?["timestamp"] as? String == "2026-11-07T09:00:00.500Z" && moved?["sessionId"] as? String == "s",
                     "a transcript line's time moves the same way")
        suite.expect(RemoteClock.line(line, by: 0.4) == line && RemoteClock.line(Data("not json".utf8), by: 60) == Data("not json".utf8),
                     "a small offset or a line that isn't JSON passes unchanged")
    }

    static func transcripts(_ suite: TestSuite) {
        var tail = RemoteTranscriptTail()
        let first = tail.take(RemoteChunk(path: "p", offset: 0, bytes: Data("{\"a\":1}\n{\"b\"".utf8)))
        suite.expect(first.lines == [Data("{\"a\":1}".utf8)] && !first.restarted && tail.offset == 12,
                     "whole lines go on, a cut one waits")
        let second = tail.take(RemoteChunk(path: "p", offset: 12, bytes: Data(":2}\n\n".utf8)))
        suite.expect(second.lines == [Data("{\"b\":2}".utf8)] && !second.restarted, "the next piece completes it")
        let again = tail.take(RemoteChunk(path: "p", offset: 0, bytes: Data("{\"c\":3}\n".utf8)))
        suite.expect(again.restarted && again.lines == [Data("{\"c\":3}".utf8)] && tail.offset == 8,
                     "a file that started over is read from its start")
    }

    static func installer(_ suite: TestSuite) {
        let nc = RemoteHookInstaller.command(client: .nc("/usr/bin/nc"), socket: "/home/d'v/.openbar/a.sock", bundle: bundle)
        suite.expect(nc == #"'/usr/bin/nc' -U '/home/d'\''v/.openbar/a.sock' 2>/dev/null; exit 0 # openbar-remote-hook com.openbar.app.dev"#,
                     "the hook writes with the host's nc, and never fails Claude Code")
        let socat = RemoteHookInstaller.command(client: .socat("/usr/bin/socat"), socket: "/h/a.sock", bundle: bundle)
        let python = RemoteHookInstaller.command(client: .python3("/usr/bin/python3"), socket: "/h/a.sock", bundle: bundle)
        suite.expect(socat.hasPrefix("'/usr/bin/socat' - 'UNIX-CONNECT:/h/a.sock' 2>/dev/null; exit 0")
                     && python.hasPrefix("'/usr/bin/python3' -c 'import socket,sys;") && !python.dropFirst(20).contains("''"),
                     "socat and python3 stand in where nc can't")
        suite.expect(!RemoteHookInstaller.marker(bundle: bundle).contains(ClaudeHookInstaller.marker(bundle: "com.openbar.app")),
                     "an Openbar on the host itself never takes these hooks for its own")

        let own = ["hooks": ["Stop": [["hooks": [["type": "command", "command": "say done"]]]]], "theme": "dark"] as [String: Any]
        let installed = RemoteHookInstaller.installing(into: own, command: nc, bundle: bundle)
        suite.expect(ClaudeHookInstaller.isInstalled(in: installed, command: nc) && RemoteHookInstaller.hasAny(in: installed, bundle: bundle),
                     "every event gets the hook")
        let stops = (installed["hooks"] as? [String: Any])?["Stop"] as? [[String: Any]] ?? []
        suite.expect(stops.count == 2, "the person's own hooks stay beside it")

        let original = Data("{\n  \"theme\" : \"dark\",   \"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"say done\"}]}]}\n}\n".utf8)
        let current = (try? RemoteHookInstaller.serialize(RemoteHookInstaller.installing(into: (try? RemoteHookInstaller.parse(original)) ?? [:],
                                                                                          command: nc, bundle: bundle))) ?? Data()
        suite.expect((try? RemoteHookInstaller.removal(current: current, backup: .file(original), bundle: bundle)) == .restore,
                     "only the hooks changed: the file goes back as it was, spacing and all")
        suite.expect((try? RemoteHookInstaller.removal(current: (try? RemoteHookInstaller.serialize(
            RemoteHookInstaller.installing(into: [:], command: nc, bundle: bundle))) ?? Data(), backup: RemoteHookInstaller.Backup.none,
                                                       bundle: bundle)) == .restore,
                     "no file before setup: none after")
        var edited = (try? RemoteHookInstaller.parse(current)) ?? [:]
        edited["model"] = "opus"
        if case .write(let data, let keep)? = try? RemoteHookInstaller.removal(current: try? RemoteHookInstaller.serialize(edited),
                                                                     backup: .file(original), bundle: bundle) {
            let kept = (try? RemoteHookInstaller.parse(data)) ?? [:]
            suite.expect(kept["model"] as? String == "opus" && !RemoteHookInstaller.hasAny(in: kept, bundle: bundle) && !keep,
                         "something else changed too: the hooks come out and the change stays")
        } else {
            suite.expect(false, "an edited file is written without the hooks")
        }
        // Two Openbars on one host (a Mac and a PC): the first one out keeps the
        // copy, and the last one out puts the file back byte for byte.
        let other = RemoteHookInstaller.command(client: .nc("/usr/bin/nc"), socket: "/h/b.sock", bundle: "com.openbar.island")
        let both = RemoteHookInstaller.installing(into: RemoteHookInstaller.installing(into: (try? RemoteHookInstaller.parse(original)) ?? [:],
                                                                                    command: nc, bundle: bundle),
                                                  command: other, bundle: "com.openbar.island")
        if case .write(let data, let keep)? = try? RemoteHookInstaller.removal(current: try? RemoteHookInstaller.serialize(both),
                                                                               backup: .file(original), bundle: "com.openbar.island") {
            suite.expect(keep && (try? RemoteHookInstaller.removal(current: data, backup: .file(original), bundle: bundle)) == .restore,
                         "the first Openbar out keeps the copy for the last one")
        } else {
            suite.expect(false, "the first Openbar out writes the file without its hooks")
        }
        suite.expect((try? RemoteHookInstaller.removal(current: Data("not json".utf8), backup: nil, bundle: bundle)) == nil,
                     "a file that isn't JSON is left alone")
    }

    // MARK: The script itself

    /// Runs one mode of the script with `sh`, as the loader on a host would.
    static func runScript(_ mode: String, input: [String] = [], home: URL) -> [String]? {
        guard let preamble = RemoteSSH.preamble(mode) else { return nil }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", #"IFS= read -r s; eval "$(printf %s "$s" | base64 -d)""#]
        process.environment = ["HOME": home.path, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
        let feed = Pipe()
        let output = Pipe()
        process.standardInput = feed
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return nil }
        feed.fileHandleForWriting.write(Data((preamble + input.map { $0 + "\n" }.joined()).utf8))
        try? feed.fileHandleForWriting.close()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        let lines = String(decoding: data, as: UTF8.self).split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        return lines.last == "" ? Array(lines.dropLast()) : lines
    }

    static func script(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "openbar-remote-home-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        try? manager.createDirectory(at: home.appending(path: ".claude/sessions"), withIntermediateDirectories: true)
        try? manager.createDirectory(at: home.appending(path: ".openbar"), withIntermediateDirectories: true)
        manager.createFile(atPath: home.appending(path: ".openbar/x.sock").path, contents: Data())

        let probe = runScript("probe x.sock \(bundle)", home: home) ?? []
        let info = RemoteAnswer.probe(probe.dropLast())
        suite.expect(probe.last == "." && info?.home == home.path && info?.system == "Darwin" && info?.hooksInstalled == false,
                     "the probe answers and ends with a lone dot")
        suite.expect(!manager.fileExists(atPath: home.appending(path: ".openbar/x.sock").path),
                     "a socket left from the last connection is cleared before the next one")
        suite.expect(runScript("probe ../x.sock \(bundle)", home: home)?.first == "E name", "a socket name can't leave ~/.openbar")

        // Setup then removal leave the file byte for byte as it was.
        let settings = home.appending(path: ".claude/settings.json")
        let original = Data("{\n    \"theme\":   \"dark\"\n}\n".utf8)
        manager.createFile(atPath: settings.path, contents: original)
        let read = runScript("read", home: home) ?? []
        let sum = read.first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        suite.expect(read.contains("F \(original.base64EncodedString())") && !sum.isEmpty && !read.contains { $0.hasPrefix("B ") },
                     "read gives the file and its checksum, and no copy before setup")
        let command = RemoteHookInstaller.command(client: .nc("/usr/bin/nc"), socket: home.path + "/.openbar/x.sock", bundle: bundle)
        let new = (try? RemoteHookInstaller.serialize(RemoteHookInstaller.installing(
            into: (try? RemoteHookInstaller.parse(original)) ?? [:], command: command, bundle: bundle))) ?? Data()
        suite.expect(runScript("install", input: ["0-0", new.base64EncodedString()], home: home)?.first == "F changed"
                     && manager.contents(atPath: settings.path) == original, "a file changed since it was read is left alone")
        suite.expect(runScript("install", input: [sum, new.base64EncodedString()], home: home)?.first == "O"
                     && manager.contents(atPath: settings.path) == new
                     && manager.contents(atPath: settings.path + ".openbar-remote.bak") == original,
                     "setup writes the hooks and keeps the file as it was")
        suite.expect(RemoteAnswer.probe((runScript("probe x.sock \(bundle)", home: home) ?? []).dropLast())?.hooksInstalled == true,
                     "the probe then sees them")
        let after = runScript("read", home: home) ?? []
        let backup = after.first { $0.hasPrefix("B ") }.flatMap { Data(base64Encoded: String($0.dropFirst(2))) }
        let current = after.first { $0.hasPrefix("F ") }.flatMap { Data(base64Encoded: String($0.dropFirst(2))) }
        let sum2 = after.first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        suite.expect(backup == original && current == new, "read gives the copy back")
        suite.expect((try? RemoteHookInstaller.removal(current: current, backup: backup.map { .file($0) }, bundle: bundle)) == .restore,
                     "nothing else changed")
        // "keep" writes without these hooks and leaves the copy for another Openbar's removal.
        let kept = (try? RemoteHookInstaller.serialize(RemoteHookInstaller.removing(from: (try? RemoteHookInstaller.parse(new)) ?? [:],
                                                                                    bundle: bundle))) ?? Data()
        suite.expect(runScript("remove", input: ["x.sock", sum2, "keep", kept.base64EncodedString()], home: home)?.first == "O"
                     && manager.contents(atPath: settings.path + ".openbar-remote.bak") == original,
                     "the first Openbar out keeps the copy")
        _ = runScript("install", input: [(runScript("read", home: home) ?? []).first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? "",
                                         new.base64EncodedString()], home: home)
        let sum4 = (runScript("read", home: home) ?? []).first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        suite.expect(runScript("remove", input: ["x.sock", sum4, "restore"], home: home)?.first == "O"
                     && manager.contents(atPath: settings.path) == original
                     && !manager.fileExists(atPath: settings.path + ".openbar-remote.bak")
                     && !manager.fileExists(atPath: home.appending(path: ".openbar").path),
                     "Remove from host puts the file back exactly and leaves nothing of Openbar")

        // No settings before setup: none after removal.
        try? manager.removeItem(at: settings)
        let fresh = (try? RemoteHookInstaller.serialize(RemoteHookInstaller.installing(into: [:], command: command, bundle: bundle))) ?? Data()
        _ = runScript("install", input: ["none", fresh.base64EncodedString()], home: home)
        let attrs = try? manager.attributesOfItem(atPath: settings.path)
        suite.expect((attrs?[.posixPermissions] as? NSNumber)?.intValue == 0o600, "a new settings file is the owner's alone")
        let sum3 = (runScript("read", home: home) ?? []).first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        suite.expect(runScript("remove", input: ["x.sock", sum3, "restore"], home: home)?.first == "O"
                     && !manager.fileExists(atPath: settings.path), "a host with no settings before has none after")

        // The agent: a live session, its transcript read in pieces, and a reply refused outside tmux.
        let sleeper = Process()
        sleeper.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        sleeper.arguments = ["-c", "import time; time.sleep(20)", "claude"]
        guard (try? sleeper.run()) != nil else { return suite.expect(false, "a stand-in session starts") }
        defer { sleeper.terminate() }
        let pid = sleeper.processIdentifier
        let id = "11111111-2222-3333-4444-555555555555"
        let record = #"{"pid":\#(pid),"sessionId":"\#(id)","cwd":"/tmp/p","procStart":"x","status":"idle","pidDomain":"darwin"}"#
        manager.createFile(atPath: home.appending(path: ".claude/sessions/\(pid).json").path, contents: Data(record.utf8))
        manager.createFile(atPath: home.appending(path: ".claude/sessions/1.json").path, contents: Data(record.utf8))
        try? manager.createDirectory(at: home.appending(path: ".claude/projects/-tmp-p"), withIntermediateDirectories: true)
        let transcript = home.appending(path: ".claude/projects/-tmp-p/\(id).jsonl")
        manager.createFile(atPath: transcript.path, contents: Data("{\"one\":1}\n{\"two\"".utf8))
        let path = b64(transcript.path)
        let agent = runScript("agent", input: ["snap", "t 0 \(path)", "end", "snap", "t 10 \(path)", "t 0 \(b64("/nope"))", "end",
                                               "paste \(pid) \(id) \(b64("hi"))", "paste 1;rm \(id) x", "ping", "bogus"], home: home) ?? []
        let answers = agent.split(separator: ".", omittingEmptySubsequences: false).map(Array.init)
        suite.expect(answers.first == ["HELLO 1"], "the agent says hello first")
        let snap = RemoteAnswer.snapshot(answers.count > 1 ? answers[1] : [], alias: "box")
        suite.expect(snap.records.map(\.pid) == [pid] && snap.records.first?.host == .remote(alias: "box", tmux: false)
                     && snap.transcripts[id] == transcript.path,
                     "only the live session is listed, with its transcript; a record of a gone or foreign process is not")
        suite.expect(snap.chunks.first?.offset == 0 && snap.chunks.first?.bytes == Data("{\"one\":1}\n{\"two\"".utf8),
                     "a transcript comes from where the island left off")
        let next = RemoteAnswer.snapshot(answers.count > 2 ? answers[2] : [], alias: "box")
        suite.expect(next.chunks.first?.offset == 10 && next.chunks.first?.bytes == Data("{\"two\"".utf8)
                     && next.gone == ["/nope"], "and only what's new the next time")
        suite.expect(answers.count > 6 && answers[3] == ["F unreachable"] && answers[4] == ["F unreachable"]
                     && answers[5] == ["O"] && answers[6] == ["F unknown"],
                     "no tmux, no reply; a request that isn't a number and an id is refused; unknown verbs too")
    }
}
