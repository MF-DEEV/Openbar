// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// A running `ssh`, fed lines on standard input and read line by line on
/// standard output. Its own lock guards everything, so any queue may ask.
final class RemoteSSHProcess {
    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let errors = Pipe()
    private let lock = NSCondition()
    private var lines: [String] = []
    private var partial = Data()
    private var errorText = Data()
    /// Standard output ended: nothing more will come.
    private var ended = false
    private var exited = false

    init(arguments: [String]) {
        process.executableURL = URL(fileURLWithPath: RemoteSSH.binary)
        process.arguments = arguments
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        // Never a dialog: a host asking for a password fails at once.
        var environment = ProcessInfo.processInfo.environment
        environment["SSH_ASKPASS_REQUIRE"] = "never"
        environment.removeValue(forKey: "DISPLAY")
        process.environment = environment
    }

    var pid: pid_t { process.processIdentifier }

    var isRunning: Bool { lock.withLock { !ended && !exited } }

    func start() -> Bool {
        // A write racing the end of ssh is an error, never a SIGPIPE that ends the app.
        guard fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1) != -1 else { return false }
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            self?.received(data)
        }
        errors.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard let self, !data.isEmpty else { return }
            self.lock.withLock {
                self.errorText.append(data)
                if self.errorText.count > 8192 { self.errorText = Data(self.errorText.suffix(4096)) }
            }
        }
        process.terminationHandler = { [weak self] _ in
            guard let self else { return }
            self.lock.withLock {
                self.exited = true
                self.lock.broadcast()
            }
        }
        do { try process.run() } catch {
            lock.withLock { ended = true; exited = true }
            return false
        }
        return true
    }

    private func received(_ data: Data) {
        lock.withLock {
            if data.isEmpty {
                ended = true
            } else {
                partial.append(data)
                while let end = partial.firstIndex(of: 0x0A) {
                    lines.append(String(decoding: partial[partial.startIndex..<end], as: UTF8.self))
                    partial = Data(partial[partial.index(after: end)...])
                }
            }
            lock.broadcast()
        }
    }

    func write(_ text: String) -> Bool {
        guard isRunning else { return false }
        do {
            try input.fileHandleForWriting.write(contentsOf: Data(text.utf8))
            return true
        } catch {
            return false
        }
    }

    func closeInput() { try? input.fileHandleForWriting.close() }

    /// The lines of one answer, up to its closing "."; nil when it does not
    /// come in time or `ssh` ends first.
    func answer(timeout: TimeInterval) -> [String]? {
        let deadline = Date().addingTimeInterval(timeout)
        lock.lock()
        defer { lock.unlock() }
        var taken: [String] = []
        while true {
            while !lines.isEmpty {
                let line = lines.removeFirst()
                if line == "." { return taken }
                taken.append(line)
            }
            // What ssh printed before it ended is still read to its end.
            if ended { return nil }
            if !lock.wait(until: exited ? min(deadline, Date().addingTimeInterval(1)) : deadline) { return nil }
        }
    }

    var stderr: String {
        // What ssh wrote just before it ended may not have been handed on yet.
        if lock.withLock({ exited }) {
            errors.fileHandleForReading.readabilityHandler = nil
            let rest = (try? errors.fileHandleForReading.readToEnd()) ?? nil
            if let rest, !rest.isEmpty { lock.withLock { errorText.append(rest) } }
        }
        return lock.withLock { String(decoding: errorText, as: UTF8.self) }
    }

    var status: Int32 { process.isRunning ? 0 : process.terminationStatus }

    func stop() {
        closeInput()
        if process.isRunning { process.terminate() }
        output.fileHandleForReading.readabilityHandler = nil
    }

    /// Waits for `ssh` to end, a moment at most.
    func wait(_ seconds: TimeInterval) {
        let deadline = Date().addingTimeInterval(seconds)
        lock.lock()
        while !exited, lock.wait(until: deadline) {}
        lock.unlock()
    }
}

/// One short `ssh` running a mode of the agent script.
enum RemoteOneShot {
    struct Result {
        let lines: [String]?
        let reason: String
    }

    static func run(_ alias: String, mode: String, input: [String] = [], timeout: TimeInterval = 25) -> Result {
        guard let preamble = RemoteSSH.preamble(mode) else { return Result(lines: nil, reason: "The host script is missing") }
        let ssh = RemoteSSHProcess(arguments: RemoteSSH.oneShot(alias))
        guard ssh.start() else { return Result(lines: nil, reason: "ssh could not start") }
        _ = ssh.write(preamble + input.map { $0 + "\n" }.joined())
        ssh.closeInput()
        let lines = ssh.answer(timeout: timeout)
        ssh.wait(lines == nil ? 2 : 0.2)
        ssh.stop()
        ssh.wait(1)
        return Result(lines: lines, reason: lines == nil ? RemoteSSH.reason(ssh.stderr, status: ssh.status) : "")
    }
}

/// The connection to one host: the probe, then the `ssh` that forwards the
/// host's hook socket to the island and answers the island's questions,
/// started again with growing pauses whenever it ends. Everything it does
/// waits on its own queue, never on the island's.
final class RemoteHostLink {
    let alias: String
    private let bundle: String
    private let localSocket: String
    private let queue: DispatchQueue
    private let lock = NSLock()
    private var ssh: RemoteSSHProcess?
    private var stopped = false
    private var attempt = 0
    private var generation = 0
    /// Transcripts read so far, by their path on the host.
    private var offsets: [String: UInt64] = [:]
    private(set) var info: RemoteHostInfo?
    /// The last state handed on. On the link's queue.
    private(set) var state: RemoteHostState?

    /// The state, on the link's queue.
    var onState: (RemoteHostState) -> Void = { _ in }
    /// Each look at the host's sessions, on the link's queue.
    var onSnapshot: (RemoteSnapshot) -> Void = { _ in }
    /// How long a look waits for the next one.
    var interval: () -> TimeInterval = { 1.5 }

    init(alias: String, bundle: String, localSocket: String) {
        self.alias = alias
        self.bundle = bundle
        self.localSocket = localSocket
        queue = DispatchQueue(label: "openbar.remote-host.\(alias)")
    }

    /// The `ssh` behind the forwarded socket: hooks from this host reach the
    /// island through it, so its process number tells them apart.
    var peer: pid_t? { lock.withLock { ssh.map(\.pid) } }

    func start() {
        queue.async { [self] in connect() }
    }

    func stop() {
        let running = lock.withLock { () -> RemoteSSHProcess? in
            stopped = true
            generation += 1
            defer { ssh = nil }
            return ssh
        }
        running?.stop()
    }

    /// Reads every transcript from the start again, as when content
    /// previews were turned on.
    func rereadTranscripts() {
        queue.async { [self] in offsets = offsets.mapValues { _ in 0 } }
    }

    /// Runs `body` on the link's queue with the connection, if there is one.
    func perform(_ body: @escaping (RemoteHostLink) -> Void) {
        queue.async { [self] in body(self) }
    }

    /// One request to the agent and its answer. On the link's queue.
    func request(_ lines: [String], timeout: TimeInterval = 8) -> [String]? {
        guard let ssh = lock.withLock({ ssh }), ssh.isRunning else { return nil }
        guard ssh.write(lines.map { $0 + "\n" }.joined()) else { return nil }
        return ssh.answer(timeout: timeout)
    }

    // MARK: Connecting

    private func connect() {
        let current = lock.withLock { () -> Int? in stopped ? nil : generation }
        guard let current else { return }
        setState(.connecting)
        let name = RemoteSSH.socketName(bundle: bundle)
        let probe = RemoteOneShot.run(alias, mode: "probe \(name) \(bundle)")
        guard let lines = probe.lines, let found = RemoteAnswer.probe(lines) else {
            return retry(probe.lines == nil ? probe.reason : "The host answered in a way Openbar didn’t expect", generation: current)
        }
        info = found
        let remoteSocket = found.home + "/.openbar/" + name
        let started = RemoteSSHProcess(arguments: RemoteSSH.link(alias, remoteSocket: remoteSocket, localSocket: localSocket))
        guard started.start(), let preamble = RemoteSSH.preamble("agent"), started.write(preamble) else {
            started.stop()
            return retry("ssh could not start", generation: current)
        }
        guard let hello = started.answer(timeout: 20), hello.first == "HELLO 1" else {
            started.stop()
            started.wait(1)
            return retry(RemoteSSH.reason(started.stderr, status: started.status), generation: current)
        }
        let kept = lock.withLock { () -> Bool in
            guard !stopped, generation == current else { return false }
            ssh = started
            return true
        }
        guard kept else { return started.stop() }
        attempt = 0
        offsets = [:]
        setState(.connected(found))
        poll(generation: current)
    }

    private func setState(_ new: RemoteHostState) {
        state = new
        onState(new)
    }

    /// After a setup or a removal. On the link's queue.
    func markHooks(installed: Bool) {
        info?.hooksInstalled = installed
        if case .connected(var found) = state {
            found.hooksInstalled = installed
            state = .connected(found)
        }
    }

    private func retry(_ reason: String, generation current: Int) {
        let gone = lock.withLock { () -> RemoteSSHProcess? in
            defer { ssh = nil }
            return ssh
        }
        gone?.stop()
        guard lock.withLock({ !stopped && generation == current }) else { return }
        attempt += 1
        let delay = RemoteBackoff.delay(attempt: attempt)
        setState(.disconnected(reason: reason, retry: Date().addingTimeInterval(delay)))
        queue.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self, lock.withLock({ !self.stopped && self.generation == current }) else { return }
            connect()
        }
    }

    /// Asks for the sessions, hands them on, and asks again in a moment.
    private func poll(generation current: Int) {
        guard lock.withLock({ !stopped && generation == current }) else { return }
        var request = ["snap"]
        for (path, offset) in offsets { request.append("t \(offset) \(RemoteAnswer.encode(path))") }
        request.append("end")
        let sent = Date()
        guard let lines = self.request(request, timeout: 15) else {
            let reason = lock.withLock { ssh.map { RemoteSSH.reason($0.stderr, status: $0.status) } }
            return retry(reason ?? "The connection ended", generation: current)
        }
        let read = RemoteAnswer.snapshot(lines, alias: alias)
        let snapshot = RemoteClock.shifted(read, by: RemoteClock.offset(sent: sent, received: Date(), host: read.hostTime))
        for chunk in snapshot.chunks { offsets[chunk.path] = chunk.offset + UInt64(chunk.bytes.count) }
        // Only live sessions' transcripts are followed.
        let live = Set(snapshot.transcripts.values)
        offsets = offsets.filter { live.contains($0.key) }
        for path in live where offsets[path] == nil { offsets[path] = 0 }
        onSnapshot(snapshot)
        queue.asyncAfter(deadline: .now() + interval()) { [weak self] in self?.poll(generation: current) }
    }

    // MARK: Replies

    /// Types the steps into the session's tmux pane on the host. On the link's queue.
    func type(_ steps: [ClaudeTypingStep], pid: Int32, session: String) -> ClaudeReplyFailure? {
        for step in steps {
            switch step {
            case .paste(let text):
                if let failure = RemoteAnswer.outcome(request(["paste \(pid) \(session) \(RemoteAnswer.encode(text))"])) {
                    return failure
                }
            case .wait(let seconds):
                Thread.sleep(forTimeInterval: seconds)
            case .enter:
                if let failure = RemoteAnswer.outcome(request(["enter \(pid) \(session)"])) { return failure }
            }
        }
        return nil
    }

    func focus(pid: Int32, session: String) -> Bool {
        RemoteAnswer.outcome(request(["focus \(pid) \(session)"])) == nil
    }
}

/// Setting a host up and taking the hooks out again, each one short `ssh`.
enum RemoteHostSetup {
    enum Failure: Error, Equatable {
        case unreachable(String)
        case noClient
        case unreadable
        /// Claude Code changed the file meanwhile, three times in a row.
        case busy
    }

    struct Settings {
        let current: Data?
        let backup: RemoteHookInstaller.Backup?
        let sum: String
    }

    static func read(_ alias: String) -> Result<Settings, Failure> {
        let result = RemoteOneShot.run(alias, mode: "read")
        guard let lines = result.lines else { return .failure(.unreachable(result.reason)) }
        var current: Data?
        var backup: RemoteHookInstaller.Backup?
        var sum: String?
        for line in lines {
            let words = line.split(separator: " ").map(String.init)
            switch words.first {
            case "F": current = words.count > 1 ? Data(base64Encoded: words[1]) : Data()
            case "B": backup = words.count > 1 && words[1] != "-" ? Data(base64Encoded: words[1]).map { .file($0) } : RemoteHookInstaller.Backup.none
            case "S": sum = words.count > 1 ? words[1] : nil
            default: break
            }
        }
        guard let sum else { return .failure(.unreadable) }
        return .success(Settings(current: current, backup: backup, sum: sum))
    }

    static func install(_ alias: String, info: RemoteHostInfo, bundle: String) -> Failure? {
        guard let client = info.client else { return .noClient }
        let socket = info.home + "/.openbar/" + RemoteSSH.socketName(bundle: bundle)
        let command = RemoteHookInstaller.command(client: client, socket: socket, bundle: bundle)
        for _ in 0..<3 {
            let settings: Settings
            switch read(alias) {
            case .success(let read): settings = read
            case .failure(let failure): return failure
            }
            guard let parsed = try? RemoteHookInstaller.parse(settings.current),
                  let data = try? RemoteHookInstaller.serialize(
                    RemoteHookInstaller.installing(into: parsed, command: command, bundle: bundle)) else { return .unreadable }
            let result = RemoteOneShot.run(alias, mode: "install", input: [settings.sum, data.base64EncodedString()])
            guard let lines = result.lines else { return .unreachable(result.reason) }
            if lines.contains("O") { return nil }
            if !lines.contains("F changed") { return .unreachable(lines.first ?? "") }
        }
        return .busy
    }

    static func remove(_ alias: String, bundle: String) -> Failure? {
        for _ in 0..<3 {
            let settings: Settings
            switch read(alias) {
            case .success(let read): settings = read
            case .failure(let failure): return failure
            }
            guard let removal = try? RemoteHookInstaller.removal(current: settings.current, backup: settings.backup,
                                                                 bundle: bundle) else { return .unreadable }
            var input = [RemoteSSH.socketName(bundle: bundle), settings.sum]
            switch removal {
            case .restore: input.append("restore")
            case .write(let data): input += ["write", data.base64EncodedString()]
            }
            let result = RemoteOneShot.run(alias, mode: "remove", input: input)
            guard let lines = result.lines else { return .unreachable(result.reason) }
            if lines.contains("O") { return nil }
            if !lines.contains("F changed") { return .unreachable(lines.first ?? "") }
        }
        return .busy
    }
}
