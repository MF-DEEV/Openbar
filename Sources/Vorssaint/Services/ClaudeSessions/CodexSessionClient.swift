// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import CryptoKit
import Darwin
import Foundation

/// The island's connection to Codex's app-server daemon (docs/codex-sessions.md §3): a
/// WebSocket over the daemon's user-only Unix socket, speaking its JSON-RPC
/// dialect. It never starts the daemon; while there is none it looks again
/// every few seconds. It joins a thread (`thread/resume`) only while the
/// thread works or waits, or its page is open, so a closed terminal's thread
/// can unload. Every method may be called from any thread; `changed` runs on
/// the client's own thread.
final class CodexSessionClient {
    static let retry: TimeInterval = 3
    static let poll: TimeInterval = 0.1
    static let clientName = "vorssaint"

    /// Streamed text and output the island never shows, asked not to be
    /// sent (`capabilities.optOutNotificationMethods`): a turn streams
    /// them by the token. Names as Codex 0.163 and 0.164 send them.
    static let quietNotifications = [
        "item/agentMessage/delta", "item/plan/delta",
        "item/reasoning/textDelta", "item/reasoning/summaryTextDelta", "item/reasoning/summaryPartAdded",
        "item/commandExecution/outputDelta", "item/fileChange/outputDelta",
        "thread/realtime/item/transcript/delta", "thread/realtime/transcript/delta",
        "thread/realtime/outputAudio/delta",
    ]

    /// `$CODEX_HOME`, or `~/.codex`.
    static func home() -> URL? {
        if let value = getenv("CODEX_HOME"), case let home = String(cString: value), !home.isEmpty {
            return URL(fileURLWithPath: home, isDirectory: true)
        }
        return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex", isDirectory: true)
    }

    static func socketPath(home: URL) -> URL {
        home.appendingPathComponent("app-server-control", isDirectory: true)
            .appendingPathComponent("app-server-control.sock")
    }

    static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
    }

    /// What a response answers.
    private enum Waiting {
        case initialize, loadedList, other
        case read(Int)
        case resume(Int)
    }

    private let home: URL?
    private let clientVersion: String
    private let retryInterval: TimeInterval

    private let lock = NSLock()
    // Everything below is guarded by `lock`.
    private let state = CodexSessionState()
    private var nextID = 0
    private var waiting: [Int: Waiting] = [:]
    /// Text frames queued for the daemon; nil while not connected.
    private var outbox: [Data]?
    /// Session pages open in the island, kept joined while open.
    private var focused: Set<String> = []
    private var isConnected = false
    private var daemonVersion: String?
    private var started = false
    private var stopped = false

    /// Wakes the connection's thread to send, or to stop.
    private let wakeRead: Int32
    private let wakeWrite: Int32
    private let stopSignal = DispatchSemaphore(value: 0)
    private let finished = DispatchSemaphore(value: 0)

    /// `home` nil means no Codex to look for; tests pass a temporary one.
    init(home: URL? = CodexSessionClient.home(), clientVersion: String = CodexSessionClient.appVersion,
         retry: TimeInterval = CodexSessionClient.retry) {
        self.home = home
        self.clientVersion = clientVersion
        self.retryInterval = retry
        var fds: [Int32] = [-1, -1]
        if pipe(&fds) == 0 {
            for fd in fds {
                _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
                _ = fcntl(fd, F_SETFD, FD_CLOEXEC)
            }
        }
        wakeRead = fds[0]
        wakeWrite = fds[1]
    }

    deinit {
        if wakeRead >= 0 { close(wakeRead) }
        if wakeWrite >= 0 { close(wakeWrite) }
    }

    /// Starts looking for the daemon; `changed` runs whenever sessions may
    /// have changed. Once per client.
    func start(changed: @escaping () -> Void) {
        guard lock.withLock({ () -> Bool in
            guard !started, !stopped else { return false }
            started = true
            return true
        }) else { return }
        let thread = Foundation.Thread { [self] in
            run(changed: changed)
            finished.signal()
        }
        thread.name = "codex-app-server"
        thread.qualityOfService = .utility
        thread.start()
    }

    /// Leaves the daemon and stops looking for it; waits a moment for the
    /// connection's thread to end.
    func stop() {
        let wasRunning = lock.withLock { () -> Bool in
            let running = started && !stopped
            stopped = true
            return running
        }
        guard wasRunning else { return }
        wake()
        stopSignal.signal()
        _ = finished.wait(timeout: .now() + 2)
    }

    func snapshot(now: Date, previews: Bool) -> (sessions: [ClaudeSession], events: [ClaudeSessionEvent]) {
        lock.withLock {
            if state.previews != previews {
                state.setPreviews(previews)
                if previews {
                    for id in state.threads.keys.sorted() {
                        call("thread/read", ["threadId": id, "includeTurns": true], .read(state.chatRevision))
                    }
                }
            }
            return state.snapshot(now: now, previews: previews)
        }
    }

    func owns(_ session: String) -> Bool {
        lock.withLock { state.threads[session] != nil }
    }

    func acknowledge(_ session: String, now: Date) {
        lock.withLock { state.acknowledge(session, now: now) }
    }

    /// Keeps these threads joined while their pages are open, so their tools
    /// and turns show.
    func focus(_ sessions: Set<String>) {
        lock.withLock {
            focused = sessions
            follow()
        }
    }

    /// Answers the request with this island id, the session's own or one
    /// of its subagents'; false when it is gone or of another kind.
    func answer(_ session: String, requestID: String, answer: CodexAnswer) -> Bool {
        lock.withLock {
            guard let pending = state.answerable(session: session, requestID: requestID)?.pending,
                  let body = CodexSessionState.result(pending, answer),
                  state.takePending(thread: session, requestID: requestID) != nil else { return false }
            send(["id": pending.rpcID.json, "result": body])
            return true
        }
    }

    /// Whether the request with this island id still waits on the session's row.
    func waits(_ session: String, requestID: String) -> Bool {
        lock.withLock { state.pending(session: session).contains { $0.requestID == requestID } }
    }

    /// A prompt for an idle thread, or one queued after the current turn of a working one.
    func reply(_ session: String, text: String, images: [URL] = []) -> Bool {
        lock.withLock {
            guard isConnected, let thread = state.threads[session], state.pending(session: session).isEmpty,
                  let input = Self.input(text: text, images: images)
            else { return false }
            if thread.isActive {
                call("thread/queue/add", ["threadId": session, "input": input, "clientUserMessageId": Self.uuidV7()],
                     .other)
            } else {
                call("turn/start", ["threadId": session, "input": input], .other)
            }
            return true
        }
    }

    /// What a reply gives Codex: its text, unless there is none, then each
    /// image as the file Codex reads it from. Nil when there is neither.
    static func input(text: String, images: [URL]) -> [[String: Any]]? {
        var input: [[String: Any]] = []
        if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            input.append(["type": "text", "text": text, "text_elements": [Any]()])
        }
        for image in images.prefix(ClaudeImageAttachments.limit) where image.isFileURL {
            input.append(["type": "localImage", "path": image.path])
        }
        return input.isEmpty ? nil : input
    }

    /// Joined to Codex's app-server right now.
    var connected: Bool { lock.withLock { isConnected } }

    /// The daemon's user agent, which names its version.
    var version: String? { lock.withLock { daemonVersion } }

    // MARK: Messages (under `lock`)

    private func send(_ message: [String: Any]) {
        guard outbox != nil,
              let data = try? JSONSerialization.data(withJSONObject: message, options: [.withoutEscapingSlashes])
        else { return }
        outbox?.append(data)
        wake()
    }

    private func call(_ method: String, _ params: [String: Any], _ kind: Waiting) {
        nextID += 1
        waiting[nextID] = kind
        send(["id": nextID, "method": method, "params": params])
    }

    /// Joins threads that work or wait, and leaves the quiet ones nobody
    /// looks at. Codex's helper threads are never joined, nor a thread
    /// before a read says what it is.
    private func follow() {
        var join: [String] = []
        var leave: [String] = []
        for (id, thread) in state.threads {
            let wanted = thread.described && !thread.ephemeral
                && (thread.isActive || !thread.pending.isEmpty || focused.contains(id))
            if wanted, !thread.subscribed { join.append(id) }
            if !wanted, thread.subscribed { leave.append(id) }
        }
        for id in join.sorted() {
            state.setSubscribed(id, true)
            // No overrides: with them Codex would change the person's thread.
            call("thread/resume", ["threadId": id, "excludeTurns": !state.previews], .resume(state.chatRevision))
        }
        for id in leave.sorted() {
            state.setSubscribed(id, false)
            call("thread/unsubscribe", ["threadId": id], .other)
        }
    }

    /// Handles one message; true when what the island shows may have changed.
    private func handle(_ text: String, now: Date) -> Bool {
        guard let data = text.data(using: .utf8),
              let message = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return false }
        return lock.withLock { () -> Bool in
            let method = message["method"] as? String
            let params = message["params"] as? [String: Any] ?? [:]
            let id = CodexRPCID(json: message["id"])
            switch (method, id) {
            // A request: approvals and questions the island may answer.
            case (let method?, let id?):
                let kept = state.request(id, method: method, params: params, now: now)
                follow()
                return kept
            case (let method?, nil):
                let notified = state.notify(method, params: params, now: now)
                if let new = notified.read {
                    call("thread/read", ["threadId": new, "includeTurns": state.previews], .read(state.chatRevision))
                }
                // A page's thread that closed or unloaded is no longer kept.
                if let thread = params["threadId"] as? String, state.threads[thread] == nil {
                    focused.remove(thread)
                }
                follow()
                return notified.changed
            case (nil, let id?):
                guard case .number(let number) = id, let kind = waiting.removeValue(forKey: number) else { return false }
                let result = message["result"] as? [String: Any] ?? [:]
                switch kind {
                case .initialize:
                    daemonVersion = result["userAgent"] as? String
                    send(["method": "initialized"])
                    call("thread/loaded/list", [:], .loadedList)
                    return false
                case .loadedList:
                    for thread in (result["data"] as? [Any] ?? []).compactMap({ $0 as? String }) {
                        call(
                            "thread/read", ["threadId": thread, "includeTurns": state.previews],
                            .read(state.chatRevision))
                    }
                    if let cursor = result["nextCursor"] as? String {
                        call("thread/loaded/list", ["cursor": cursor], .loadedList)
                    }
                    return false
                case .read(let revision), .resume(let revision):
                    guard let thread = result["thread"] as? [String: Any] else { return false }
                    state.upsertThread(thread, historyRevision: revision, historyOrder: number)
                    if let id = thread["id"] as? String, state.threads[id] == nil { focused.remove(id) }
                    follow()
                    return true
                case .other:
                    return false
                }
            default:
                return false
            }
        }
    }

    // MARK: Connection

    private var isStopped: Bool { lock.withLock { stopped } }

    private func run(changed: @escaping () -> Void) {
        while !isStopped {
            if let home, FileManager.default.fileExists(atPath: Self.socketPath(home: home).path) {
                try? session(path: Self.socketPath(home: home), changed: changed)
                lock.withLock {
                    isConnected = false
                    outbox = nil
                    waiting.removeAll()
                    focused.removeAll()
                    state.reset()
                }
                changed()
            }
            if isStopped { break }
            _ = stopSignal.wait(timeout: .now() + retryInterval)
        }
    }

    private func wake() {
        guard wakeWrite >= 0 else { return }
        var byte: UInt8 = 1
        _ = write(wakeWrite, &byte, 1)
    }

    private func drainWake() {
        var buffer = [UInt8](repeating: 0, count: 64)
        while read(wakeRead, &buffer, buffer.count) > 0 {}
    }

    /// One connection, from handshake to the daemon going away.
    private func session(path: URL, changed: () -> Void) throws {
        let socket = try CodexSocket(path: path.path)
        defer { socket.close() }
        var decoder = CodexWebSocket.Decoder()
        decoder.append(try CodexWebSocket.handshake(socket, deadline: Date().addingTimeInterval(5)))
        var assembler = CodexWebSocket.Assembler()
        lock.withLock {
            outbox = []
            isConnected = true
            call("initialize", [
                "clientInfo": ["name": Self.clientName, "title": "Vorssaint", "version": clientVersion],
                "capabilities": ["experimentalApi": true, "optOutNotificationMethods": Self.quietNotifications],
            ], .initialize)
        }
        while !isStopped {
            // Everything queued for the daemon goes out between reads.
            let queued = lock.withLock { () -> [Data] in
                let queued = outbox ?? []
                outbox = outbox.map { _ in [] }
                return queued
            }
            for message in queued {
                try socket.write(CodexWebSocket.encode(opcode: .text, payload: message, mask: CodexWebSocket.newMask()))
            }
            while let frame = try decoder.next() {
                guard let message = try assembler.push(frame) else { continue }
                switch message {
                case .text(let text):
                    if handle(text, now: Date()) { changed() }
                case .binary, .pong:
                    break
                case .ping(let payload):
                    try socket.write(CodexWebSocket.encode(opcode: .pong, payload: payload, mask: CodexWebSocket.newMask()))
                case .close(let payload):
                    // Echo the code back, as the protocol asks, then go.
                    try? socket.write(CodexWebSocket.encode(opcode: .close, payload: payload.prefix(2),
                                                            mask: CodexWebSocket.newMask()))
                    return
                }
            }
            switch try socket.wait(alsoWatching: wakeRead, timeout: Self.poll) {
            case .readable:
                guard let data = try socket.read() else { return }
                decoder.append(data)
            case .woken:
                drainWake()
            case .timedOut:
                break
            }
        }
    }

    /// A UUIDv7 for Codex's client message ids: time-ordered, the rest random.
    static func uuidV7(now: Date = Date()) -> String {
        var bytes = (0..<16).map { _ in UInt8.random(in: .min ... .max) }
        let millis = UInt64(max(now.timeIntervalSince1970 * 1000, 0))
        for index in 0..<6 { bytes[index] = UInt8(truncatingIfNeeded: millis >> (8 * (5 - index))) }
        bytes[6] = (bytes[6] & 0x0f) | 0x70
        bytes[8] = (bytes[8] & 0x3f) | 0x80
        let hex = bytes.map { String(format: "%02x", $0) }.joined()
        let parts = [0..<8, 8..<12, 12..<16, 16..<20, 20..<32].map { range in
            String(hex[hex.index(hex.startIndex, offsetBy: range.lowerBound)..<hex.index(hex.startIndex, offsetBy: range.upperBound)])
        }
        return parts.joined(separator: "-")
    }
}

// MARK: - Socket

struct CodexSocketError: Error, CustomStringConvertible {
    let description: String

    static func errno(_ call: String) -> CodexSocketError {
        CodexSocketError(description: "\(call): \(String(cString: strerror(Darwin.errno)))")
    }
}

/// A connected stream socket: blocking writes, reads after a poll.
final class CodexSocket {
    enum Readiness { case readable, woken, timedOut }

    private(set) var fd: Int32

    /// Connects to the Unix socket at `path`, made by this same user.
    init(path: String) throws {
        // `sun_path` holds 104 bytes; Codex's path is a link, and where it
        // leads may be shorter.
        var target = path
        if target.utf8.count >= Self.pathCapacity {
            target = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        }
        guard target.utf8.count < Self.pathCapacity else {
            throw CodexSocketError(description: "socket path too long: \(path)")
        }
        fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw CodexSocketError.errno("socket") }
        _ = fcntl(fd, F_SETFD, FD_CLOEXEC)
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        // A daemon that stops reading must not hold the island's thread forever.
        var timeout = timeval(tv_sec: 5, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        var address = Self.address(target)
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard result == 0 else {
            let error = CodexSocketError.errno("connect")
            close()
            throw error
        }
        // Only a daemon of this same user is trusted with what the island says.
        var uid: uid_t = 0
        var gid: gid_t = 0
        guard getpeereid(fd, &uid, &gid) == 0, uid == geteuid() else {
            close()
            throw CodexSocketError(description: "the socket belongs to another user")
        }
    }

    /// Wraps a descriptor already connected, as a test's socket pair.
    init(fd: Int32) {
        self.fd = fd
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
    }

    deinit { close() }

    static let pathCapacity = MemoryLayout<sockaddr_un>.size - MemoryLayout<sockaddr_un>.offset(of: \sockaddr_un.sun_path)!

    static func address(_ path: String) -> sockaddr_un {
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        let bytes = Array(path.utf8.prefix(pathCapacity - 1))
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            buffer.copyBytes(from: bytes)
        }
        return address
    }

    func close() {
        guard fd >= 0 else { return }
        Darwin.close(fd)
        fd = -1
    }

    func write(_ data: Data) throws {
        var offset = 0
        try data.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            guard let base = buffer.baseAddress else { return }
            while offset < buffer.count {
                let written = Darwin.write(fd, base + offset, buffer.count - offset)
                if written < 0 {
                    if Darwin.errno == EINTR { continue }
                    throw CodexSocketError.errno("write")
                }
                offset += written
            }
        }
    }

    /// What arrived, or nil once the other end closed.
    func read() throws -> Data? {
        var buffer = [UInt8](repeating: 0, count: 64 * 1024)
        while true {
            let count = Darwin.read(fd, &buffer, buffer.count)
            if count > 0 { return Data(buffer[..<count]) }
            if count == 0 { return nil }
            if Darwin.errno == EINTR || Darwin.errno == EAGAIN { continue }
            throw CodexSocketError.errno("read")
        }
    }

    /// Waits until the socket can be read, `other` can be read, or `timeout` passes.
    func wait(alsoWatching other: Int32 = -1, timeout: TimeInterval) throws -> Readiness {
        var fds = [pollfd(fd: fd, events: Int16(POLLIN), revents: 0)]
        if other >= 0 { fds.append(pollfd(fd: other, events: Int16(POLLIN), revents: 0)) }
        let result = poll(&fds, nfds_t(fds.count), Int32(max(timeout, 0) * 1000))
        if result < 0 {
            if Darwin.errno == EINTR { return .timedOut }
            throw CodexSocketError.errno("poll")
        }
        if result == 0 { return .timedOut }
        // A hang-up reads as the end of the stream.
        if fds[0].revents & Int16(POLLIN | POLLHUP | POLLERR) != 0 { return .readable }
        if fds[0].revents & Int16(POLLNVAL) != 0 { throw CodexSocketError(description: "poll: socket closed") }
        return .woken
    }
}

// MARK: - WebSocket

/// The client half of RFC 6455, enough for Codex's control socket: the
/// upgrade, masked frames out, fragmented and control frames in.
enum CodexWebSocket {
    static let guid = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
    /// A larger frame from the daemon is taken as a broken stream.
    static let payloadLimit = 64 * 1024 * 1024
    static let headerLimit = 16 * 1024

    enum Opcode: UInt8 {
        case continuation = 0x0, text = 0x1, binary = 0x2, close = 0x8, ping = 0x9, pong = 0xA

        var isControl: Bool { rawValue & 0x8 != 0 }
    }

    struct Frame: Equatable {
        var fin = true
        let opcode: Opcode
        var payload: Data
        /// Arrived masked, as only a client's frames are.
        var masked = false
    }

    enum Message: Equatable {
        case text(String)
        case binary(Data)
        case ping(Data)
        case pong(Data)
        /// The close frame's payload: its code and reason, when given.
        case close(Data)
    }

    struct ProtocolError: Error, Equatable, CustomStringConvertible {
        let description: String
    }

    static func newMask() -> [UInt8] {
        (0..<4).map { _ in UInt8.random(in: .min ... .max) }
    }

    /// One frame; masked when `mask` is given, as every client frame must be.
    static func encode(opcode: Opcode, payload: Data, fin: Bool = true, mask: [UInt8]?) -> Data {
        var out = Data()
        out.append((fin ? 0x80 : 0) | opcode.rawValue)
        let maskBit: UInt8 = mask == nil ? 0 : 0x80
        let length = payload.count
        if length < 126 {
            out.append(maskBit | UInt8(length))
        } else if length <= 0xFFFF {
            out.append(maskBit | 126)
            out.append(UInt8(length >> 8))
            out.append(UInt8(length & 0xFF))
        } else {
            out.append(maskBit | 127)
            for shift in stride(from: 56, through: 0, by: -8) { out.append(UInt8(truncatingIfNeeded: UInt64(length) >> UInt64(shift))) }
        }
        if let mask, mask.count == 4 {
            out.append(contentsOf: mask)
            var masked = [UInt8](payload)
            for index in masked.indices { masked[index] ^= mask[index & 3] }
            out.append(contentsOf: masked)
        } else {
            out.append(payload)
        }
        return out
    }

    /// Cuts frames out of the bytes as they arrive.
    struct Decoder {
        private var buffer = [UInt8]()
        private var start = 0

        init() {}

        mutating func append(_ data: Data) {
            if start > 0, start == buffer.count {
                buffer.removeAll(keepingCapacity: true)
                start = 0
            }
            buffer.append(contentsOf: data)
        }

        /// The next whole frame, unmasked, or nil until more bytes come.
        mutating func next() throws -> Frame? {
            let available = buffer.count - start
            guard available >= 2 else { return nil }
            let first = buffer[start]
            let second = buffer[start + 1]
            guard first & 0x70 == 0 else { throw ProtocolError(description: "reserved bits set") }
            guard let opcode = Opcode(rawValue: first & 0x0F) else {
                throw ProtocolError(description: "unknown opcode \(first & 0x0F)")
            }
            let masked = second & 0x80 != 0
            var length = UInt64(second & 0x7F)
            var offset = 2
            if length == 126 {
                guard available >= 4 else { return nil }
                length = UInt64(buffer[start + 2]) << 8 | UInt64(buffer[start + 3])
                offset = 4
            } else if length == 127 {
                guard available >= 10 else { return nil }
                length = 0
                for index in 0..<8 { length = length << 8 | UInt64(buffer[start + 2 + index]) }
                offset = 10
            }
            guard length <= UInt64(payloadLimit) else { throw ProtocolError(description: "frame too large") }
            if opcode.isControl {
                guard first & 0x80 != 0, length <= 125 else { throw ProtocolError(description: "bad control frame") }
            }
            let maskLength = masked ? 4 : 0
            let total = offset + maskLength + Int(length)
            guard available >= total else { return nil }
            var payload = [UInt8](buffer[(start + offset + maskLength)..<(start + total)])
            if masked {
                let key = Array(buffer[(start + offset)..<(start + offset + 4)])
                for index in payload.indices { payload[index] ^= key[index & 3] }
            }
            start += total
            if start > 1 << 20 {
                buffer.removeFirst(start)
                start = 0
            }
            return Frame(fin: first & 0x80 != 0, opcode: opcode, payload: Data(payload), masked: masked)
        }
    }

    /// Joins fragmented messages; control frames pass between fragments.
    struct Assembler {
        private var opcode: Opcode?
        private var parts = Data()

        init() {}

        mutating func push(_ frame: Frame) throws -> Message? {
            switch frame.opcode {
            case .ping: return .ping(frame.payload)
            case .pong: return .pong(frame.payload)
            case .close: return .close(frame.payload)
            case .text, .binary:
                guard opcode == nil else { throw ProtocolError(description: "new message inside a fragmented one") }
                if !frame.fin {
                    opcode = frame.opcode
                    parts = frame.payload
                    return nil
                }
                return try message(frame.opcode, frame.payload)
            case .continuation:
                guard let kind = opcode else { throw ProtocolError(description: "continuation without a message") }
                guard parts.count + frame.payload.count <= payloadLimit else {
                    throw ProtocolError(description: "message too large")
                }
                parts.append(frame.payload)
                guard frame.fin else { return nil }
                let whole = parts
                opcode = nil
                parts = Data()
                return try message(kind, whole)
            }
        }

        private func message(_ kind: Opcode, _ payload: Data) throws -> Message {
            guard kind == .text else { return .binary(payload) }
            guard let text = String(data: payload, encoding: .utf8) else {
                throw ProtocolError(description: "text frame is not UTF-8")
            }
            return .text(text)
        }
    }

    static func acceptKey(for key: String) -> String {
        Data(Insecure.SHA1.hash(data: Data((key + guid).utf8))).base64EncodedString()
    }

    static func request(key: String) -> Data {
        Data(("GET /rpc HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
              + "Sec-WebSocket-Key: \(key)\r\nSec-WebSocket-Version: 13\r\n\r\n").utf8)
    }

    /// Checks the server's answer to the upgrade.
    static func validate(response head: String, key: String) throws {
        let lines = head.components(separatedBy: "\r\n")
        let status = lines.first?.split(separator: " ")
        guard let status, status.count >= 2, status[0].hasPrefix("HTTP/1."), status[1] == "101" else {
            throw ProtocolError(description: "upgrade refused: \(lines.first ?? "")")
        }
        var headers: [String: String] = [:]
        for line in lines.dropFirst() {
            guard let colon = line.firstIndex(of: ":") else { continue }
            headers[line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()] =
                line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        }
        guard headers["upgrade"]?.lowercased() == "websocket",
              headers["connection"]?.lowercased().contains("upgrade") == true,
              headers["sec-websocket-accept"] == acceptKey(for: key) else {
            throw ProtocolError(description: "upgrade answer does not match")
        }
    }

    /// Upgrades the connection to `ws://localhost/rpc`; returns the bytes
    /// read past the server's answer, the start of its first frames.
    static func handshake(_ socket: CodexSocket, deadline: Date) throws -> Data {
        let key = Data((0..<16).map { _ in UInt8.random(in: .min ... .max) }).base64EncodedString()
        try socket.write(request(key: key))
        var received = Data()
        let end = Data("\r\n\r\n".utf8)
        while true {
            if let range = received.range(of: end) {
                try validate(response: String(decoding: received[..<range.lowerBound], as: UTF8.self), key: key)
                return Data(received[range.upperBound...])
            }
            guard received.count < headerLimit else { throw ProtocolError(description: "upgrade answer too long") }
            let left = deadline.timeIntervalSinceNow
            guard left > 0 else { throw ProtocolError(description: "no upgrade answer") }
            guard try socket.wait(timeout: min(left, 0.5)) == .readable else { continue }
            guard let data = try socket.read() else { throw ProtocolError(description: "closed during upgrade") }
            received.append(data)
        }
    }
}
