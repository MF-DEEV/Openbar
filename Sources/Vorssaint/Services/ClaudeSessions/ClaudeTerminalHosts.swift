// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Darwin
import Foundation

/// Terminal apps the island knows by name, besides Terminal itself.
enum ClaudeTerminalApps {
    static let iterm = "com.googlecode.iterm2"
    static let ghostty = "com.mitchellh.ghostty"
    static let wezterm = "com.github.wez.wezterm"
    static let kitty = "net.kovidgoyal.kitty"
    static let warp = "dev.warp.Warp-Stable"
    static let alacritty = "org.alacritty"

    /// The app behind a terminal's own `TERM_PROGRAM`. The innermost
    /// terminal sets it, so it wins over a bundle identifier inherited from
    /// whatever started that terminal.
    static func bundle(program: String?) -> String? {
        switch program {
        case "Apple_Terminal": ClaudeSessionHosts.terminal
        case "iTerm.app": iterm
        case "ghostty": ghostty
        case "WezTerm": wezterm
        case "WarpTerminal": warp
        default: nil
        }
    }

    /// The name an app shows, for "Open in …" and the reply notice.
    static func name(of bundle: String) -> String {
        names.lock()
        defer { names.unlock() }
        if let name = named[bundle] { return name }
        var name = known[bundle] ?? bundle
        if known[bundle] == nil, let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) {
            name = (FileManager.default.displayName(atPath: url.path) as NSString).deletingPathExtension
        }
        named[bundle] = name
        return name
    }

    private static let names = NSLock()
    private static var named: [String: String] = [:]

    private static let known = [
        ClaudeSessionHosts.terminal: "Terminal", iterm: "iTerm2", ghostty: "Ghostty", wezterm: "WezTerm",
        kitty: "kitty", warp: "Warp", alacritty: "Alacritty",
    ]
}

extension ClaudeSessionHost {
    /// What the session runs in, as the person knows it: "iTerm2", or
    /// "tmux in Ghostty" for a pane of a tmux that Ghostty shows.
    var displayName: String? {
        if case .tmux(let target) = self {
            return target.outer.map { "tmux in " + ClaudeTerminalApps.name(of: $0) } ?? "tmux"
        }
        return bundle.map(ClaudeTerminalApps.name(of:))
    }
}

/// An editor built on VS Code, whose integrated terminal the bridge
/// extension reaches. Its command line and extensions folder come from the
/// editor's own `product.json`, the way the editor works them out itself.
struct ClaudeIDE: Equatable, Identifiable {
    let bundle: String
    let name: String
    var id: String { bundle }

    static let antigravity = ClaudeIDE(bundle: ClaudeSessionHosts.antigravity, name: "Antigravity")
    static let all: [ClaudeIDE] = [
        antigravity,
        ClaudeIDE(bundle: "com.microsoft.VSCode", name: "Visual Studio Code"),
        ClaudeIDE(bundle: "com.microsoft.VSCodeInsiders", name: "Visual Studio Code - Insiders"),
        ClaudeIDE(bundle: "com.todesktop.230313mzl4w4u92", name: "Cursor"),
        ClaudeIDE(bundle: "com.exafunction.windsurf", name: "Windsurf"),
        ClaudeIDE(bundle: "com.vscodium", name: "VSCodium"),
    ]

    static func find(_ bundle: String?) -> ClaudeIDE? { all.first { $0.bundle == bundle } }

    var app: URL? { NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundle) }

    /// The command line in the app, and the folder its extensions go to.
    func places(home: URL) -> (cli: URL, extensions: URL)? {
        guard let app else { return nil }
        return Self.places(app: app, home: home)
    }

    static func places(app: URL, home: URL) -> (cli: URL, extensions: URL)? {
        let resources = app.appending(path: "Contents/Resources/app")
        guard let data = FileManager.default.contents(atPath: resources.appending(path: "product.json").path),
              data.count < 1 << 20,
              let product = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let command = product["applicationName"] as? String, Self.plain(command),
              let folder = product["dataFolderName"] as? String, Self.plain(folder) else { return nil }
        return (resources.appending(path: "bin/\(command)"), home.appending(path: "\(folder)/extensions"))
    }

    /// A name with no way out of the folder it is joined to.
    private static func plain(_ name: String) -> Bool {
        !name.isEmpty && !name.contains("/") && name != "." && name != ".."
    }
}

/// What typing a reply into a terminal comes down to, whatever the terminal:
/// pastes that keep line breaks as text, pauses, and the return that sends it.
enum ClaudeTypingStep: Equatable {
    case paste(String)
    case wait(Double)
    case enter
}

enum ClaudeTyping {
    /// Claude Code attaches an image when a paste is only its path and reads
    /// it a moment later, so the text and the return wait for the images.
    static func steps(_ text: String, images: [String] = []) -> [ClaudeTypingStep] {
        var steps: [ClaudeTypingStep] = []
        for image in images { steps += [.paste(image), .wait(0.06)] }
        if !images.isEmpty { steps.append(.wait(ClaudeTerminalScripts.imageSettle(images.count))) }
        if !text.isEmpty { steps.append(.paste(text)) }
        steps += [.wait(0.08), .enter]
        return steps
    }

    /// The longest a sequence takes, for a timeout that outlasts it.
    static func duration(_ steps: [ClaudeTypingStep]) -> Double {
        steps.reduce(0) { total, step in if case .wait(let seconds) = step { total + seconds } else { total } }
    }
}

/// Runs a terminal's own command line and reads its answer, without ever
/// waiting on it for long.
enum ClaudeHostCommand {
    /// Standard output when the tool exits with 0, nil otherwise.
    static func run(_ tool: String, _ arguments: [String], input: String? = nil,
                    environment: [String: String] = [:], timeout: TimeInterval = 4) -> String? {
        guard FileManager.default.isExecutableFile(atPath: tool) else { return nil }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        if !environment.isEmpty {
            process.environment = ProcessInfo.processInfo.environment.merging(environment) { $1 }
        }
        let output = Pipe()
        let feed = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        process.standardInput = input == nil ? FileHandle.nullDevice : feed
        let done = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in done.signal() }
        guard (try? process.run()) != nil else { return nil }
        if let input {
            feed.fileHandleForWriting.write(Data(input.utf8))
            try? feed.fileHandleForWriting.close()
        }
        // Read while it runs, so a long answer never fills the pipe and stalls it.
        var data = Data()
        let reader = DispatchQueue(label: "vorssaint.host-command")
        let read = DispatchSemaphore(value: 0)
        reader.async {
            data = output.fileHandleForReading.readDataToEndOfFile()
            read.signal()
        }
        if done.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            _ = done.wait(timeout: .now() + 1)
        }
        _ = read.wait(timeout: .now() + 1)
        guard process.terminationReason == .exit, process.terminationStatus == 0 else { return nil }
        return String(decoding: data, as: UTF8.self)
    }
}

// MARK: iTerm2 and Ghostty

/// iTerm2 and Ghostty name the device of each session, so a session is found
/// by the one its shell runs on, as in Terminal.
enum ClaudeTTYTerminalScripts {
    /// The ways the app may write a device: "/dev/ttys004" or "ttys004".
    private static func names(_ tty: String) -> String {
        let short = tty.hasPrefix("/dev/") ? String(tty.dropFirst(5)) : tty
        return "{\(AppleScriptRunner.literal("/dev/" + short)), \(AppleScriptRunner.literal(short))}"
    }

    static func itermFocus(tty: String) -> String {
        iterm(tty: tty, found: """
            select theWindow
            select theTab
            select theSession
            activate
            return "found"
            """)
    }

    static func itermType(_ steps: [ClaudeTypingStep], tty: String) -> String {
        let lines = steps.map { step -> String in
            switch step {
            case .paste(let text):
                "tell theSession to write text (marker & \"[200~\" & \(AppleScriptRunner.literal(text)) & marker & \"[201~\") newline no"
            case .wait(let seconds): "delay \(seconds)"
            case .enter: "tell theSession to write text (character id 13) newline no"
            }
        }
        return iterm(tty: tty, found: (lines + ["return \"sent\""]).joined(separator: "\n"))
    }

    private static func iterm(tty: String, found: String) -> String {
        """
        set marker to character id 27
        set wanted to \(names(tty))
        tell application id "\(ClaudeTerminalApps.iterm)"
            repeat with theWindow in windows
                repeat with theTab in tabs of theWindow
                    repeat with theSession in sessions of theTab
                        if wanted contains (tty of theSession) then
        \(found)
                        end if
                    end repeat
                end repeat
            end repeat
        end tell
        return "missing"
        """
    }

    static func ghosttyFocus(tty: String) -> String {
        ghostty(tty: tty, found: """
            activate window theWindow
            select tab theTab
            focus theTerminal
            activate
            return "found"
            """)
    }

    /// Ghostty's `input text` is a paste, bracketed when the program asks
    /// for it, so line breaks stay text.
    static func ghosttyType(_ steps: [ClaudeTypingStep], tty: String) -> String {
        let lines = steps.map { step -> String in
            switch step {
            case .paste(let text): "input text \(AppleScriptRunner.literal(text)) to theTerminal"
            case .wait(let seconds): "delay \(seconds)"
            case .enter: "send key \"enter\" to theTerminal"
            }
        }
        return ghostty(tty: tty, found: (lines + ["return \"sent\""]).joined(separator: "\n"))
    }

    /// Ghostty before 1.4 has no `tty`; asking for it fails, and the session
    /// then counts as not found, never as some other terminal.
    private static func ghostty(tty: String, found: String) -> String {
        """
        set wanted to \(names(tty))
        tell application id "\(ClaudeTerminalApps.ghostty)"
            repeat with theWindow in windows
                repeat with theTab in tabs of theWindow
                    repeat with theTerminal in terminals of theTab
                        if wanted contains (tty of theTerminal) then
        \(found)
                        end if
                    end repeat
                end repeat
            end repeat
        end tell
        return "missing"
        """
    }
}

// MARK: tmux

/// A tmux pane a session runs in: its id and the server that owns it, from
/// the `TMUX` and `TMUX_PANE` tmux puts in every pane's environment.
struct ClaudeTmuxPane: Equatable {
    let pane: String
    let socket: String
    let server: Int32
    /// The app of the terminal showing that tmux, when one is attached.
    var outer: String?

    /// `TMUX` is "socket path,server pid,session index"; a pane id is "%3".
    init?(tmux: String, pane: String) {
        let parts = tmux.split(separator: ",", omittingEmptySubsequences: false)
        guard parts.count >= 2, parts[0].hasPrefix("/"), let server = Int32(parts[1]), server > 1,
              pane.count > 1, pane.first == "%", pane.dropFirst().allSatisfy(\.isNumber) else { return nil }
        self.pane = pane
        self.socket = String(parts[0])
        self.server = server
        self.outer = nil
    }
}

/// A terminal attached to a tmux session, which Open brings forward.
struct ClaudeTmuxClient: Equatable {
    let tty: String
    let pid: Int32
    /// Showing another session of the server, so it has to switch first.
    let elsewhere: Bool
}

/// Reaches a session in tmux through tmux itself: the pane is checked
/// against the session's device before anything is selected or typed.
enum ClaudeTmux {
    static let candidates = ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux", "/opt/local/bin/tmux", "/usr/bin/tmux"]

    /// The server's own binary first, so client and server always match.
    static func binary(server: Int32) -> String? {
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        if proc_pidpath(server, &path, UInt32(path.count)) > 0 {
            let found = String(cString: path)
            if (found as NSString).lastPathComponent == "tmux", FileManager.default.isExecutableFile(atPath: found) {
                return found
            }
        }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ target: ClaudeTmuxPane, _ arguments: [String], input: String? = nil) -> String? {
        guard let tmux = binary(server: target.server) else { return nil }
        return ClaudeHostCommand.run(tmux, ["-S", target.socket] + arguments, input: input)
    }

    static func tty(of target: ClaudeTmuxPane) -> String? {
        run(target, ["display-message", "-p", "-t", target.pane, "#{pane_tty}"])?
            .trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
    }

    /// The pane still runs the session: a pane id comes back after a server
    /// restart, on another pane.
    static func holds(_ target: ClaudeTmuxPane, tty: String?) -> Bool {
        guard let tty, let paneTTY = self.tty(of: target) else { return false }
        return paneTTY == tty
    }

    static func send(_ steps: [ClaudeTypingStep], to target: ClaudeTmuxPane, tty: String?) -> ClaudeReplyFailure? {
        guard holds(target, tty: tty) else { return .unreachable }
        for step in steps {
            switch step {
            case .paste(let text):
                // A buffer of its own, pasted as a bracketed paste when the
                // program asked for one, with its line breaks kept and then deleted.
                let buffer = "vorssaint-\(UUID().uuidString)"
                guard run(target, ["load-buffer", "-b", buffer, "-"], input: text) != nil,
                      run(target, ["paste-buffer", "-p", "-r", "-d", "-b", buffer, "-t", target.pane]) != nil else {
                    return .unreachable
                }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                guard run(target, ["send-keys", "-t", target.pane, "Enter"]) != nil else { return .unreachable }
            }
        }
        return nil
    }

    /// Selects the pane's window and the pane, and names the terminal that
    /// shows it; nil when no terminal is attached to that server.
    static func focus(_ target: ClaudeTmuxPane, tty: String?) -> ClaudeTmuxClient? {
        guard holds(target, tty: tty),
              let session = run(target, ["display-message", "-p", "-t", target.pane, "#{session_id}"])?
                .trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty else { return nil }
        run(target, ["select-window", "-t", target.pane])
        run(target, ["select-pane", "-t", target.pane])
        guard let listed = run(target, ["list-clients", "-F", "#{client_tty}\t#{client_pid}\t#{session_id}\t#{client_activity}"]),
              let client = client(from: listed, session: session) else { return nil }
        if client.elsewhere { run(target, ["switch-client", "-c", client.tty, "-t", target.pane]) }
        return client
    }

    /// The terminal attached to the pane's session, without selecting anything.
    static func client(of target: ClaudeTmuxPane) -> ClaudeTmuxClient? {
        guard let session = run(target, ["display-message", "-p", "-t", target.pane, "#{session_id}"])?
                .trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
              let listed = run(target, ["list-clients", "-F", "#{client_tty}\t#{client_pid}\t#{session_id}\t#{client_activity}"])
        else { return nil }
        return client(from: listed, session: session)
    }

    /// The terminal to show the pane in: the one on its session used last,
    /// else the one used last at all, which then switches to it.
    static func client(from listed: String, session: String) -> ClaudeTmuxClient? {
        let clients = listed.split(separator: "\n").compactMap { line -> (tty: String, pid: Int32, session: String, activity: Int)? in
            let fields = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
            guard fields.count >= 4, fields[0].hasPrefix("/dev/"), let pid = Int32(fields[1]), pid > 1 else { return nil }
            return (fields[0], pid, fields[2], Int(fields[3]) ?? 0)
        }
        let here = clients.filter { $0.session == session }
        if let best = here.max(by: { $0.activity < $1.activity }) {
            return ClaudeTmuxClient(tty: best.tty, pid: best.pid, elsewhere: false)
        }
        guard let best = clients.max(by: { $0.activity < $1.activity }) else { return nil }
        return ClaudeTmuxClient(tty: best.tty, pid: best.pid, elsewhere: true)
    }
}

// MARK: WezTerm

/// Reaches a pane of WezTerm through its command line, by the pane id
/// WezTerm puts in the pane's environment, checked against its device.
enum ClaudeWezTerm {
    static func cli() -> String? {
        let bundled = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeTerminalApps.wezterm)?
            .appending(path: "Contents/MacOS/wezterm").path
        return ([bundled].compactMap { $0 } + ["/opt/homebrew/bin/wezterm", "/usr/local/bin/wezterm"])
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ arguments: [String], socket: String?, input: String? = nil) -> String? {
        guard let cli = cli() else { return nil }
        return ClaudeHostCommand.run(cli, ["cli"] + arguments, input: input,
                                     environment: socket.map { ["WEZTERM_UNIX_SOCKET": $0] } ?? [:])
    }

    struct Pane: Equatable {
        let pane: Int
        let window: Int
        let tab: Int
        let tty: String?
        let windowTitle: String?
    }

    static func panes(socket: String?) -> [Pane] {
        guard let listed = run(["list", "--format", "json"], socket: socket) else { return [] }
        return panes(from: Data(listed.utf8))
    }

    static func panes(from json: Data) -> [Pane] {
        guard let entries = (try? JSONSerialization.jsonObject(with: json)) as? [[String: Any]] else { return [] }
        return entries.compactMap { entry in
            guard let pane = entry["pane_id"] as? Int, let window = entry["window_id"] as? Int,
                  let tab = entry["tab_id"] as? Int else { return nil }
            return Pane(pane: pane, window: window, tab: tab, tty: entry["tty_name"] as? String,
                        windowTitle: entry["window_title"] as? String)
        }
    }

    /// The pane, when it still runs on the session's device.
    static func pane(_ id: Int, socket: String?, tty: String?) -> Pane? {
        guard let tty, let found = panes(socket: socket).first(where: { $0.pane == id }), found.tty == tty else { return nil }
        return found
    }

    static func send(_ steps: [ClaudeTypingStep], pane id: Int, socket: String?, tty: String?) -> ClaudeReplyFailure? {
        guard pane(id, socket: socket, tty: tty) != nil else { return .unreachable }
        for step in steps {
            switch step {
            case .paste(let text):
                // A paste, bracketed when the program asked for one.
                guard run(["send-text", "--pane-id", String(id)], socket: socket, input: text) != nil else { return .unreachable }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                guard run(["send-text", "--pane-id", String(id), "--no-paste"], socket: socket, input: "\r") != nil else {
                    return .unreachable
                }
            }
        }
        return nil
    }

    /// Shows the pane's tab, then the pane in it, and names the window
    /// title to raise. Activating the pane alone leaves another tab in front.
    static func focus(pane id: Int, socket: String?, tty: String?) -> Pane? {
        guard let found = pane(id, socket: socket, tty: tty),
              run(["activate-tab", "--tab-id", String(found.tab)], socket: socket) != nil,
              run(["activate-pane", "--pane-id", String(id)], socket: socket) != nil else { return nil }
        return found
    }
}

// MARK: kitty

/// Reaches a kitty window through kitty's remote control, which the person
/// turns on in kitty.conf (`allow_remote_control` and `listen_on`). The
/// window is checked to run the session before anything is typed.
enum ClaudeKitty {
    static func cli() -> String? {
        let bundled = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeTerminalApps.kitty)?
            .appending(path: "Contents/MacOS/kitten").path
        return ([bundled].compactMap { $0 } + ["/opt/homebrew/bin/kitten", "/usr/local/bin/kitten"])
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ arguments: [String], socket: String, input: String? = nil) -> String? {
        guard let cli = cli() else { return nil }
        return ClaudeHostCommand.run(cli, ["@", "--to", socket] + arguments, input: input)
    }

    /// The window still runs the session's process.
    static func holds(window: Int, pid: Int32, socket: String) -> Bool {
        guard let listed = run(["ls", "--match", "id:\(window)"], socket: socket) else { return false }
        return pids(in: Data(listed.utf8), window: window).contains(pid)
    }

    /// The processes kitty lists in a window: its shell and what runs in front.
    static func pids(in json: Data, window: Int) -> Set<Int32> {
        guard let instances = (try? JSONSerialization.jsonObject(with: json)) as? [[String: Any]] else { return [] }
        var found: Set<Int32> = []
        for instance in instances {
            for tab in instance["tabs"] as? [[String: Any]] ?? [] {
                for entry in tab["windows"] as? [[String: Any]] ?? [] where entry["id"] as? Int == window {
                    if let pid = entry["pid"] as? Int { found.insert(Int32(pid)) }
                    for process in entry["foreground_processes"] as? [[String: Any]] ?? [] {
                        if let pid = process["pid"] as? Int { found.insert(Int32(pid)) }
                    }
                }
            }
        }
        return found
    }

    static func send(_ steps: [ClaudeTypingStep], window: Int, pid: Int32, socket: String) -> ClaudeReplyFailure? {
        guard holds(window: window, pid: pid, socket: socket) else { return .unreachable }
        let match = ["--match", "id:\(window)"]
        for step in steps {
            switch step {
            case .paste(let text):
                // kitty would wrap every read of the input, the empty last one
                // too, so the paste markers go in the text and kitty sends it as is.
                let paste = "\u{1B}[200~" + text + "\u{1B}[201~"
                guard run(["send-text"] + match + ["--bracketed-paste", "disable", "--stdin"], socket: socket, input: paste) != nil else {
                    return .unreachable
                }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                guard run(["send-key"] + match + ["enter"], socket: socket) != nil else { return .unreachable }
            }
        }
        return nil
    }

    static func focus(window: Int, pid: Int32, socket: String) -> Bool {
        holds(window: window, pid: pid, socket: socket)
            && run(["focus-window", "--match", "id:\(window)"], socket: socket) != nil
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
