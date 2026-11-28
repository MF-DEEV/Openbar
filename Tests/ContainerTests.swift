// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// Docker containers and devcontainers (issue #330): the pure parts, the
/// relay script run by this Mac's `sh` against a throwaway home, and a hook
/// handed to the hook server through a socket pair.
enum ContainerTests {
    static func run(_ suite: TestSuite) {
        suite.run("containers targets and prefs") { targets(suite) }
        suite.run("containers docker inspect") { inspect(suite) }
        suite.run("containers commands and hooks") { commands(suite) }
        suite.run("containers relay script") { relay(suite) }
        suite.run("containers hook server adoption") { adoption(suite) }
    }

    static let bundle = "com.openbar.app.dev"

    static func targets(_ suite: TestSuite) {
        let devcontainer = ContainerTarget(folder: "/Users/me/Code/shop")
        let plain = ContainerTarget(name: "agent-box")
        suite.expect(devcontainer.alias == "container:folder:/Users/me/Code/shop" && devcontainer.title == "shop"
                     && plain.alias == "container:name:agent-box" && plain.title == "agent-box",
                     "a devcontainer goes by its folder and shows its project; any other by its name")
        suite.expect(RemoteHostName.isContainer(devcontainer.alias) && !RemoteHostName.isContainer("openbar-linux")
                     && RemoteHostName.title(devcontainer.alias) == "shop" && RemoteHostName.title("openbar-linux") == "openbar-linux"
                     && RemoteHostName.symbol(plain.alias) == "shippingbox" && RemoteHostName.symbol("box") == "network",
                     "a badge tells a container from an SSH host")
        suite.expect(plain.isValid && devcontainer.isValid && !ContainerTarget(name: "-it").isValid
                     && !ContainerTarget(name: "a b").isValid && !ContainerTarget(name: "").isValid
                     && !ContainerTarget(folder: "relative/x").isValid && !ContainerTarget().isValid,
                     "no name docker would read as an option, nothing empty or relative")
        var rebuilt = ContainerInfo(id: "new", name: "musing_goldwasser", image: "img")
        rebuilt.folder = "/Users/me/Code/shop"
        suite.expect(devcontainer.matches(rebuilt) && !plain.matches(rebuilt)
                     && plain.matches(ContainerInfo(id: "1", name: "agent-box", image: "x")),
                     "a rebuilt devcontainer, with a new name and ID, is still the one picked")
        suite.expect(ContainerTarget(folder: "/a", setUp: "1") == ContainerTarget(folder: "/a"),
                     "the setup's mark doesn't make it another container")

        let defaults = UserDefaults(suiteName: "com.openbar.tests.containers")!
        defaults.removePersistentDomain(forName: "com.openbar.tests.containers")
        defer { defaults.removePersistentDomain(forName: "com.openbar.tests.containers") }
        ContainerPrefs.save([plain, devcontainer, plain, ContainerTarget(name: "-x")], in: defaults)
        suite.expect(ContainerPrefs.active(in: defaults).isEmpty, "off by default: nothing is reached until the switch is on")
        defaults.set(true, forKey: DefaultsKey.notchSessionsContainersEnabled)
        suite.expect(ContainerPrefs.active(in: defaults) == [plain, devcontainer], "picked ones, once each, and only valid ones")
        suite.expect(defaults.string(forKey: DefaultsKey.notchSessionsContainers)?.contains("\\/") == false,
                     "folders are stored as they read")
        ContainerPrefs.markSetUp(devcontainer, container: "abc", in: defaults)
        suite.expect(ContainerPrefs.targets(in: defaults).last?.setUp == "abc", "the container set up is remembered")
        ContainerPrefs.markSetUp(devcontainer, container: nil, in: defaults)
        suite.expect(ContainerPrefs.targets(in: defaults).last?.setUp == nil, "and forgotten at Remove")

        var info = RemoteHostInfo(home: "/home/node", system: "Linux", client: .relay, tmux: true, claude: true)
        info.hooksInstalled = false
        info.container = ContainerInfo(id: "new", name: "n", image: "i")
        suite.expect(ContainerStrings.gone(info, target: ContainerTarget(folder: "/a", setUp: "old")) == ContainerStrings.rebuilt
                     && ContainerStrings.gone(info, target: ContainerTarget(folder: "/a", setUp: "new")) == ContainerStrings.hooksGone
                     && ContainerStrings.gone(info, target: ContainerTarget(folder: "/a")) == nil,
                     "hooks gone from a rebuilt container, from the same one, or never set up")
        info.hooksInstalled = true
        suite.expect(ContainerStrings.gone(info, target: ContainerTarget(folder: "/a", setUp: "old")) == nil,
                     "set up again: nothing to offer")
    }

    static func inspect(_ suite: TestSuite) {
        // As Docker and the devcontainers CLI give it (trimmed), then Podman.
        let docker = #"""
        [{"Id":"e9f22d8f53c1","Name":"/musing_goldwasser","State":{"Running":true},
          "Config":{"Image":"openbar-e330:latest","User":"","Labels":{
            "devcontainer.local_folder":"/Users/me/ob/demo-app",
            "devcontainer.config_file":"/Users/me/ob/demo-app/.devcontainer/devcontainer.json",
            "devcontainer.metadata":"[{\"id\":\"ghcr.io/x\"},{\"remoteUser\":\"root\"},{\"remoteUser\":\"node\"}]"}},
          "Mounts":[{"Type":"bind","Source":"/Users/me/ob/demo-app","Destination":"/workspaces/demo-app"}]},
         {"Id":"10773fc25185","Name":"/ob-plain","State":{"Running":true},"Config":{"Image":"debian","Labels":null}},
         {"Id":"dead","Name":"/stopped","State":{"Running":false},"Config":{"Image":"x"}}]
        """#
        let found = ContainerEngine.parse(Data(docker.utf8))
        suite.expect(found.map(\.name) == ["musing_goldwasser", "ob-plain"], "running containers, named without the slash")
        suite.expect(found.first?.folder == "/Users/me/ob/demo-app" && found.first?.user == "node"
                     && found.first?.workspace == "/workspaces/demo-app" && found.first?.title == "demo-app"
                     && found.first?.target == ContainerTarget(folder: "/Users/me/ob/demo-app"),
                     "a devcontainer: its folder, the last remoteUser, and where the folder is inside")
        suite.expect(found.last?.folder == nil && found.last?.user == nil && found.last?.target == ContainerTarget(name: "ob-plain"),
                     "a plain container runs as its own user")
        let podman = #"[{"Id":"abc","Name":"box","State":{"Running":true},"Config":{"Image":"localhost/x","Labels":{}}}]"#
        suite.expect(ContainerEngine.parse(Data(podman.utf8)).first?.name == "box", "Podman's names have no slash")
        suite.expect(ContainerEngine.parse(Data("nope".utf8)).isEmpty && ContainerEngine.remoteUser("[{\"remoteUser\":\"-x\"}]") == nil
                     && ContainerEngine.remoteUser(nil) == nil, "nothing odd is taken")
    }

    static func commands(_ suite: TestSuite) {
        let engine = ContainerEngine(binary: "/opt/homebrew/bin/docker")
        let exec = engine.exec("e9f22d8f53c1", user: "node")
        suite.expect(exec.executable == "/opt/homebrew/bin/docker"
                     && exec.arguments == ["exec", "-i", "-u", "node", "--", "e9f22d8f53c1", "sh", "-c", ContainerEngine.loader],
                     "docker exec -i as the devcontainer's user, the container after --, running only the loader")
        suite.expect(engine.exec("x", user: nil).arguments == ["exec", "-i", "--", "x", "sh", "-c", ContainerEngine.loader],
                     "no -u: the container's own user")
        suite.expect(exec.environment?["PATH"]?.hasPrefix("/opt/homebrew/bin:") == true, "the CLI's helpers next to it are found")

        let command = RemoteHookInstaller.containerCommand(home: "/home/node", bundle: bundle)
        suite.expect(command == "sh '/home/node/.openbar/com.openbar.app.dev.hook' 2>/dev/null; exit 0 # openbar-container-hook com.openbar.app.dev",
                     "each hook runs the relay's hook with sh, and never fails Claude Code")
        let info = RemoteHostInfo(home: "/home/node", system: "Linux", client: .relay, tmux: true, claude: true)
        suite.expect(RemoteHookInstaller.hook(info: info, bundle: bundle)?.command == command
                     && RemoteHookInstaller.hook(info: info, bundle: bundle)?.marker == RemoteHookInstaller.containerMarker(bundle: bundle),
                     "a container's setup writes its own marker")
        suite.expect(RemoteHookInstaller.hook(info: RemoteHostInfo(home: "/h", system: "", client: nil, tmux: false, claude: false),
                                              bundle: bundle) == nil, "no way to reach the island, no setup")

        // Both kinds in one file: the copy stays until the last one is out.
        let original = Data("{\"theme\": \"dark\"}\n".utf8)
        let ssh = RemoteHookInstaller.command(client: .nc("/usr/bin/nc"), socket: "/home/node/.openbar/x.sock", bundle: bundle)
        let both = ClaudeHookInstaller.installing(
            into: RemoteHookInstaller.installing(into: (try? RemoteHookInstaller.parse(original)) ?? [:], command: ssh, bundle: bundle),
            command: command, marker: RemoteHookInstaller.containerMarker(bundle: bundle))
        let data = (try? RemoteHookInstaller.serialize(both)) ?? Data()
        if case .write(_, let keep)? = try? RemoteHookInstaller.removal(current: data, backup: .file(original),
                                                                       marker: RemoteHookInstaller.containerMarker(bundle: bundle)) {
            suite.expect(keep, "the SSH hooks still there keep the copy")
        } else { suite.expect(false, "taking the container's hooks out leaves the SSH ones") }
        if case .write(_, let keep)? = try? RemoteHookInstaller.removal(current: data, backup: .file(original), bundle: bundle) {
            suite.expect(keep, "and the other way round")
        } else { suite.expect(false, "taking the SSH hooks out leaves the container's") }
        let only = ClaudeHookInstaller.installing(into: (try? RemoteHookInstaller.parse(original)) ?? [:], command: command,
                                                  marker: RemoteHookInstaller.containerMarker(bundle: bundle))
        suite.expect((try? RemoteHookInstaller.removal(current: try? RemoteHookInstaller.serialize(only), backup: .file(original),
                                                       marker: RemoteHookInstaller.containerMarker(bundle: bundle))) == .restore,
                     "the container's alone: the file goes back as it was")

        var container = ContainerInfo(id: "10773fc25185", name: "ob-plain", image: "x")
        container.user = "node"
        let line = ContainerOpen.attachLine(engine: "/opt/homebrew/bin/docker", container: container,
                                            pane: RemoteTmuxPane(socket: "/tmp/tmux-1000/default", pane: "%0", tty: "/dev/pts/1"))
        suite.expect(line == "'/opt/homebrew/bin/docker' 'exec' '-it' '-u' 'node' '10773fc25185' 'tmux' '-u' '-S' '/tmp/tmux-1000/default' 'attach-session' '-t' '%0'",
                     "Open attaches a terminal to the pane's tmux in the container, in UTF-8")
        suite.expect(ContainerEditor.url(folder: "/Users/me/a b", workspace: "/workspaces/a b")?.absoluteString
                     == "vscode://vscode-remote/dev-container+2f55736572732f6d652f612062/workspaces/a%20b",
                     "a devcontainer opens in its VS Code window")
        suite.expect(ContainerEditor.url(folder: "/Users/me/shop", workspace: nil)?.path == "/dev-container+2f55736572732f6d652f73686f70/workspaces/shop",
                     "the usual workspace folder when the mount isn't known")
    }

    // MARK: The relay

    static func relay(_ suite: TestSuite) {
        let manager = FileManager.default
        let home = manager.temporaryDirectory.appending(path: "openbar-container-home-\(UUID().uuidString)")
        defer { try? manager.removeItem(at: home) }
        try? manager.createDirectory(at: home.appending(path: ".claude/sessions"), withIntermediateDirectories: true)
        let probe = RemoteHostTests.runScript("probe x.sock \(bundle) container", home: home) ?? []
        suite.expect(RemoteAnswer.probe(probe.dropLast())?.hooksInstalled == false, "the probe looks for the container's marker")
        let settings = home.appending(path: ".claude/settings.json")
        manager.createFile(atPath: settings.path, contents: Data("{\"hooks\":{\"Stop\":[{\"hooks\":[{\"command\":\"x # openbar-container-hook \(bundle)\"}]}]}}".utf8))
        suite.expect(RemoteAnswer.probe((RemoteHostTests.runScript("probe x.sock \(bundle) container", home: home) ?? []).dropLast())?
                     .hooksInstalled == true, "and sees it")
        suite.expect(RemoteAnswer.probe((RemoteHostTests.runScript("probe x.sock \(bundle)", home: home) ?? []).dropLast())?
                     .hooksInstalled == false, "an SSH host's probe never takes it for its own")
        suite.expect(RemoteHostTests.runScript("relay ../x", home: home)?.first == "E name", "the relay's name can't leave ~/.openbar")

        // Restoring keeps the file's own mode, not the copy's 600.
        let original = Data("{\n  \"a\": 1\n}\n".utf8)
        manager.createFile(atPath: settings.path, contents: original, attributes: [.posixPermissions: 0o644])
        let sum = (RemoteHostTests.runScript("read", home: home) ?? []).first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        _ = RemoteHostTests.runScript("install", input: [sum, Data("{}\n".utf8).base64EncodedString()], home: home)
        let sum2 = (RemoteHostTests.runScript("read", home: home) ?? []).first { $0.hasPrefix("S ") }.map { String($0.dropFirst(2)) } ?? ""
        _ = RemoteHostTests.runScript("remove", input: ["x.sock", sum2, "restore"], home: home)
        let mode = (try? manager.attributesOfItem(atPath: settings.path))?[.posixPermissions] as? NSNumber
        suite.expect(manager.contents(atPath: settings.path) == original && mode?.intValue == 0o644,
                     "Remove puts the file back byte for byte, with its mode")

        // The relay, as `docker exec -i` runs it, and a hook beside it.
        guard let preamble = RemoteSSH.preamble("relay \(bundle)") else { return suite.expect(false, "the script") }
        let relay = Process()
        relay.executableURL = URL(fileURLWithPath: "/bin/sh")
        relay.arguments = ["-c", ContainerEngine.loader]
        relay.environment = ["HOME": home.path, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
        let feed = Pipe()
        let output = Pipe()
        relay.standardInput = feed
        relay.standardOutput = output
        relay.standardError = FileHandle.nullDevice
        guard (try? relay.run()) != nil else { return suite.expect(false, "the relay starts") }
        defer { if relay.isRunning { relay.terminate() } }
        feed.fileHandleForWriting.write(Data(preamble.utf8))
        let lines = LineReader(output.fileHandleForReading)
        suite.expect(lines.next(5) == "HELLO 1" && lines.next(5) == ".", "the relay says hello")
        let hookPath = RemoteHookInstaller.containerHook(home: home.path, bundle: bundle)
        suite.expect(manager.fileExists(atPath: hookPath)
                     && ((try? manager.attributesOfItem(atPath: home.appending(path: ".openbar").path))?[.posixPermissions] as? NSNumber)?
                        .intValue == 0o700, "it writes the hook in a folder only its user opens")

        let event = "{\"hook_event_name\":\"PermissionRequest\",\"tool_input\":{\"command\":\"ls 'a b'\"},\"pad\":\"\(String(repeating: "x", count: 100_000))\"}"
        let hook = Process()
        hook.executableURL = URL(fileURLWithPath: "/bin/sh")
        hook.arguments = [hookPath]
        hook.environment = ["HOME": home.path, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin"]
        let hookIn = Pipe()
        let hookOut = Pipe()
        hook.standardInput = hookIn
        hook.standardOutput = hookOut
        guard (try? hook.run()) != nil else { return suite.expect(false, "the hook runs") }
        hookIn.fileHandleForWriting.write(Data(event.utf8))
        try? hookIn.fileHandleForWriting.close()
        let line = lines.next(10) ?? ""
        let words = line.split(separator: " ", maxSplits: 2).map(String.init)
        suite.expect(words.count == 3 && words[0] == "H" && words[1] == String(hook.processIdentifier)
                     && Data(base64Encoded: words[2]) == Data(event.utf8),
                     "the hook's event reaches the island whole, under the hook's id")
        let answer = "{\"hookSpecificOutput\":{\"decision\":{\"behavior\":\"allow\"}}}"
        feed.fileHandleForWriting.write(Data("A \(words.count > 1 ? words[1] : "0") \(Data(answer.utf8).base64EncodedString())\n".utf8))
        let printed = hookOut.fileHandleForReading.readDataToEndOfFile()
        hook.waitUntilExit()
        suite.expect(String(decoding: printed, as: UTF8.self) == answer && hook.terminationStatus == 0,
                     "the island's answer is what the hook prints")

        // The island goes: the relay ends, and a hook doesn't wait on it.
        try? feed.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(5)
        while relay.isRunning, Date() < deadline { usleep(50_000) }
        let started = Date()
        let late = Process()
        late.executableURL = URL(fileURLWithPath: "/bin/sh")
        late.arguments = [hookPath]
        late.environment = ["HOME": home.path, "PATH": "/usr/bin:/bin"]
        late.standardInput = FileHandle.nullDevice
        late.standardOutput = FileHandle.nullDevice
        try? late.run()
        late.waitUntilExit()
        suite.expect(!relay.isRunning && late.terminationStatus == 0 && Date().timeIntervalSince(started) < 2
                     && !manager.fileExists(atPath: home.appending(path: ".openbar/\(bundle).pid").path),
                     "with no island, the relay is gone and a hook ends at once")
    }

    /// Lines from a pipe, waiting a moment for each.
    final class LineReader {
        private let handle: FileHandle
        private var buffer = Data()
        init(_ handle: FileHandle) { self.handle = handle }

        func next(_ timeout: TimeInterval) -> String? {
            let deadline = Date().addingTimeInterval(timeout)
            while true {
                if let end = buffer.firstIndex(of: 0x0A) {
                    let line = String(decoding: buffer[buffer.startIndex..<end], as: UTF8.self)
                    buffer = Data(buffer[buffer.index(after: end)...])
                    return line
                }
                guard Date() < deadline else { return nil }
                var poller = pollfd(fd: handle.fileDescriptor, events: Int16(POLLIN), revents: 0)
                guard poll(&poller, 1, 200) > 0 else { continue }
                let chunk = handle.availableData
                if chunk.isEmpty { return nil }
                buffer.append(chunk)
            }
        }
    }

    // MARK: Adoption

    static func adoption(_ suite: TestSuite) {
        var template = Array("/tmp/obct.XXXXXX".utf8CString)
        guard let made = mkdtemp(&template) else { return suite.expect(false, "a short folder") }
        let folder = String(cString: made)
        defer { try? FileManager.default.removeItem(atPath: folder) }
        let queue = DispatchQueue(label: "openbar.tests.container-adopt")
        let got = DispatchSemaphore(value: 0)
        var peer: pid_t = 0
        var payload = Data()
        let server = ClaudeHookServer(path: folder + "/h.sock", queue: queue) { data, connection in
            payload = data
            peer = connection.peer
            connection.finish(Data("{\"ok\":true}".utf8))
            got.signal()
        }
        suite.expect(queue.sync { server.start() }, "the server starts")
        var pair: [Int32] = [-1, -1]
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &pair) == 0 else { return suite.expect(false, "a socket pair") }
        queue.async { server.adopt(pair[1], peer: 4242) }
        let event = Data("{\"hook_event_name\":\"PermissionRequest\",\"session_id\":\"s\"}".utf8)
        let reply = ContainerRelay.exchange(pair[0], event)
        close(pair[0])
        suite.expect(got.wait(timeout: .now() + 5) == .success && payload == event && peer == 4242,
                     "a relayed hook reaches the handler like a connected one, with the relay as its peer")
        suite.expect(reply == Data("{\"ok\":true}".utf8), "and its answer goes back the same way")
        suite.expect(ContainerRelay.id("4082") == "4082" && ContainerRelay.id("1;rm") == nil && ContainerRelay.id("") == nil,
                     "a hook's id is a number and nothing else")
        queue.sync { server.stop() }
    }
}
