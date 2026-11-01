// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// What Claude Code's Agent View says about a background session.
enum ClaudeBackgroundState: String, Equatable {
    case working, blocked, done, failed, stopped
}

/// What makes a session a background one: it has no terminal, only the job
/// `claude attach`, `logs`, `stop` and `rm` know it by.
struct ClaudeBackgroundDetails: Equatable {
    let job: String
    var state: ClaudeBackgroundState? = nil
    /// Its process is gone; `claude attach` brings it back.
    var ended = false
    /// The config folder it lives in, for `CLAUDE_CONFIG_DIR`.
    var configHome: String? = nil
}

/// One background session as `claude agents --json --all` lists it.
struct ClaudeBackgroundJob: Equatable {
    let id: String
    let sessionID: String
    let cwd: String
    let name: String
    let state: ClaudeBackgroundState?
    /// Nil once its process has ended.
    let pid: Int32?
    let started: Date?
    var configHome: String? = nil

    var ended: Bool { pid == nil }
}

enum ClaudeBackgroundJobs {
    /// Registry records say "bg", Agent View's list says "background".
    static func isBackground(kind: String?) -> Bool { kind == "bg" || kind == "background" }

    /// How long a finished session stays listed, and how many at most.
    static let keptFor: TimeInterval = 24 * 60 * 60
    static let keptAtMost = 6

    /// The list's background sessions; nil when the output isn't that list,
    /// as when Agent View is off or this Claude Code has none.
    static func decode(_ data: Data, configHome: String?) -> [ClaudeBackgroundJob]? {
        guard let list = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] else { return nil }
        return list.compactMap { item in
            guard isBackground(kind: item["kind"] as? String),
                  let id = item["id"] as? String, !id.isEmpty,
                  let session = item["sessionId"] as? String, !session.isEmpty,
                  let cwd = item["cwd"] as? String, !cwd.isEmpty else { return nil }
            let pid = (item["pid"] as? Int).flatMap { $0 > 0 && $0 <= Int(Int32.max) ? Int32($0) : nil }
            let started = (item["startedAt"] as? NSNumber).map { Date(timeIntervalSince1970: $0.doubleValue / 1000) }
            return ClaudeBackgroundJob(id: id, sessionID: session, cwd: cwd,
                                       name: item["name"] as? String ?? (cwd as NSString).lastPathComponent,
                                       state: (item["state"] as? String).flatMap(ClaudeBackgroundState.init(rawValue:)),
                                       pid: pid, started: started, configHome: configHome)
        }
    }

    /// When each ended job ended, as near as can be told: now for one seen
    /// alive before, else when it started. Keeps only the jobs still ended.
    static func endings(_ jobs: [ClaudeBackgroundJob], known: [String: Date], alive: Set<String>, now: Date) -> [String: Date] {
        var result: [String: Date] = [:]
        for job in jobs where job.ended {
            result[job.id] = known[job.id] ?? (alive.contains(job.id) ? now : job.started ?? now)
        }
        return result
    }

    /// The ended jobs still worth listing, newest first.
    static func kept(_ jobs: [ClaudeBackgroundJob], endings: [String: Date], now: Date) -> [ClaudeBackgroundJob] {
        let recent = jobs.filter { job in
            guard job.ended, let ended = endings[job.id] else { return false }
            return now.timeIntervalSince(ended) < keptFor
        }
        return Array(recent.sorted { (endings[$0.id] ?? .distantPast) > (endings[$1.id] ?? .distantPast) }.prefix(keptAtMost))
    }

    /// A row for a session whose process has ended. Quiet: it counts as no
    /// work and asks for nothing.
    static func session(_ job: ClaudeBackgroundJob, ended: Date, branch: String?) -> ClaudeSession {
        ClaudeSession(id: job.sessionID, pid: 0, cwd: job.cwd, project: AgentLogParser.projectName(job.cwd),
                      name: job.name, nameSource: "user", branch: branch, model: nil,
                      host: .background(job: job.id), phase: .idle, phaseSince: ended, turnStarted: nil,
                      lastActive: ended, waitingFor: nil, currentTool: nil, permission: nil, timeline: [],
                      subagents: [], tasks: nil, tokens: AgentTokens(), cost: 0, title: nil, lastText: nil, hooked: false,
                      background: ClaudeBackgroundDetails(job: job.id, state: job.state, ended: true, configHome: job.configHome))
    }

    /// Whether settings turn Agent View off, as `disableAgentView` or the
    /// variable in their `env`.
    static func settingsDisable(_ data: Data) -> Bool {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return false }
        if json["disableAgentView"] as? Bool == true { return true }
        guard let env = json["env"] as? [String: Any] else { return false }
        return isOn(env[ClaudeCLI.disableVariable].map { "\($0)" })
    }

    static func isOn(_ value: String?) -> Bool {
        guard let value = value?.trimmingCharacters(in: .whitespaces).lowercased(), !value.isEmpty else { return false }
        return !["0", "false", "no", "off"].contains(value)
    }
}

/// A folder Claude Code has worked in, for starting a new session there.
struct ClaudeRecentFolder: Equatable, Identifiable {
    let path: String
    let configHome: String?
    let used: Date?

    var id: String { path }
    var name: String { (path as NSString).lastPathComponent }
}

enum ClaudeRecentFolders {
    static let shownAtMost = 12

    /// `~/.claude.json` belongs to the standard folder; a moved config folder
    /// keeps its own inside.
    static func configFile(home: URL, user: URL) -> URL {
        home.standardizedFileURL.path == user.appending(path: ".claude").standardizedFileURL.path
            ? user.appending(path: ".claude.json") : home.appending(path: ".claude.json")
    }

    /// The name of a folder's transcripts under `projects/`.
    static func transcriptFolder(_ path: String) -> String {
        String(path.map { $0.isASCII && ($0.isLetter || $0.isNumber) ? $0 : "-" })
    }

    /// The projects a config file names.
    static func projects(in data: Data) -> [String] {
        guard let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let projects = json["projects"] as? [String: Any] else { return [] }
        return Array(projects.keys)
    }

    /// Folders still on disk across the config folders, the one worked in
    /// last first; where one is named twice, the latest use wins.
    static func read(homes: [URL], user: URL = FileManager.default.homeDirectoryForCurrentUser) -> [ClaudeRecentFolder] {
        var found: [String: ClaudeRecentFolder] = [:]
        for home in homes {
            guard let data = FileManager.default.contents(atPath: configFile(home: home, user: user).path) else { continue }
            let configHome = home.standardizedFileURL.path
            for path in projects(in: data) where isFolder(path) {
                let transcripts = home.appending(path: "projects").appending(path: transcriptFolder(path))
                let used = (try? FileManager.default.attributesOfItem(atPath: transcripts.path))?[.modificationDate] as? Date
                if let known = found[path], (known.used ?? .distantPast) >= (used ?? .distantPast) { continue }
                found[path] = ClaudeRecentFolder(path: path, configHome: configHome, used: used)
            }
        }
        return ordered(Array(found.values))
    }

    static func ordered(_ folders: [ClaudeRecentFolder]) -> [ClaudeRecentFolder] {
        Array(folders.sorted {
            let left = $0.used ?? .distantPast, right = $1.used ?? .distantPast
            return left != right ? left > right : $0.path < $1.path
        }.prefix(shownAtMost))
    }

    private static func isFolder(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &directory) && directory.boolValue
    }
}

/// Runs Claude Code's command line the way the person's own shell would.
/// The app starts with almost no `PATH`, and a background service started from
/// here keeps the environment it is given for every session it runs.
enum ClaudeCLI {
    static let disableVariable = "CLAUDE_CODE_DISABLE_AGENT_VIEW"

    struct Shell: Equatable {
        var environment: [String: String]
        var executable: String?
    }

    struct Output {
        let status: Int32
        let out: Data
        let error: String
    }

    private static let lock = NSLock()
    private static var cached: Shell?

    /// The login shell's environment and the `claude` found through it. Asked
    /// once; a person who has just installed Claude Code gets looked for again.
    static func shell(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> Shell {
        lock.lock()
        if let cached, cached.executable != nil { lock.unlock(); return cached }
        lock.unlock()
        var environment = ProcessInfo.processInfo.environment
        if let result = run("/bin/zsh", ["-ilc", "env -0"], environment: environment, timeout: 5), result.status == 0 {
            let loaded = parseEnvironment(result.out)
            if loaded["PATH"] != nil { environment = loaded }
        }
        let shell = Shell(environment: cleaned(environment),
                          executable: locate(path: environment["PATH"], home: home))
        lock.lock()
        cached = shell
        lock.unlock()
        return shell
    }

    /// `env -0` output. A shell profile that prints something leaves it in
    /// front of the first variable, whose name then isn't one.
    static func parseEnvironment(_ data: Data) -> [String: String] {
        var result: [String: String] = [:]
        for entry in data.split(separator: 0) {
            let text = String(decoding: entry, as: UTF8.self)
            guard let equals = text.firstIndex(of: "=") else { continue }
            let name = String(text[..<equals])
            guard !name.isEmpty, name.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "_") }) else { continue }
            result[name] = String(text[text.index(after: equals)...])
        }
        return result
    }

    /// Drops what marks this environment as a session's or as this app's.
    /// The switch that turns Agent View off stays: it is the person's.
    static func cleaned(_ environment: [String: String]) -> [String: String] {
        environment.filter { key, _ in
            if key == disableVariable || key == ClaudeConfigHomes.variable { return true }
            return !key.hasPrefix("CLAUDE") && key != "__CFBundleIdentifier"
        }
    }

    /// The environment for a command acting in a config folder.
    static func environment(_ base: [String: String], configHome: String?,
                            home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [String: String] {
        var environment = base
        if let setting = configVariable(configHome, home: home) {
            environment[ClaudeConfigHomes.variable] = setting
        } else {
            environment[ClaudeConfigHomes.variable] = nil
        }
        return environment
    }

    /// The `CLAUDE_CONFIG_DIR` a config folder needs; nil for the standard one.
    static func configVariable(_ configHome: String?, home: URL) -> String? {
        guard let configHome else { return nil }
        let standard = home.appending(path: ".claude").standardizedFileURL.path
        return URL(fileURLWithPath: configHome).standardizedFileURL.path == standard ? nil : configHome
    }

    /// `claude` on the person's `PATH`, then where its installers put it.
    static func locate(path: String?, home: URL, isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:)) -> String? {
        let onPath = (path ?? "").split(separator: ":").map { "\($0)/claude" }
        let usual = [".local/bin/claude", ".claude/local/claude", ".bun/bin/claude", ".npm-global/bin/claude"]
            .map { home.appending(path: $0).path } + ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"]
        return (onPath + usual).first(where: isExecutable)
    }

    /// One `claude` command. Nil when it can't be run.
    static func claude(_ arguments: [String], configHome: String? = nil, in folder: String? = nil, timeout: TimeInterval = 15) -> Output? {
        let shell = shell()
        guard let executable = shell.executable else { return nil }
        return run(executable, arguments, environment: environment(shell.environment, configHome: configHome),
                   folder: folder, timeout: timeout)
    }

    /// Waits for the command, and takes what it printed up to a moment after
    /// it ends: a service it left running may hold the pipes open.
    static func run(_ path: String, _ arguments: [String], environment: [String: String],
                            folder: String? = nil, timeout: TimeInterval) -> Output? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        process.environment = environment
        if let folder { process.currentDirectoryURL = URL(fileURLWithPath: folder, isDirectory: true) }
        let out = Pipe(), error = Pipe()
        process.standardOutput = out
        process.standardError = error
        process.standardInput = FileHandle.nullDevice
        let ended = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in ended.signal() }
        guard (try? process.run()) != nil else { return nil }
        let collected = Collected()
        let reading = DispatchGroup()
        for (pipe, isError) in [(out, false), (error, true)] {
            reading.enter()
            DispatchQueue.global(qos: .utility).async {
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                collected.set(data, isError: isError)
                reading.leave()
            }
        }
        if ended.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            return nil
        }
        _ = reading.wait(timeout: .now() + 0.5)
        let (stdout, stderr) = collected.values
        return Output(status: process.terminationStatus, out: stdout,
                      error: String(decoding: stderr, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
    }

    private final class Collected: @unchecked Sendable {
        private let lock = NSLock()
        private var out = Data(), error = Data()
        func set(_ data: Data, isError: Bool) {
            lock.lock()
            if isError { error = data } else { out = data }
            lock.unlock()
        }
        var values: (Data, Data) {
            lock.lock()
            defer { lock.unlock() }
            return (out, error)
        }
    }

    /// The word a shell reads as exactly this text.
    static func quoted(_ text: String) -> String {
        "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// The line a terminal runs for `claude` with these arguments.
    static func line(executable: String, arguments: [String], configHome: String?, folder: String?,
                     home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String {
        var parts: [String] = []
        if let folder { parts.append("cd \(quoted(folder)) &&") }
        if let setting = configVariable(configHome, home: home) { parts.append("\(ClaudeConfigHomes.variable)=\(quoted(setting))") }
        parts.append(quoted(executable))
        parts.append(contentsOf: arguments.map(quoted))
        return parts.joined(separator: " ")
    }

    /// The arguments that start a session; `--` keeps a prompt that begins
    /// with a dash from reading as an option.
    static func startArguments(prompt: String?, background: Bool, model: String? = nil) -> [String] {
        var arguments = background ? ["--bg"] : []
        if let model, !model.isEmpty { arguments += ["--model", model] }
        if let prompt, !prompt.isEmpty { arguments += ["--", prompt] }
        return arguments
    }
}

/// Reads what `claude logs` prints: the last stretch of a terminal's stream,
/// its text placed with cursor moves, which simply removing the codes would
/// run together.
enum ClaudeTerminalScreen {
    /// Whether it says anything to read. A session that only animates a
    /// spinner fills `claude logs` with that alone.
    static func isReadable(_ text: String) -> Bool {
        text.split(separator: "\n").filter { $0.contains { $0.isLetter } }.count >= minimumLines
    }

    static let minimumLines = 3

    static func text(_ data: Data) -> String {
        let scalars = Array(String(decoding: data, as: UTF8.self).unicodeScalars)
        var rows: [Int: [Int: Unicode.Scalar]] = [:]
        var row = 0, column = 0
        var saved = (row: 0, column: 0)
        var index = 0
        while index < scalars.count {
            let scalar = scalars[index]
            index += 1
            switch scalar.value {
            case 0x1B:
                guard index < scalars.count else { break }
                let kind = scalars[index]
                index += 1
                switch kind {
                case "[":
                    var parameters = ""
                    while index < scalars.count, !(0x40...0x7E).contains(scalars[index].value) {
                        parameters.unicodeScalars.append(scalars[index])
                        index += 1
                    }
                    guard index < scalars.count else { break }
                    let final = scalars[index]
                    index += 1
                    // A private sequence (`?`, `<`, `>`) sets modes; it moves nothing.
                    guard !parameters.hasPrefix("?"), !parameters.hasPrefix("<"), !parameters.hasPrefix(">") else { break }
                    let numbers = parameters.split(separator: ";", omittingEmptySubsequences: false).map { Int($0) }
                    let first = numbers.first.flatMap { $0 }
                    switch final {
                    case "A": row = max(0, row - max(1, first ?? 1))
                    case "B": row += max(1, first ?? 1)
                    case "C": column += max(1, first ?? 1)
                    case "D": column = max(0, column - max(1, first ?? 1))
                    case "G": column = max(0, (first ?? 1) - 1)
                    case "H", "f":
                        row = max(0, (first ?? 1) - 1)
                        column = numbers.count > 1 ? max(0, (numbers[1] ?? 1) - 1) : 0
                    case "J":
                        if first == 2 || first == 3 { rows = [:] }
                        else if first == nil || first == 0 {
                            rows = rows.filter { $0.key < row }.merging([row: (rows[row] ?? [:]).filter { $0.key < column }]) { _, new in new }
                        }
                    case "K":
                        if first == nil || first == 0 { rows[row] = rows[row]?.filter { $0.key < column } }
                        else if first == 2 { rows[row] = nil }
                    default: break
                    }
                case "]", "P", "_", "^":
                    // A title or similar, up to its terminator.
                    while index < scalars.count, scalars[index].value != 0x07 {
                        if scalars[index].value == 0x1B { index += 1; break }
                        index += 1
                    }
                    index += 1
                case "7": saved = (row, column)
                case "8": (row, column) = saved
                case "(", ")": index += 1
                default: break
                }
            case 0x0D: column = 0
            case 0x0A: row += 1; column = 0
            case 0x08: column = max(0, column - 1)
            case 0..<0x20, 0x7F: break
            default:
                // A mark combining with the letter before it takes no cell.
                if scalar.properties.generalCategory == .nonspacingMark { break }
                rows[row, default: [:]][column] = scalar
                column += 1
            }
        }
        return lines(rows).joined(separator: "\n")
    }

    /// The rows as lines, without the blank stretches a full-screen layout
    /// leaves between its parts.
    private static func lines(_ rows: [Int: [Int: Unicode.Scalar]]) -> [String] {
        guard let last = rows.keys.max() else { return [] }
        var lines: [String] = []
        for number in 0...last {
            var line = String.UnicodeScalarView()
            if let cells = rows[number], let end = cells.keys.max() {
                for column in 0...end { line.append(cells[column] ?? " ") }
            }
            var text = String(line)
            while text.last?.isWhitespace == true { text.removeLast() }
            if text.isEmpty, lines.last?.isEmpty != false { continue }
            lines.append(text)
        }
        while lines.last?.isEmpty == true { lines.removeLast() }
        return lines
    }
}

/// Types into a background session through `claude attach` in a terminal of
/// the app's own that nobody sees, then leaves with the documented Ctrl+Z. Claude
/// Code takes this as the person's own typing, which its peer socket is not.
/// Other terminals attached to the session stay attached.
enum ClaudeBackgroundReply {
    static let detach: UInt8 = 0x1A

    /// What goes to the session's prompt, in order, with the pause each needs:
    /// images are pasted as their paths, which Claude Code takes a moment to
    /// read, then the text; the return alone at the end sends it.
    static func keystrokes(_ text: String, images: [String]) -> [(data: Data, pause: TimeInterval)] {
        var steps: [(data: Data, pause: TimeInterval)] = []
        func paste(_ value: String) -> Data { Data("\u{1B}[200~\(value)\u{1B}[201~".utf8) }
        for image in images { steps.append((paste(image), 0.2)) }
        if !images.isEmpty { steps[steps.count - 1].pause = ClaudeTerminalScripts.imageSettle(images.count) }
        if !text.isEmpty { steps.append((paste(text), 0.4)) }
        steps.append((Data([0x0D]), 0.5))
        return steps
    }

    static func send(_ text: String, images: [String], job: String, configHome: String?) -> ClaudeReplyFailure? {
        let attached = attach(job: job, configHome: configHome) { terminal, isRunning in
            for step in keystrokes(text, images: images) {
                guard isRunning(), write(terminal, step.data) else { return false }
                Thread.sleep(forTimeInterval: step.pause)
            }
            return true
        }
        return attached?.done == true ? nil : .unreachable
    }

    /// The session's whole screen, which it paints anew for whoever attaches.
    static func screen(job: String, configHome: String?) -> String? {
        attach(job: job, configHome: configHome) { _, _ in true }.map { ClaudeTerminalScreen.text($0.output) }
    }

    /// Attaches, once its screen has settled does `work`, and leaves. Nil when
    /// it could not attach.
    private static func attach(job: String, configHome: String?,
                               work: (FileHandle, () -> Bool) -> Bool) -> (done: Bool, output: Data)? {
        let shell = ClaudeCLI.shell()
        guard let executable = shell.executable else { return nil }
        var master: Int32 = -1, slave: Int32 = -1
        var size = winsize(ws_row: 50, ws_col: 200, ws_xpixel: 0, ws_ypixel: 0)
        guard openpty(&master, &slave, nil, nil, &size) == 0 else { return nil }
        let terminal = FileHandle(fileDescriptor: master, closeOnDealloc: true)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = ["attach", job]
        var environment = ClaudeCLI.environment(shell.environment, configHome: configHome)
        environment["TERM"] = "xterm-256color"
        process.environment = environment
        let child = FileHandle(fileDescriptor: slave, closeOnDealloc: true)
        process.standardInput = child
        process.standardOutput = child
        process.standardError = child
        let ended = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in ended.signal() }
        guard (try? process.run()) != nil else { return nil }
        try? child.close()

        // Its output must be read for it to go on, and tells when it is ready.
        let activity = Activity()
        DispatchQueue.global(qos: .userInitiated).async {
            var buffer = [UInt8](repeating: 0, count: 1 << 15)
            while true {
                let count = read(master, &buffer, buffer.count)
                guard count > 0 else { break }
                activity.add(buffer[0..<count])
            }
        }
        guard activity.settled(after: 0.5, within: 6), process.isRunning else {
            if process.isRunning { process.terminate() }
            return nil
        }
        let output = activity.output
        let done = work(terminal) { process.isRunning }
        _ = write(terminal, Data([detach]))
        if ended.wait(timeout: .now() + 3) == .timedOut { process.terminate() }
        return (done, output)
    }

    private static func write(_ terminal: FileHandle, _ data: Data) -> Bool {
        (try? terminal.write(contentsOf: data)) != nil
    }

    private final class Activity: @unchecked Sendable {
        private let lock = NSLock()
        private var last: Date?
        private var data = Data()
        /// More than any screen takes to paint.
        private static let limit = 1 << 21

        func add(_ bytes: ArraySlice<UInt8>) {
            lock.lock()
            last = Date()
            if data.count < Self.limit { data.append(contentsOf: bytes) }
            lock.unlock()
        }

        var output: Data {
            lock.lock()
            defer { lock.unlock() }
            return data
        }

        /// Waits for the first output and then a quiet moment; false if it
        /// never printed anything. A screen that keeps moving, as a working
        /// session's does, counts as ready when the time is up.
        func settled(after quiet: TimeInterval, within limit: TimeInterval) -> Bool {
            let deadline = Date().addingTimeInterval(limit)
            while Date() < deadline {
                Thread.sleep(forTimeInterval: 0.1)
                lock.lock()
                let seen = last
                lock.unlock()
                if let seen, Date().timeIntervalSince(seen) >= quiet { return true }
            }
            lock.lock()
            defer { lock.unlock() }
            return last != nil
        }
    }
}

/// Opens a new window of the person's terminal running a command line.
enum ClaudeTerminalLauncher {
    /// Terminals that open a `.command` file by running it.
    static let others = ["com.googlecode.iterm2", "com.mitchellh.ghostty", "com.github.wez.wezterm",
                         "net.kovidgoyal.kitty", "org.alacritty", "dev.warp.Warp-Stable"]

    /// The terminal the person's sessions run in, the one active last first;
    /// nil means Terminal.
    static func preferred(from sessions: [ClaudeSession]) -> String? {
        let terminals = sessions
            .sorted { ($0.lastActive ?? .distantPast) > ($1.lastActive ?? .distantPast) }
            .compactMap { session -> String? in
                switch session.host {
                case .terminal: return ClaudeSessionHosts.terminal
                case .ide, .background, .unknown: return nil
                // A tmux's own terminal is not known without asking tmux; its pane names it when it could.
                case .tmux(let target): return target.outer.flatMap { others.contains($0) || $0 == ClaudeSessionHosts.terminal ? $0 : nil }
                default: return session.host.bundle.flatMap { others.contains($0) ? $0 : nil }
                }
            }
        return terminals.first
    }

    /// A script that runs the line and removes itself, for it may name a prompt.
    static func script(_ line: String) -> String {
        "#!/bin/zsh -il\nrm -f -- \"$0\"\n\(line)\n"
    }

    static func open(_ line: String, terminal: String?) -> Bool {
        if let terminal, terminal != ClaudeSessionHosts.terminal, openScript(line, bundle: terminal) { return true }
        // Without leave to script Terminal, it still runs a `.command` file it opens.
        guard AppleScriptRunner.consentToAutomate(bundleID: ClaudeSessionHosts.terminal) else {
            return openScript(line, bundle: ClaudeSessionHosts.terminal)
        }
        let script = """
        tell application "Terminal"
            do script \(AppleScriptRunner.literal(line))
            activate
        end tell
        """
        return AppleScriptRunner.run(script).ok
    }

    private static func openScript(_ line: String, bundle: String) -> Bool {
        let file = FileManager.default.temporaryDirectory.appending(path: "vorssaint-\(UUID().uuidString).command")
        guard FileManager.default.createFile(atPath: file.path, contents: Data(script(line).utf8),
                                             attributes: [.posixPermissions: 0o700]) else { return false }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        process.arguments = ["-b", bundle, file.path]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else {
            try? FileManager.default.removeItem(at: file)
            return false
        }
        process.waitUntilExit()
        if process.terminationStatus != 0 { try? FileManager.default.removeItem(at: file) }
        return process.terminationStatus == 0
    }
}

/// A model a new session can start with. `id` nil keeps the agent's own
/// default, the one its settings name.
struct NewSessionModel: Equatable, Identifiable {
    let id: String?
    let name: String
    /// What the agent's default is, when it can be told.
    var detail: String?

    var key: String { id ?? "" }
}

/// The models offered for each agent on the New session page.
enum NewSessionModels {
    /// Claude Code's aliases for its latest models (`claude --help`), so
    /// the list doesn't age with each release.
    static let claudeAliases: [(id: String, name: String)] = [
        ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku"),
    ]

    static func claude(defaultName: String, current: String? = nil) -> [NewSessionModel] {
        [NewSessionModel(id: nil, name: defaultName, detail: current.map(claudeName))]
            + claudeAliases.map { NewSessionModel(id: $0.id, name: $0.name) }
    }

    /// An alias or full model name as a person reads it: `opus[1m]` as Opus.
    static func claudeName(_ model: String) -> String {
        let base = model.split(separator: "[").first.map(String.init) ?? model
        if let alias = claudeAliases.first(where: { $0.id == base.lowercased() }) { return alias.name }
        return base
    }

    /// The `model` Claude Code's settings name, if any.
    static func claudeSetting(_ data: Data) -> String? {
        guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let model = object["model"] as? String, !model.isEmpty else { return nil }
        return model
    }

    /// Codex's models from the catalog it keeps in `models_cache.json`,
    /// the listed ones in its own order, after the default from `config.toml`.
    static func codex(cache: Data?, config: String?, defaultName: String) -> [NewSessionModel] {
        let listed = codexCatalog(cache)
        let configured = config.flatMap(codexSetting)
        let detail = configured.map { id in listed.first { $0.id == id }?.name ?? id } ?? listed.first?.name
        return [NewSessionModel(id: nil, name: defaultName, detail: detail)] + listed
    }

    static func codexCatalog(_ data: Data?) -> [NewSessionModel] {
        guard let data, let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let models = object["models"] as? [[String: Any]] else { return [] }
        let shown = models.compactMap { model -> (priority: Int, model: NewSessionModel)? in
            guard let id = (model["slug"] ?? model["id"]) as? String, !id.isEmpty,
                  (model["visibility"] as? String ?? "list") == "list" else { return nil }
            let name = (model["display_name"] ?? model["displayName"]) as? String
            return ((model["priority"] as? Int) ?? Int.max, NewSessionModel(id: id, name: name?.isEmpty == false ? name! : id))
        }
        var seen = Set<String>()
        return shown.enumerated()
            .sorted { ($0.element.priority, $0.offset) < ($1.element.priority, $1.offset) }
            .map(\.element.model)
            .filter { seen.insert($0.key).inserted }
    }

    /// The top-level `model = "…"` of a `config.toml`; one inside a table is
    /// a profile's or a provider's, not the default.
    static func codexSetting(_ toml: String) -> String? {
        for raw in toml.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("[") { return nil }
            guard line.hasPrefix("model"), let equals = line.firstIndex(of: "=") else { continue }
            guard line[..<equals].trimmingCharacters(in: .whitespaces) == "model" else { continue }
            var value = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
            if let hash = value.range(of: " #") { value = String(value[..<hash.lowerBound]).trimmingCharacters(in: .whitespaces) }
            guard value.count >= 2, let quote = value.first, quote == "\"" || quote == "'", value.last == quote else { return nil }
            let inner = String(value.dropFirst().dropLast())
            return inner.isEmpty ? nil : inner
        }
        return nil
    }

    static func readCodex(home: URL?, defaultName: String) -> [NewSessionModel] {
        let cache = home.flatMap { try? Data(contentsOf: $0.appending(path: "models_cache.json")) }
        let config = home.flatMap { try? String(contentsOf: $0.appending(path: "config.toml"), encoding: .utf8) }
        return codex(cache: cache, config: config, defaultName: defaultName)
    }

    static func readClaude(configHome: String?, defaultName: String,
                           home: URL = FileManager.default.homeDirectoryForCurrentUser) -> [NewSessionModel] {
        let folder = configHome.map { URL(fileURLWithPath: $0, isDirectory: true) } ?? home.appending(path: ".claude")
        let setting = (try? Data(contentsOf: folder.appending(path: "settings.json"))).flatMap(claudeSetting)
        return claude(defaultName: defaultName, current: setting)
    }
}

/// Codex's command line, found and run with the login shell's environment
/// as `claude` is.
enum CodexCLI {
    /// `codex` on the person's `PATH`, then where its installers put it.
    static func locate(path: String?, home: URL, isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:)) -> String? {
        let onPath = (path ?? "").split(separator: ":").map { "\($0)/codex" }
        let usual = [".local/bin/codex", ".npm-global/bin/codex", ".bun/bin/codex", ".volta/bin/codex"]
            .map { home.appending(path: $0).path }
            + ["/opt/homebrew/bin/codex", "/usr/local/bin/codex", "/Applications/Codex.app/Contents/Resources/codex"]
        return (onPath + usual).first(where: isExecutable)
    }

    static func executable(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String? {
        locate(path: ClaudeCLI.shell(home: home).environment["PATH"], home: home)
    }

    /// The arguments that start Codex's terminal UI.
    static func startArguments(prompt: String?, model: String?) -> [String] {
        var arguments: [String] = []
        if let model, !model.isEmpty { arguments += ["-m", model] }
        if let prompt, !prompt.isEmpty { arguments += ["--", prompt] }
        return arguments
    }

    /// Starts Codex's shared app-server, which runs threads with no terminal.
    static let daemonStart = ["app-server", "daemon", "start"]

    static func run(_ arguments: [String], timeout: TimeInterval = 30) -> ClaudeCLI.Output? {
        let shell = ClaudeCLI.shell()
        guard let executable = locate(path: shell.environment["PATH"], home: FileManager.default.homeDirectoryForCurrentUser)
        else { return nil }
        return ClaudeCLI.run(executable, arguments, environment: shell.environment, timeout: timeout)
    }

    /// The line a terminal runs for `codex` in a folder.
    static func line(executable: String, arguments: [String], folder: String?) -> String {
        var parts: [String] = []
        if let folder { parts.append("cd \(ClaudeCLI.quoted(folder)) &&") }
        parts.append(ClaudeCLI.quoted(executable))
        parts.append(contentsOf: arguments.map(ClaudeCLI.quoted))
        return parts.joined(separator: " ")
    }
}
