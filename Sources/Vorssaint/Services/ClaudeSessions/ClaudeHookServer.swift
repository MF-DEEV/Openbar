// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import Darwin
import Foundation

/// The local socket Claude Code's hooks write their events to. Each hook
/// connects, writes one JSON object and waits: most events are answered by
/// closing at once, a permission request by the decision or by closing
/// when the island has none. Only processes of the same user may connect,
/// and the socket is readable by its owner alone.
final class ClaudeHookServer {
    /// One hook waiting on the island. Its answer may be written from any
    /// queue; reading happens on the server's queue.
    ///
    /// The descriptor has one owner at a time: while a read source watches
    /// it, the source's cancel handler closes it, and only once that source
    /// is gone does `finish` close it directly.
    final class Connection {
        fileprivate let descriptor: Int32
        // Server queue only.
        fileprivate var buffer = Data()
        fileprivate var delivered = false
        /// The hook process on the other end. The hook stops writing as soon
        /// as its event is sent, so only this tells that it is still there.
        let peer: pid_t

        private let lock = NSLock()
        private var source: DispatchSourceRead?
        /// A read source exists whose cancel handler has not run yet.
        private var watched = false
        /// `finish` was called; the answer may still be being written.
        private var finishing = false
        /// The descriptor is given up: set in the same step that decides who
        /// closes it, so the cancel handler and `finish` never both do.
        private var closed = false

        fileprivate init(descriptor: Int32, peer: pid_t) {
            self.descriptor = descriptor
            self.peer = peer
        }

        var isClosed: Bool { lock.withLock { closed } }

        /// The hook was killed, as when its session answered in the terminal
        /// and moved on, or quit.
        var peerIsGone: Bool { peer > 0 && kill(peer, 0) != 0 && errno == ESRCH }

        fileprivate func watch(_ source: DispatchSourceRead) {
            lock.withLock {
                self.source = source
                watched = true
            }
            source.setCancelHandler { [self] in
                let close = lock.withLock { () -> Bool in
                    watched = false
                    return closed
                }
                if close { Darwin.close(descriptor) }
            }
            source.resume()
        }

        /// Stops reading, keeping the connection for the answer.
        fileprivate func stopReading() {
            let source = lock.withLock { () -> DispatchSourceRead? in
                defer { self.source = nil }
                return self.source
            }
            source?.cancel()
        }

        /// Writes the answer, if any, and ends the connection. Safe from any queue.
        func finish(_ reply: Data? = nil) {
            let first = lock.withLock { () -> Bool in
                defer { finishing = true }
                return !finishing
            }
            guard first else { return }
            if let reply, !reply.isEmpty { write(reply) }
            let (source, watched) = lock.withLock { () -> (DispatchSourceRead?, Bool) in
                defer {
                    self.source = nil
                    closed = true
                }
                return (self.source, self.watched)
            }
            if let source {
                source.cancel()
            } else if !watched {
                Darwin.close(descriptor)
            }
            // Otherwise a cancelled source's handler is still to run, and closes it.
        }

        /// The whole answer, waiting for room in the socket for a moment.
        private func write(_ reply: Data) {
            _ = fcntl(descriptor, F_SETFL, fcntl(descriptor, F_GETFL) & ~O_NONBLOCK)
            var wait = timeval(tv_sec: 2, tv_usec: 0)
            setsockopt(descriptor, SOL_SOCKET, SO_SNDTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
            reply.withUnsafeBytes { bytes in
                guard var base = bytes.baseAddress else { return }
                var left = bytes.count
                while left > 0 {
                    let written = Darwin.write(descriptor, base, left)
                    if written > 0 { left -= written; base += written; continue }
                    if written < 0, errno == EINTR { continue }
                    break
                }
            }
        }
    }

    static let maximumInput = 4 << 20

    let path: String
    private let queue: DispatchQueue
    private let handler: (Data, Connection) -> Void
    private var listener: DispatchSourceRead?

    /// `handler` runs on `queue` with each complete event. The queue should do
    /// nothing slow: hooks wait on it.
    init(path: String, queue: DispatchQueue, handler: @escaping (Data, Connection) -> Void) {
        self.path = path
        self.queue = queue
        self.handler = handler
    }

    deinit { stop() }

    var isRunning: Bool { listener != nil }

    /// Socket paths are limited to a little over a hundred bytes.
    static func fits(_ path: String) -> Bool { path.utf8.count < MemoryLayout.size(ofValue: sockaddr_un().sun_path) }

    /// Call on the queue.
    func start() -> Bool {
        guard listener == nil, Self.fits(path) else { return listener != nil }
        let folder = (path as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true,
                                                 attributes: [.posixPermissions: 0o700])
        unlink(path)
        let descriptor = socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return false }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &address.sun_path) { raw in
            let bytes = Array(path.utf8)
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        // Only the owner may connect, from the moment the socket exists.
        let mask = umask(0o077)
        let bound = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(descriptor, $0, size) }
        }
        umask(mask)
        guard bound == 0, chmod(path, 0o600) == 0, listen(descriptor, 16) == 0 else {
            Darwin.close(descriptor)
            return false
        }
        _ = fcntl(descriptor, F_SETFL, fcntl(descriptor, F_GETFL) | O_NONBLOCK)
        let source = DispatchSource.makeReadSource(fileDescriptor: descriptor, queue: queue)
        source.setEventHandler { [weak self] in self?.accept(on: descriptor) }
        source.setCancelHandler { Darwin.close(descriptor) }
        source.resume()
        listener = source
        return true
    }

    /// Call on the queue. Open connections are left to finish on their own.
    func stop() {
        guard let listener else { return }
        listener.cancel()
        self.listener = nil
        unlink(path)
    }

    private func accept(on descriptor: Int32) {
        while true {
            let client = Darwin.accept(descriptor, nil, nil)
            guard client >= 0 else { return }
            var uid: uid_t = 0
            var gid: gid_t = 0
            guard getpeereid(client, &uid, &gid) == 0, uid == getuid() else {
                Darwin.close(client)
                continue
            }
            var noSignal: Int32 = 1
            setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size))
            _ = fcntl(client, F_SETFL, fcntl(client, F_GETFL) | O_NONBLOCK)
            var peer: pid_t = 0
            var peerSize = socklen_t(MemoryLayout<pid_t>.size)
            getsockopt(client, SOL_LOCAL, LOCAL_PEERPID, &peer, &peerSize)
            let connection = Connection(descriptor: client, peer: peer)
            let source = DispatchSource.makeReadSource(fileDescriptor: client, queue: queue)
            // Held strongly: a read source left without its reader would fire
            // forever. Cancelling it, on EOF or `finish`, lets both go.
            source.setEventHandler { [self, connection] in read(connection) }
            connection.watch(source)
        }
    }

    private func read(_ connection: Connection) {
        var chunk = [UInt8](repeating: 0, count: 64 << 10)
        while true {
            let count = Darwin.read(connection.descriptor, &chunk, chunk.count)
            if count > 0 {
                guard !connection.delivered else { continue }
                connection.buffer.append(contentsOf: chunk[0..<count])
                if connection.buffer.count > Self.maximumInput { return connection.finish() }
                continue
            }
            if count < 0, errno == EINTR { continue }
            if count < 0, errno == EAGAIN || errno == EWOULDBLOCK { break }
            // The hook ended its side once it sent the event, and keeps
            // waiting for the answer: stop reading, keep the connection.
            connection.stopReading()
            if connection.delivered { return }
            if !ClaudeHookMessage.isComplete(connection.buffer) { return connection.finish() }
            break
        }
        guard !connection.delivered, ClaudeHookMessage.isComplete(connection.buffer) else { return }
        connection.delivered = true
        let payload = connection.buffer
        connection.buffer = Data()
        handler(payload, connection)
    }
}
