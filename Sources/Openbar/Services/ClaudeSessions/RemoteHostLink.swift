// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// A program that runs the agent script's loader: `ssh` to a host, or
/// `docker exec` into a container (issue #330).
struct RemoteCommand: Equatable {
    let executable: String
    let arguments: [String]
    var environment: [String: String]? = nil

    static func ssh(_ arguments: [String]) -> RemoteCommand {
        RemoteCommand(executable: RemoteSSH.binary, arguments: arguments)
    }
}

/// A running `ssh` (or `docker exec`), fed lines on standard input and read
/// line by line on standard output. Its own lock guards everything, so any
/// queue may ask.
final class RemoteSSHProcess {
    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let errors = Pipe()
    private let lock = NSCondition()
    /// Writes from several queues go out one whole line after another.
    private let writing = NSLock()
    private var lines: [String] = []
    private var partial = Data()
    private var errorText = Data()
    /// Standard output ended: nothing more will come.
    private var ended = false
    private var exited = false

    convenience init(arguments: [String]) {
        self.init(.ssh(arguments))
    }

    init(_ command: RemoteCommand) {
        process.executableURL = URL(fileURLWithPath: command.executable)
        process.arguments = command.arguments
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        // Never a dialog: a host asking for a password fails at once.
        var environment = command.environment ?? ProcessInfo.processInfo.environment
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
        return writing.withLock {
            do {
                try input.fileHandleForWriting.write(contentsOf: Data(text.utf8))
                return true
            } catch {
                return false
            }
        }
    }

    /// The next line as it comes, for a stream with no closing "." (the
    /// relay's); nil once standard output ends.
    func nextLine() -> String? {
        lock.lock()
        defer { lock.unlock() }
        while true {
            if !lines.isEmpty { return lines.removeFirst() }
            if ended { return nil }
            lock.wait()
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
        run(.ssh(RemoteSSH.oneShot(alias)), mode: mode, input: input, timeout: timeout)
    }

    static func run(_ command: RemoteCommand, mode: String, input: [String] = [], timeout: TimeInterval = 25) -> Result {
        guard let preamble = RemoteSSH.preamble(mode) else { return Result(lines: nil, reason: "The host script is missing") }
        let ssh = RemoteSSHProcess(command)
        guard ssh.start() else { return Result(lines: nil, reason: "\(URL(fileURLWithPath: command.executable).lastPathComponent) could not start") }
        _ = ssh.write(preamble + input.map { $0 + "\n" }.joined())
        ssh.closeInput()
        let lines = ssh.answer(timeout: timeout)
        ssh.wait(lines == nil ? 2 : 0.2)
        ssh.stop()
        ssh.wait(1)
        return Result(lines: lines, reason: lines == nil ? RemoteSSH.reason(ssh.stderr, status: ssh.status) : "")
    }
}

/// How the island reaches a host: `ssh` by its alias, or `docker exec` into
/// a container the person picked (issue #330, docs/containers.md).
enum RemoteTransport: Equatable {
    case ssh(String)
    case container(ContainerTarget)
}

/// The connection to one host: the probe, then the `ssh` that forwards the
/// host's hook socket to the island and answers the island's questions,
/// started again with growing pauses whenever it ends. Everything it does
/// waits on its own queue, never on the island's.
///
/// A container has no socket to forward: its agent runs on one
/// `docker exec -i`, and a second one runs the relay that carries its hooks
/// (`ContainerRelay`).
final class RemoteHostLink {
    let alias: String
    let transport: RemoteTransport
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
    /// The socket on this Mac that `ssh -L` leads to the host's Codex.
    let codexSocket: String
    /// Codex's socket is forwarded on the running connection, and the last
    /// look found Codex's daemon on the host. Guarded by `lock`.
    private var codexForwarded = false
    private var codexLive = false
    /// A container's relay, its CLI and the container it found last.
    /// Guarded by `lock`.
    private var relay: ContainerRelay?
    private var engine: ContainerEngine?
    var container: ContainerInfo? { lock.withLock { foundContainer } }
    private var foundContainer: ContainerInfo?
    /// Hands a relayed hook to the island's server (`ClaudeHookServer.adopt`).
    private let adopt: (Int32, pid_t) -> Void
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
        transport = .ssh(alias)
        self.bundle = bundle
        self.localSocket = localSocket
        adopt = { descriptor, _ in close(descriptor) }
        codexSocket = RemoteSSH.codexSocket(alias: alias, bundle: bundle)
        queue = DispatchQueue(label: "openbar.remote-host.\(alias)")
    }

    init(container target: ContainerTarget, bundle: String, adopt: @escaping (Int32, pid_t) -> Void) {
        alias = target.alias
        transport = .container(target)
        self.bundle = bundle
        localSocket = ""
        self.adopt = adopt
        codexSocket = ""
        queue = DispatchQueue(label: "openbar.container.\(target.id)")
    }

    var isContainer: Bool { if case .container = transport { return true } else { return false } }

    /// The command that reaches the container as it runs now; nil before
    /// the first connection found it.
    var containerCommand: RemoteCommand? {
        lock.withLock { () -> RemoteCommand? in
            guard let engine, let foundContainer else { return nil }
            return engine.exec(foundContainer.id, user: foundContainer.user)
        }
    }

    /// Runs one mode of the agent script on the host, as Set up and Remove do.
    func oneShot(mode: String, input: [String] = []) -> RemoteOneShot.Result {
        switch transport {
        case .ssh(let alias): return RemoteOneShot.run(alias, mode: mode, input: input)
        case .container:
            guard let command = containerCommand else {
                return RemoteOneShot.Result(lines: nil, reason: ContainerStrings.notRunning)
            }
            return RemoteOneShot.run(command, mode: mode, input: input)
        }
    }

    /// Whether the host's Codex can be reached through `codexSocket` now.
    /// Any thread.
    var codexReady: Bool {
        lock.withLock { ssh != nil && codexLive && (codexForwarded || (relay != nil && foundContainer != nil)) }
    }

    /// Forgets the Codex forward of a connection that ended; its socket goes too.
    private func dropCodex() {
        lock.withLock {
            codexForwarded = false
            codexLive = false
        }
        if !codexSocket.isEmpty { try? FileManager.default.removeItem(atPath: codexSocket) }
    }

    /// The `ssh` behind the forwarded socket: hooks from this host reach the
    /// island through it, so its process number tells them apart. For a
    /// container, its relay's `docker exec`.
    var peer: pid_t? { lock.withLock { isContainer ? relay?.pid : ssh.map(\.pid) } }

    func start() {
        queue.async { [self] in connect() }
    }

    func stop() {
        let (running, relayed) = lock.withLock { () -> (RemoteSSHProcess?, ContainerRelay?) in
            stopped = true
            generation += 1
            defer { ssh = nil; relay = nil }
            return (ssh, relay)
        }
        running?.stop()
        relayed?.stop()
        dropCodex()
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

    /// Connects again at once, without counting it as a failure: after a
    /// setup, as Remove from host may have taken the forwarded socket with
    /// it. On the link's queue.
    func reconnectNow() {
        let (running, relayed) = lock.withLock { () -> (RemoteSSHProcess?, ContainerRelay?) in
            guard !stopped else { return (nil, nil) }
            generation += 1
            defer { ssh = nil; relay = nil }
            return (ssh, relay)
        }
        running?.stop()
        relayed?.stop()
        dropCodex()
        guard lock.withLock({ !stopped }) else { return }
        attempt = 0
        connect()
    }

    // MARK: Connecting

    private func connect() {
        let current = lock.withLock { () -> Int? in stopped ? nil : generation }
        guard let current else { return }
        setState(.connecting)
        if case .container(let target) = transport { return connectContainer(target, generation: current) }
        let name = RemoteSSH.socketName(bundle: bundle)
        let probe = RemoteOneShot.run(alias, mode: "probe \(name) \(bundle)")
        guard let lines = probe.lines, let found = RemoteAnswer.probe(lines) else {
            return retry(probe.lines == nil ? probe.reason : "The host answered in a way Openbar didn’t expect", generation: current)
        }
        info = found
        let remoteSocket = found.home + "/.openbar/" + name
        // Codex's socket comes the other way on the same connection. Its
        // folder is the app's own, readable by this user only.
        var codex: (local: String, remote: String)?
        if let remote = found.codexSocket, RemoteSSH.canForward(local: codexSocket, remote: remote) {
            let folder = (codexSocket as NSString).deletingLastPathComponent
            if (try? FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true,
                                                         attributes: [.posixPermissions: 0o700])) != nil {
                try? FileManager.default.removeItem(atPath: codexSocket)
                codex = (codexSocket, remote)
            }
        }
        let started = RemoteSSHProcess(arguments: RemoteSSH.link(alias, remoteSocket: remoteSocket, localSocket: localSocket,
                                                                 codex: codex))
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
            codexForwarded = codex != nil
            codexLive = false
            return true
        }
        guard kept else { return started.stop() }
        attempt = 0
        offsets = [:]
        setState(.connected(found))
        poll(generation: current)
    }

    /// A container: found again by its folder or name, as a rebuilt one
    /// has a new ID; probed, then the agent and the relay started in it.
    private func connectContainer(_ target: ContainerTarget, generation current: Int) {
        guard let engine = ContainerEngine.find() else { return retry(ContainerStrings.noEngine, generation: current) }
        let listed: [ContainerInfo]
        switch engine.running() {
        case .success(let list): listed = list
        case .failure(.engine(let reason)): return retry(reason, generation: current)
        case .failure(.noEngine): return retry(ContainerStrings.noEngine, generation: current)
        }
        guard let container = listed.first(where: target.matches) else {
            return retry(ContainerStrings.notRunning, generation: current)
        }
        lock.withLock {
            self.engine = engine
            foundContainer = container
        }
        let command = engine.exec(container.id, user: container.user)
        let name = RemoteSSH.socketName(bundle: bundle)
        let probe = RemoteOneShot.run(command, mode: "probe \(name) \(bundle) container")
        guard let lines = probe.lines, var found = RemoteAnswer.probe(lines) else {
            return retry(probe.lines == nil ? probe.reason : ContainerStrings.unexpected, generation: current)
        }
        guard let preamble = RemoteSSH.preamble("agent") else { return retry("The host script is missing", generation: current) }
        let started = RemoteSSHProcess(command)
        guard started.start(), started.write(preamble) else {
            started.stop()
            return retry(ContainerStrings.cantStart(engine.name), generation: current)
        }
        guard let hello = started.answer(timeout: 20), hello.first == "HELLO 1" else {
            started.stop()
            started.wait(1)
            return retry(ContainerEngine.reason(Data(started.stderr.utf8)), generation: current)
        }
        let relayed = ContainerRelay(command: command, bundle: bundle, adopt: adopt)
        guard relayed.start() else {
            started.stop()
            relayed.stop()
            return retry(relayed.failure ?? ContainerStrings.relayEnded, generation: current)
        }
        found.client = .relay
        found.container = container
        info = found
        let kept = lock.withLock { () -> Bool in
            guard !stopped, generation == current else { return false }
            ssh = started
            relay = relayed
            codexForwarded = false
            codexLive = false
            return true
        }
        guard kept else {
            started.stop()
            relayed.stop()
            return
        }
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
        let (gone, relayed) = lock.withLock { () -> (RemoteSSHProcess?, ContainerRelay?) in
            defer { ssh = nil; relay = nil }
            return (ssh, relay)
        }
        gone?.stop()
        relayed?.stop()
        dropCodex()
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
        // A container's hooks come only while its relay runs.
        if isContainer, lock.withLock({ relay?.isRunning != true }) {
            return retry(ContainerStrings.relayEnded, generation: current)
        }
        guard let lines = self.request(request, timeout: 15) else {
            let reason = lock.withLock { ssh.map { isContainer ? ContainerEngine.reason(Data($0.stderr.utf8)) : RemoteSSH.reason($0.stderr, status: $0.status) } }
            return retry(reason ?? "The connection ended", generation: current)
        }
        let read = RemoteAnswer.snapshot(lines, alias: alias)
        let snapshot = RemoteClock.shifted(read, by: RemoteClock.offset(sent: sent, received: Date(), host: read.hostTime))
        for chunk in snapshot.chunks { offsets[chunk.path] = chunk.offset + UInt64(chunk.bytes.count) }
        // Only live sessions' transcripts are followed.
        let live = Set(snapshot.transcripts.values)
        offsets = offsets.filter { live.contains($0.key) }
        for path in live where offsets[path] == nil { offsets[path] = 0 }
        lock.withLock { codexLive = snapshot.codexLive }
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
            case .interrupt, .keys:
                // The host's helper types text and returns only.
                return .unreachable
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

    /// Runs one mode of the agent script on the host and gives its answer.
    typealias Runner = (_ mode: String, _ input: [String]) -> RemoteOneShot.Result

    static func runner(_ alias: String) -> Runner { { RemoteOneShot.run(alias, mode: $0, input: $1) } }

    static func read(_ alias: String) -> Result<Settings, Failure> { read(runner(alias)) }

    static func read(_ run: Runner) -> Result<Settings, Failure> {
        let result = run("read", [])
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
        install(runner(alias), info: info, bundle: bundle)
    }

    static func install(_ run: Runner, info: RemoteHostInfo, bundle: String) -> Failure? {
        guard let hook = RemoteHookInstaller.hook(info: info, bundle: bundle) else { return .noClient }
        for _ in 0..<3 {
            let settings: Settings
            switch read(run) {
            case .success(let read): settings = read
            case .failure(let failure): return failure
            }
            guard let parsed = try? RemoteHookInstaller.parse(settings.current),
                  let data = try? RemoteHookInstaller.serialize(
                    ClaudeHookInstaller.installing(into: parsed, command: hook.command, marker: hook.marker)) else { return .unreadable }
            let result = run("install", [settings.sum, data.base64EncodedString()])
            guard let lines = result.lines else { return .unreachable(result.reason) }
            if lines.contains("O") { return nil }
            if !lines.contains("F changed") { return .unreachable(lines.first ?? "") }
        }
        return .busy
    }

    static func remove(_ alias: String, bundle: String) -> Failure? {
        remove(runner(alias), marker: RemoteHookInstaller.marker(bundle: bundle), bundle: bundle)
    }

    /// Takes the hooks with `marker` out; `bundle` names what `~/.openbar`
    /// held for this build.
    static func remove(_ run: Runner, marker: String, bundle: String) -> Failure? {
        for _ in 0..<3 {
            let settings: Settings
            switch read(run) {
            case .success(let read): settings = read
            case .failure(let failure): return failure
            }
            guard let removal = try? RemoteHookInstaller.removal(current: settings.current, backup: settings.backup,
                                                                 marker: marker) else { return .unreadable }
            var input = [RemoteSSH.socketName(bundle: bundle), settings.sum]
            switch removal {
            case .restore: input.append("restore")
            case .write(let data, let keep): input += [keep ? "keep" : "write", data.base64EncodedString()]
            }
            let result = run("remove", input)
            guard let lines = result.lines else { return .unreachable(result.reason) }
            if lines.contains("O") { return nil }
            if !lines.contains("F changed") { return .unreachable(lines.first ?? "") }
        }
        return .busy
    }
}
