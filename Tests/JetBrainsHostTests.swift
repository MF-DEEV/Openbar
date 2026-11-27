// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import AppKit
import Foundation

/// JetBrains IDEs as hosts (#329): telling a session in one, the IDE apps
/// and their plugins folders, the bridge plugin's install, the project
/// window, and the lock file read without Claude's token.
enum JetBrainsHostTests {
    static func run(_ suite: TestSuite) {
        suite.run("sessions JetBrains hosts") { hosts(suite) }
        suite.run("sessions JetBrains IDEs") { ides(suite) }
        suite.run("sessions JetBrains lock file") { lockFile(suite) }
        suite.run("sessions JetBrains bridge") { bridge(suite) }
    }

    private static let jetbrains = ["TERMINAL_EMULATOR": "JetBrains-JediTerm"]

    static func hosts(_ suite: TestSuite) {
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.jetbrains.pycharm", tty: "/dev/ttys040", port: nil,
                                                 environment: jetbrains, owner: "com.jetbrains.pycharm")
                     == .jetbrains(bundle: "com.jetbrains.pycharm", ide: nil),
                     "a session in PyCharm's terminal is PyCharm's")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys040", port: nil,
                                                 environment: jetbrains, owner: "com.jetbrains.WebStorm")
                     == .jetbrains(bundle: "com.jetbrains.WebStorm", ide: nil),
                     "the app responsible for the process wins over an identifier the IDE inherited")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.jetbrains.goland", tty: "/dev/ttys041", port: nil)
                     == .jetbrains(bundle: "com.jetbrains.goland", ide: nil),
                     "an IDE's identifier alone is enough")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.jetbrains.intellij", tty: "/dev/ttys042", port: 5000,
                                                 environment: jetbrains.merging(["TERM_PROGRAM": "vscode"]) { $1 },
                                                 owner: "com.microsoft.VSCode")
                     == .ide(bundle: "com.microsoft.VSCode", port: 5000),
                     "VS Code started from an IDE's terminal is VS Code")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.jetbrains.intellij", tty: "/dev/ttys043", port: nil,
                                                 environment: jetbrains.merging(["TERM_PROGRAM": "iTerm.app"]) { $1 },
                                                 owner: "com.googlecode.iterm2")
                     == .iterm(tty: "/dev/ttys043"),
                     "a terminal naming itself wins over the variable it inherited")
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.jetbrains.intellij", tty: "/dev/ttys044", port: nil,
                                                 environment: jetbrains.merging(["TERM": "xterm-kitty", "KITTY_WINDOW_ID": "3"]) { $1 },
                                                 owner: "net.kovidgoyal.kitty")
                     == .kitty(window: 3, socket: nil),
                     "so does kitty, by its TERM")
        let tmux = jetbrains.merging(["TMUX": "/private/tmp/tmux-501/default,4242,0", "TMUX_PANE": "%3"]) { $1 }
        if case .tmux = ClaudeSessionHosts.classify(bundle: "com.jetbrains.pycharm", tty: "/dev/ttys045", port: nil,
                                                    environment: tmux, owner: "com.jetbrains.pycharm") {
            suite.expect(true, "a tmux pane in an IDE's terminal is tmux's")
        } else {
            suite.expect(false, "a tmux pane in an IDE's terminal is tmux's")
        }
        suite.expect(ClaudeSessionHosts.classify(bundle: "com.apple.Terminal", tty: "/dev/ttys046", port: nil,
                                                 environment: jetbrains, owner: "com.apple.Terminal")
                     == .terminal(tty: "/dev/ttys046"),
                     "the variable alone, in an app that is no IDE, changes nothing")
        suite.expect(ClaudeSessionHosts.hostKeys.contains("TERMINAL_EMULATOR"), "the variable is read from the session's environment")
        suite.expect(ClaudeJetBrains.isIDE("com.google.android.studio") && ClaudeJetBrains.isIDE("com.jetbrains.rustrover")
                     && !ClaudeJetBrains.isIDE("com.jetbrains.toolbox-not-installed") && !ClaudeJetBrains.isIDE("com.apple.Terminal")
                     && !ClaudeJetBrains.isIDE(nil),
                     "the IDEs are known by their identifiers, other apps aren't IDEs")

        // The IDE process is the nearest ancestor that is the IDE's app.
        let parents: [Int32: Int32] = [500: 400, 400: 300, 300: 200, 200: 1]
        let bundles: [Int32: String] = [300: "com.jetbrains.pycharm", 200: "com.apple.Terminal"]
        suite.expect(ClaudeJetBrains.process(above: 500, bundle: "com.jetbrains.pycharm", parent: { parents[$0] }, bundleOf: { bundles[$0] }) == 300,
                     "the IDE process is found above the session's shell")
        suite.expect(ClaudeJetBrains.process(above: 500, bundle: "com.jetbrains.WebStorm", parent: { parents[$0] }, bundleOf: { bundles[$0] }) == nil,
                     "another IDE's process never stands in")

        let host = ClaudeSessionHost.jetbrains(bundle: "com.jetbrains.pycharm", ide: 999_999)
        suite.expect(host.bundle == "com.jetbrains.pycharm" && host.multiplexer == nil, "the row names the IDE's app")
        suite.expect(!host.acceptsReplies && !ClaudeSessionHost.jetbrains(bundle: "com.jetbrains.pycharm", ide: nil).acceptsReplies,
                     "without the plugin listening, replies don't reach it")
        suite.expect(!ClaudeTyping.takesKeys(host), "nor keys")
        suite.expect(UsageStatsSupport.terminalID(host) == "jetbrains", "usage stats count it as JetBrains, never by app")
        let text = FeatureStrings.notchClaudeSessions(.enUS)
        if !ClaudeJetBrains.installed(bundle: "com.jetbrains.pycharm") {
            suite.expect(text.replyUnavailable(for: host) == text.replyJetBrainsBridge, "the reply field asks for the bridge")
        }
        suite.expect(text.replyJetBrainsRestart("PyCharm") == "Restart PyCharm to reply", "or for a restart once it is in")
        suite.expect(ClaudeTerminalFocus.state(.jetbrains(bundle: "com.jetbrains.pycharm", ide: nil), pid: 1, hostOf: { _ in .unknown }) == .unknown,
                     "without the plugin, whether the tab is in front is unknown and the sound plays")

        // The project window, by its title.
        let titles = ["alpha – README.md", "beta [~/work/beta] – main.py", "openbar", "Welcome to IntelliJ IDEA"]
        suite.expect(ClaudeJetBrains.project(inTitle: "beta [~/work/beta] – main.py") == "beta"
                     && ClaudeJetBrains.project(inTitle: "alpha - x.kt") == "alpha" && ClaudeJetBrains.project(inTitle: "gamma") == "gamma",
                     "a window title names its project first")
        suite.expect(ClaudeJetBrains.window(for: "/tmp/jb/alpha", titles: titles) == 0, "the window of the session's project")
        suite.expect(ClaudeJetBrains.window(for: "/Users/me/work/beta/src/deep", titles: titles) == 1,
                     "the project holding the session's folder, from deep inside it")
        suite.expect(ClaudeJetBrains.window(for: "/Users/me/elsewhere/gamma", titles: titles) == nil, "no window for a project not open")
        suite.expect(ClaudeJetBrains.window(for: "/w/alpha", titles: ["alpha – a", "alpha – b"]) == nil,
                     "two windows of the same name are left alone")
        suite.expect(ClaudeJetBrains.window(for: "/w/app/sub", titles: ["app – x", "sub – y"]) == 1, "the deepest project wins")
        suite.expect(ClaudeJetBrains.window(for: "/w/alphabet", titles: ["alpha – a"]) == nil, "a name is matched whole")
    }

    static func ides(_ suite: TestSuite) {
        let root = URL(fileURLWithPath: "/tmp/ob-jb-\(getpid())")
        defer { try? FileManager.default.removeItem(at: root) }
        func app(_ name: String, bundle: String, product: [String: Any]) -> URL {
            let app = root.appending(path: "Applications/\(name).app")
            try? FileManager.default.createDirectory(at: app.appending(path: "Contents/Resources"), withIntermediateDirectories: true)
            let plist: [String: Any] = ["CFBundleIdentifier": bundle, "CFBundlePackageType": "APPL"]
            (plist as NSDictionary).write(to: app.appending(path: "Contents/Info.plist"), atomically: true)
            try? JSONSerialization.data(withJSONObject: product).write(to: app.appending(path: "Contents/Resources/product-info.json"))
            return app
        }
        let pycharm = app("PyCharm", bundle: "com.jetbrains.pycharm",
                          product: ["name": "PyCharm", "version": "2026.3", "productCode": "PY", "dataDirectoryName": "PyCharm2026.3",
                                    "productVendor": "JetBrains", "launch": [["os": "macOS"]]])
        let studio = app("Android Studio", bundle: "com.google.android.studio",
                         product: ["name": "Android Studio", "version": "2026.1", "productCode": "AI",
                                   "dataDirectoryName": "AndroidStudio2026.1", "productVendor": "Google"])
        let gateway = app("JetBrains Gateway", bundle: "com.jetbrains.gateway",
                          product: ["name": "Gateway", "productCode": "GW", "dataDirectoryName": "JetBrainsGateway2026.3"])
        let escaping = app("Odd", bundle: "com.jetbrains.odd", product: ["name": "Odd", "dataDirectoryName": "../../x"])
        let home = root.appending(path: "home")

        let ide = ClaudeJetBrains.ide(app: pycharm)
        suite.expect(ide?.name == "PyCharm" && ide?.bundle == "com.jetbrains.pycharm" && ide?.dataDirectory == "PyCharm2026.3"
                     && ide?.vendor == "JetBrains" && ide?.plugin == "PyCharm 2026.3",
                     "an IDE is read from its product-info.json")
        suite.expect(ide?.plugins(home: home).path == home.path + "/Library/Application Support/JetBrains/PyCharm2026.3/plugins",
                     "its plugins folder is under its settings folder")
        suite.expect(ClaudeJetBrains.ide(app: studio)?.plugins(home: home).path
                     == home.path + "/Library/Application Support/Google/AndroidStudio2026.1/plugins",
                     "Android Studio keeps its settings under Google")
        suite.expect(ClaudeJetBrains.ide(app: gateway) == nil, "Gateway has no terminal of its own and is left out")
        suite.expect(ClaudeJetBrains.ide(app: escaping) == nil, "a settings folder that leaves its parent is refused")
        suite.expect(ClaudeJetBrains.ide(app: URL(fileURLWithPath: "/System/Applications/Calculator.app")) == nil, "any other app is none")

        // Install, update and remove.
        guard let ide else { return }
        let bundled = root.appending(path: "bundled.jar")
        try? Data("plugin v1".utf8).write(to: bundled)
        suite.expect(ClaudeJetBrains.state(ide, home: home, bundled: bundled) == .missing, "a fresh IDE has no plugin")
        suite.expect(ClaudeJetBrains.install(ide, home: home, bundled: bundled) == nil, "the plugin goes in, its folder made")
        let installed = ide.plugins(home: home).appending(path: ClaudeJetBrains.pluginFile)
        suite.expect(FileManager.default.contents(atPath: installed.path) == Data("plugin v1".utf8), "as the jar the app carries")
        suite.expect(ClaudeJetBrains.state(ide, home: home, bundled: bundled) == .current, "and is current")
        try? Data("plugin v2".utf8).write(to: bundled)
        suite.expect(ClaudeJetBrains.state(ide, home: home, bundled: bundled) == .outdated, "a newer jar in the app makes it outdated")
        suite.expect(ClaudeJetBrains.install(ide, home: home, bundled: bundled) == nil
                     && FileManager.default.contents(atPath: installed.path) == Data("plugin v2".utf8),
                     "an update puts the new jar in the old one's place")
        let leftovers = (try? FileManager.default.contentsOfDirectory(atPath: ide.plugins(home: home).path)) ?? []
        suite.expect(leftovers == [ClaudeJetBrains.pluginFile], "nothing half written is left beside it: \(leftovers)")
        suite.expect(ClaudeJetBrains.install(ide, home: home, bundled: root.appending(path: "none.jar")) == .missing,
                     "a build without the plugin says so")
        suite.expect(ClaudeJetBrains.remove(ide, home: home) == nil && ClaudeJetBrains.state(ide, home: home, bundled: bundled) == .missing,
                     "Remove takes it out")
        suite.expect(ClaudeJetBrains.remove(ide, home: home) == nil, "removing it again is no failure")
        suite.expect(!ClaudeJetBrains.isRunning(ide), "a copy nobody started isn't running")
    }

    static func lockFile(_ suite: TestSuite) {
        let token = "secret-\(UUID().uuidString)"
        let json = #"{"pid":4321,"workspaceFolders":["/Users/me/alpha","/Users/me/beta"],"ideName":"PyCharm","transport":"ws","runningInWindows":false,"authToken":"\#(token)"}"#
        let lock = ClaudeIDELockFile.decode(Data(json.utf8))
        suite.expect(lock?.pid == 4321 && lock?.ideName == "PyCharm" && lock?.workspaceFolders == ["/Users/me/alpha", "/Users/me/beta"]
                     && lock?.transport == "ws", "the lock file gives the IDE's process and its open folders")
        suite.expect(Mirror(reflecting: lock as Any).descendant("some", "authToken") == nil
                     && !Mirror(reflecting: lock!).children.contains { $0.label?.lowercased().contains("token") == true },
                     "the type that reads it has no field for Claude's token")
        suite.expect(!String(describing: lock as Any).contains(token) && !String(reflecting: lock as Any).contains(token),
                     "and the token is nowhere in what was read")
        suite.expect(ClaudeIDELockFile.decode(Data("not json".utf8)) == nil, "a broken lock file is none")
        suite.expect(ClaudeIDELockFile.decode(Data(repeating: 0x20, count: 1 << 17)) == nil, "an outsized one is none")

        let home = URL(fileURLWithPath: "/tmp/ob-jb-lock-\(getpid())")
        defer { try? FileManager.default.removeItem(at: home) }
        try? FileManager.default.createDirectory(at: home.appending(path: "ide"), withIntermediateDirectories: true)
        try? Data(json.utf8).write(to: home.appending(path: "ide/63342.lock"))
        try? Data(#"{"pid":777,"workspaceFolders":["/Users/me/gamma"],"authToken":"x"}"#.utf8).write(to: home.appending(path: "ide/63343.lock"))
        try? Data("junk".utf8).write(to: home.appending(path: "ide/63344.lock"))
        suite.expect(ClaudeIDELock.all(homes: [home]).count == 2, "every readable lock file is read, junk left out")
        suite.expect(ClaudeJetBrains.openProject(cwd: "/Users/me/beta/src", ide: 4321, homes: [home]) == "/Users/me/beta",
                     "Open asks the IDE for the project holding the session, open in that very IDE")
        suite.expect(ClaudeJetBrains.openProject(cwd: "/Users/me/gamma", ide: 4321, homes: [home]) == nil,
                     "never for a project another IDE has open, which would open it here")
        suite.expect(ClaudeIDELock.folders(port: 63342, homes: [home]) == ["/Users/me/alpha", "/Users/me/beta"],
                     "an editor's lock file by its port reads the same way")
    }

    static func bridge(_ suite: TestSuite) {
        suite.expect(ClaudeTypingStep.paste("hi").bridgeStep as? [String: String] == ["paste": "hi"]
                     && ClaudeTypingStep.wait(0.08).bridgeStep as? [String: Int] == ["wait": 80]
                     && ClaudeTypingStep.enter.bridgeStep as? [String: Bool] == ["enter": true]
                     && ClaudeTypingStep.interrupt.bridgeStep as? [String: String] == ["text": "\u{3}"]
                     && ClaudeTypingStep.keys([.down, .digit(2)]).bridgeStep as? [String: String] == ["text": "\u{1B}[B2"],
                     "steps are spelled as the bridges take them")

        // The plugin listens in ~/.openbar/terminals as <IDE pid>.sock.
        let home = URL(fileURLWithPath: "/tmp/ob-jb-br-\(getpid())")
        let folder = ClaudeTerminalBridge.folder(home: home)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: home) }
        let queue = DispatchQueue(label: "test.jetbrains-bridge")
        let lock = NSLock()
        var requests: [[String: Any]] = []
        let ide: Int32 = 4242
        let server = ClaudeHookServer(path: folder.appending(path: "\(ide).sock").path, queue: queue) { payload, connection in
            let request = (try? JSONSerialization.jsonObject(with: payload)) as? [String: Any] ?? [:]
            lock.withLock { requests.append(request) }
            let answer: String
            switch (request["shellPid"] as? Int, request["command"] as? String) {
            case (222?, "shown"?): answer = #"{"ok":true,"shown":true}"#
            case (223?, "shown"?): answer = #"{"ok":true,"shown":false}"#
            case (222?, _), (223?, _): answer = #"{"ok":true}"#
            default: answer = #"{"ok":false,"error":"missing"}"#
            }
            connection.finish(Data(answer.utf8 + [0x0A]))
        }
        suite.expect(queue.sync { server.start() }, "a stand-in plugin listens")
        defer { queue.sync { server.stop() } }
        suite.expect(ClaudeJetBrains.listening(ide: ide, home: home) && !ClaudeJetBrains.listening(ide: ide + 1, home: home),
                     "the island sees the plugin listening in that IDE by its socket")
        try? Data().write(to: folder.appending(path: "7.sock"))
        suite.expect(!ClaudeJetBrains.listening(ide: 7, home: home), "a plain file is no socket")

        let steps = ClaudeTyping.steps("line one\nline two", enterAfter: 0.5)
        suite.expect(ClaudeTerminalBridge.steps(steps, shell: 222, folder: folder), "a reply goes to the IDE holding the shell")
        let sent = lock.withLock { requests.last }
        let sentSteps = sent?["steps"] as? [[String: Any]] ?? []
        suite.expect(sent?["command"] as? String == "steps" && sent?["shellPid"] as? Int == 222 && sentSteps.count == 3
                     && sentSteps[0]["paste"] as? String == "line one\nline two" && sentSteps[1]["wait"] as? Int == 500
                     && sentSteps[2]["enter"] as? Bool == true,
                     "as one paste, the agent's pause and a return")
        suite.expect(!ClaudeTerminalBridge.steps(steps, shell: 333, folder: folder), "a shell no IDE holds is no reply")
        suite.expect(ClaudeTerminalBridge.shown(shell: 222, folder: folder) == true, "the plugin says the tab is in front")
        suite.expect(ClaudeTerminalBridge.shown(shell: 223, folder: folder) == false, "or behind")
        suite.expect(ClaudeTerminalBridge.shown(shell: 333, folder: folder) == nil, "and nothing for a tab it doesn't hold")
    }
}
