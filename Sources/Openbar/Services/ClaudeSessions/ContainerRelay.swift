// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import Darwin
import Foundation

/// The relay's `docker exec -i` for one container (issue #330): the agent
/// script's `relay` mode, which passes each of the container's hooks on as an
/// `H <id> <event>` line and takes the answer back as `A <id> <answer>`.
///
/// Every hook becomes a socket pair: one end goes to the island's hook server
/// as if the hook had connected (`ClaudeHookServer.adopt`), the other is
/// written the event and read for the answer. So the server, the permission
/// queue and every rule about which hooks wait stay exactly as for a local or
/// an SSH hook, and nothing listens anywhere for it.
final class ContainerRelay {
    private let process: RemoteSSHProcess
    private let bundle: String
    private let adopt: (Int32, pid_t) -> Void
    private let answers = DispatchQueue(label: "openbar.container-relay.answers", qos: .userInitiated, attributes: .concurrent)
    /// Why it didn't start, from the script or the CLI.
    private(set) var failure: String?

    init(command: RemoteCommand, bundle: String, adopt: @escaping (Int32, pid_t) -> Void) {
        process = RemoteSSHProcess(command)
        self.bundle = bundle
        self.adopt = adopt
    }

    var pid: pid_t { process.pid }

    var isRunning: Bool { process.isRunning }

    /// Starts the relay and waits for its hello; then hooks flow on a thread of its own.
    func start() -> Bool {
        let name = RemoteSSH.socketName(bundle: bundle).dropLast(".sock".count)
        guard let preamble = RemoteSSH.preamble("relay \(name)"), process.start(), process.write(preamble) else {
            failure = "The relay could not start"
            return false
        }
        guard let hello = process.answer(timeout: 20), hello.first == "HELLO 1" else {
            process.stop()
            process.wait(1)
            failure = ContainerRelay.reason(process.stderr)
            return false
        }
        let thread = Thread { [self] in
            while let line = process.nextLine() { take(line) }
        }
        thread.name = "container-relay"
        thread.qualityOfService = .userInitiated
        thread.start()
        return true
    }

    func stop() {
        process.stop()
        process.wait(1)
    }

    static func reason(_ stderr: String) -> String {
        let line = stderr.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.last { !$0.isEmpty }
        return line.map { String($0.prefix(160)) } ?? ContainerStrings.relayEnded
    }

    /// One line from the relay: a hook's event, handed to the server.
    private func take(_ line: String) {
        let words = line.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: false)
        guard words.count >= 2, words[0] == "H", let id = Self.id(words[1]) else { return }
        guard words.count == 3, let event = Data(base64Encoded: String(words[2])),
              event.count <= ClaudeHookServer.maximumInput else {
            return answer(id, nil)
        }
        var pair: [Int32] = [-1, -1]
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &pair) == 0 else { return answer(id, nil) }
        for fd in pair { _ = fcntl(fd, F_SETFD, FD_CLOEXEC) }
        let mine = pair[0]
        var noSignal: Int32 = 1
        setsockopt(mine, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size))
        adopt(pair[1], process.pid)
        answers.async { [self] in
            let reply = Self.exchange(mine, event)
            close(mine)
            answer(id, reply)
        }
    }

    /// The hook's ids are its process number in the container.
    static func id(_ field: Substring) -> String? {
        !field.isEmpty && field.count <= 12 && field.allSatisfy(\.isASCII) && field.allSatisfy(\.isNumber) ? String(field) : nil
    }

    /// Writes the event, ends that side, and reads the island's answer to its end.
    static func exchange(_ fd: Int32, _ event: Data) -> Data {
        let sent = event.withUnsafeBytes { bytes -> Bool in
            guard var base = bytes.baseAddress else { return true }
            var left = bytes.count
            while left > 0 {
                let written = Darwin.write(fd, base, left)
                if written > 0 { left -= written; base += written; continue }
                if written < 0, errno == EINTR { continue }
                return false
            }
            return true
        }
        guard sent else { return Data() }
        shutdown(fd, SHUT_WR)
        var reply = Data()
        var chunk = [UInt8](repeating: 0, count: 64 << 10)
        while true {
            let count = Darwin.read(fd, &chunk, chunk.count)
            if count > 0 {
                reply.append(contentsOf: chunk[0..<count])
                if reply.count > ClaudeHookServer.maximumInput { break }
                continue
            }
            if count < 0, errno == EINTR { continue }
            break
        }
        return reply
    }

    private func answer(_ id: String, _ reply: Data?) {
        let field = reply.flatMap { $0.isEmpty ? nil : $0.base64EncodedString() } ?? "-"
        _ = process.write("A \(id) \(field)\n")
    }
}

/// Codex's app-server inside a container: `docker exec -i` running the agent
/// script's `codex` mode, which joins its standard input and output to the
/// daemon's socket there. The island's end is a socket pair, so the Codex
/// client talks to it exactly as to a socket of this Mac's.
enum ContainerCodex {
    static func connect(_ command: RemoteCommand) throws -> CodexSocket {
        guard let preamble = RemoteSSH.preamble("codex") else { throw CodexSocketError(description: "the host script is missing") }
        var pair: [Int32] = [-1, -1]
        guard socketpair(AF_UNIX, SOCK_STREAM, 0, &pair) == 0 else { throw CodexSocketError.errno("socketpair") }
        _ = fcntl(pair[0], F_SETFD, FD_CLOEXEC)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: command.executable)
        process.arguments = command.arguments
        process.environment = command.environment
        let child = FileHandle(fileDescriptor: pair[1], closeOnDealloc: false)
        process.standardInput = child
        process.standardOutput = child
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            close(pair[0])
            close(pair[1])
            throw CodexSocketError(description: "\(command.executable) could not start")
        }
        close(pair[1])
        let socket = CodexSocket(fd: pair[0]) { if process.isRunning { process.terminate() } }
        do {
            try socket.write(Data(preamble.utf8))
        } catch {
            socket.close()
            throw error
        }
        return socket
    }
}

/// Open for a session in a container (issue #330): its devcontainer's VS Code
/// window, else a terminal attached to the tmux it runs in.
enum ContainerOpen {
    /// nil leaves it to the pane brought forward in the container's tmux.
    static func open(link: RemoteHostLink, pane: RemoteTmuxPane?) -> ClaudeHostOpen? {
        guard let container = link.container else { return nil }
        if let folder = container.folder, let url = ContainerEditor.url(folder: folder, workspace: container.workspace),
           NSWorkspace.shared.urlForApplication(toOpen: url) != nil {
            return NSWorkspace.shared.open(url) ? .opened : nil
        }
        guard let pane, let command = link.containerCommand else { return nil }
        return .detached(attach: attachLine(engine: command.executable, container: container, pane: pane))
    }

    /// `docker exec -it` into the container, attached to the pane's tmux session.
    static func attachLine(engine: String, container: ContainerInfo, pane: RemoteTmuxPane) -> String {
        var words = [engine, "exec", "-it"]
        if let user = container.user { words += ["-u", user] }
        words += [container.id, "tmux", "-S", pane.socket, "attach-session", "-t", pane.pane]
        return words.map(RemoteHookInstaller.quoted).joined(separator: " ")
    }
}
